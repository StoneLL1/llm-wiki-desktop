use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::errors::{BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_UNAVAILABLE};
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::services::import_v2::capability_pack::ResolvedCapabilityPack;
use crate::services::import_v2::engine::{
    validate_engine_result, EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::pack_protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tasks::task_model::CancellationToken;

pub struct PackProcessEngine {
    pack: ResolvedCapabilityPack,
    descriptor: EngineDescriptor,
    timeout: Duration,
    supported_extensions: Vec<String>,
}

impl PackProcessEngine {
    pub fn new(
        pack: ResolvedCapabilityPack,
        route: String,
        supported_extensions: Vec<String>,
        timeout: Duration,
    ) -> Self {
        let descriptor = EngineDescriptor {
            engine_id: format!("pack.{}", pack.manifest.pack_id),
            engine_version: pack.manifest.version.clone(),
            route,
        };
        Self {
            pack,
            descriptor,
            timeout,
            supported_extensions,
        }
    }
}

impl ImportEngine for PackProcessEngine {
    fn descriptor(&self) -> EngineDescriptor {
        self.descriptor.clone()
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
            && self.supported_extensions.iter().any(|extension| {
                input
                    .locator
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{extension}"))
            })
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let child = Command::new(&self.pack.entrypoint)
            .current_dir(&self.pack.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| engine_error("The capability process could not be started."))?;
        let mut child = ProcessGuard(child);
        let rpc = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: request.request_id.clone(),
            method: "import.execute".into(),
            params: request.clone(),
        };
        let mut stdin = child.0
            .stdin
            .take()
            .ok_or_else(|| engine_error("The capability process stdin is unavailable."))?;
        serde_json::to_writer(&mut stdin, &rpc)
            .map_err(|_| engine_error("The capability request could not be encoded."))?;
        stdin
            .write_all(b"\n")
            .map_err(|_| engine_error("The capability request could not be sent."))?;
        drop(stdin);
        let stdout = child.0
            .stdout
            .take()
            .ok_or_else(|| engine_error("The capability process stdout is unavailable."))?;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(response) = serde_json::from_str::<JsonRpcResponse<EngineResult>>(&line) {
                    let _ = sender.send(response);
                    break;
                }
            }
        });
        let started = Instant::now();
        loop {
            if cancellation.is_cancelled() {
                terminate_tree(&mut child.0);
                return Err(cancelled());
            }
            if started.elapsed() >= self.timeout {
                terminate_tree(&mut child.0);
                return Err(engine_error("The capability process timed out."));
            }
            if let Ok(response) = receiver.try_recv() {
                response.validate(&request.request_id)?;
                let result = response
                    .result
                    .ok_or_else(|| engine_error("The capability process reported an error."))?;
                validate_engine_result(&request.staging_root, &result)?;
                return Ok(result);
            }
            if child.0
                .try_wait()
                .map_err(|_| engine_error("The capability process state is unavailable."))?
                .is_some()
            {
                return Err(engine_error(
                    "The capability process exited without a result.",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

struct ProcessGuard(Child);

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            terminate_tree(&mut self.0);
        }
    }
}

fn terminate_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
fn cancelled() -> BackendError {
    BackendError::new(
        IMPORT_V2_CANCELLED,
        "The capability process was cancelled.",
        false,
        false,
    )
}
fn engine_error(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_ENGINE_UNAVAILABLE, message, true, false)
}

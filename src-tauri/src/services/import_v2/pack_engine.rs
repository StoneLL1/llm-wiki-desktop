use std::io::{BufRead, BufReader, Read, Write};
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

const MAX_STDOUT_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDOUT_LINES: usize = 256;
const MAX_STDERR_BYTES: u64 = 1024 * 1024;

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
        let mut command = Command::new(&self.pack.entrypoint);
        command
            .current_dir(&self.pack.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
        }
        let child = command
            .spawn()
            .map_err(|_| engine_error("The capability process could not be started."))?;
        let mut child = ProcessGuard(child, None, None);
        let rpc = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: request.request_id.clone(),
            method: "import.execute".into(),
            params: request.clone(),
        };
        let mut stdin = child
            .0
            .stdin
            .take()
            .ok_or_else(|| engine_error("The capability process stdin is unavailable."))?;
        serde_json::to_writer(&mut stdin, &rpc)
            .map_err(|_| engine_error("The capability request could not be encoded."))?;
        stdin
            .write_all(b"\n")
            .map_err(|_| engine_error("The capability request could not be sent."))?;
        drop(stdin);
        let stdout = child
            .0
            .stdout
            .take()
            .ok_or_else(|| engine_error("The capability process stdout is unavailable."))?;
        let stderr = child
            .0
            .stderr
            .take()
            .ok_or_else(|| engine_error("The capability process stderr is unavailable."))?;
        let stderr_reader = std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = stderr.take(MAX_STDERR_BYTES + 1).read_to_end(&mut sink);
        });
        child.2 = Some(stderr_reader);
        let (sender, receiver) = mpsc::channel::<Result<JsonRpcResponse<EngineResult>, ()>>();
        let stdout_reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            for _ in 0..MAX_STDOUT_LINES {
                let mut bytes = Vec::new();
                match reader
                    .by_ref()
                    .take((MAX_STDOUT_LINE_BYTES + 1) as u64)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) => break,
                    Ok(_) if bytes.len() > MAX_STDOUT_LINE_BYTES || !bytes.ends_with(b"\n") => {
                        let _ = sender.send(Err(()));
                        return;
                    }
                    Err(_) => {
                        let _ = sender.send(Err(()));
                        return;
                    }
                    _ => {}
                }
                if let Ok(line) = std::str::from_utf8(&bytes) {
                    if let Ok(response) =
                        serde_json::from_str::<JsonRpcResponse<EngineResult>>(line.trim_end())
                    {
                        let _ = sender.send(Ok(response));
                        return;
                    }
                }
            }
            let _ = sender.send(Err(()));
        });
        child.1 = Some(stdout_reader);
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
                let response = response.map_err(|_| {
                    engine_error(
                        "The capability process output exceeded protocol limits or was invalid.",
                    )
                })?;
                response.validate(&request.request_id)?;
                let result = response
                    .result
                    .ok_or_else(|| engine_error("The capability process reported an error."))?;
                validate_engine_result(&request.staging_root, &result)?;
                terminate_tree(&mut child.0);
                child.join_readers();
                return Ok(result);
            }
            if child
                .0
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

struct ProcessGuard(
    Child,
    Option<std::thread::JoinHandle<()>>,
    Option<std::thread::JoinHandle<()>>,
);

impl ProcessGuard {
    fn join_readers(&mut self) {
        if let Some(reader) = self.1.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.2.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            terminate_tree(&mut self.0);
        }
        self.join_readers();
    }
}

fn terminate_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new(r"C:\Windows\System32\taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-TERM", &group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for _ in 0..10 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = Command::new("kill")
            .args(["-KILL", &group])
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

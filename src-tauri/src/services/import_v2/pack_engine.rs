use sha2::{Digest, Sha256};
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
        let private_entrypoint = prepare_verified_entrypoint(&self.pack)?;
        let mut command = Command::new(private_entrypoint.as_os_str());
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
        let mut child = ProcessGuard(child, None, None, Some(private_entrypoint));
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
            let _ = sender.send(read_response(stdout));
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

fn prepare_verified_entrypoint(
    pack: &ResolvedCapabilityPack,
) -> Result<tempfile::TempPath, BackendError> {
    let metadata = std::fs::symlink_metadata(&pack.entrypoint)
        .map_err(|_| engine_error("The capability entrypoint is unavailable."))?;
    if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_file() {
        return Err(engine_error(
            "The capability entrypoint changed after verification.",
        ));
    }
    let canonical = std::fs::canonicalize(&pack.entrypoint)
        .map_err(|_| engine_error("The capability entrypoint cannot be resolved."))?;
    if !canonical.starts_with(&pack.root) {
        return Err(engine_error(
            "The capability entrypoint escaped its verified install root.",
        ));
    }
    let bytes = std::fs::read(&canonical)
        .map_err(|_| engine_error("The capability entrypoint cannot be verified."))?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != pack.entrypoint_sha256 {
        return Err(engine_error(
            "The capability entrypoint changed after verification.",
        ));
    }
    let suffix = pack
        .entrypoint
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let mut private = tempfile::Builder::new()
        .prefix("llm-wiki-capability-")
        .suffix(&suffix)
        .tempfile()
        .map_err(|_| engine_error("A private capability entrypoint could not be created."))?;
    private
        .write_all(&bytes)
        .and_then(|_| private.flush())
        .map_err(|_| engine_error("The private capability entrypoint could not be written."))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        private
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|_| engine_error("The private capability entrypoint could not be secured."))?;
    }
    Ok(private.into_temp_path())
}

#[cfg(windows)]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &std::fs::Metadata) -> bool {
    false
}

fn read_response(reader: impl Read) -> Result<JsonRpcResponse<EngineResult>, ()> {
    let mut reader = BufReader::new(reader);
    for _ in 0..MAX_STDOUT_LINES {
        let mut bytes = Vec::new();
        match reader
            .by_ref()
            .take((MAX_STDOUT_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut bytes)
        {
            Ok(0) => break,
            Ok(_) if bytes.len() > MAX_STDOUT_LINE_BYTES || !bytes.ends_with(b"\n") => {
                return Err(())
            }
            Err(_) => return Err(()),
            _ => {}
        }
        if let Ok(line) = std::str::from_utf8(&bytes) {
            if let Ok(response) =
                serde_json::from_str::<JsonRpcResponse<EngineResult>>(line.trim_end())
            {
                return Ok(response);
            }
        }
    }
    Err(())
}

struct ProcessGuard(
    Child,
    Option<std::thread::JoinHandle<()>>,
    Option<std::thread::JoinHandle<()>>,
    Option<tempfile::TempPath>,
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
        let _private_entrypoint = &self.3;
        // Always terminate the recorded process group/tree. The direct child may
        // have exited while a grandchild still owns inherited stdio handles.
        terminate_tree(&mut self.0);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::import_v2::capability_pack::CapabilityPackManifest;
    use std::io::{Cursor, Read};

    #[test]
    fn rejects_stdout_without_newline_beyond_eight_mib() {
        assert!(read_response(Cursor::new(vec![b'x'; MAX_STDOUT_LINE_BYTES + 1])).is_err());
    }

    #[test]
    fn rejects_entrypoint_replaced_after_registration() {
        let root = std::env::temp_dir().join(format!("pack-swap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let entrypoint = root.join("runner.bin");
        std::fs::write(&entrypoint, b"verified").unwrap();
        let pack = ResolvedCapabilityPack {
            manifest: CapabilityPackManifest {
                schema_version: 1,
                pack_id: "fixture".into(),
                version: "1".into(),
                protocol_version: "2".into(),
                target_triples: vec![],
                archive_sha256: String::new(),
                license_expression: "MIT".into(),
                entrypoint: "runner.bin".into(),
                compressed_bytes: 0,
                installed_bytes: 0,
                signing_key_id: "fixture".into(),
                signature: String::new(),
            },
            root: root.canonicalize().unwrap(),
            entrypoint: entrypoint.canonicalize().unwrap(),
            entrypoint_sha256: format!("{:x}", Sha256::digest(b"verified")),
        };
        std::fs::write(&entrypoint, b"replaced").unwrap();
        assert!(prepare_verified_entrypoint(&pack).is_err());
        std::fs::remove_dir_all(root).ok();
    }

    struct SlowReader {
        bytes: Cursor<Vec<u8>>,
    }
    impl Read for SlowReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            std::thread::yield_now();
            let count = out.len().min(3);
            self.bytes.read(&mut out[..count])
        }
    }

    #[test]
    fn accepts_a_valid_response_arriving_in_slow_chunks() {
        let json = b"{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n".to_vec();
        let response = read_response(SlowReader {
            bytes: Cursor::new(json),
        })
        .unwrap();
        assert_eq!(response.id, "r");
    }

    #[test]
    fn process_guard_joins_reader_after_termination() {
        #[cfg(windows)]
        let child = Command::new(r"C:\Windows\System32\cmd.exe")
            .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .spawn()
            .unwrap();
        #[cfg(unix)]
        let child = Command::new("sh").args(["-c", "sleep 30"]).spawn().unwrap();
        let joined = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = joined.clone();
        let reader =
            std::thread::spawn(move || signal.store(true, std::sync::atomic::Ordering::SeqCst));
        drop(ProcessGuard(child, Some(reader), None, None));
        assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
    }
}

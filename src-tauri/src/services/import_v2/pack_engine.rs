use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::errors::{BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_UNAVAILABLE};
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::services::import_v2::capability_pack::{verify_runtime_integrity, ResolvedCapabilityPack};
use crate::services::import_v2::domain_limiter::DomainLimiter;
use crate::services::import_v2::engine::{
    validate_engine_result, EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::pack_protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{WebFetchContent, WebFetchPolicy, WebFetchService};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;

const MAX_STDOUT_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDOUT_LINES: usize = 256;
const MAX_STDERR_BYTES: u64 = 1024 * 1024;

pub struct PackProcessEngine {
    pack: ResolvedCapabilityPack,
    descriptor: EngineDescriptor,
    timeout: Duration,
    supported_extensions: Vec<String>,
    domain_limiter: Arc<DomainLimiter>,
    web_targets: Arc<WebTargetStore>,
}

impl PackProcessEngine {
    pub fn new(
        pack: ResolvedCapabilityPack,
        route: String,
        supported_extensions: Vec<String>,
        timeout: Duration,
        web_targets: Arc<WebTargetStore>,
    ) -> Self {
        let descriptor = EngineDescriptor {
            engine_id: format!("pack.{}.{route}", pack.manifest.pack_id),
            engine_version: pack.manifest.version.clone(),
            route,
        };
        Self {
            pack,
            descriptor,
            timeout,
            supported_extensions,
            domain_limiter: Arc::new(DomainLimiter::default()),
            web_targets,
        }
    }
}

impl ImportEngine for PackProcessEngine {
    fn descriptor(&self) -> EngineDescriptor {
        self.descriptor.clone()
    }

    fn supports(&self, input: &ImportInput) -> bool {
        (input.kind == ImportInputKind::Url && self.supported_extensions.is_empty())
            || (input.kind == ImportInputKind::File
                && self.supported_extensions.iter().any(|extension| {
                    input
                        .locator
                        .to_ascii_lowercase()
                        .ends_with(&format!(".{extension}"))
                }))
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let request = if request.input.kind == ImportInputKind::Url {
            prepare_web_request(
                request,
                cancellation,
                self.domain_limiter.clone(),
                self.web_targets.clone(),
            )?
        } else {
            request.clone()
        };
        let authenticated_profile = self.web_targets.authenticated_profile(&request.item_id)?;
        validate_entrypoint_unchanged(&self.pack)?;
        let runtime_temp = std::path::Path::new(&request.project_root)
            .join(&request.staging_root)
            .join("runtime-temp");
        std::fs::create_dir_all(&runtime_temp)
            .map_err(|_| engine_error("The capability runtime temp directory could not be created."))?;
        let mut command = Command::new(&self.pack.entrypoint);
        command
            .current_dir(&self.pack.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for key in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.env("TEMP", &runtime_temp).env("TMP", &runtime_temp);
        if let Some(profile) = authenticated_profile {
            command.env("LLM_WIKI_CONNECTOR_PROFILE", profile);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            unsafe {
                command.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
        }
        let mut child = command
            .spawn()
            .map_err(|_| engine_error("The capability process could not be started."))?;
        let platform_job = match attach_platform_job(&child) {
            Ok(job) => job,
            Err(error) => {
                terminate_tree(&mut child);
                return Err(error);
            }
        };
        let mut child = ProcessGuard(child, None, None, platform_job);
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
        let (sender, receiver) = mpsc::channel::<Result<PackResponse, ()>>();
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
                response.rpc.validate(&request.request_id)?;
                if let Some(error) = response.rpc.error.as_ref() {
                    let stable = error
                        .data
                        .as_ref()
                        .and_then(|data| data.get("code"))
                        .and_then(|code| code.as_str())
                        .filter(|code| code.starts_with("IMPORT_WEB_"))
                        .unwrap_or(crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE);
                    return Err(BackendError::new(
                        stable.to_string(),
                        "The web capability reported a typed failure.",
                        true,
                        stable.contains("LOGIN") || stable.contains("CHALLENGE"),
                    ));
                }
                let mut result = response
                    .rpc
                    .result
                    .ok_or_else(|| engine_error("The capability process reported an error."))?;
                localize_remote_assets(
                    &request,
                    &mut result,
                    response.remote_assets,
                    cancellation,
                    self.domain_limiter.clone(),
                )?;
                validate_engine_result(&request.staging_root, &result)?;
                terminate_tree(&mut child.0);
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

fn prepare_web_request(
    request: &EngineRequest,
    cancellation: &CancellationToken,
    limiter: Arc<DomainLimiter>,
    targets: Arc<WebTargetStore>,
) -> Result<EngineRequest, BackendError> {
    let target = targets.resolve(
        &request.input.locator,
        request.input.normalized_locator.as_deref(),
    )?;
    let private_grant = targets.take_private(&request.item_id)?;
    let sensitive = matches!(target.public.host.as_str(), "mp.weixin.qq.com")
        || target.public.host.ends_with(".xiaohongshu.com")
        || matches!(target.public.host.as_str(), "x.com" | "twitter.com");
    let token = cancellation.clone();
    let item_id = request.item_id.clone();
    let fetched = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|_| engine_error("The web runtime could not be started."))?;
        runtime.block_on(async move {
            let _permit = limiter
                .acquire(&target.public.host, sensitive)
                .await
                .map_err(|_| engine_error("The domain limiter is unavailable."))?;
            let policy = WebFetchPolicy::default();
            fetch_with_safe_retries(target, &policy, private_grant, &item_id, &token).await
        })
    })
    .join()
    .map_err(|_| engine_error("The web fetch worker failed."))??;
    let root = std::path::Path::new(&request.project_root).join(&request.staging_root);
    std::fs::create_dir_all(&root)
        .map_err(|_| engine_error("The web staging directory could not be created."))?;
    std::fs::write(root.join("fetched.html"), &fetched.bytes)
        .map_err(|_| engine_error("The fetched web response could not be staged."))?;
    let mut prepared = request.clone();
    prepared.input.locator = fetched.final_session_target.request_url.to_string();
    prepared.input.normalized_locator = Some(fetched.final_public_url);
    prepared.chained_input = Some("fetched.html".into());
    Ok(prepared)
}

async fn fetch_with_safe_retries(
    target: crate::services::import_v2::url_policy::SessionWebTarget,
    policy: &WebFetchPolicy,
    private_grant: Option<crate::services::import_v2::url_policy::PrivateTargetGrant>,
    item_id: &str,
    token: &CancellationToken,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let mut last = None;
    for _ in 0..policy.max_attempts_per_route.max(1) {
        if token.is_cancelled() {
            return Err(cancelled());
        }
        match WebFetchService
            .fetch(
                target.clone(),
                &UrlPolicy,
                policy,
                private_grant.as_ref(),
                item_id,
                |_| {},
                || token.is_cancelled(),
            )
            .await
        {
            Ok(value) => return Ok(value),
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "IMPORT_V2_CONNECTOR_RATE_LIMITED"
                        | "IMPORT_V2_DNS_FAILED"
                        | "IMPORT_V2_FETCH_FAILED"
                ) =>
            {
                last = Some(error)
            }
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| engine_error("The web route exhausted its retry budget.")))
}

pub(super) fn validate_entrypoint_unchanged(
    pack: &ResolvedCapabilityPack,
) -> Result<(), BackendError> {
    verify_runtime_integrity(pack)?;
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
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != pack.entrypoint_sha256 {
        return Err(engine_error(
            "The capability entrypoint changed after verification.",
        ));
    }
    Ok(())
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteAssetRequest {
    placeholder: String,
    url: String,
    kind: String,
}
struct PackResponse {
    rpc: JsonRpcResponse<EngineResult>,
    remote_assets: Vec<RemoteAssetRequest>,
}

fn read_response(reader: impl Read) -> Result<PackResponse, ()> {
    let mut reader = BufReader::new(reader);
    let mut remote_assets = Vec::new();
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
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim_end()) else {
                continue;
            };
            if value.get("method").and_then(|v| v.as_str()) == Some("import.remoteAsset") {
                if remote_assets.len() >= 32 {
                    return Err(());
                }
                let request: RemoteAssetRequest =
                    serde_json::from_value(value.get("params").cloned().ok_or(())?)
                        .map_err(|_| ())?;
                if request.url.len() > 8192
                    || request.placeholder.is_empty()
                    || !request
                        .placeholder
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
                    || !matches!(request.kind.as_str(), "image" | "subtitle")
                {
                    return Err(());
                }
                remote_assets.push(request);
                continue;
            }
            if let Ok(rpc) = serde_json::from_value::<JsonRpcResponse<EngineResult>>(value) {
                return Ok(PackResponse { rpc, remote_assets });
            }
        }
    }
    Err(())
}

fn localize_remote_assets(
    request: &EngineRequest,
    result: &mut EngineResult,
    assets: Vec<RemoteAssetRequest>,
    cancellation: &CancellationToken,
    limiter: Arc<DomainLimiter>,
) -> Result<(), BackendError> {
    if assets.is_empty() {
        return Ok(());
    }
    let root = std::path::Path::new(&request.project_root).join(&request.staging_root);
    let markdown_path = root.join(&result.markdown_path);
    let source_path = root.join(&result.source_snapshot_path);
    let mut markdown = std::fs::read_to_string(&markdown_path)
        .map_err(|_| engine_error("The web candidate could not be reopened."))?;
    let mut source = std::fs::read_to_string(&source_path)
        .map_err(|_| engine_error("The sanitized web snapshot could not be reopened."))?;
    for (index, asset) in assets.into_iter().enumerate() {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let target = UrlPolicy.normalize_for_session(&asset.url)?;
        let token = cancellation.clone();
        let item_id = request.item_id.clone();
        let content = if asset.kind == "image" {
            WebFetchContent::Image
        } else {
            WebFetchContent::Subtitle
        };
        let mut policy = WebFetchPolicy::default();
        policy.content = content;
        policy.max_response_bytes = if content == WebFetchContent::Image {
            8 * 1024 * 1024
        } else {
            4 * 1024 * 1024
        };
        let limiter = limiter.clone();
        let fetched = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|_| engine_error("The asset runtime could not be started."))?;
            runtime.block_on(async move {
                let _permit = limiter
                    .acquire(&target.public.host, false)
                    .await
                    .map_err(|_| engine_error("The domain limiter is unavailable."))?;
                fetch_with_safe_retries(target, &policy, None, &item_id, &token).await
            })
        })
        .join()
        .map_err(|_| engine_error("The asset fetch worker failed."))??;
        let extension = safe_asset_extension(&fetched.content_type, content)?;
        let directory = if content == WebFetchContent::Image {
            "assets"
        } else {
            "subtitles"
        };
        let relative = format!("{directory}/web-{index}.{extension}");
        let destination = root.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| engine_error("The asset directory could not be created."))?;
        }
        let transcript = (content == WebFetchContent::Subtitle)
            .then(|| render_subtitle_markdown(&fetched.bytes, &extension))
            .flatten();
        std::fs::write(&destination, fetched.bytes)
            .map_err(|_| engine_error("A localized web asset could not be written."))?;
        let marker = format!("asset://{}", asset.placeholder);
        markdown = markdown.replace(&marker, &relative);
        if let Some(transcript) = transcript {
            markdown.push_str("\n\n## Transcript\n\n");
            markdown.push_str(&transcript);
        }
        source = source.replace(&marker, &relative);
        result.asset_paths.push(relative);
    }
    std::fs::write(markdown_path, markdown)
        .map_err(|_| engine_error("The localized candidate could not be written."))?;
    std::fs::write(source_path, source)
        .map_err(|_| engine_error("The localized snapshot could not be written."))?;
    Ok(())
}

fn render_subtitle_markdown(bytes: &[u8], extension: &str) -> Option<String> {
    if extension != "vtt" && extension != "srt" {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut output = String::new();
    let mut timestamp = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line == "WEBVTT" || line.chars().all(|value| value.is_ascii_digit()) {
            continue;
        }
        if line.contains("-->") {
            timestamp = line.split("-->").next().map(str::trim);
            continue;
        }
        if line.starts_with("NOTE") || line.starts_with("STYLE") || line.starts_with("REGION") {
            continue;
        }
        let clean = line.replace('<', "&lt;").replace('>', "&gt;");
        if let Some(start) = timestamp.take() {
            output.push_str(&format!("- [{start}] {clean}\n"));
        } else if !output.ends_with(&format!("{clean}\n")) {
            output.push_str(&format!("  {clean}\n"));
        }
        if output.len() > 4 * 1024 * 1024 {
            break;
        }
    }
    (!output.trim().is_empty()).then_some(output)
}

fn safe_asset_extension(
    content_type: &str,
    kind: WebFetchContent,
) -> Result<&'static str, BackendError> {
    let mime = content_type.split(';').next().unwrap_or("").trim();
    match (kind, mime) {
        (WebFetchContent::Image, "image/jpeg") => Ok("jpg"),
        (WebFetchContent::Image, "image/png") => Ok("png"),
        (WebFetchContent::Image, "image/gif") => Ok("gif"),
        (WebFetchContent::Image, "image/webp") => Ok("webp"),
        (WebFetchContent::Subtitle, "text/vtt") => Ok("vtt"),
        (WebFetchContent::Subtitle, "application/x-subrip")
        | (WebFetchContent::Subtitle, "text/plain") => Ok("srt"),
        (WebFetchContent::Subtitle, "application/json") => Ok("json"),
        _ => Err(engine_error("The remote asset MIME type is not allowed.")),
    }
}

struct ProcessGuard(
    Child,
    Option<std::thread::JoinHandle<()>>,
    Option<std::thread::JoinHandle<()>>,
    Option<PlatformJob>,
);

impl ProcessGuard {
    fn detach_readers(&mut self) {
        self.1.take();
        self.2.take();
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // Always terminate the recorded process group/tree. The direct child may
        // have exited while a grandchild still owns inherited stdio handles.
        terminate_tree(&mut self.0);
        self.3.take();
        self.detach_readers();
    }
}

pub(super) fn terminate_tree(child: &mut Child) {
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

#[cfg(windows)]
pub(super) struct PlatformJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for PlatformJob {}
#[cfg(windows)]
impl Drop for PlatformJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(windows)]
pub(super) fn attach_platform_job(child: &Child) -> Result<Option<PlatformJob>, BackendError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(engine_error(
                "A kill-on-close Job Object could not be created.",
            ));
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as _,
            std::mem::size_of_val(&info) as u32,
        ) == 0
            || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0
        {
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(engine_error(
                "The capability process could not be assigned to its Job Object.",
            ));
        }
        Ok(Some(PlatformJob(job)))
    }
}
#[cfg(not(windows))]
pub(super) struct PlatformJob;
#[cfg(not(windows))]
pub(super) fn attach_platform_job(_: &Child) -> Result<Option<PlatformJob>, BackendError> {
    Ok(None)
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
    fn renders_localized_vtt_as_timestamped_markdown_without_html() {
        let markdown = render_subtitle_markdown(
            b"WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello <script>\n",
            "vtt",
        )
        .unwrap();
        assert!(markdown.contains("[00:00:01.000] Hello &lt;script&gt;"));
        assert!(!markdown.contains("<script>"));
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
                files: vec![],
            },
            root: root.canonicalize().unwrap(),
            entrypoint: entrypoint.canonicalize().unwrap(),
            entrypoint_sha256: format!("{:x}", Sha256::digest(b"verified")),
        };
        std::fs::write(&entrypoint, b"replaced").unwrap();
        assert!(validate_entrypoint_unchanged(&pack).is_err());
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
        assert_eq!(response.rpc.id, "r");
    }

    #[test]
    fn captures_bounded_remote_assets_only_from_typed_notifications() {
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"import.remoteAsset\",\"params\":{\"placeholder\":\"webasset-0\",\"url\":\"https://cdn.example/image.jpg?signature=secret\",\"kind\":\"image\"}}\n{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n";
        let response = read_response(Cursor::new(input)).unwrap();
        assert_eq!(response.remote_assets.len(), 1);
        assert_eq!(response.remote_assets[0].placeholder, "webasset-0");
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
        let job = attach_platform_job(&child).unwrap();
        drop(ProcessGuard(child, Some(reader), None, job));
        for _ in 0..100 {
            if joined.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
    }
}

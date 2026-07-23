#[cfg(test)]
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, RwLock};
use std::time::{Duration, Instant};

use crate::errors::{BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_UNAVAILABLE};
use crate::models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode};
use crate::services::import_v2::capability_pack::{
    hash_file, verify_runtime_integrity, ResolvedCapabilityPack,
};
use crate::services::import_v2::domain_limiter::DomainLimiter;
use crate::services::import_v2::engine::{
    validate_engine_result, EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::media_router::{
    link_or_copy, move_staged_file, TemporaryMediaWorkspace,
};
use crate::services::import_v2::pack_protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::services::import_v2::redaction::redact_sensitive_text;
use crate::services::import_v2::subtitle::render_subtitle_markdown;
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{WebFetchContent, WebFetchPolicy, WebFetchService};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;

const MAX_STDOUT_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDOUT_LINES: usize = 256;
const MAX_REMOTE_ASSETS: usize = 128;
const MAX_STDERR_BYTES: u64 = 1024 * 1024;

pub struct PackProcessEngine {
    pack: ResolvedCapabilityPack,
    descriptor: EngineDescriptor,
    timeout: Duration,
    supported_extensions: Vec<String>,
    domain_limiter: Arc<DomainLimiter>,
    web_targets: Arc<WebTargetStore>,
    connector_profiles_root: Arc<RwLock<Option<std::path::PathBuf>>>,
}

impl PackProcessEngine {
    pub fn new(
        pack: ResolvedCapabilityPack,
        route: String,
        supported_extensions: Vec<String>,
        timeout: Duration,
        web_targets: Arc<WebTargetStore>,
        connector_profiles_root: Arc<RwLock<Option<std::path::PathBuf>>>,
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
            connector_profiles_root,
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
        let mut request = if request.input.kind == ImportInputKind::Url {
            if self.descriptor.route == "web.generic.browser" {
                // Browser packs must receive the one-shot request URL, not
                // the opaque WebTargetStore reference.  Do not prefetch here:
                // the browser is the authenticated/dynamic fetch path.
                prepare_browser_request(request, self.web_targets.clone())?
            } else {
                prepare_web_request(
                    request,
                    cancellation,
                    self.domain_limiter.clone(),
                    self.web_targets.clone(),
                )?
            }
        } else {
            request.clone()
        };
        if self.descriptor.route == "web.bilibili.metadata" {
            request.input.locator = request
                .input
                .normalized_locator
                .clone()
                .ok_or_else(|| engine_error("The Bilibili public target is unavailable."))?;
        }
        let _fetched_cleanup = if request.input.kind == ImportInputKind::Url {
            if let Some(relative) = request.chained_input.as_deref() {
                let path = std::path::Path::new(&request.project_root)
                    .join(&request.staging_root)
                    .join(relative);
                Some(TemporaryMediaWorkspace::adopt_existing(
                    path.parent()
                        .ok_or_else(|| engine_error("The fetched web workspace is invalid."))?,
                )?)
            } else {
                None
            }
        } else {
            None
        };
        let authenticated_profile = if self.descriptor.route.starts_with("web.")
            && self.descriptor.route != "web.generic.readability"
        {
            let bound = self.web_targets.take_authenticated_profile(
                &request.project_id,
                &request.session_id,
                &request.item_id,
            )?;
            if bound.is_some() {
                bound
            } else {
                persistent_connector_profile(&self.connector_profiles_root, &request)
            }
        } else {
            None
        };
        validate_entrypoint_unchanged(&self.pack)?;
        let project_root = std::fs::canonicalize(&request.project_root)
            .map_err(|_| engine_error("The capability project root is unavailable."))?;
        let requested_staging = std::path::Path::new(&request.staging_root);
        let staging_candidate = if requested_staging.is_absolute() {
            requested_staging.to_path_buf()
        } else {
            project_root.join(requested_staging)
        };
        let staging_root = std::fs::canonicalize(staging_candidate)
            .map_err(|_| engine_error("The capability staging root is unavailable."))?;
        if !staging_root.starts_with(&project_root) {
            return Err(engine_error(
                "The capability staging root is outside the project.",
            ));
        }
        let runtime_temp_workspace =
            TemporaryMediaWorkspace::create_unique(&staging_root, ".capability-runtime")?;
        let runtime_temp = runtime_temp_workspace.path();
        let mut command = Command::new(&self.pack.entrypoint);
        command
            .args(&self.pack.manifest.entrypoint_args)
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
        command.env("TEMP", runtime_temp).env("TMP", runtime_temp);
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
                    let stable = stable_capability_error_code(
                        error
                            .data
                            .as_ref()
                            .and_then(|data| data.get("code"))
                            .and_then(|code| code.as_str()),
                    );
                    return Err(BackendError::new(
                        stable.to_string(),
                        "The capability reported a typed failure.",
                        true,
                        stable.contains("LOGIN") || stable.contains("CHALLENGE"),
                    ));
                }
                let mut result = response
                    .rpc
                    .result
                    .ok_or_else(|| engine_error("The capability process reported an error."))?;
                if result.continuation.is_some() {
                    return Err(engine_error(
                        "Capability packs cannot create privileged continuations directly.",
                    ));
                }
                validate_engine_result(&request.staging_root, &result)?;
                sanitize_capability_text_artifacts(&request, &result)?;
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

fn stable_capability_error_code(code: Option<&str>) -> &str {
    code.filter(|value| {
        value.starts_with("IMPORT_WEB_")
            || value.starts_with("IMPORT_ASR_")
            || value.starts_with("IMPORT_OCR_")
    })
    .unwrap_or(IMPORT_V2_ENGINE_UNAVAILABLE)
}

fn persistent_connector_profile(
    root: &Arc<RwLock<Option<std::path::PathBuf>>>,
    request: &EngineRequest,
) -> Option<std::path::PathBuf> {
    let locator = request
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&request.input.locator);
    let host = url::Url::parse(locator)
        .ok()?
        .host_str()?
        .to_ascii_lowercase();
    let platform = if host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com")
    {
        "bilibili"
    } else if host == "xiaohongshu.com"
        || host.ends_with(".xiaohongshu.com")
        || host == "xhslink.com"
        || host.ends_with(".xhslink.com")
    {
        "xiaohongshu"
    } else if host == "douyin.com"
        || host.ends_with(".douyin.com")
        || host == "iesdouyin.com"
        || host.ends_with(".iesdouyin.com")
    {
        "douyin"
    } else if host == "mp.weixin.qq.com" {
        "wechat"
    } else {
        return None;
    };
    let root = root.read().ok()?.clone()?;
    let root_metadata = std::fs::symlink_metadata(&root).ok()?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return None;
    }
    let profile = root.join(platform);
    let metadata = std::fs::symlink_metadata(&profile).ok()?;
    (metadata.is_dir() && !metadata.file_type().is_symlink()).then_some(profile)
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
        || target.public.host == "xiaohongshu.com"
        || target.public.host.ends_with(".xhslink.com")
        || target.public.host == "xhslink.com"
        || target.public.host.ends_with(".douyin.com")
        || target.public.host == "douyin.com"
        || target.public.host.ends_with(".iesdouyin.com")
        || target.public.host == "iesdouyin.com"
        || target.public.host.ends_with(".bilibili.com")
        || target.public.host == "bilibili.com"
        || target.public.host == "b23.tv"
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
    let fetch_workspace = TemporaryMediaWorkspace::create_unique(&root, ".web-fetch")?;
    let fetch_path = fetch_workspace.path().join("page.html");
    std::fs::write(&fetch_path, &fetched.bytes)
        .map_err(|_| engine_error("The fetched web response could not be staged."))?;
    let mut prepared = request.clone();
    prepared.input.locator = fetched.final_session_target.request_url.to_string();
    prepared.input.normalized_locator = Some(fetched.final_public_url);
    let workspace = fetch_workspace.retain();
    prepared.chained_input = Some(
        workspace
            .join("page.html")
            .strip_prefix(&root)
            .map_err(|_| engine_error("The fetched web response escaped staging."))?
            .to_string_lossy()
            .replace('\\', "/"),
    );
    Ok(prepared)
}

fn prepare_browser_request(
    request: &EngineRequest,
    targets: Arc<WebTargetStore>,
) -> Result<EngineRequest, BackendError> {
    let target = targets.resolve(
        &request.input.locator,
        request.input.normalized_locator.as_deref(),
    )?;
    let mut prepared = request.clone();
    prepared.input.locator = target.request_url.to_string();
    prepared.input.normalized_locator = Some(target.public.public_url);
    Ok(prepared)
}

async fn fetch_with_safe_retries(
    target: crate::services::import_v2::url_policy::SessionWebTarget,
    policy: &WebFetchPolicy,
    private_grant: Option<crate::services::import_v2::url_policy::PrivateTargetGrant>,
    item_id: &str,
    token: &CancellationToken,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    fetch_with_safe_retries_to_path(target, policy, private_grant, item_id, None, token).await
}

async fn fetch_with_safe_retries_to_path(
    target: crate::services::import_v2::url_policy::SessionWebTarget,
    policy: &WebFetchPolicy,
    private_grant: Option<crate::services::import_v2::url_policy::PrivateTargetGrant>,
    item_id: &str,
    destination: Option<&Path>,
    token: &CancellationToken,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let mut last = None;
    for _ in 0..policy.max_attempts_per_route.max(1) {
        if token.is_cancelled() {
            return Err(cancelled());
        }
        let fetched = if let Some(destination) = destination {
            WebFetchService
                .fetch_to_file(
                    target.clone(),
                    &UrlPolicy,
                    policy,
                    private_grant.as_ref(),
                    item_id,
                    destination,
                    |_| {},
                    || token.is_cancelled(),
                )
                .await
        } else {
            WebFetchService
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
        };
        match fetched {
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
    let actual = hash_file(&canonical)
        .map_err(|_| engine_error("The capability entrypoint cannot be verified."))?;
    if !actual.eq_ignore_ascii_case(&pack.entrypoint_sha256) {
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
    let mut remote_assets: Vec<RemoteAssetRequest> = Vec::new();
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
                if remote_assets.len() >= MAX_REMOTE_ASSETS {
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
                    || !matches!(
                        request.kind.as_str(),
                        "image" | "media" | "subtitle" | "temporary_media" | "temporary_image"
                    )
                {
                    return Err(());
                }
                if request.kind == "temporary_media"
                    && remote_assets
                        .iter()
                        .any(|asset| asset.kind == "temporary_media")
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
    let mut markdown = std::fs::read_to_string(&markdown_path)
        .map_err(|_| engine_error("The web candidate could not be reopened."))?;
    let mut transcription_ready = false;
    let mut saw_media = false;
    let mut saw_image = false;
    let mut successful_images = 0usize;
    let mut localized_media = BTreeMap::<String, String>::new();
    for (index, asset) in assets.into_iter().enumerate() {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let temporary_image = asset.kind == "temporary_image";
        let content = match asset.kind.as_str() {
            "image" | "temporary_image" => WebFetchContent::Image,
            "media" => WebFetchContent::Media,
            "subtitle" => WebFetchContent::Subtitle,
            "temporary_media" => WebFetchContent::TemporaryMedia,
            _ => return Err(engine_error("The remote asset kind is not allowed.")),
        };
        let marker = format!("asset://{}", asset.placeholder);
        saw_image |= content == WebFetchContent::Image;
        saw_media |= matches!(
            content,
            WebFetchContent::Media | WebFetchContent::TemporaryMedia
        );
        if matches!(
            content,
            WebFetchContent::Subtitle | WebFetchContent::TemporaryMedia
        ) && transcription_ready
        {
            continue;
        }
        if matches!(content, WebFetchContent::Image | WebFetchContent::Media)
            && !temporary_image
            && request.media_save_mode == MediaSaveMode::ExtractOnly
        {
            // Extraction-only imports must not leave a durable image or media
            // copy. Only the derived Markdown candidate is changed; the raw
            // source snapshot has already crossed its immutable write boundary.
            markdown = remove_asset_reference(&markdown, &marker);
            continue;
        }
        if content == WebFetchContent::TemporaryMedia {
            if let Some(source_relative) = localized_media.get(&asset.url) {
                let workspace = TemporaryMediaWorkspace::create_unique(&root, ".asr-input")?;
                let name = workspace
                    .path()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let extension = Path::new(source_relative)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("mp4");
                let relative = format!("{name}/input.{extension}");
                link_or_copy(&root.join(source_relative), &root.join(&relative))
                    .map_err(|_| engine_error("The temporary ASR input could not be staged."))?;
                workspace.retain();
                result.continuation = Some(
                    crate::services::import_v2::engine::EngineContinuation::LocalAsr {
                        temporary_input_path: relative,
                        media_kind: "audio".into(),
                    },
                );
                continue;
            }
        }
        let target = UrlPolicy.normalize_for_session(&asset.url)?;
        let token = cancellation.clone();
        let item_id = request.item_id.clone();
        let mut policy = WebFetchPolicy::default();
        policy.content = content;
        policy.referer = request
            .input
            .normalized_locator
            .clone()
            .or_else(|| Some(request.input.locator.clone()));
        policy.max_response_bytes = match content {
            WebFetchContent::Image => 8 * 1024 * 1024,
            WebFetchContent::Media => 1024 * 1024 * 1024,
            WebFetchContent::Subtitle => 4 * 1024 * 1024,
            WebFetchContent::TemporaryMedia => 1024 * 1024 * 1024,
            WebFetchContent::Page => unreachable!(),
        };
        if matches!(
            content,
            WebFetchContent::Media | WebFetchContent::TemporaryMedia
        ) {
            policy.total_timeout_ms = 30 * 60 * 1000;
        }
        let streamed_workspace = matches!(
            content,
            WebFetchContent::Media | WebFetchContent::TemporaryMedia
        )
        .then(|| TemporaryMediaWorkspace::create_unique(&root, ".media-fetch"))
        .transpose()?;
        let streamed_path = streamed_workspace
            .as_ref()
            .map(|workspace| workspace.path().join("response.bin"));
        let worker_destination = streamed_path.clone();
        let limiter = limiter.clone();
        let fetched = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|_| engine_error("The asset runtime could not be started."))?;
            runtime.block_on(async move {
                let _permit = limiter
                    .acquire(&target.public.host, false)
                    .await
                    .map_err(|_| engine_error("The domain limiter is unavailable."))?;
                fetch_with_safe_retries_to_path(
                    target,
                    &policy,
                    None,
                    &item_id,
                    worker_destination.as_deref(),
                    &token,
                )
                .await
            })
        })
        .join()
        .map_err(|_| engine_error("The asset fetch worker failed."))?;
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(error) if remote_asset_failure_is_partial(content, transcription_ready) => {
                markdown = remove_asset_reference(&markdown, &marker);
                let label = match content {
                    WebFetchContent::Subtitle => "Platform subtitle",
                    WebFetchContent::Image => "Platform image",
                    WebFetchContent::Media => "Original media",
                    _ => unreachable!(),
                };
                result
                    .warnings
                    .push(format!("{label} was not localized: {}", error.message));
                continue;
            }
            Err(error) => return Err(error),
        };
        let extension = match safe_asset_extension(&fetched.content_type, content) {
            Ok(extension) => extension,
            Err(error) if remote_asset_failure_is_partial(content, transcription_ready) => {
                markdown = remove_asset_reference(&markdown, &marker);
                let label = match content {
                    WebFetchContent::Subtitle => "Platform subtitle",
                    WebFetchContent::Image => "Platform image",
                    WebFetchContent::Media => "Original media",
                    _ => unreachable!(),
                };
                result.warnings.push(format!(
                    "{label} format was not supported: {}",
                    error.message
                ));
                continue;
            }
            Err(error) => return Err(error),
        };
        if matches!(
            content,
            WebFetchContent::Media | WebFetchContent::TemporaryMedia
        ) && fetched.byte_len == 0
        {
            return Err(engine_error("The remote media response was empty."));
        }
        if temporary_image {
            let workspace = TemporaryMediaWorkspace::create_unique(&root, ".ocr-input")?;
            let name = workspace
                .path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let temporary_relative = format!("{name}/input.{extension}");
            std::fs::write(root.join(&temporary_relative), &fetched.bytes)
                .map_err(|_| engine_error("A temporary OCR image could not be written."))?;
            workspace.retain();
            match result.continuation.as_mut() {
                Some(crate::services::import_v2::engine::EngineContinuation::LocalOcr {
                    temporary_input_paths,
                }) => temporary_input_paths.push(temporary_relative),
                None => {
                    result.continuation = Some(
                        crate::services::import_v2::engine::EngineContinuation::LocalOcr {
                            temporary_input_paths: vec![temporary_relative],
                        },
                    );
                }
                Some(_) => {
                    return Err(engine_error(
                        "A web import cannot request OCR and ASR continuations together.",
                    ));
                }
            }
            if request.media_save_mode == MediaSaveMode::PreserveOriginal {
                let durable_relative = format!("assets/web-{index}.{extension}");
                std::fs::create_dir_all(root.join("assets"))
                    .map_err(|_| engine_error("The image directory could not be created."))?;
                std::fs::write(root.join(&durable_relative), &fetched.bytes)
                    .map_err(|_| engine_error("A localized web image could not be written."))?;
                markdown = markdown.replace(&marker, &durable_relative);
                result.asset_paths.push(durable_relative);
            } else {
                markdown = remove_asset_reference(&markdown, &marker);
            }
            successful_images += 1;
            continue;
        }
        let directory = match content {
            WebFetchContent::Image | WebFetchContent::Media => "assets",
            WebFetchContent::Subtitle => "subtitles",
            WebFetchContent::TemporaryMedia => "",
            WebFetchContent::Page => unreachable!(),
        };
        let workspace = if content == WebFetchContent::TemporaryMedia {
            Some(TemporaryMediaWorkspace::create_unique(&root, ".asr-input")?)
        } else {
            None
        };
        let relative = if let Some(workspace) = workspace.as_ref() {
            let name = workspace
                .path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            format!("{name}/input.{extension}")
        } else {
            format!("{directory}/web-{index}.{extension}")
        };
        let destination = root.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| engine_error("The asset directory could not be created."))?;
        }
        let transcript = (content == WebFetchContent::Subtitle)
            .then(|| render_subtitle_markdown(&fetched.bytes, &extension))
            .flatten();
        if content == WebFetchContent::Subtitle && transcript.is_none() {
            result
                .warnings
                .push("Platform subtitle was downloaded but could not be parsed.".into());
        }
        if let Some(streamed_path) = streamed_path.as_ref() {
            move_staged_file(streamed_path, &destination)
                .map_err(|_| engine_error("A streamed web asset could not be staged."))?;
        } else {
            std::fs::write(&destination, &fetched.bytes)
                .map_err(|_| engine_error("A localized web asset could not be written."))?;
        }
        if content == WebFetchContent::TemporaryMedia {
            if let Some(workspace) = workspace {
                workspace.retain();
            }
            result.continuation = Some(
                crate::services::import_v2::engine::EngineContinuation::LocalAsr {
                    temporary_input_path: relative,
                    media_kind: "audio".into(),
                },
            );
            continue;
        }
        markdown = markdown.replace(&marker, &relative);
        if content == WebFetchContent::Image {
            successful_images += 1;
        }
        if let Some(transcript) = transcript {
            markdown.push_str("\n\n## Transcript\n\n");
            markdown.push_str(&transcript);
            transcription_ready = true;
        }
        if content == WebFetchContent::Media {
            localized_media.insert(asset.url, relative.clone());
        }
        result.asset_paths.push(relative);
    }
    if saw_media
        && !transcription_ready
        && !request.local_asr_authorized
        && !request.allow_missing_transcript
    {
        return Err(BackendError::new(
            "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
            "The platform subtitle candidates were unavailable or not parseable.",
            true,
            true,
        ));
    }
    if remote_image_output_is_empty(saw_image, successful_images, result.text_coverage) {
        return Err(BackendError::new(
            "IMPORT_WEB_MEDIA_UNAVAILABLE",
            "The image post had no text and none of its images could be localized.",
            true,
            true,
        ));
    }
    std::fs::write(markdown_path, markdown)
        .map_err(|_| engine_error("The localized candidate could not be written."))?;
    Ok(())
}

fn remote_asset_failure_is_partial(content: WebFetchContent, transcription_ready: bool) -> bool {
    matches!(content, WebFetchContent::Image | WebFetchContent::Subtitle)
        || (content == WebFetchContent::Media && transcription_ready)
}

fn remote_image_output_is_empty(
    saw_image: bool,
    successful_images: usize,
    text_coverage: Option<f64>,
) -> bool {
    saw_image && successful_images == 0 && text_coverage.unwrap_or_default() <= 0.0
}

fn remove_asset_reference(markdown: &str, marker: &str) -> String {
    let had_trailing_newline = markdown.ends_with('\n');
    let mut lines = markdown
        .lines()
        .filter_map(|line| {
            if !line.contains(marker) {
                return Some(line.to_owned());
            }
            let trimmed = line.trim();
            if trimmed.contains("](") && trimmed.ends_with(')') {
                None
            } else {
                Some(line.replace(marker, ""))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if had_trailing_newline {
        lines.push('\n');
    }
    lines
}

fn sanitize_capability_text_artifacts(
    request: &EngineRequest,
    result: &EngineResult,
) -> Result<(), BackendError> {
    let root = std::path::Path::new(&request.project_root).join(&request.staging_root);
    let mut paths = vec![
        root.join(&result.source_snapshot_path),
        root.join(&result.markdown_path),
    ];
    if let Some(metadata) = result.metadata_path.as_deref() {
        paths.push(root.join(metadata));
    }
    for path in paths {
        let bytes = std::fs::read(&path)
            .map_err(|_| engine_error("A capability artifact could not be reopened."))?;
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        std::fs::write(&path, redact_sensitive_text(&text))
            .map_err(|_| engine_error("A capability artifact could not be sanitized."))?;
    }
    Ok(())
}

fn safe_asset_extension(
    content_type: &str,
    kind: WebFetchContent,
) -> Result<&'static str, BackendError> {
    let mime = content_type.split(';').next().unwrap_or("").trim();
    let media = matches!(
        kind,
        WebFetchContent::Media | WebFetchContent::TemporaryMedia
    );
    match (kind, mime) {
        (WebFetchContent::Image, "image/jpeg") => Ok("jpg"),
        (WebFetchContent::Image, "image/png") => Ok("png"),
        (WebFetchContent::Image, "image/gif") => Ok("gif"),
        (WebFetchContent::Image, "image/webp") => Ok("webp"),
        (WebFetchContent::Image, "image/avif") => Ok("avif"),
        (WebFetchContent::Subtitle, "text/vtt") => Ok("vtt"),
        (WebFetchContent::Subtitle, "application/x-subrip")
        | (WebFetchContent::Subtitle, "text/plain") => Ok("srt"),
        (WebFetchContent::Subtitle, "text/ass")
        | (WebFetchContent::Subtitle, "text/x-ass")
        | (WebFetchContent::Subtitle, "application/x-ass")
        | (WebFetchContent::Subtitle, "text/ssa")
        | (WebFetchContent::Subtitle, "text/x-ssa")
        | (WebFetchContent::Subtitle, "application/x-ssa") => Ok("ass"),
        (WebFetchContent::Subtitle, "application/json") => Ok("json"),
        (_, "audio/mpeg") if media => Ok("mp3"),
        (_, "audio/wav" | "audio/x-wav") if media => Ok("wav"),
        (_, "audio/mp4") if media => Ok("m4a"),
        (_, "video/mp4") if media => Ok("mp4"),
        (_, "video/webm") if media => Ok("webm"),
        (_, "video/quicktime") if media => Ok("mov"),
        (_, "video/x-matroska") if media => Ok("mkv"),
        (_, "application/octet-stream") if media => Ok("mp4"),
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
    fn preserves_only_namespaced_capability_error_codes() {
        assert_eq!(
            stable_capability_error_code(Some("IMPORT_OCR_IMAGE_TOO_LARGE")),
            "IMPORT_OCR_IMAGE_TOO_LARGE"
        );
        assert_eq!(
            stable_capability_error_code(Some("IMPORT_ASR_TIMEOUT")),
            "IMPORT_ASR_TIMEOUT"
        );
        assert_eq!(
            stable_capability_error_code(Some("UNTRUSTED_ERROR")),
            IMPORT_V2_ENGINE_UNAVAILABLE
        );
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
                entrypoint_args: Vec::new(),
                executable_files: Vec::new(),
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
    fn remote_asset_bound_supports_a_hundred_image_post_without_becoming_unbounded() {
        let response_line = "{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n";
        let build = |count: usize| {
            let mut input = String::new();
            for index in 0..count {
                input.push_str(&format!(
                    "{{\"jsonrpc\":\"2.0\",\"method\":\"import.remoteAsset\",\"params\":{{\"placeholder\":\"webasset-{index}\",\"url\":\"https://cdn.example/{index}.jpg\",\"kind\":\"image\"}}}}\n"
                ));
            }
            input.push_str(response_line);
            input
        };
        assert_eq!(
            read_response(Cursor::new(build(100)))
                .unwrap()
                .remote_assets
                .len(),
            100
        );
        assert!(read_response(Cursor::new(build(MAX_REMOTE_ASSETS + 1))).is_err());
    }

    #[test]
    fn removing_an_unavailable_asset_drops_its_markdown_link_line() {
        let markdown = "## 图片\n\n1. ![第 1 张](asset://webasset-0)\n2. ![第 2 张](asset://webasset-1)\n\n正文 asset://webasset-0\n";
        let cleaned = remove_asset_reference(markdown, "asset://webasset-0");
        assert!(!cleaned.contains("![第 1 张]()"));
        assert!(!cleaned.contains("asset://webasset-0"));
        assert!(cleaned.contains("2. ![第 2 张](asset://webasset-1)"));
        assert!(cleaned.contains("正文 \n"));
    }

    #[test]
    fn an_image_only_remote_post_requires_at_least_one_localized_image() {
        assert!(remote_image_output_is_empty(true, 0, Some(0.0)));
        assert!(!remote_image_output_is_empty(true, 1, Some(0.0)));
        assert!(!remote_image_output_is_empty(true, 0, Some(1.0)));
    }

    #[test]
    fn image_failures_are_partial_but_asr_media_failures_are_fatal() {
        assert!(remote_asset_failure_is_partial(
            WebFetchContent::Image,
            false
        ));
        assert!(remote_asset_failure_is_partial(
            WebFetchContent::Subtitle,
            false
        ));
        assert!(remote_asset_failure_is_partial(
            WebFetchContent::Media,
            true
        ));
        assert!(!remote_asset_failure_is_partial(
            WebFetchContent::Media,
            false
        ));
        assert!(!remote_asset_failure_is_partial(
            WebFetchContent::TemporaryMedia,
            false
        ));
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

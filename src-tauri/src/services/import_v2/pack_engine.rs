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
    validate_engine_result, EngineDescriptor, EngineProgress, EngineProgressReporter,
    EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::media_router::{
    link_or_copy, move_staged_file, TemporaryMediaWorkspace,
};
use crate::services::import_v2::pack_protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::services::import_v2::platform_network_policy::{
    trusted_platform_page_host_suffixes, upgrade_trusted_platform_page_to_https,
};
use crate::services::import_v2::redaction::redact_sensitive_text;
use crate::services::import_v2::subtitle::render_subtitle_markdown;
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{
    WebFetchContent, WebFetchPolicy, WebFetchProgress, WebFetchService,
};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;
use crate::utils::process_lifetime::{configure_isolated_process, ProcessLifetimeGuard};

const MAX_STDOUT_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDOUT_LINES: usize = 256;
const MAX_REMOTE_ASSETS: usize = 128;
const MAX_STDERR_BYTES: u64 = 1024 * 1024;
const PROCESS_OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const CAPABILITY_HEALTH_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HEALTH_RESPONSE_BYTES: u64 = 64 * 1024;

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

pub(crate) fn probe_capability_pack(
    pack: &ResolvedCapabilityPack,
    capability_id: &str,
    route: &str,
    cancellation: &CancellationToken,
) -> Result<(), BackendError> {
    validate_entrypoint_unchanged(pack)?;
    let request_id = format!("health-{}", uuid::Uuid::new_v4());
    let mut command = Command::new(&pack.entrypoint);
    command
        .args(&pack.manifest.entrypoint_args)
        .current_dir(&pack.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_clear();
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    configure_isolated_process(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| health_error("The capability health process could not be started."))?;
    let lifetime = ProcessLifetimeGuard::attach_capability(&mut child)
        .map_err(|_| health_error("The capability health process could not be isolated."))?;
    let mut child = ProcessGuard(child, None, None, Some(lifetime));
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "capability.health",
        "params": {
            "protocolVersion": "2",
            "capabilityId": capability_id,
            "route": route,
        },
    });
    let mut stdin = child
        .0
        .stdin
        .take()
        .ok_or_else(|| health_error("The capability health process stdin is unavailable."))?;
    serde_json::to_writer(&mut stdin, &request)
        .map_err(|_| health_error("The capability health request could not be encoded."))?;
    stdin
        .write_all(b"\n")
        .map_err(|_| health_error("The capability health request could not be sent."))?;
    drop(stdin);
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or_else(|| health_error("The capability health process stdout is unavailable."))?;
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(MAX_HEALTH_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    child.1 = Some(reader);
    let started = Instant::now();
    let bytes = loop {
        if cancellation.is_cancelled() {
            terminate_tree(&mut child.0);
            return Err(cancelled());
        }
        if started.elapsed() >= CAPABILITY_HEALTH_TIMEOUT {
            terminate_tree(&mut child.0);
            return Err(health_error("The capability health process timed out."));
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(bytes)) => break bytes,
            Ok(Err(_)) => {
                return Err(health_error(
                    "The capability health response could not be read.",
                ))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(health_error(
                    "The capability health response reader stopped unexpectedly.",
                ))
            }
        }
    };
    if bytes.is_empty() || bytes.len() as u64 > MAX_HEALTH_RESPONSE_BYTES {
        return Err(health_error("The capability health response is invalid."));
    }
    let response: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| health_error("The capability health response is invalid."))?;
    let healthy = response.get("jsonrpc").and_then(|value| value.as_str()) == Some("2.0")
        && response.get("id").and_then(|value| value.as_str()) == Some(request_id.as_str())
        && response
            .pointer("/result/healthy")
            .and_then(|value| value.as_bool())
            == Some(true)
        && response
            .pointer("/result/protocolVersion")
            .and_then(|value| value.as_str())
            == Some("2")
        && response
            .pointer("/result/capabilityId")
            .and_then(|value| value.as_str())
            == Some(capability_id)
        && response
            .pointer("/result/route")
            .and_then(|value| value.as_str())
            == Some(route)
        && response.get("error").is_none_or(serde_json::Value::is_null);
    if !healthy {
        return Err(health_error(
            "The capability health response did not confirm readiness.",
        ));
    }
    Ok(())
}

fn health_error(message: &str) -> BackendError {
    BackendError::new(
        "IMPORT_V2_CAPABILITY_HEALTH_CHECK_FAILED",
        message,
        true,
        false,
    )
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
        self.execute_with_progress(request, cancellation, &|_| Ok(()))
    }

    fn execute_with_progress(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
        report_progress: &EngineProgressReporter<'_>,
    ) -> Result<EngineResult, BackendError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        if self.descriptor.route == "media.asr" {
            report_progress(EngineProgress {
                current: 0,
                total: Some(100),
                label: "asr.preparing".into(),
            })?;
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
        if self.descriptor.route == "web.generic.browser"
            && request.input.kind == ImportInputKind::Url
        {
            if let Some(grant) = self
                .web_targets
                .private_for_operation(&request.item_id, &request.task_id)?
            {
                let authority = serde_json::json!({
                    "scheme": &grant.scheme,
                    "host": &grant.host,
                    "port": grant.port,
                    "resolvedIps": grant
                        .resolved_ips
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                });
                command.env(
                    "LLM_WIKI_PRIVATE_TARGET_AUTHORITY",
                    serde_json::to_string(&authority)
                        .map_err(|_| engine_error("The private target authority is invalid."))?,
                );
            }
        }
        configure_isolated_process(&mut command);
        let mut child = command
            .spawn()
            .map_err(|_| engine_error("The capability process could not be started."))?;
        let lifetime = ProcessLifetimeGuard::attach_capability(&mut child)
            .map_err(|_| engine_error("The capability process could not be isolated."))?;
        let mut child = ProcessGuard(child, None, None, Some(lifetime));
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
        let (sender, receiver) = mpsc::channel::<PackOutputEvent>();
        let stdout_reader = std::thread::spawn(move || {
            let progress_sender = sender.clone();
            let response = read_response_with_progress(stdout, move |progress| {
                let _ = progress_sender.send(PackOutputEvent::Progress(progress));
            });
            let _ = sender.send(PackOutputEvent::Response(response));
        });
        child.1 = Some(stdout_reader);
        let started = Instant::now();
        let mut process_exited_at = None;
        loop {
            if cancellation.is_cancelled() {
                terminate_tree(&mut child.0);
                return Err(cancelled());
            }
            if started.elapsed() >= self.timeout {
                terminate_tree(&mut child.0);
                return Err(engine_error("The capability process timed out."));
            }
            let event = match receiver.try_recv() {
                Ok(event) => Some(event),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(engine_error(
                        "The capability process output reader stopped unexpectedly.",
                    ))
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if process_exited_at.is_none()
                        && child
                            .0
                            .try_wait()
                            .map_err(|_| {
                                engine_error("The capability process state is unavailable.")
                            })?
                            .is_some()
                    {
                        process_exited_at = Some(Instant::now());
                    }
                    match process_exited_at {
                        Some(exited_at) => Some(receive_output_after_exit(&receiver, exited_at)?),
                        None => None,
                    }
                }
            };
            if let Some(event) = event {
                let response = match event {
                    PackOutputEvent::Progress(progress) => {
                        report_progress(EngineProgress {
                            current: progress.current,
                            total: Some(progress.total),
                            label: progress.label,
                        })?;
                        continue;
                    }
                    PackOutputEvent::Response(response) => response,
                };
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
                    self.web_targets
                        .private_for_operation(&request.item_id, &request.task_id)?
                        .as_ref(),
                    report_progress,
                )?;
                validate_engine_result(&request.staging_root, &result)?;
                terminate_tree(&mut child.0);
                return Ok(result);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn receive_output_after_exit(
    receiver: &mpsc::Receiver<PackOutputEvent>,
    exited_at: Instant,
) -> Result<PackOutputEvent, BackendError> {
    let remaining = PROCESS_OUTPUT_DRAIN_TIMEOUT
        .checked_sub(exited_at.elapsed())
        .ok_or_else(|| engine_error("The capability process exited without a result."))?;
    receiver
        .recv_timeout(remaining)
        .map_err(|_| engine_error("The capability process exited without a result."))
}

fn stable_capability_error_code(code: Option<&str>) -> &str {
    code.filter(|value| {
        value.starts_with("IMPORT_WEB_")
            || value.starts_with("IMPORT_ASR_")
            || value.starts_with("IMPORT_OCR_")
            || *value == "IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE"
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
        || host == "xhslink.cn"
        || host.ends_with(".xhslink.cn")
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
    let target = upgrade_trusted_platform_page_to_https(target)?;
    let private_grant = targets.private_for_operation(&request.item_id, &request.task_id)?;
    let sensitive = matches!(target.public.host.as_str(), "mp.weixin.qq.com")
        || target.public.host.ends_with(".xiaohongshu.com")
        || target.public.host == "xiaohongshu.com"
        || target.public.host.ends_with(".xhslink.com")
        || target.public.host == "xhslink.com"
        || target.public.host.ends_with(".xhslink.cn")
        || target.public.host == "xhslink.cn"
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
            let page_hosts = trusted_platform_page_host_suffixes(target.request_url.as_str());
            let mut policy = WebFetchPolicy::default();
            if !page_hosts.is_empty() {
                policy.require_https = true;
                policy.allowed_host_suffixes =
                    page_hosts.iter().map(|suffix| (*suffix).into()).collect();
            }
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
    let target = upgrade_trusted_platform_page_to_https(target)?;
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
    fetch_with_safe_retries_to_path(
        target,
        policy,
        private_grant,
        item_id,
        None,
        token,
        None,
        |_| {},
    )
    .await
}

async fn fetch_with_safe_retries_to_path<F>(
    target: crate::services::import_v2::url_policy::SessionWebTarget,
    policy: &WebFetchPolicy,
    private_grant: Option<crate::services::import_v2::url_policy::PrivateTargetGrant>,
    item_id: &str,
    destination: Option<&Path>,
    token: &CancellationToken,
    worker_stop: Option<&CancellationToken>,
    mut progress: F,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError>
where
    F: FnMut(WebFetchProgress),
{
    let mut last = None;
    for _ in 0..policy.max_attempts_per_route.max(1) {
        if token.is_cancelled() || worker_stop.is_some_and(CancellationToken::is_cancelled) {
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
                    &mut progress,
                    || {
                        token.is_cancelled()
                            || worker_stop.is_some_and(CancellationToken::is_cancelled)
                    },
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
                    &mut progress,
                    || {
                        token.is_cancelled()
                            || worker_stop.is_some_and(CancellationToken::is_cancelled)
                    },
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteAssetRequest {
    placeholder: String,
    url: String,
    kind: String,
    automatic: Option<bool>,
    language: Option<String>,
    label: Option<String>,
}
struct PackResponse {
    rpc: JsonRpcResponse<EngineResult>,
    remote_assets: Vec<RemoteAssetRequest>,
}

enum PackOutputEvent {
    Progress(CapabilityProgress),
    Response(Result<PackResponse, ()>),
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityProgress {
    current: u64,
    total: u64,
    label: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityProgressNotification {
    jsonrpc: String,
    method: String,
    params: CapabilityProgress,
}

#[cfg(test)]
fn read_response(reader: impl Read) -> Result<PackResponse, ()> {
    read_response_with_progress(reader, |_| {})
}

fn read_response_with_progress(
    reader: impl Read,
    mut on_progress: impl FnMut(CapabilityProgress),
) -> Result<PackResponse, ()> {
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
                    || !subtitle_metadata_is_valid(&request)
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
            if value.get("method").and_then(|v| v.as_str()) == Some("import.progress") {
                let notification: CapabilityProgressNotification =
                    serde_json::from_value(value).map_err(|_| ())?;
                if notification.jsonrpc != "2.0" || notification.method != "import.progress" {
                    return Err(());
                }
                let progress = notification.params;
                if progress.total == 0
                    || progress.total > 10_000
                    || progress.current > progress.total
                    || progress.label.is_empty()
                    || progress.label.len() > 64
                    || !progress.label.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
                    })
                {
                    return Err(());
                }
                on_progress(progress);
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
    private_grant: Option<&crate::services::import_v2::url_policy::PrivateTargetGrant>,
    report_progress: &EngineProgressReporter<'_>,
) -> Result<(), BackendError> {
    let root = std::path::Path::new(&request.project_root).join(&request.staging_root);
    let markdown_path = root.join(&result.markdown_path);
    let mut markdown = std::fs::read_to_string(&markdown_path)
        .map_err(|_| engine_error("The web candidate could not be reopened."))?;
    let bilibili_video = metadata_declares_platform_video(&root, result, "bilibili");
    let xiaohongshu_video = metadata_declares_platform_video(&root, result, "xiaohongshu");
    let mut transcription_ready = false;
    let mut saw_media = false;
    let mut localized_original_media = false;
    let mut saw_image = false;
    let mut saw_temporary_image = false;
    let mut successful_images = 0usize;
    let image_total = assets
        .iter()
        .filter(|asset| matches!(asset.kind.as_str(), "image" | "temporary_image"))
        .count() as u64;
    let mut processed_images = 0_u64;
    if image_total > 0 {
        report_progress(EngineProgress {
            current: 0,
            total: Some(image_total),
            label: "images.downloading".into(),
        })?;
    }
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
        saw_temporary_image |= temporary_image;
        let source_image_number = if content == WebFetchContent::Image {
            let source_image_number = processed_images + 1;
            report_progress(EngineProgress {
                current: processed_images,
                total: Some(image_total),
                label: "images.downloading".into(),
            })?;
            processed_images = source_image_number;
            Some(source_image_number)
        } else {
            None
        };
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
        if let Some(suffixes) = platform_asset_redirect_suffixes(
            request
                .input
                .normalized_locator
                .as_deref()
                .unwrap_or(&request.input.locator),
        ) {
            policy.require_https = true;
            policy.allowed_host_suffixes = suffixes.iter().map(|suffix| (*suffix).into()).collect();
        }
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
        let worker_private_grant = private_grant.cloned();
        let limiter = limiter.clone();
        let report_download = matches!(
            content,
            WebFetchContent::Media | WebFetchContent::TemporaryMedia
        );
        let progress_stop = CancellationToken::new();
        let worker_stop = progress_stop.clone();
        let (progress_sender, progress_receiver) = mpsc::channel::<WebFetchProgress>();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|_| engine_error("The asset runtime could not be started."))?;
            runtime.block_on(async move {
                let _permit = limiter
                    .acquire(&target.public.host, false)
                    .await
                    .map_err(|_| engine_error("The domain limiter is unavailable."))?;
                let mut last_progress_bucket = None;
                fetch_with_safe_retries_to_path(
                    target,
                    &policy,
                    worker_private_grant,
                    &item_id,
                    worker_destination.as_deref(),
                    &token,
                    Some(&worker_stop),
                    move |progress| {
                        if !report_download {
                            return;
                        }
                        let bucket = progress
                            .total_bytes
                            .filter(|total| *total > 0)
                            .map(|total| {
                                progress.downloaded_bytes.min(total).saturating_mul(100) / total
                            })
                            .unwrap_or(0);
                        if last_progress_bucket == Some(bucket) {
                            return;
                        }
                        last_progress_bucket = Some(bucket);
                        let _ = progress_sender.send(progress);
                    },
                )
                .await
            })
        });
        let mut progress_error = None;
        while !worker.is_finished() {
            while let Ok(progress) = progress_receiver.try_recv() {
                if let Err(error) = report_progress(EngineProgress {
                    current: progress.downloaded_bytes,
                    total: progress.total_bytes,
                    label: "media.downloading".into(),
                }) {
                    progress_stop.cancel();
                    progress_error = Some(error);
                    break;
                }
            }
            if progress_error.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let fetched = worker
            .join()
            .map_err(|_| engine_error("The asset fetch worker failed."))?;
        if let Some(error) = progress_error {
            return Err(error);
        }
        while let Ok(progress) = progress_receiver.try_recv() {
            report_progress(EngineProgress {
                current: progress.downloaded_bytes,
                total: progress.total_bytes,
                label: "media.downloading".into(),
            })?;
        }
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(error) if remote_asset_failure_is_partial(content, transcription_ready) => {
                markdown = mark_remote_asset_unavailable(&markdown, &marker, content);
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
                markdown = mark_remote_asset_unavailable(&markdown, &marker, content);
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
            let temporary_relative = format!(
                "{name}/image-{:03}.{extension}",
                source_image_number.unwrap_or(1)
            );
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
            append_platform_transcript(&mut markdown, &transcript, &asset);
            update_transcript_metadata(&root, result, &asset)?;
            transcription_ready = true;
        }
        if content == WebFetchContent::Media {
            localized_media.insert(asset.url, relative.clone());
            localized_original_media = true;
        }
        result.asset_paths.push(relative);
    }
    if image_total > 0 {
        report_progress(EngineProgress {
            current: image_total,
            total: Some(image_total),
            label: "images.downloading".into(),
        })?;
    }
    let local_asr_ready = matches!(
        result.continuation.as_ref(),
        Some(crate::services::import_v2::engine::EngineContinuation::LocalAsr { .. })
    );
    let declared_video = bilibili_video || xiaohongshu_video;
    let transcript_missing = transcript_is_missing(saw_media, declared_video, transcription_ready);
    if transcript_failure_required(transcript_missing, local_asr_ready) {
        return Err(BackendError::new(
            "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
            "The platform subtitle candidates were unavailable or not parseable.",
            true,
            true,
        ));
    }
    if xiaohongshu_video
        && request.media_save_mode == MediaSaveMode::PreserveOriginal
        && !localized_original_media
    {
        return Err(BackendError::new(
            "IMPORT_WEB_MEDIA_UNAVAILABLE",
            "The Xiaohongshu video did not expose a downloadable original media stream.",
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
    if required_ocr_image_output_is_empty(saw_temporary_image, successful_images) {
        return Err(BackendError::new(
            "IMPORT_WEB_MEDIA_UNAVAILABLE",
            "None of the note images could be downloaded for required OCR.",
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

fn platform_asset_redirect_suffixes(source_url: &str) -> Option<&'static [&'static str]> {
    const BILIBILI_SUFFIXES: &[&str] = &[
        "bilibili.com",
        "b23.tv",
        "bilivideo.com",
        "bilivideo.cn",
        "hdslb.com",
        "biliimg.com",
        "edge.mountaintoys.cn",
    ];
    const XIAOHONGSHU_SUFFIXES: &[&str] = &[
        "xiaohongshu.com",
        "xhslink.com",
        "xhslink.cn",
        "xhscdn.com",
        "xhscdn.net",
    ];
    let host = url::Url::parse(source_url)
        .ok()?
        .host_str()?
        .to_ascii_lowercase();
    if host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com") {
        Some(BILIBILI_SUFFIXES)
    } else if host == "xiaohongshu.com"
        || host.ends_with(".xiaohongshu.com")
        || host == "xhslink.com"
        || host.ends_with(".xhslink.com")
        || host == "xhslink.cn"
        || host.ends_with(".xhslink.cn")
    {
        Some(XIAOHONGSHU_SUFFIXES)
    } else {
        None
    }
}

fn metadata_declares_platform_video(root: &Path, result: &EngineResult, platform: &str) -> bool {
    let Some(relative) = result.metadata_path.as_deref() else {
        return false;
    };
    let Ok(bytes) = std::fs::read(root.join(relative)) else {
        return false;
    };
    let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    metadata.get("platform").and_then(serde_json::Value::as_str) == Some(platform)
        && metadata
            .get("contentType")
            .and_then(serde_json::Value::as_str)
            == Some("video")
}

fn transcript_is_missing(saw_media: bool, declared_video: bool, transcription_ready: bool) -> bool {
    (saw_media || declared_video) && !transcription_ready
}

fn transcript_failure_required(unresolved_transcript: bool, local_asr_ready: bool) -> bool {
    unresolved_transcript && !local_asr_ready
}

fn subtitle_metadata_is_valid(request: &RemoteAssetRequest) -> bool {
    let has_metadata =
        request.automatic.is_some() || request.language.is_some() || request.label.is_some();
    if request.kind != "subtitle" && has_metadata {
        return false;
    }
    [request.language.as_deref(), request.label.as_deref()]
        .into_iter()
        .flatten()
        .all(|value| !value.is_empty() && value.len() <= 64 && !value.chars().any(char::is_control))
}

fn append_platform_transcript(markdown: &mut String, transcript: &str, asset: &RemoteAssetRequest) {
    let source = match asset.automatic {
        Some(true) => "平台自动字幕",
        Some(false) => "平台人工字幕",
        None => "平台字幕",
    };
    let mut provenance = vec![source.to_string()];
    if let Some(label) = asset.label.as_deref() {
        provenance.push(format!(
            "标签：{}",
            escape_metadata_text(&redact_sensitive_text(label))
        ));
    }
    if let Some(language) = asset.language.as_deref() {
        provenance.push(format!(
            "语言：{}",
            escape_metadata_text(&redact_sensitive_text(language))
        ));
    }
    markdown.push_str("\n\n## 字幕 / 转写\n\n> 来源：");
    markdown.push_str(&provenance.join(" · "));
    markdown.push_str("\n\n");
    markdown.push_str(transcript);
}

fn update_transcript_metadata(
    root: &Path,
    result: &EngineResult,
    asset: &RemoteAssetRequest,
) -> Result<(), BackendError> {
    let Some(relative) = result.metadata_path.as_deref() else {
        return Ok(());
    };
    let path = root.join(relative);
    let bytes = std::fs::read(&path)
        .map_err(|_| engine_error("The browser metadata could not be reopened."))?;
    let mut metadata = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|_| engine_error("The browser metadata was invalid."))?;
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| engine_error("The browser metadata was not an object."))?;
    let source = match asset.automatic {
        Some(true) => "platform_auto_subtitle",
        Some(false) => "platform_human_subtitle",
        None => "platform_subtitle",
    };
    object.insert("transcriptSource".into(), source.into());
    if let Some(language) = asset.language.as_deref() {
        object.insert(
            "transcriptLanguage".into(),
            redact_sensitive_text(language).into(),
        );
    }
    if let Some(label) = asset.label.as_deref() {
        object.insert(
            "transcriptLabel".into(),
            redact_sensitive_text(label).into(),
        );
    }
    let serialized = serde_json::to_vec_pretty(&metadata)
        .map_err(|_| engine_error("The browser metadata could not be serialized."))?;
    std::fs::write(path, serialized)
        .map_err(|_| engine_error("The browser metadata could not be updated."))
}

fn escape_metadata_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn remote_image_output_is_empty(
    saw_image: bool,
    successful_images: usize,
    text_coverage: Option<f64>,
) -> bool {
    saw_image && successful_images == 0 && text_coverage.unwrap_or_default() <= 0.0
}

fn required_ocr_image_output_is_empty(saw_temporary_image: bool, successful_images: usize) -> bool {
    saw_temporary_image && successful_images == 0
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

fn mark_remote_asset_unavailable(markdown: &str, marker: &str, content: WebFetchContent) -> String {
    if content != WebFetchContent::Image {
        return remove_asset_reference(markdown, marker);
    }
    let had_trailing_newline = markdown.ends_with('\n');
    let mut lines = markdown
        .lines()
        .map(|line| {
            if !line.contains(marker) {
                return line.to_owned();
            }
            if let Some(image_start) = line.find("![") {
                return format!("{}（图片不可用）", &line[..image_start]);
            }
            line.replace(marker, "（图片不可用）")
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
    Option<ProcessLifetimeGuard>,
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
    fn platform_pack_assets_share_the_builtin_https_redirect_allowlists() {
        let suffixes =
            platform_asset_redirect_suffixes("https://www.bilibili.com/video/BV1example")
                .expect("Bilibili imports should constrain remote asset redirects");
        assert!(suffixes.contains(&"edge.mountaintoys.cn"));
        assert!(
            platform_asset_redirect_suffixes("https://bilibili.com.evil.example/video").is_none()
        );
        let xhs = platform_asset_redirect_suffixes("http://xhslink.cn/o/example")
            .expect("Xiaohongshu imports should constrain remote asset redirects");
        assert!(xhs.contains(&"xhscdn.com"));
        assert!(xhs.contains(&"xhscdn.net"));
        assert!(platform_asset_redirect_suffixes("https://xhslink.cn.evil.example/o/x").is_none());
    }

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
            stable_capability_error_code(Some("IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE")),
            "IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE"
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
        assert!(markdown.contains("### [00:00:01.000]\n\nHello &lt;script&gt;"));
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
    fn preserves_bounded_platform_subtitle_provenance_in_markdown_and_metadata() {
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"import.remoteAsset\",\"params\":{\"placeholder\":\"platform-subtitle-0\",\"url\":\"https://sns-subtitle-s2.xhscdn.com/source.srt\",\"kind\":\"subtitle\",\"automatic\":true,\"language\":\"zh-CN\",\"label\":\"source\"}}\n{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n";
        let response = read_response(Cursor::new(input)).unwrap();
        let asset = &response.remote_assets[0];
        assert_eq!(asset.automatic, Some(true));
        assert_eq!(asset.language.as_deref(), Some("zh-CN"));
        assert_eq!(asset.label.as_deref(), Some("source"));

        let mut markdown = "# Video".to_string();
        append_platform_transcript(&mut markdown, "[00:00:01.000] 你好", asset);
        assert!(markdown.contains("## 字幕 / 转写"));
        assert!(markdown.contains("平台自动字幕 · 标签：source · 语言：zh-CN"));

        let root =
            std::env::temp_dir().join(format!("subtitle-provenance-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("metadata.json"),
            br#"{"platform":"xiaohongshu","contentType":"video"}"#,
        )
        .unwrap();
        let result = EngineResult {
            source_snapshot_path: "source.html".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            metadata_path: Some("metadata.json".into()),
            title: "Video".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: Vec::new(),
        };
        assert!(metadata_declares_platform_video(
            &root,
            &result,
            "xiaohongshu"
        ));
        assert!(!metadata_declares_platform_video(
            &root, &result, "bilibili"
        ));
        update_transcript_metadata(&root, &result, asset).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata["transcriptSource"], "platform_auto_subtitle");
        assert_eq!(metadata["transcriptLanguage"], "zh-CN");
        assert_eq!(metadata["transcriptLabel"], "source");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_subtitle_metadata_on_non_subtitle_remote_assets() {
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"import.remoteAsset\",\"params\":{\"placeholder\":\"webasset-0\",\"url\":\"https://cdn.example/image.jpg\",\"kind\":\"image\",\"language\":\"zh-CN\"}}\n{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n";
        assert!(read_response(Cursor::new(input)).is_err());
    }

    #[test]
    fn streams_only_bounded_typed_progress_notifications() {
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"import.progress\",\"params\":{\"current\":48,\"total\":100,\"label\":\"asr.recognizing\"}}\n{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n";
        let mut reported = Vec::new();
        let response =
            read_response_with_progress(Cursor::new(input), |progress| reported.push(progress))
                .unwrap();
        assert_eq!(response.rpc.id, "r");
        assert_eq!(
            reported,
            vec![CapabilityProgress {
                current: 48,
                total: 100,
                label: "asr.recognizing".into(),
            }]
        );

        let invalid = b"{\"jsonrpc\":\"2.0\",\"method\":\"import.progress\",\"params\":{\"current\":101,\"total\":100,\"label\":\"asr.recognizing\"}}\n{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n";
        assert!(read_response_with_progress(Cursor::new(invalid), |_| {}).is_err());

        let response_shaped = b"{\"jsonrpc\":\"2.0\",\"id\":\"not-a-notification\",\"method\":\"import.progress\",\"params\":{\"current\":48,\"total\":100,\"label\":\"asr.recognizing\"}}\n";
        assert!(read_response_with_progress(Cursor::new(response_shaped), |_| {}).is_err());
        let unknown_param = b"{\"jsonrpc\":\"2.0\",\"method\":\"import.progress\",\"params\":{\"current\":48,\"total\":100,\"label\":\"asr.recognizing\",\"detail\":\"raw\"}}\n";
        assert!(read_response_with_progress(Cursor::new(unknown_param), |_| {}).is_err());
    }

    #[test]
    fn drains_a_terminal_response_queued_just_after_process_exit() {
        let (sender, receiver) = mpsc::channel();
        let response = read_response(Cursor::new(
            b"{\"jsonrpc\":\"2.0\",\"id\":\"r\",\"result\":null,\"error\":{\"code\":-1,\"message\":\"x\",\"data\":null}}\n",
        ));
        let writer = std::thread::spawn(move || {
            sender
                .send(PackOutputEvent::Progress(CapabilityProgress {
                    current: 48,
                    total: 100,
                    label: "asr.recognizing".into(),
                }))
                .unwrap();
            std::thread::sleep(Duration::from_millis(20));
            sender.send(PackOutputEvent::Response(response)).unwrap();
        });
        let exited_at = Instant::now();
        assert!(matches!(
            receive_output_after_exit(&receiver, exited_at).unwrap(),
            PackOutputEvent::Progress(_)
        ));
        assert!(matches!(
            receive_output_after_exit(&receiver, exited_at).unwrap(),
            PackOutputEvent::Response(Ok(_))
        ));
        writer.join().unwrap();
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
    fn unavailable_image_keeps_an_explicit_partial_preview_notice() {
        let markdown =
            "## 图片\n\n1. ![第 1 张](asset://webasset-0)\n2. ![第 2 张](asset://webasset-1)\n";
        let marked =
            mark_remote_asset_unavailable(markdown, "asset://webasset-0", WebFetchContent::Image);
        assert!(marked.contains("1. （图片不可用）"));
        assert!(marked.contains("2. ![第 2 张](asset://webasset-1)"));
        assert!(!marked.contains("asset://webasset-0"));
    }

    #[test]
    fn an_image_only_remote_post_requires_at_least_one_localized_image() {
        assert!(remote_image_output_is_empty(true, 0, Some(0.0)));
        assert!(!remote_image_output_is_empty(true, 1, Some(0.0)));
        assert!(!remote_image_output_is_empty(true, 0, Some(1.0)));
    }

    #[test]
    fn asr_authorization_without_a_local_asr_continuation_still_fails_closed() {
        assert!(transcript_failure_required(true, false));
        assert!(!transcript_failure_required(true, true));
        assert!(!transcript_failure_required(false, false));
    }

    #[test]
    fn bilibili_metadata_only_candidates_still_require_an_explicit_transcript_outcome() {
        assert!(transcript_is_missing(false, true, false));
        assert!(!transcript_is_missing(false, true, true));
        assert!(!transcript_is_missing(false, false, false));
    }

    #[test]
    fn bilibili_metadata_only_candidate_is_rejected() {
        let project_root =
            std::env::temp_dir().join(format!("bilibili-metadata-{}", uuid::Uuid::new_v4()));
        let staging_root = "staging";
        let staging = project_root.join(staging_root);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("candidate.md"),
            "# Fixture\n\n## 原始描述\n\nDescription\n",
        )
        .unwrap();
        std::fs::write(
            staging.join("metadata.json"),
            br#"{"platform":"bilibili","contentType":"video"}"#,
        )
        .unwrap();
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: "request-1".into(),
            project_id: "project-1".into(),
            session_id: "session-1".into(),
            item_id: "item-1".into(),
            task_id: "task-1".into(),
            operation: crate::services::import_v2::engine::EngineOperation::Extract,
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Bilibili fixture".into(),
                locator: "https://www.bilibili.com/video/BV1fixture".into(),
                normalized_locator: None,
                source_identity: None,
                media_save_mode: MediaSaveMode::ExtractOnly,
            },
            project_root: project_root.to_string_lossy().into_owned(),
            staging_root: staging_root.into(),
            chained_input: None,
            local_asr_authorized: false,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: MediaSaveMode::ExtractOnly,
        };
        let mut result = EngineResult {
            source_snapshot_path: "source.json".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            metadata_path: Some("metadata.json".into()),
            title: "Fixture".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: Vec::new(),
        };

        let error = localize_remote_assets(
            &request,
            &mut result,
            Vec::new(),
            &CancellationToken::new(),
            Arc::new(DomainLimiter::default()),
            None,
            &|_| Ok(()),
        )
        .unwrap_err();

        assert_eq!(error.code, "IMPORT_WEB_SUBTITLE_UNAVAILABLE");
        let markdown = std::fs::read_to_string(staging.join("candidate.md")).unwrap();
        assert!(!markdown.contains("## 字幕 / 转写"));
        assert!(result.warnings.is_empty());
        std::fs::remove_dir_all(project_root).ok();
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
        assert!(required_ocr_image_output_is_empty(true, 0));
        assert!(!required_ocr_image_output_is_empty(true, 1));
        assert!(!required_ocr_image_output_is_empty(false, 0));
    }

    #[test]
    fn process_guard_joins_reader_after_termination() {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new(r"C:\Windows\System32\cmd.exe");
            command.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
            command
        };
        #[cfg(unix)]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 30"]);
            command
        };
        configure_isolated_process(&mut command);
        let mut child = command.spawn().unwrap();
        let lifetime = ProcessLifetimeGuard::attach_capability(&mut child).unwrap();
        let joined = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = joined.clone();
        let reader =
            std::thread::spawn(move || signal.store(true, std::sync::atomic::Ordering::SeqCst));
        drop(ProcessGuard(child, Some(reader), None, Some(lifetime)));
        for _ in 0..100 {
            if joined.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
    }
}

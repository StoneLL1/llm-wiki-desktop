use crate::{
    errors::BackendError,
    services::import_v2::url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy},
};
use futures_util::StreamExt;
use reqwest::{header, redirect::Policy, Client, StatusCode};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    path::Path,
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebFetchContent {
    Page,
    Image,
    Media,
    Subtitle,
    TemporaryMedia,
}
#[derive(Debug, Clone)]
pub struct WebFetchPolicy {
    pub max_response_bytes: u64,
    pub max_redirects: u8,
    pub max_attempts_per_route: u8,
    pub connect_timeout_ms: u64,
    pub total_timeout_ms: u64,
    pub content: WebFetchContent,
    /// Optional page origin used by platform APIs/CDN requests. It is never
    /// persisted and is only sent as the HTTP Referer header.
    pub referer: Option<String>,
}
impl Default for WebFetchPolicy {
    fn default() -> Self {
        Self {
            max_response_bytes: 16 * 1024 * 1024,
            max_redirects: 8,
            max_attempts_per_route: 2,
            connect_timeout_ms: 10_000,
            total_timeout_ms: 60_000,
            content: WebFetchContent::Page,
            referer: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RedirectLedgerEntry {
    pub from: String,
    pub to: String,
    pub status: u16,
}
#[derive(Debug, Clone)]
pub struct WebFetchArtifact {
    pub bytes: Vec<u8>,
    pub byte_len: u64,
    pub final_public_url: String,
    pub final_session_target: SessionWebTarget,
    pub content_type: String,
    pub sanitized_headers: BTreeMap<String, String>,
    pub redirects: Vec<RedirectLedgerEntry>,
    pub elapsed_ms: u64,
}

#[derive(Default)]
pub struct WebFetchService;
impl WebFetchService {
    pub async fn fetch<F, C>(
        &self,
        target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        progress: F,
        cancelled: C,
    ) -> Result<WebFetchArtifact, BackendError>
    where
        F: FnMut(u64),
        C: Fn() -> bool,
    {
        self.fetch_inner(
            target, policy, limits, grant, item_id, None, progress, cancelled,
        )
        .await
    }

    pub async fn fetch_to_file<F, C>(
        &self,
        target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        destination: &Path,
        progress: F,
        cancelled: C,
    ) -> Result<WebFetchArtifact, BackendError>
    where
        F: FnMut(u64),
        C: Fn() -> bool,
    {
        self.fetch_inner(
            target,
            policy,
            limits,
            grant,
            item_id,
            Some(destination),
            progress,
            cancelled,
        )
        .await
    }

    async fn fetch_inner<F, C>(
        &self,
        mut target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        destination: Option<&Path>,
        mut progress: F,
        cancelled: C,
    ) -> Result<WebFetchArtifact, BackendError>
    where
        F: FnMut(u64),
        C: Fn() -> bool,
    {
        let started = Instant::now();
        let mut redirects = Vec::new();
        for _ in 0..=limits.max_redirects {
            if cancelled() {
                return Err(err(
                    "IMPORT_V2_CANCELLED",
                    "Web fetch was cancelled.",
                    false,
                ));
            }
            let host = target.public.host.clone();
            let port = target
                .request_url
                .port_or_known_default()
                .ok_or_else(|| err("IMPORT_V2_URL_REJECTED", "URL port is invalid.", false))?;
            let resolved = tokio::net::lookup_host((host.as_str(), port))
                .await
                .map_err(|_| err("IMPORT_V2_DNS_FAILED", "DNS resolution failed.", true))?
                .map(|a| a.ip())
                .collect::<Vec<IpAddr>>();
            let connected = *resolved
                .first()
                .ok_or_else(|| err("IMPORT_V2_DNS_FAILED", "DNS returned no addresses.", true))?;
            policy.validate_resolved_target(&target, &resolved, connected, grant, item_id)?;
            let client = Client::builder()
                .redirect(Policy::none())
                .connect_timeout(Duration::from_millis(limits.connect_timeout_ms))
                .timeout(Duration::from_millis(limits.total_timeout_ms))
                .resolve(&host, SocketAddr::new(connected, port))
                .build()
                .map_err(|_| {
                    err(
                        "IMPORT_V2_FETCH_FAILED",
                        "HTTP client could not be created.",
                        true,
                    )
                })?;
            let mut request = client
                .get(target.request_url.clone())
                .header(
                    header::USER_AGENT,
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
                )
                .header(header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
                .header("Sec-Fetch-Dest", "document")
                .header("Sec-Fetch-Mode", "navigate")
                .header("Sec-Fetch-Site", "none")
                .header("Sec-Fetch-User", "?1")
                .header("Upgrade-Insecure-Requests", "1")
                .header(
                    header::ACCEPT,
                    "text/html,application/xhtml+xml,application/json;q=0.8,text/plain;q=0.5",
                );
            if let Some(referer) = limits.referer.as_deref() {
                request = request.header(header::REFERER, referer);
            }
            let response = request.send().await.map_err(|_| {
                err(
                    "IMPORT_V2_TLS_OR_FETCH_FAILED",
                    "TLS or HTTP connection failed.",
                    true,
                )
            })?;
            if response.status().is_redirection() {
                let status = response.status().as_u16();
                let loc = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        err(
                            "IMPORT_V2_REDIRECT_REJECTED",
                            "Redirect omitted a valid Location.",
                            false,
                        )
                    })?;
                let next = policy.validate_redirect(&target, loc)?;
                redirects.push(RedirectLedgerEntry {
                    from: target.public.public_url.clone(),
                    to: next.public.public_url.clone(),
                    status,
                });
                target = next;
                continue;
            }
            if matches!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
            ) {
                return Err(err(
                    "IMPORT_V2_CONNECTOR_RATE_LIMITED",
                    "Remote service is temporarily unavailable.",
                    true,
                ));
            }
            if !response.status().is_success() {
                return Err(err(
                    "IMPORT_V2_RESPONSE_FAILED",
                    "Remote service returned an unsuccessful status.",
                    response.status().is_server_error(),
                ));
            }
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            let accepted = match limits.content {
                WebFetchContent::Page => {
                    content_type.contains("text/")
                        || content_type.contains("html")
                        || content_type.contains("json")
                        || (content_type.starts_with("image/") && !content_type.contains("svg"))
                }
                WebFetchContent::Image => {
                    content_type.starts_with("image/") && !content_type.contains("svg")
                }
                WebFetchContent::Subtitle => {
                    matches!(
                        content_type.split(';').next().unwrap_or("").trim(),
                        "text/vtt"
                            | "application/x-subrip"
                            | "text/plain"
                            | "text/ass"
                            | "text/x-ass"
                            | "application/x-ass"
                            | "text/ssa"
                            | "text/x-ssa"
                            | "application/x-ssa"
                            | "application/json"
                    )
                }
                WebFetchContent::Media | WebFetchContent::TemporaryMedia => {
                    content_type.starts_with("audio/")
                        || content_type.starts_with("video/")
                        || content_type == "application/octet-stream"
                }
            };
            if !accepted {
                return Err(err(
                    "IMPORT_V2_CONTENT_REJECTED",
                    "Executable or unsupported response type was blocked.",
                    false,
                ));
            }
            if response
                .content_length()
                .is_some_and(|length| length > limits.max_response_bytes)
            {
                return Err(err(
                    "IMPORT_V2_RESPONSE_TOO_LARGE",
                    "Response exceeded the configured byte limit.",
                    false,
                ));
            }
            let mut headers = BTreeMap::new();
            for name in [
                header::CONTENT_TYPE,
                header::CONTENT_LANGUAGE,
                header::LAST_MODIFIED,
            ] {
                if let Some(v) = response.headers().get(&name).and_then(|v| v.to_str().ok()) {
                    headers.insert(name.as_str().into(), v.chars().take(512).collect());
                }
            }
            let mut bytes = Vec::new();
            let mut destination_file = match destination {
                Some(path) => Some(tokio::fs::File::create(path).await.map_err(|_| {
                    err(
                        "IMPORT_V2_FETCH_FAILED",
                        "The response destination could not be created.",
                        true,
                    )
                })?),
                None => None,
            };
            let mut downloaded = 0u64;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                if cancelled() {
                    return Err(err(
                        "IMPORT_V2_CANCELLED",
                        "Web fetch was cancelled.",
                        false,
                    ));
                }
                let chunk = chunk
                    .map_err(|_| err("IMPORT_V2_FETCH_FAILED", "Response stream failed.", true))?;
                if downloaded + chunk.len() as u64 > limits.max_response_bytes {
                    return Err(err(
                        "IMPORT_V2_RESPONSE_TOO_LARGE",
                        "Response exceeded the configured byte limit.",
                        false,
                    ));
                }
                downloaded += chunk.len() as u64;
                if let Some(file) = destination_file.as_mut() {
                    file.write_all(&chunk).await.map_err(|_| {
                        err(
                            "IMPORT_V2_FETCH_FAILED",
                            "The response destination could not be written.",
                            true,
                        )
                    })?;
                } else {
                    bytes.extend_from_slice(&chunk);
                }
                progress(downloaded);
            }
            if let Some(file) = destination_file.as_mut() {
                file.flush().await.map_err(|_| {
                    err(
                        "IMPORT_V2_FETCH_FAILED",
                        "The response destination could not be finalized.",
                        true,
                    )
                })?;
            }
            return Ok(WebFetchArtifact {
                bytes,
                byte_len: downloaded,
                final_public_url: target.public.public_url.clone(),
                final_session_target: target,
                content_type,
                sanitized_headers: headers,
                redirects,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
        }
        Err(err(
            "IMPORT_V2_REDIRECT_REJECTED",
            "Redirect limit exceeded.",
            false,
        ))
    }
}
fn err(code: &'static str, message: &str, retryable: bool) -> BackendError {
    BackendError::new(code, message, retryable, false)
}

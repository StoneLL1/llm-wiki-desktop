use crate::{
    errors::BackendError,
    services::import_v2::url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy},
};
use futures_util::StreamExt;
use reqwest::{header, redirect::Policy, Client, StatusCode};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebFetchContent {
    Page,
    Image,
    Subtitle,
}
#[derive(Debug, Clone)]
pub struct WebFetchPolicy {
    pub max_response_bytes: u64,
    pub max_redirects: u8,
    pub max_attempts_per_route: u8,
    pub connect_timeout_ms: u64,
    pub total_timeout_ms: u64,
    pub content: WebFetchContent,
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
    pub final_public_url: String,
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
        mut target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
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
            let response = client
                .get(target.request_url.clone())
                .header(
                    header::ACCEPT,
                    "text/html,application/xhtml+xml,application/json;q=0.8,text/plain;q=0.5",
                )
                .send()
                .await
                .map_err(|_| {
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
                }
                WebFetchContent::Image => {
                    content_type.starts_with("image/") && !content_type.contains("svg")
                }
                WebFetchContent::Subtitle => {
                    content_type.contains("text/")
                        || content_type.contains("json")
                        || content_type.contains("vtt")
                        || content_type.contains("subrip")
                }
            };
            if !accepted {
                return Err(err(
                    "IMPORT_V2_CONTENT_REJECTED",
                    "Executable or unsupported response type was blocked.",
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
                if bytes.len() as u64 + chunk.len() as u64 > limits.max_response_bytes {
                    return Err(err(
                        "IMPORT_V2_RESPONSE_TOO_LARGE",
                        "Response exceeded the configured byte limit.",
                        false,
                    ));
                }
                bytes.extend_from_slice(&chunk);
                progress(bytes.len() as u64);
            }
            return Ok(WebFetchArtifact {
                bytes,
                final_public_url: target.public.public_url,
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

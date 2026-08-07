use crate::{
    errors::BackendError,
    services::import_v2::url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy},
};
use futures_util::{Stream, StreamExt};
use reqwest::{header, redirect::Policy, Client, StatusCode};
use std::{
    collections::BTreeMap,
    future::Future,
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
    /// Optional restrictions applied to the initial target and every redirect
    /// before DNS resolution or an HTTP request is attempted.
    pub require_https: bool,
    pub allowed_host_suffixes: Vec<String>,
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
            require_https: false,
            allowed_host_suffixes: Vec::new(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebFetchProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
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
        F: FnMut(WebFetchProgress),
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
        F: FnMut(WebFetchProgress),
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
        F: FnMut(WebFetchProgress),
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
            validate_fetch_target(&target, limits)?;
            let host = target.public.host.clone();
            let port = target
                .request_url
                .port_or_known_default()
                .ok_or_else(|| err("IMPORT_V2_URL_REJECTED", "URL port is invalid.", false))?;
            let resolved =
                await_or_cancel(tokio::net::lookup_host((host.as_str(), port)), &cancelled)
                    .await?
                    .map_err(|_| err("IMPORT_V2_DNS_FAILED", "DNS resolution failed.", true))?
                    .map(|a| a.ip())
                    .collect::<Vec<IpAddr>>();
            let connected = *resolved
                .first()
                .ok_or_else(|| err("IMPORT_V2_DNS_FAILED", "DNS returned no addresses.", true))?;
            let trusted_fake_ip_host =
                limits.require_https && !limits.allowed_host_suffixes.is_empty();
            policy.validate_resolved_target_for_fetch(
                &target,
                &resolved,
                connected,
                grant,
                item_id,
                trusted_fake_ip_host,
            )?;
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
            let response = await_or_cancel(request.send(), &cancelled)
                .await?
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
            let total_bytes = response.content_length();
            progress(WebFetchProgress {
                downloaded_bytes: 0,
                total_bytes,
            });
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
            while let Some(chunk) = next_stream_item_or_cancel(&mut stream, &cancelled).await? {
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
                progress(WebFetchProgress {
                    downloaded_bytes: downloaded,
                    total_bytes,
                });
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

async fn next_stream_item_or_cancel<S, C>(
    stream: &mut S,
    cancelled: &C,
) -> Result<Option<S::Item>, BackendError>
where
    S: Stream + Unpin,
    C: Fn() -> bool,
{
    await_or_cancel(stream.next(), cancelled).await
}

async fn await_or_cancel<F, C>(future: F, cancelled: &C) -> Result<F::Output, BackendError>
where
    F: Future,
    C: Fn() -> bool,
{
    tokio::pin!(future);
    loop {
        if cancelled() {
            return Err(err(
                "IMPORT_V2_CANCELLED",
                "Web fetch was cancelled.",
                false,
            ));
        }
        match tokio::time::timeout(Duration::from_millis(200), &mut future).await {
            Ok(output) => return Ok(output),
            Err(_) => continue,
        }
    }
}

fn validate_fetch_target(
    target: &SessionWebTarget,
    limits: &WebFetchPolicy,
) -> Result<(), BackendError> {
    if limits.require_https && target.request_url.scheme() != "https" {
        return Err(err(
            "IMPORT_V2_REDIRECT_REJECTED",
            "The fetch target must use HTTPS.",
            false,
        ));
    }
    if !limits.allowed_host_suffixes.is_empty() {
        let host = target.public.host.to_ascii_lowercase();
        let allowed = limits.allowed_host_suffixes.iter().any(|suffix| {
            let suffix = suffix.to_ascii_lowercase();
            host == suffix || host.ends_with(&format!(".{suffix}"))
        });
        if !allowed {
            return Err(err(
                "IMPORT_V2_REDIRECT_REJECTED",
                "The fetch target left the verified host allowlist.",
                false,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn stalled_response_stream_observes_cancellation_promptly() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_signal = cancelled.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation_signal.store(true, Ordering::SeqCst);
        });
        let mut stream = futures_util::stream::pending::<()>();
        let started = Instant::now();

        let error = next_stream_item_or_cancel(&mut stream, &|| cancelled.load(Ordering::SeqCst))
            .await
            .expect_err("the stalled stream should stop after cancellation");

        assert_eq!(error.code, "IMPORT_V2_CANCELLED");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn stalled_response_headers_observe_cancellation_promptly() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_signal = cancelled.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation_signal.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();

        let error = await_or_cancel(std::future::pending::<()>(), &|| {
            cancelled.load(Ordering::SeqCst)
        })
        .await
        .expect_err("a request stalled before headers should stop after cancellation");

        assert_eq!(error.code, "IMPORT_V2_CANCELLED");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn restricted_fetch_policy_rejects_downgrade_and_untrusted_redirect_targets() {
        let policy = WebFetchPolicy {
            require_https: true,
            allowed_host_suffixes: vec!["edge.mountaintoys.cn".into()],
            ..WebFetchPolicy::default()
        };
        let trusted = UrlPolicy
            .normalize_for_session("https://809al93l.edge.mountaintoys.cn:4483/upgcxcode/video.mp4")
            .unwrap();
        let downgraded = UrlPolicy
            .normalize_for_session("http://809al93l.edge.mountaintoys.cn:4483/upgcxcode/video.mp4")
            .unwrap();
        let untrusted = UrlPolicy
            .normalize_for_session("https://edge.mountaintoys.cn.evil.example/video.mp4")
            .unwrap();

        assert!(validate_fetch_target(&trusted, &policy).is_ok());
        assert_eq!(
            validate_fetch_target(&downgraded, &policy)
                .unwrap_err()
                .code,
            "IMPORT_V2_REDIRECT_REJECTED"
        );
        assert_eq!(
            validate_fetch_target(&untrusted, &policy).unwrap_err().code,
            "IMPORT_V2_REDIRECT_REJECTED"
        );
    }
}
fn err(code: &'static str, message: &str, retryable: bool) -> BackendError {
    BackendError::new(code, message, retryable, false)
}

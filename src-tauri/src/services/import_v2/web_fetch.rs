use crate::{
    errors::BackendError,
    services::import_v2::url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy},
};
use futures_util::{Stream, StreamExt};
use reqwest::{header, redirect::Policy, Client, StatusCode};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    future::Future,
    net::{IpAddr, SocketAddr},
    path::Path,
    time::{Duration, Instant},
};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFetchResume {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub partial_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFetchCheckpoint {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub partial_sha256: String,
    pub range_supported: bool,
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
            target,
            policy,
            limits,
            grant,
            item_id,
            None,
            None,
            None,
            progress,
            |_| Ok(()),
            cancelled,
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
            None,
            None,
            progress,
            |_| Ok(()),
            cancelled,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_to_file_resumable<F, K, C>(
        &self,
        target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        destination: &Path,
        resume: Option<&WebFetchResume>,
        mut progress: F,
        checkpoint: K,
        cancelled: C,
    ) -> Result<WebFetchArtifact, BackendError>
    where
        F: FnMut(WebFetchProgress),
        K: FnMut(WebFetchCheckpoint) -> Result<(), BackendError>,
        C: Fn() -> bool,
    {
        if let Some(resume) = resume.filter(|resume| {
            resume.total_bytes == Some(resume.downloaded_bytes) && resume.downloaded_bytes > 0
        }) {
            let (length, sha256, _) = hash_existing_partial(destination)?;
            if length != resume.downloaded_bytes
                || !sha256.eq_ignore_ascii_case(&resume.partial_sha256)
            {
                return Err(err(
                    "IMPORT_WEB_PARTIAL_INVALID",
                    "The completed remote-media partial failed identity verification.",
                    true,
                ));
            }
            progress(WebFetchProgress {
                downloaded_bytes: length,
                total_bytes: Some(length),
            });
            return Ok(WebFetchArtifact {
                bytes: Vec::new(),
                byte_len: length,
                final_public_url: target.public.public_url.clone(),
                final_session_target: target,
                content_type: "application/octet-stream".into(),
                sanitized_headers: BTreeMap::new(),
                redirects: Vec::new(),
                elapsed_ms: 0,
            });
        }
        self.fetch_inner(
            target,
            policy,
            limits,
            grant,
            item_id,
            Some(destination),
            resume,
            None,
            progress,
            checkpoint,
            cancelled,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn fetch_to_open_file_resumable<F, K, C>(
        &self,
        target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        destination: std::fs::File,
        resume: Option<&WebFetchResume>,
        mut progress: F,
        checkpoint: K,
        cancelled: C,
    ) -> Result<WebFetchArtifact, BackendError>
    where
        F: FnMut(WebFetchProgress),
        K: FnMut(WebFetchCheckpoint) -> Result<(), BackendError>,
        C: Fn() -> bool,
    {
        if let Some(resume) = resume.filter(|resume| {
            resume.total_bytes == Some(resume.downloaded_bytes) && resume.downloaded_bytes > 0
        }) {
            let (length, sha256, _) =
                hash_existing_partial_file(destination.try_clone().map_err(|_| {
                    err(
                        "IMPORT_WEB_PARTIAL_INVALID",
                        "The completed remote-media partial could not be inspected.",
                        true,
                    )
                })?)?;
            if length != resume.downloaded_bytes
                || !sha256.eq_ignore_ascii_case(&resume.partial_sha256)
            {
                return Err(err(
                    "IMPORT_WEB_PARTIAL_INVALID",
                    "The completed remote-media partial failed identity verification.",
                    true,
                ));
            }
            progress(WebFetchProgress {
                downloaded_bytes: length,
                total_bytes: Some(length),
            });
            return Ok(WebFetchArtifact {
                bytes: Vec::new(),
                byte_len: length,
                final_public_url: target.public.public_url.clone(),
                final_session_target: target,
                content_type: "application/octet-stream".into(),
                sanitized_headers: BTreeMap::new(),
                redirects: Vec::new(),
                elapsed_ms: 0,
            });
        }
        self.fetch_inner(
            target,
            policy,
            limits,
            grant,
            item_id,
            None,
            resume,
            Some(destination),
            progress,
            checkpoint,
            cancelled,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_inner<F, K, C>(
        &self,
        mut target: SessionWebTarget,
        policy: &UrlPolicy,
        limits: &WebFetchPolicy,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        destination: Option<&Path>,
        resume: Option<&WebFetchResume>,
        mut opened_destination: Option<std::fs::File>,
        mut progress: F,
        mut checkpoint: K,
        cancelled: C,
    ) -> Result<WebFetchArtifact, BackendError>
    where
        F: FnMut(WebFetchProgress),
        K: FnMut(WebFetchCheckpoint) -> Result<(), BackendError>,
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
            if let Some(resume) = resume.filter(|resume| resume.downloaded_bytes > 0) {
                request =
                    request.header(header::RANGE, format!("bytes={}-", resume.downloaded_bytes));
                if let Some(if_range) = resume.etag.as_deref().or(resume.last_modified.as_deref()) {
                    request = request.header(header::IF_RANGE, if_range);
                }
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
            let response_status = response.status();
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
            let response_etag = response
                .headers()
                .get(header::ETAG)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.chars().take(512).collect::<String>());
            let response_last_modified = response
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.chars().take(512).collect::<String>());
            let content_range = response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_range);
            let requested_offset = resume.map_or(0, |resume| resume.downloaded_bytes);
            if response_status == StatusCode::PARTIAL_CONTENT
                && content_range
                    .is_none_or(|range| range.end < range.start || range.start != requested_offset)
            {
                return Err(err(
                    "IMPORT_WEB_PARTIAL_INVALID",
                    "The remote service returned an invalid or unexpected byte range.",
                    true,
                ));
            }
            let can_resume = requested_offset > 0
                && response_status == StatusCode::PARTIAL_CONTENT
                && content_range.is_some_and(|range| range.start == requested_offset)
                && resume.is_some_and(|resume| {
                    (resume.etag.is_some() || resume.last_modified.is_some())
                        && resume
                            .etag
                            .as_ref()
                            .is_none_or(|expected| response_etag.as_ref() == Some(expected))
                        && resume.last_modified.as_ref().is_none_or(|expected| {
                            response_last_modified.as_ref() == Some(expected)
                        })
                        && resume.total_bytes.is_none_or(|expected| {
                            content_range.and_then(|range| range.total) == Some(expected)
                        })
                });
            if requested_offset > 0 && response_status == StatusCode::PARTIAL_CONTENT && !can_resume
            {
                return Err(err(
                    "IMPORT_WEB_PARTIAL_IDENTITY_CHANGED",
                    "The remote media changed while a saved partial was being resumed.",
                    true,
                ));
            }
            let resumed_from = if can_resume { requested_offset } else { 0 };
            let total_bytes = content_range.and_then(|range| range.total).or_else(|| {
                response
                    .content_length()
                    .map(|length| length.saturating_add(resumed_from))
            });
            let expected_response_bytes =
                content_range.map(|range| range.end.saturating_sub(range.start).saturating_add(1));
            if expected_response_bytes.is_some_and(|expected| {
                response
                    .content_length()
                    .is_some_and(|length| length != expected)
            }) {
                return Err(err(
                    "IMPORT_WEB_PARTIAL_INVALID",
                    "The remote service returned a byte range with an inconsistent length.",
                    true,
                ));
            }
            if total_bytes.is_some_and(|length| length > limits.max_response_bytes) {
                return Err(err(
                    "IMPORT_V2_RESPONSE_TOO_LARGE",
                    "Response exceeded the configured byte limit.",
                    false,
                ));
            }
            progress(WebFetchProgress {
                downloaded_bytes: resumed_from,
                total_bytes,
            });
            let mut headers: BTreeMap<String, String> = BTreeMap::new();
            for name in [
                header::CONTENT_TYPE,
                header::CONTENT_LANGUAGE,
                header::LAST_MODIFIED,
                header::ETAG,
                header::ACCEPT_RANGES,
            ] {
                if let Some(v) = response.headers().get(&name).and_then(|v| v.to_str().ok()) {
                    headers.insert(name.as_str().into(), v.chars().take(512).collect());
                }
            }
            let mut bytes = Vec::new();
            let mut hasher = Sha256::new();
            if resumed_from > 0 {
                let (length, sha256, seeded) = match opened_destination.as_ref() {
                    Some(file) => hash_existing_partial_file(file.try_clone().map_err(|_| {
                        err(
                            "IMPORT_WEB_PARTIAL_INVALID",
                            "The saved remote-media partial could not be inspected.",
                            true,
                        )
                    })?)?,
                    None => {
                        let path = destination.ok_or_else(|| {
                            err(
                                "IMPORT_WEB_PARTIAL_INVALID",
                                "A resumable fetch requires a durable partial file.",
                                true,
                            )
                        })?;
                        hash_existing_partial(path)?
                    }
                };
                let expected = resume.expect("positive offset requires resume facts");
                if length != expected.downloaded_bytes
                    || !sha256.eq_ignore_ascii_case(&expected.partial_sha256)
                {
                    return Err(err(
                        "IMPORT_WEB_PARTIAL_INVALID",
                        "The saved remote-media partial failed identity verification.",
                        true,
                    ));
                }
                hasher = seeded;
            }
            let mut destination_file = match opened_destination.take() {
                Some(file) => {
                    if resumed_from == 0 {
                        file.set_len(0).map_err(|_| {
                            err(
                                "IMPORT_V2_FETCH_FAILED",
                                "The response destination could not be truncated.",
                                true,
                            )
                        })?;
                    }
                    let mut file = tokio::fs::File::from_std(file);
                    file.seek(std::io::SeekFrom::Start(resumed_from))
                        .await
                        .map_err(|_| {
                            err(
                                "IMPORT_V2_FETCH_FAILED",
                                "The response destination could not be positioned.",
                                true,
                            )
                        })?;
                    Some(file)
                }
                None => match destination {
                    Some(path) => Some(
                        tokio::fs::OpenOptions::new()
                            .create(true)
                            .write(true)
                            .append(resumed_from > 0)
                            .truncate(resumed_from == 0)
                            .open(path)
                            .await
                            .map_err(|_| {
                                err(
                                    "IMPORT_V2_FETCH_FAILED",
                                    "The response destination could not be created.",
                                    true,
                                )
                            })?,
                    ),
                    None => None,
                },
            };
            let mut downloaded = resumed_from;
            let mut response_bytes = 0_u64;
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
                response_bytes = response_bytes.saturating_add(chunk.len() as u64);
                if expected_response_bytes.is_some_and(|expected| response_bytes > expected) {
                    return Err(err(
                        "IMPORT_WEB_PARTIAL_INVALID",
                        "The remote service returned more bytes than its declared range.",
                        true,
                    ));
                }
                hasher.update(&chunk);
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
                checkpoint(WebFetchCheckpoint {
                    downloaded_bytes: downloaded,
                    total_bytes,
                    etag: response_etag.clone(),
                    last_modified: response_last_modified.clone(),
                    partial_sha256: format!("{:x}", hasher.clone().finalize()),
                    range_supported: response_status == StatusCode::PARTIAL_CONTENT
                        || headers
                            .get(header::ACCEPT_RANGES.as_str())
                            .is_some_and(|value| value.eq_ignore_ascii_case("bytes")),
                })?;
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
            if expected_response_bytes.is_some_and(|expected| response_bytes != expected)
                || total_bytes.is_some_and(|total| downloaded != total)
            {
                return Err(err(
                    "IMPORT_WEB_PARTIAL_INCOMPLETE",
                    "The remote media response ended before the declared byte range was complete.",
                    true,
                ));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentRange {
    start: u64,
    end: u64,
    total: Option<u64>,
}

fn parse_content_range(value: &str) -> Option<ContentRange> {
    let value = value.trim().strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some(ContentRange {
        start: start.parse().ok()?,
        end: end.parse().ok()?,
        total: (total != "*").then(|| total.parse().ok()).flatten(),
    })
}

fn hash_existing_partial(path: &Path) -> Result<(u64, String, Sha256), BackendError> {
    let file = std::fs::File::open(path).map_err(|_| {
        err(
            "IMPORT_WEB_PARTIAL_INVALID",
            "The saved remote-media partial could not be read.",
            true,
        )
    })?;
    hash_existing_partial_file(file)
}

fn hash_existing_partial_file(
    mut file: std::fs::File,
) -> Result<(u64, String, Sha256), BackendError> {
    use std::io::{Read, Seek};
    file.seek(std::io::SeekFrom::Start(0)).map_err(|_| {
        err(
            "IMPORT_WEB_PARTIAL_INVALID",
            "The saved remote-media partial could not be positioned.",
            true,
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            err(
                "IMPORT_WEB_PARTIAL_INVALID",
                "The saved remote-media partial could not be verified.",
                true,
            )
        })?;
        if read == 0 {
            break;
        }
        length = length.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    Ok((length, format!("{:x}", hasher.clone().finalize()), hasher))
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

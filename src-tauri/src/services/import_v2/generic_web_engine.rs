use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode};
use crate::services::import_v2::bilibili;
use crate::services::import_v2::connectors::{wechat, xiaohongshu, ConnectorFailure};
use crate::services::import_v2::engine::{
    EngineDescriptor, EngineProgress, EngineProgressReporter, EngineRequest, EngineResult,
    ImportEngine,
};
use crate::services::import_v2::markdown_normalizer::{
    decode_text, html_to_markdown, normalize_markdown,
};
use crate::services::import_v2::media_router::{
    link_or_copy, move_staged_file, TemporaryMediaWorkspace,
};
use crate::services::import_v2::platform_network_policy::{
    trusted_platform_page_host_suffixes, upgrade_trusted_platform_page_to_https,
};
use crate::services::import_v2::platform_provider::{
    extract_platform_document, Platform, PlatformSubtitleKind,
};
use crate::services::import_v2::redaction::redact_sensitive_text;
use crate::services::import_v2::subtitle::{parse_subtitle_segments, render_subtitle_markdown};
use crate::services::import_v2::url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy};
use crate::services::import_v2::web_fetch::{
    WebFetchArtifact, WebFetchContent, WebFetchPolicy, WebFetchProgress, WebFetchService,
};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct GenericWebEngine {
    web_targets: Arc<WebTargetStore>,
    artifact_source: Arc<dyn WebArtifactSource>,
    route: &'static str,
    engine_id: &'static str,
}

pub trait WebArtifactSource: Send + Sync {
    fn fetch(
        &self,
        target: SessionWebTarget,
        policy: WebFetchPolicy,
        private_grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<WebFetchArtifact, BackendError>;

    fn supports_live_platform_api(&self) -> bool {
        true
    }
}

#[derive(Default)]
pub(crate) struct NetworkWebArtifactSource;

impl WebArtifactSource for NetworkWebArtifactSource {
    fn fetch(
        &self,
        target: SessionWebTarget,
        policy: WebFetchPolicy,
        private_grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<WebFetchArtifact, BackendError> {
        let item_id = item_id.to_owned();
        let private_grant = private_grant.cloned();
        let cancellation = cancellation.clone();
        let worker = std::thread::Builder::new()
            .name("import-web-fetch".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| unavailable("The web fetch runtime could not be started."))?;
                runtime.block_on(WebFetchService::default().fetch(
                    target,
                    &UrlPolicy::default(),
                    &policy,
                    private_grant.as_ref(),
                    &item_id,
                    |_| {},
                    || cancellation.is_cancelled(),
                ))
            })
            .map_err(|_| unavailable("The web fetch worker could not be started."))?;
        worker
            .join()
            .map_err(|_| unavailable("The web fetch worker stopped unexpectedly."))?
    }
}

impl GenericWebEngine {
    pub fn new(
        web_targets: Arc<WebTargetStore>,
        engine_id: &'static str,
        route: &'static str,
    ) -> Self {
        Self::new_with_artifact_source(
            web_targets,
            engine_id,
            route,
            Arc::new(NetworkWebArtifactSource),
        )
    }

    pub fn new_with_artifact_source(
        web_targets: Arc<WebTargetStore>,
        engine_id: &'static str,
        route: &'static str,
        artifact_source: Arc<dyn WebArtifactSource>,
    ) -> Self {
        Self {
            web_targets,
            artifact_source,
            route,
            engine_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebMetadata<'a> {
    engine_id: &'a str,
    engine_version: &'a str,
    route: &'a str,
    final_public_url: &'a str,
    content_type: &'a str,
    redirect_count: usize,
    warnings: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title_source: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_kind: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcript_source: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcript_language: Option<&'a str>,
    image_count: usize,
    hashtag_count: usize,
    media_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_size_bytes: Option<u64>,
    restricted_content: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletedMediaDownload {
    schema_version: u32,
    complete: bool,
    content_type: String,
    byte_len: u64,
    sha256: String,
}

impl ImportEngine for GenericWebEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: self.engine_id.into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            route: self.route.into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
            && (self.route == "web.bilibili.video" || !is_bilibili_target(input))
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
        if !self.supports(&request.input) {
            return Err(unavailable(
                "The generic web engine supports URL inputs only.",
            ));
        }
        let target = self.web_targets.resolve(
            &request.input.locator,
            request.input.normalized_locator.as_deref(),
        )?;
        let target = upgrade_trusted_platform_page_to_https(target)?;
        let item_id = request.item_id.clone();
        let private_grant = self
            .web_targets
            .private_for_operation(&item_id, &request.task_id)?;
        let mut fetch_policy = WebFetchPolicy::default();
        let requested_locator = request
            .input
            .normalized_locator
            .as_deref()
            .unwrap_or(&request.input.locator);
        let requested_platform = Platform::from_url(requested_locator);
        let trusted_page_hosts = trusted_platform_page_host_suffixes(requested_locator);
        if !trusted_page_hosts.is_empty() {
            fetch_policy.require_https = true;
            fetch_policy.allowed_host_suffixes = trusted_page_hosts
                .iter()
                .map(|suffix| (*suffix).into())
                .collect();
        }
        let direct_media_url = is_direct_media_locator(&request.input);
        let direct_image_url = is_direct_image_locator(&request.input);
        if direct_media_url {
            fetch_policy.content = WebFetchContent::Media;
            fetch_policy.max_response_bytes = 256 * 1024 * 1024;
        } else if direct_image_url {
            fetch_policy.content = WebFetchContent::Image;
            fetch_policy.max_response_bytes = 8 * 1024 * 1024;
        }
        let page_artifact = self.artifact_source.fetch(
            target,
            fetch_policy,
            private_grant.as_ref(),
            &item_id,
            cancellation,
        );
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let bilibili_api_request = page_artifact.as_ref().ok().map(|artifact| {
            let mut resolved = request.clone();
            resolved.input.normalized_locator = Some(artifact.final_public_url.clone());
            resolved
        });
        let bilibili_api = if self.artifact_source.supports_live_platform_api()
            && self.route == "web.bilibili.video"
            && requested_platform == Some(Platform::Bilibili)
        {
            match bilibili::fetch(
                bilibili_api_request.as_ref().unwrap_or(request),
                cancellation,
            ) {
                Ok(result) => result,
                Err(error) if error.code == "IMPORT_V2_CANCELLED" => return Err(error),
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "IMPORT_WEB_LOGIN_REQUIRED"
                            | "IMPORT_WEB_CHALLENGE_DETECTED"
                            | "IMPORT_WEB_CONTENT_REMOVED"
                    ) =>
                {
                    return Err(error)
                }
                Err(_) => None,
            }
        } else {
            None
        };
        let (artifact, api_is_source) = select_primary_web_artifact(
            page_artifact,
            bilibili_api.as_ref(),
            bilibili_api_request.as_ref(),
            request,
        )?;
        let platform = platform_after_redirect(requested_platform, &artifact.final_public_url);
        if is_media_content_type(&artifact.content_type)
            || (direct_media_url && artifact.content_type.contains("octet-stream"))
        {
            return direct_media_result(request, cancellation, &artifact);
        }
        if (direct_image_url || artifact.content_type.starts_with("image/"))
            && (artifact.content_type.starts_with("image/")
                || artifact.content_type.contains("octet-stream"))
        {
            if artifact.bytes.len() > 8 * 1024 * 1024 {
                return Err(BackendError::new(
                    "IMPORT_V2_RESPONSE_TOO_LARGE",
                    "Image response exceeded the configured byte limit.",
                    false,
                    true,
                ));
            }
            return direct_image_result(request, cancellation, &artifact);
        }
        let body = if api_is_source {
            bilibili_api
                .as_ref()
                .map(|api| api.source_body.clone())
                .ok_or_else(|| unavailable("The Bilibili API source was unavailable."))?
        } else {
            decode_text(&artifact.bytes)?
        };
        if bilibili_api.is_none() && platform == Some(Platform::Xiaohongshu) {
            if let Some(failure) = xiaohongshu::classify_page(&body) {
                return Err(xiaohongshu_error(failure));
            }
        } else if bilibili_api.is_none() && is_platform_auth_challenge(request, &body) {
            return Err(BackendError::new(
                "IMPORT_WEB_LOGIN_REQUIRED",
                "The platform returned a login or verification page. Complete login and retry.",
                false,
                true,
            ));
        }
        if wechat::is_wechat_target(
            request
                .input
                .normalized_locator
                .as_deref()
                .unwrap_or(&request.input.locator),
        ) && wechat::is_challenge_html(&body)
        {
            return Err(BackendError::new(
                "IMPORT_WEB_CHALLENGE_DETECTED",
                "WeChat returned a verification page. Complete verification and retry.",
                false,
                true,
            ));
        }
        let platform_document = if let Some(api) = bilibili_api.as_ref() {
            Some(api.document.clone())
        } else if platform == Some(Platform::Xiaohongshu) {
            Some(
                xiaohongshu::extract_page(&body, &artifact.final_public_url)
                    .map_err(xiaohongshu_error)?,
            )
        } else {
            platform.and_then(|platform| {
                extract_platform_document(platform, &body, &artifact.final_public_url)
            })
        };
        if platform_image_requires_ocr(platform_document.as_ref(), request.local_ocr_authorized) {
            return Err(ocr_unavailable(
                "Xiaohongshu image posts require verified local OCR before preview.",
            ));
        }
        let (mut markdown, mut warnings) = platform_document
            .as_ref()
            .map(render_platform_markdown)
            .map(|markdown| (markdown, Vec::new()))
            .unwrap_or_else(|| {
                if artifact.content_type.contains("html") {
                    html_to_markdown(&body)
                } else {
                    (normalize_markdown(&body), Vec::new())
                }
            });
        if markdown.trim().is_empty() {
            return Err(unavailable(
                "The web response did not contain readable text.",
            ));
        }
        let staging = resolve_inside(Path::new(&request.project_root), &request.staging_root)?;
        std::fs::create_dir_all(&staging)
            .map_err(|_| unavailable("The web item staging directory could not be created."))?;
        let mut asset_paths = Vec::new();
        if let Some(document) = platform_document.as_ref() {
            let relative = format!("source-evidence/{}-provider.json", document.platform);
            let target = staging.join(&relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|_| {
                    unavailable("The platform provider evidence directory could not be created.")
                })?;
            }
            let serialized = serde_json::to_string_pretty(document).map_err(|_| {
                unavailable("The platform provider evidence could not be serialized.")
            })?;
            std::fs::write(&target, redact_sensitive_text(&serialized)).map_err(|_| {
                unavailable("The platform provider evidence snapshot could not be written.")
            })?;
            asset_paths.push(relative);
        }
        if !api_is_source {
            if let Some(api) = bilibili_api.as_ref() {
                let relative = "source-evidence/bilibili-api.json";
                let target = staging.join(relative);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|_| {
                        unavailable("The Bilibili API evidence directory could not be created.")
                    })?;
                }
                std::fs::write(&target, redact_sensitive_text(&api.source_body)).map_err(|_| {
                    unavailable("The Bilibili API evidence snapshot could not be written.")
                })?;
                asset_paths.push(relative.into());
            }
        }
        let mut continuation = None;
        let image_ocr_enabled =
            should_run_platform_image_ocr(platform_document.as_ref(), request.local_ocr_authorized);
        let mut temporary_ocr_inputs = Vec::new();
        let image_urls = platform_document
            .as_ref()
            .map(|document| document.images.clone())
            .unwrap_or_else(|| extract_html_image_urls(&body, &artifact.final_public_url));
        let image_total = image_urls.len() as u64;
        let report_image_progress = image_total > 0
            && platform_document
                .as_ref()
                .is_none_or(|document| document.content_type != "video");
        if report_image_progress {
            report_progress(EngineProgress {
                current: 0,
                total: Some(image_total),
                label: "images.downloading".into(),
            })?;
        }
        let mut successful_images = 0usize;
        for (index, image_url) in image_urls.into_iter().enumerate() {
            if report_image_progress {
                report_progress(EngineProgress {
                    current: index as u64,
                    total: Some(image_total),
                    label: "images.downloading".into(),
                })?;
            }
            if let Some(platform) = platform {
                if !is_trusted_platform_asset_url(platform, &image_url) {
                    markdown = replace_markdown_asset_reference(
                        &markdown,
                        &image_url,
                        "（图片来源未获允许）",
                    );
                    warnings.push("Platform image host was not in the verified allowlist.".into());
                    continue;
                }
            }
            match fetch_image(
                &image_url,
                &item_id,
                cancellation,
                &artifact.final_public_url,
                platform,
                self.artifact_source.as_ref(),
                private_grant.as_ref(),
            ) {
                Ok(image) => {
                    if platform.is_some_and(|platform| {
                        !is_trusted_platform_asset_url(platform, &image.final_public_url)
                    }) {
                        markdown = replace_markdown_asset_reference(
                            &markdown,
                            &image_url,
                            "（图片来源未获允许）",
                        );
                        warnings.push(
                            "Platform image redirect left the verified host allowlist.".into(),
                        );
                        continue;
                    }
                    let extension = image_extension(&image.content_type);
                    if image_ocr_enabled {
                        temporary_ocr_inputs.push(stage_temporary_ocr_input(
                            &staging,
                            index,
                            extension,
                            &image.bytes,
                        )?);
                    }
                    let relative = format!("assets/images/{:03}.{extension}", index + 1);
                    if std::fs::create_dir_all(staging.join("assets/images")).is_err()
                        || std::fs::write(staging.join(&relative), &image.bytes).is_err()
                    {
                        markdown = replace_markdown_asset_reference(
                            &markdown,
                            &image_url,
                            "（原图保留失败）",
                        );
                        warnings.push(
                            "Original image preservation failed after text extraction succeeded."
                                .into(),
                        );
                    } else {
                        successful_images += 1;
                        markdown = markdown.replace(&image_url, &relative);
                        asset_paths.push(relative);
                    }
                }
                Err(error) => {
                    markdown =
                        replace_markdown_asset_reference(&markdown, &image_url, "（原图不可用）");
                    warnings.push(format!(
                        "Original image was not localized: {}",
                        error.message
                    ));
                }
            }
        }
        if report_image_progress {
            report_progress(EngineProgress {
                current: image_total,
                total: Some(image_total),
                label: "images.downloading".into(),
            })?;
        }
        if platform_document.as_ref().is_some_and(|document| {
            !platform_image_output_is_meaningful(document, successful_images)
        }) {
            return Err(BackendError::new(
                "IMPORT_WEB_MEDIA_UNAVAILABLE",
                "The image post is incomplete because one or more required source images could not be localized.",
                true,
                true,
            ));
        }
        if image_ocr_enabled {
            if temporary_ocr_inputs.is_empty() {
                return Err(BackendError::new(
                    "IMPORT_WEB_MEDIA_UNAVAILABLE",
                    "None of the Xiaohongshu note images could be downloaded for required OCR.",
                    true,
                    true,
                ));
            } else {
                continuation = Some(
                    crate::services::import_v2::engine::EngineContinuation::LocalOcr {
                        temporary_input_paths: temporary_ocr_inputs,
                    },
                );
            }
        }
        let mut transcription_ready = false;
        let mut transcript_source = None::<String>;
        let mut transcript_language = None::<String>;
        if let Some(document) = platform_document.as_ref() {
            for (subtitle_index, subtitle) in document.subtitles.iter().enumerate() {
                if !platform
                    .is_some_and(|platform| is_trusted_platform_asset_url(platform, &subtitle.url))
                {
                    warnings
                        .push("Platform subtitle host was not in the verified allowlist.".into());
                    continue;
                }
                match fetch_subtitle(
                    &subtitle.url,
                    &item_id,
                    cancellation,
                    &artifact.final_public_url,
                    platform,
                    self.artifact_source.as_ref(),
                    private_grant.as_ref(),
                ) {
                    Ok(subtitle_artifact) => {
                        if platform.is_some_and(|platform| {
                            !is_trusted_platform_asset_url(
                                platform,
                                &subtitle_artifact.final_public_url,
                            )
                        }) {
                            warnings.push(
                                "Platform subtitle redirect left the verified host allowlist."
                                    .into(),
                            );
                            continue;
                        }
                        let extension = subtitle_extension(
                            &subtitle_artifact.content_type,
                            &subtitle_artifact.final_public_url,
                        );
                        if let (Some(segments), Some(rendered)) = (
                            parse_subtitle_segments(&subtitle_artifact.bytes, extension),
                            render_subtitle_markdown(&subtitle_artifact.bytes, extension),
                        ) {
                            let relative =
                                format!("subtitles/platform-subtitle-{subtitle_index}.{extension}");
                            std::fs::create_dir_all(staging.join("subtitles")).map_err(|_| {
                                unavailable("The platform subtitle directory could not be created.")
                            })?;
                            std::fs::write(staging.join(&relative), &subtitle_artifact.bytes)
                                .map_err(|_| {
                                    unavailable("The platform subtitle could not be staged.")
                                })?;
                            asset_paths.push(relative);
                            let segments_relative = format!(
                                "subtitles/platform-subtitle-{subtitle_index}.segments.json"
                            );
                            let serialized =
                                serde_json::to_vec_pretty(&segments).map_err(|_| {
                                    unavailable(
                                        "The normalized subtitle segments could not be serialized.",
                                    )
                                })?;
                            std::fs::write(staging.join(&segments_relative), serialized).map_err(
                                |_| {
                                    unavailable(
                                        "The normalized subtitle segments could not be staged.",
                                    )
                                },
                            )?;
                            asset_paths.push(segments_relative);
                            if !transcription_ready && subtitle.kind.is_reliable_source() {
                                transcript_source = Some(
                                    match subtitle.kind {
                                        PlatformSubtitleKind::AuthorOriginal => {
                                            "platform_author_original_subtitle"
                                        }
                                        PlatformSubtitleKind::PlatformAutoOriginal => {
                                            "platform_auto_original_subtitle"
                                        }
                                        PlatformSubtitleKind::AuthorOther => {
                                            "platform_author_other_subtitle"
                                        }
                                        PlatformSubtitleKind::MachineTranslation => unreachable!(
                                            "machine translations are evidence, not source transcripts"
                                        ),
                                    }
                                    .into(),
                                );
                                transcript_language = subtitle.language.clone();
                                markdown.push_str("\n\n## 字幕 / 转写\n\n");
                                markdown.push_str(&format!(
                                    "> 来源：{}{}\n\n",
                                    match subtitle.kind {
                                        PlatformSubtitleKind::AuthorOriginal => "作者原语言字幕",
                                        PlatformSubtitleKind::PlatformAutoOriginal => {
                                            "平台原语言自动字幕"
                                        }
                                        PlatformSubtitleKind::AuthorOther => "作者其他语言字幕",
                                        PlatformSubtitleKind::MachineTranslation => unreachable!(
                                            "machine translations are evidence, not source transcripts"
                                        ),
                                    },
                                    subtitle
                                        .label
                                        .as_deref()
                                        .map(|label| format!(" · {label}"))
                                        .unwrap_or_default()
                                ));
                                markdown.push_str(&rendered);
                                transcription_ready = true;
                            } else if !subtitle.kind.is_reliable_source() {
                                warnings.push(
                                    "Machine-translated subtitle was retained as evidence but not used as the source transcript.".into(),
                                );
                            }
                        }
                    }
                    Err(error) => warnings.push(format!(
                        "Platform subtitle was not localized: {}",
                        error.message
                    )),
                }
            }
        }
        let media_url = platform_document
            .as_ref()
            .and_then(|document| document.media_url.clone())
            .or_else(|| {
                extract_html_media_url(&body)
                    .and_then(|value| resolve_web_asset_url(&value, &artifact.final_public_url))
            });
        let mut remote_media_bytes = platform_document
            .as_ref()
            .and_then(|document| document.media_size_bytes);
        let xiaohongshu_video = platform_document.as_ref().is_some_and(|document| {
            document.platform == "xiaohongshu" && document.content_type == "video"
        });
        if xiaohongshu_video && media_url.is_none() {
            if !transcription_ready {
                return Err(BackendError::new(
                    "IMPORT_WEB_MEDIA_UNAVAILABLE",
                    "The Xiaohongshu subtitle candidates were unusable and no media was available for local ASR.",
                    true,
                    true,
                ));
            }
            if request.media_save_mode == MediaSaveMode::PreserveOriginal {
                warnings.push(
                    "Original media was not retained because the platform did not expose a downloadable stream."
                        .into(),
                );
            }
        }
        if platform == Some(Platform::Bilibili)
            && (is_bilibili_video_locator(request)
                || is_bilibili_video_url(&artifact.final_public_url))
            && media_url.is_none()
        {
            if !transcription_ready {
                return Err(BackendError::new(
                    "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
                    "Bilibili did not expose usable subtitles and no media was available for local ASR.",
                    true,
                    true,
                ));
            } else if request.media_save_mode == MediaSaveMode::PreserveOriginal {
                warnings.push(
                    "Original media was not retained because Bilibili did not expose a downloadable stream."
                        .into(),
                );
            }
        }
        if platform.is_some() {
            if let Some(media_url) = media_url {
                let platform_media_allowed = platform
                    .is_none_or(|platform| is_trusted_platform_asset_url(platform, &media_url));
                if !platform_media_allowed {
                    markdown = replace_markdown_asset_reference(
                        &markdown,
                        &media_url,
                        "（媒体来源未获允许）",
                    );
                    warnings.push("Platform media host was not in the verified allowlist.".into());
                    if !transcription_ready {
                        return Err(BackendError::new(
                            "IMPORT_WEB_MEDIA_HOST_UNSUPPORTED",
                            "The platform media host is not in the verified allowlist.",
                            true,
                            true,
                        ));
                    }
                }
                let media = if platform_media_allowed {
                    if !transcription_ready && !request.local_asr_authorized {
                        return Err(BackendError::new(
                            "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
                            "Local ASR is required because the platform did not provide usable subtitles.",
                            true,
                            true,
                        ));
                    }
                    if request.media_save_mode == MediaSaveMode::ExtractOnly && transcription_ready
                    {
                        None
                    } else if let Some(media) = load_completed_media_download(&staging, &media_url)?
                    {
                        warnings.push("IMPORT_MEDIA_REUSED_COMPLETE_DOWNLOAD".into());
                        Some((media, None))
                    } else {
                        let download =
                            TemporaryMediaWorkspace::create_unique(&staging, ".media-fetch")?;
                        let download_path = download.path().join("response.bin");
                        match fetch_media_to_file(
                            &media_url,
                            platform,
                            &item_id,
                            cancellation,
                            request
                                .input
                                .normalized_locator
                                .as_deref()
                                .unwrap_or(&request.input.locator),
                            &download_path,
                            report_progress,
                            private_grant.as_ref(),
                        ) {
                            Ok(media) => Some((media, Some(download))),
                            Err(error) if transcription_ready => {
                                markdown = replace_markdown_asset_reference(
                                    &markdown,
                                    &media_url,
                                    "（原始媒体不可用）",
                                );
                                warnings.push(format!(
                                    "Original media was not localized: {}",
                                    error.message
                                ));
                                None
                            }
                            Err(error) => return Err(error),
                        }
                    }
                } else {
                    None
                };
                let media = match media {
                    Some((media, download))
                        if platform.is_some_and(|platform| {
                            !is_trusted_platform_asset_url(platform, &media.final_public_url)
                        }) =>
                    {
                        drop(download);
                        warnings.push(
                            "Platform media redirect left the verified host allowlist.".into(),
                        );
                        if !transcription_ready {
                            return Err(BackendError::new(
                                "IMPORT_WEB_MEDIA_HOST_UNSUPPORTED",
                                "The redirected platform media host is not in the verified allowlist.",
                                true,
                                true,
                            ));
                        }
                        None
                    }
                    media => media,
                };
                if let Some((media, download)) = media {
                    remote_media_bytes = Some(media.byte_len);
                    let extension = media_extension(&media.content_type, &media.final_public_url);
                    if media.byte_len == 0 {
                        return Err(unavailable("The platform media response was empty."));
                    }
                    let downloaded_path = if let Some(download) = download {
                        let path = store_completed_media_download(
                            &staging,
                            &download.path().join("response.bin"),
                            &media,
                        )?;
                        drop(download);
                        path
                    } else {
                        staging.join("media-download/payload.bin")
                    };
                    let durable_media = if request.media_save_mode
                        == MediaSaveMode::PreserveOriginal
                    {
                        let relative = format!("assets/original-media.{extension}");
                        let durable_path = staging.join(&relative);
                        if std::fs::create_dir_all(staging.join("assets")).is_err()
                            || link_or_copy(&downloaded_path, &durable_path).is_err()
                        {
                            markdown = replace_markdown_asset_reference(
                                &markdown,
                                &media_url,
                                "（原始媒体保留失败）",
                            );
                            warnings.push(
                                "Original media preservation failed after text extraction succeeded."
                                    .into(),
                            );
                            None
                        } else {
                            markdown = markdown.replace(&media_url, &relative);
                            markdown.push_str(&format!(
                                "\n\n## Original media\n\n[Download original media]({relative})\n"
                            ));
                            asset_paths.push(relative);
                            Some(durable_path)
                        }
                    } else {
                        markdown = replace_markdown_asset_reference(
                            &markdown,
                            &media_url,
                            "（原始媒体未保留）",
                        );
                        None
                    };
                    if !transcription_ready && request.local_asr_authorized {
                        let temporary =
                            TemporaryMediaWorkspace::create_unique(&staging, ".asr-input")?;
                        let temporary_path = temporary.path().join(format!("input.{extension}"));
                        if let Some(durable_media) = durable_media.as_ref() {
                            link_or_copy(durable_media, &temporary_path).map_err(|_| {
                                unavailable("The temporary media could not be staged.")
                            })?;
                        } else {
                            link_or_copy(&downloaded_path, &temporary_path).map_err(|_| {
                                unavailable("The temporary media could not be staged.")
                            })?;
                        }
                        let temporary_relative = temporary_path
                            .strip_prefix(&staging)
                            .map_err(|_| unavailable("The temporary media escaped staging."))?
                            .to_string_lossy()
                            .replace('\\', "/");
                        temporary.retain();
                        continuation = Some(
                            crate::services::import_v2::engine::EngineContinuation::LocalAsr {
                                temporary_input_path: temporary_relative,
                                media_kind: "video".into(),
                            },
                        );
                    }
                }
            }
        }
        let descriptor = self.descriptor();
        markdown = redact_sensitive_text(&markdown);
        let public_url = redact_sensitive_text(&artifact.final_public_url);
        let metadata = WebMetadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            final_public_url: &public_url,
            content_type: &artifact.content_type,
            redirect_count: artifact.redirects.len(),
            warnings: &warnings,
            platform: platform_document
                .as_ref()
                .map(|document| document.platform.as_str()),
            platform_id: platform_document
                .as_ref()
                .and_then(|document| document.platform_id.as_deref()),
            title_source: platform_document
                .as_ref()
                .map(|document| document.title_source.as_str()),
            content_kind: platform_document
                .as_ref()
                .map(|document| document.content_type.as_str()),
            author: platform_document
                .as_ref()
                .and_then(|document| document.author.as_deref()),
            published_at: platform_document
                .as_ref()
                .and_then(|document| document.published_at.as_deref()),
            transcript_source: transcript_source.as_deref(),
            transcript_language: transcript_language.as_deref(),
            image_count: platform_document
                .as_ref()
                .map(|document| document.images.len())
                .unwrap_or_default(),
            hashtag_count: platform_document
                .as_ref()
                .map(|document| document.hashtags.len())
                .unwrap_or_default(),
            media_present: platform_document
                .as_ref()
                .is_some_and(|document| document.media_url.is_some()),
            media_size_bytes: remote_media_bytes,
            restricted_content: platform_document
                .as_ref()
                .is_some_and(|document| document.restricted_content),
        };
        let title = platform_document
            .as_ref()
            .map(|document| document.title.clone())
            .or_else(|| extract_html_title(&body))
            .unwrap_or_else(|| request.input.display_name.clone());
        let text_coverage = if let Some(document) = platform_document.as_ref() {
            (!document.description.trim().is_empty() || transcription_ready) as u8 as f64
        } else {
            (!markdown.trim().is_empty()) as u8 as f64
        };
        let written = std::fs::write(staging.join("source.bin"), redact_sensitive_text(&body))
            .and_then(|_| std::fs::write(staging.join("document.md"), markdown.as_bytes()))
            .and_then(|_| {
                serde_json::to_vec_pretty(&metadata)
                    .map_err(std::io::Error::other)
                    .and_then(|bytes| std::fs::write(staging.join("metadata.json"), bytes))
            });
        if written.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(unavailable("The web engine could not write item staging."));
        }
        Ok(EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "document.md".into(),
            asset_paths,
            metadata_path: Some("metadata.json".into()),
            title,
            text_coverage: Some(text_coverage),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation,
            warnings,
        })
    }
}

fn select_primary_web_artifact(
    page_artifact: Result<WebFetchArtifact, BackendError>,
    bilibili_api: Option<&bilibili::BilibiliApiResult>,
    bilibili_api_request: Option<&EngineRequest>,
    request: &EngineRequest,
) -> Result<(WebFetchArtifact, bool), BackendError> {
    match (page_artifact, bilibili_api) {
        (_, Some(api)) => {
            // Exact Bilibili video URLs have a stable public JSON source.
            // Prefer it even when the HTML edge returned bytes: that edge
            // can serve compressed/anti-bot payloads which are not page
            // text and must not override successfully parsed API evidence.
            let api_locator = bilibili_api_request
                .and_then(|resolved| resolved.input.normalized_locator.as_deref())
                .or(request.input.normalized_locator.as_deref())
                .unwrap_or(&request.input.locator);
            let target = UrlPolicy::default().normalize_for_session(api_locator)?;
            Ok((
                WebFetchArtifact {
                    bytes: api.source_body.as_bytes().to_vec(),
                    byte_len: api.source_body.len() as u64,
                    final_public_url: target.public.public_url.clone(),
                    final_session_target: target,
                    content_type: "application/json".into(),
                    sanitized_headers: BTreeMap::new(),
                    redirects: Vec::new(),
                    elapsed_ms: 0,
                },
                true,
            ))
        }
        (Ok(artifact), None) => Ok((artifact, false)),
        (Err(error), None) => Err(error),
    }
}

fn resolve_inside(root: &Path, locator: &str) -> Result<PathBuf, BackendError> {
    let candidate = Path::new(locator);
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
        || !path.starts_with(root)
    {
        return Err(unavailable(
            "The web staging path is outside the project root.",
        ));
    }
    Ok(path)
}

fn cancelled() -> BackendError {
    BackendError::new(
        "IMPORT_V2_CANCELLED",
        "Web import was cancelled.",
        true,
        false,
    )
}

fn unavailable(message: &'static str) -> BackendError {
    BackendError::new("IMPORT_V2_ENGINE_UNAVAILABLE", message, true, true)
}

fn render_platform_markdown(
    document: &crate::services::import_v2::platform_provider::PlatformDocument,
) -> String {
    let mut markdown = format!("# {}\n\n", document.title);
    markdown.push_str(&format!(
        "> 来源：[{}]({})\n\n",
        platform_display_name(&document.platform),
        document.canonical_url
    ));
    markdown.push_str("## 来源信息\n\n");
    markdown.push_str(&format!(
        "- 平台：{}\n",
        platform_display_name(&document.platform)
    ));
    if let Some(platform_id) = document.platform_id.as_deref() {
        markdown.push_str(&format!("- 平台 ID：{platform_id}\n"));
    }
    if let Some(author) = document.author.as_deref() {
        markdown.push_str(&format!("- 作者：{author}\n"));
    }
    if let Some(published_at) = document.published_at.as_deref() {
        markdown.push_str(&format!("- 发布时间：{published_at}\n"));
    }
    markdown.push_str(&format!("- 来源：{}\n", document.canonical_url));
    if document.title_source == "inferred" {
        markdown.push_str("- 标题来源：由原始正文首行推断\n");
    }
    if !document.description.trim().is_empty() {
        markdown.push_str(if document.platform == "xiaohongshu" {
            "\n## 原始正文\n\n"
        } else {
            "\n## 原始描述\n\n"
        });
        markdown.push_str(document.description.trim());
        markdown.push('\n');
    }
    if !document.hashtags.is_empty() {
        markdown.push_str("\n## 话题\n\n");
        markdown.push_str(&document.hashtags.join(" "));
        markdown.push('\n');
    }
    if !document.images.is_empty() && document.content_type != "video" {
        markdown.push_str("\n## 图片\n\n");
        for (index, image) in document.images.iter().enumerate() {
            markdown.push_str(&format!("{}. ![第 {} 张]({image})\n", index + 1, index + 1));
        }
    }
    if document.content_type == "video" {
        if let Some(cover_url) = document.cover_url.as_deref() {
            markdown.push_str(&format!("\n## 封面\n\n![视频封面]({cover_url})\n"));
        }
    }
    if let Some(media_url) = document.media_url.as_deref() {
        markdown.push_str(&format!("\n## 视频 / 音频\n\n[平台媒体]({media_url})\n"));
    }
    if !document.chapters.is_empty() {
        markdown.push_str("\n## 章节\n\n");
        for chapter in &document.chapters {
            markdown.push_str(&format!("- {chapter}\n"));
        }
    }
    markdown
}

fn platform_display_name(platform: &str) -> &str {
    match platform {
        "xiaohongshu" => "小红书",
        "bilibili" => "Bilibili",
        "douyin" => "抖音",
        _ => platform,
    }
}

fn replace_markdown_asset_reference(markdown: &str, target: &str, replacement: &str) -> String {
    let had_trailing_newline = markdown.ends_with('\n');
    let mut output = markdown
        .lines()
        .map(|line| {
            if !line.contains(target) || !line.contains("](") {
                return line.replace(target, replacement);
            }
            if let Some(image_start) = line.find("![") {
                format!("{}{replacement}", &line[..image_start])
            } else {
                replacement.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if had_trailing_newline {
        output.push('\n');
    }
    output
}

fn xiaohongshu_error(failure: ConnectorFailure) -> BackendError {
    let (code, message, retryable, user_action_required) = match failure {
        ConnectorFailure::Captcha | ConnectorFailure::Challenge => (
            "IMPORT_WEB_CAPTCHA_REQUIRED",
            "Xiaohongshu requires an explicit verification session before this note can be imported.",
            false,
            true,
        ),
        ConnectorFailure::LoginRequired => (
            "IMPORT_WEB_LOGIN_REQUIRED",
            "Xiaohongshu requires login before this note can be imported.",
            false,
            true,
        ),
        ConnectorFailure::Removed => (
            "IMPORT_WEB_CONTENT_REMOVED",
            "The Xiaohongshu note is unavailable or has been removed.",
            false,
            true,
        ),
        ConnectorFailure::EmptyBody | ConnectorFailure::StructureChanged => (
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "The Xiaohongshu page did not contain a complete note payload.",
            true,
            true,
        ),
    };
    BackendError::new(code, message, retryable, user_action_required)
}

fn subtitle_extension(content_type: &str, url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if content_type.contains("vtt") || lower.contains(".vtt") {
        "vtt"
    } else if content_type.contains("json") || lower.contains(".json") {
        "json"
    } else if content_type.contains("ass") || lower.contains(".ass") {
        "ass"
    } else {
        "srt"
    }
}

fn is_bilibili_target(input: &ImportInput) -> bool {
    let value = input
        .normalized_locator
        .as_deref()
        .unwrap_or(&input.locator);
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv"
        })
}

fn direct_media_result(
    request: &EngineRequest,
    cancellation: &CancellationToken,
    artifact: &crate::services::import_v2::web_fetch::WebFetchArtifact,
) -> Result<EngineResult, BackendError> {
    let staging = resolve_inside(Path::new(&request.project_root), &request.staging_root)?;
    std::fs::create_dir_all(&staging)
        .map_err(|_| unavailable("The media staging directory could not be created."))?;
    let extension = media_extension(&artifact.content_type, &artifact.final_public_url);
    let lower_url = artifact.final_public_url.to_ascii_lowercase();
    let kind = if artifact
        .content_type
        .to_ascii_lowercase()
        .starts_with("video/")
        || [".mp4", ".webm", ".mov", ".mkv"]
            .iter()
            .any(|extension| lower_url.contains(extension))
    {
        "video"
    } else {
        "audio"
    };
    let mut markdown = format!(
        "# {}\n\nSource media: {}\n",
        request.input.display_name,
        redact_sensitive_text(&artifact.final_public_url)
    );
    let mut asset_paths = Vec::new();
    let mut warnings = Vec::new();
    if request.media_save_mode == MediaSaveMode::PreserveOriginal {
        let relative = format!("assets/original-media.{extension}");
        if std::fs::create_dir_all(staging.join("assets")).is_ok()
            && std::fs::write(staging.join(&relative), &artifact.bytes).is_ok()
        {
            markdown.push_str(&format!("\n[Download original media]({relative})\n"));
            asset_paths.push(relative);
        } else {
            warnings.push(
                "Original media preservation failed; text extraction can still continue.".into(),
            );
        }
    }
    let continuation = if request.local_asr_authorized {
        let temporary = TemporaryMediaWorkspace::create_unique(&staging, ".asr-input")?;
        let temporary_path = temporary.path().join(format!("input.{extension}"));
        std::fs::write(&temporary_path, &artifact.bytes)
            .map_err(|_| unavailable("The temporary media could not be staged."))?;
        let temporary_relative = temporary_path
            .strip_prefix(&staging)
            .map_err(|_| unavailable("The temporary media escaped staging."))?
            .to_string_lossy()
            .replace('\\', "/");
        temporary.retain();
        Some(
            crate::services::import_v2::engine::EngineContinuation::LocalAsr {
                temporary_input_path: temporary_relative,
                media_kind: kind.into(),
            },
        )
    } else {
        return Err(BackendError::new(
            "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
            "Local ASR is required to extract text from direct media URLs before the import can continue.",
            true,
            true,
        ));
    };
    let descriptor = request.input.display_name.clone();
    let public_url = redact_sensitive_text(&artifact.final_public_url);
    let metadata = WebMetadata {
        engine_id: "builtin.web-http",
        engine_version: env!("CARGO_PKG_VERSION"),
        route: "web.generic.readability",
        final_public_url: &public_url,
        content_type: &artifact.content_type,
        redirect_count: artifact.redirects.len(),
        warnings: &warnings,
        platform: None,
        platform_id: None,
        title_source: None,
        content_kind: None,
        author: None,
        published_at: None,
        transcript_source: None,
        transcript_language: None,
        image_count: 0,
        hashtag_count: 0,
        media_present: true,
        media_size_bytes: Some(artifact.byte_len),
        restricted_content: false,
    };
    std::fs::write(staging.join("source.bin"), &artifact.bytes)
        .and_then(|_| std::fs::write(staging.join("document.md"), markdown.as_bytes()))
        .and_then(|_| {
            serde_json::to_vec_pretty(&metadata)
                .map_err(std::io::Error::other)
                .and_then(|bytes| std::fs::write(staging.join("metadata.json"), bytes))
        })
        .map_err(|_| unavailable("The direct media evidence could not be staged."))?;
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    Ok(EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "document.md".into(),
        asset_paths,
        metadata_path: Some("metadata.json".into()),
        title: descriptor,
        text_coverage: Some(1.0),
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: None,
        continuation,
        warnings,
    })
}

fn direct_image_result(
    request: &EngineRequest,
    cancellation: &CancellationToken,
    artifact: &crate::services::import_v2::web_fetch::WebFetchArtifact,
) -> Result<EngineResult, BackendError> {
    let staging = resolve_inside(Path::new(&request.project_root), &request.staging_root)?;
    std::fs::create_dir_all(&staging)
        .map_err(|_| unavailable("The image staging directory could not be created."))?;
    if !request.local_ocr_authorized {
        return Err(ocr_unavailable(
            "Local OCR is required to extract text from a direct image URL.",
        ));
    }
    let extension = image_extension_for_url(&artifact.content_type, &artifact.final_public_url);
    let public_url = redact_sensitive_text(&artifact.final_public_url);
    let mut markdown = format!("# {}\n\n", request.input.display_name);
    let mut asset_paths = Vec::new();
    let mut warnings = Vec::new();
    if request.media_save_mode == MediaSaveMode::PreserveOriginal {
        let relative = format!("assets/original-image.{extension}");
        if std::fs::create_dir_all(staging.join("assets")).is_ok()
            && std::fs::write(staging.join(&relative), &artifact.bytes).is_ok()
        {
            markdown.push_str(&format!("![Original image]({relative})\n"));
            asset_paths.push(relative);
        } else {
            warnings
                .push("Original image preservation failed; local OCR can still continue.".into());
            markdown.push_str("(original image could not be retained after local OCR)\n");
        }
    } else {
        markdown.push_str("(original image not retained after local OCR)\n");
    }
    let temporary_input = stage_temporary_ocr_input(&staging, 0, extension, &artifact.bytes)?;
    let metadata = WebMetadata {
        engine_id: "builtin.web-http",
        engine_version: env!("CARGO_PKG_VERSION"),
        route: "web.generic.readability",
        final_public_url: &public_url,
        content_type: &artifact.content_type,
        redirect_count: artifact.redirects.len(),
        warnings: &warnings,
        platform: None,
        platform_id: None,
        title_source: None,
        content_kind: Some("image"),
        author: None,
        published_at: None,
        transcript_source: None,
        transcript_language: None,
        image_count: 1,
        hashtag_count: 0,
        media_present: false,
        media_size_bytes: None,
        restricted_content: false,
    };
    std::fs::write(staging.join("source.bin"), &artifact.bytes)
        .and_then(|_| std::fs::write(staging.join("document.md"), markdown.as_bytes()))
        .and_then(|_| {
            serde_json::to_vec_pretty(&metadata)
                .map_err(std::io::Error::other)
                .and_then(|bytes| std::fs::write(staging.join("metadata.json"), bytes))
        })
        .map_err(|_| unavailable("The direct image evidence could not be staged."))?;
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    Ok(EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "document.md".into(),
        asset_paths,
        metadata_path: Some("metadata.json".into()),
        title: request.input.display_name.clone(),
        text_coverage: Some(1.0),
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: Some(1.0),
        continuation: Some(
            crate::services::import_v2::engine::EngineContinuation::LocalOcr {
                temporary_input_paths: vec![temporary_input],
            },
        ),
        warnings,
    })
}

fn stage_temporary_ocr_input(
    staging: &Path,
    index: usize,
    extension: &str,
    bytes: &[u8],
) -> Result<String, BackendError> {
    let temporary = TemporaryMediaWorkspace::create_unique(staging, ".ocr-input")?;
    let temporary_path = temporary
        .path()
        .join(format!("image-{:03}.{extension}", index + 1));
    std::fs::write(&temporary_path, bytes)
        .map_err(|_| unavailable("A temporary OCR image could not be staged."))?;
    let relative = temporary_path
        .strip_prefix(staging)
        .map_err(|_| unavailable("The temporary OCR image escaped staging."))?
        .to_string_lossy()
        .replace('\\', "/");
    temporary.retain();
    Ok(relative)
}

fn should_run_platform_image_ocr(
    document: Option<&crate::services::import_v2::platform_provider::PlatformDocument>,
    authorized: bool,
) -> bool {
    authorized
        && document.is_some_and(|document| {
            document.content_type == "image_post" && !document.images.is_empty()
        })
}

fn platform_after_redirect(
    requested: Option<Platform>,
    final_public_url: &str,
) -> Option<Platform> {
    Platform::from_url(final_public_url).or(requested)
}

fn platform_image_requires_ocr(
    document: Option<&crate::services::import_v2::platform_provider::PlatformDocument>,
    authorized: bool,
) -> bool {
    document.is_some_and(|document| {
        document.platform == "xiaohongshu" && document.content_type == "image_post" && !authorized
    })
}

fn platform_image_output_is_meaningful(
    document: &crate::services::import_v2::platform_provider::PlatformDocument,
    successful_images: usize,
) -> bool {
    document.content_type != "image_post" || successful_images == document.images.len()
}

fn ocr_unavailable(message: &str) -> BackendError {
    BackendError::new("IMPORT_WEB_OCR_UNAVAILABLE", message, true, true)
}

fn is_media_content_type(content_type: &str) -> bool {
    let normalized = content_type.to_ascii_lowercase();
    normalized.starts_with("audio/") || normalized.starts_with("video/")
}

fn is_direct_media_locator(input: &ImportInput) -> bool {
    let value = input
        .normalized_locator
        .as_deref()
        .unwrap_or(&input.locator)
        .to_ascii_lowercase();
    [
        ".mp3", ".wav", ".m4a", ".ogg", ".opus", ".flac", ".mp4", ".webm", ".mov", ".mkv",
    ]
    .iter()
    .any(|extension| value.contains(extension))
}

fn is_direct_image_locator(input: &ImportInput) -> bool {
    let value = input
        .normalized_locator
        .as_deref()
        .unwrap_or(&input.locator)
        .to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".gif", ".webp", ".avif"]
        .iter()
        .any(|extension| value.contains(extension))
}

fn is_bilibili_video_locator(request: &EngineRequest) -> bool {
    let value = request
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&request.input.locator);
    is_bilibili_video_url(value)
}

fn is_bilibili_video_url(value: &str) -> bool {
    url::Url::parse(value)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?.to_ascii_lowercase();
            let path = url.path().to_ascii_lowercase();
            Some(
                (host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com"))
                    && (path.contains("/video/")
                        || path.contains("/bangumi/")
                        || path.contains("/list/")),
            )
        })
        .unwrap_or(false)
}

fn is_trusted_platform_asset_url(platform: Platform, value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    let matches_suffix = |suffix: &str| host == suffix || host.ends_with(&format!(".{suffix}"));
    trusted_platform_asset_suffixes(platform)
        .iter()
        .any(|suffix| matches_suffix(suffix))
}

fn trusted_platform_asset_suffixes(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Bilibili => &[
            "bilibili.com",
            "b23.tv",
            "bilivideo.com",
            "bilivideo.cn",
            "hdslb.com",
            "biliimg.com",
            "edge.mountaintoys.cn",
        ],
        Platform::Xiaohongshu => &[
            "xiaohongshu.com",
            "xhslink.com",
            "xhslink.cn",
            "xhscdn.com",
            "xhscdn.net",
        ],
        Platform::Douyin => &[
            "douyin.com",
            "iesdouyin.com",
            "douyinvod.com",
            "douyincdn.com",
            "douyinpic.com",
            "amemv.com",
            "byteimg.com",
            "ibytedtos.com",
            "bytecdn.cn",
            "zjcdn.com",
        ],
    }
}

fn is_platform_auth_challenge(request: &EngineRequest, body: &str) -> bool {
    let value = request
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&request.input.locator);
    let Some(host) = url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    let platform = host == "b23.tv"
        || host == "bilibili.com"
        || host.ends_with(".bilibili.com")
        || host == "xiaohongshu.com"
        || host.ends_with(".xiaohongshu.com")
        || host == "xhslink.com"
        || host.ends_with(".xhslink.com")
        || host == "xhslink.cn"
        || host.ends_with(".xhslink.cn")
        || host == "douyin.com"
        || host.ends_with(".douyin.com")
        || host == "iesdouyin.com"
        || host.ends_with(".iesdouyin.com");
    if !platform {
        return false;
    }
    let lower = body.to_ascii_lowercase();
    [
        "captcha",
        "challenge",
        "security verification",
        "verify you are human",
        "请先登录",
        "请完成验证",
        "访问过于频繁",
        "安全验证",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn extract_html_media_url(body: &str) -> Option<String> {
    for marker in [
        "property=\"og:video\"",
        "property='og:video'",
        "name=\"twitter:player:stream\"",
        "name='twitter:player:stream'",
    ] {
        if let Some(index) = body.find(marker) {
            if let Some(value) = attribute_after(&body[index..], "content") {
                return Some(value);
            }
        }
    }
    let lower = body.to_ascii_lowercase();
    for marker in ["<video", "<source"] {
        if let Some(index) = lower.find(marker) {
            if let Some(value) = attribute_after(&body[index..], "src") {
                return Some(value);
            }
        }
    }
    None
}

fn extract_html_image_urls(body: &str, base_url: &str) -> Vec<String> {
    let lower = body.to_ascii_lowercase();
    let mut cursor = 0;
    let mut result = Vec::new();
    while let Some(offset) = lower[cursor..].find("<img") {
        let start = cursor + offset;
        let end = lower[start..]
            .find('>')
            .map(|value| start + value)
            .unwrap_or(body.len());
        let fragment = &body[start..end];
        if let Some(raw) =
            attribute_after(fragment, "data-src").or_else(|| attribute_after(fragment, "src"))
        {
            if let Ok(url) = url::Url::parse(base_url).and_then(|base| base.join(&raw)) {
                let value = url.to_string();
                if !result.contains(&value) && result.len() < 24 {
                    result.push(value);
                }
            }
        }
        cursor = end.saturating_add(1);
        if cursor >= lower.len() {
            break;
        }
    }
    result
}

fn resolve_web_asset_url(value: &str, base_url: &str) -> Option<String> {
    url::Url::parse(base_url)
        .ok()?
        .join(value)
        .ok()
        .map(|value| value.to_string())
}

fn extract_html_title(body: &str) -> Option<String> {
    for marker in [
        "property=\"og:title\"",
        "property='og:title'",
        "name=\"twitter:title\"",
        "name='twitter:title'",
    ] {
        if let Some(index) = body.to_ascii_lowercase().find(marker) {
            if let Some(value) = attribute_after(&body[index..], "content") {
                if !value.trim().is_empty() {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    let lower = body.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let content_start = body[start..].find('>')? + start + 1;
    let content_end = lower[content_start..].find("</title>")? + content_start;
    let value = body[content_start..content_end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn attribute_after(fragment: &str, name: &str) -> Option<String> {
    let lower = fragment.to_ascii_lowercase();
    let index = lower.find(&format!("{name}="))? + name.len() + 1;
    let remainder = fragment[index..].trim_start();
    let quote = remainder.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let value = remainder[quote.len_utf8()..].split(quote).next()?.trim();
    (!value.is_empty()).then(|| value.replace("&amp;", "&"))
}

fn load_completed_media_download(
    staging: &Path,
    current_media_url: &str,
) -> Result<Option<WebFetchArtifact>, BackendError> {
    let root = staging.join("media-download");
    let payload = root.join("payload.bin");
    let manifest_path = root.join("complete.json");
    if !payload.exists() || !manifest_path.exists() {
        return Ok(None);
    }
    let payload_metadata = std::fs::symlink_metadata(&payload)
        .map_err(|_| unavailable("The completed media cache could not be inspected."))?;
    let manifest_metadata = std::fs::symlink_metadata(&manifest_path)
        .map_err(|_| unavailable("The completed media cache could not be inspected."))?;
    if payload_metadata.file_type().is_symlink()
        || manifest_metadata.file_type().is_symlink()
        || !payload_metadata.is_file()
        || !manifest_metadata.is_file()
    {
        return Ok(None);
    }
    let manifest_bytes = std::fs::read(&manifest_path)
        .map_err(|_| unavailable("The completed media cache manifest could not be read."))?;
    if manifest_bytes.len() > 64 * 1024 {
        return Ok(None);
    }
    let manifest: CompletedMediaDownload = match serde_json::from_slice(&manifest_bytes) {
        Ok(manifest) => manifest,
        Err(_) => return Ok(None),
    };
    if manifest.schema_version != 1
        || !manifest.complete
        || manifest.byte_len == 0
        || manifest.byte_len != payload_metadata.len()
        || manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
        || !hash_media_file(&payload)?.eq_ignore_ascii_case(&manifest.sha256)
    {
        return Ok(None);
    }
    let target = UrlPolicy.normalize_for_session(current_media_url)?;
    Ok(Some(WebFetchArtifact {
        bytes: Vec::new(),
        byte_len: manifest.byte_len,
        final_public_url: target.public.public_url.clone(),
        final_session_target: target,
        content_type: manifest.content_type,
        sanitized_headers: BTreeMap::new(),
        redirects: Vec::new(),
        elapsed_ms: 0,
    }))
}

fn store_completed_media_download(
    staging: &Path,
    downloaded_path: &Path,
    artifact: &WebFetchArtifact,
) -> Result<PathBuf, BackendError> {
    let root = staging.join("media-download");
    std::fs::create_dir_all(&root)
        .map_err(|_| unavailable("The completed media cache could not be created."))?;
    let nonce = uuid::Uuid::new_v4();
    let pending_payload = root.join(format!(".pending-payload-{nonce}"));
    let pending_manifest = root.join(format!(".pending-manifest-{nonce}"));
    move_staged_file(downloaded_path, &pending_payload)
        .map_err(|_| unavailable("The completed media payload could not be staged."))?;
    let byte_len = std::fs::metadata(&pending_payload)
        .map_err(|_| unavailable("The completed media payload could not be measured."))?
        .len();
    if byte_len == 0 || byte_len != artifact.byte_len {
        let _ = std::fs::remove_file(&pending_payload);
        return Err(unavailable("The completed media payload length changed."));
    }
    let manifest = CompletedMediaDownload {
        schema_version: 1,
        complete: true,
        content_type: artifact.content_type.clone(),
        byte_len,
        sha256: hash_media_file(&pending_payload)?,
    };
    std::fs::write(
        &pending_manifest,
        serde_json::to_vec_pretty(&manifest)
            .map_err(|_| unavailable("The completed media cache manifest is invalid."))?,
    )
    .map_err(|_| unavailable("The completed media cache manifest could not be staged."))?;
    let payload = root.join("payload.bin");
    let manifest_path = root.join("complete.json");
    for path in [&payload, &manifest_path] {
        if path.exists() {
            std::fs::remove_file(path).map_err(|_| {
                unavailable("The previous completed media cache could not be replaced.")
            })?;
        }
    }
    std::fs::rename(&pending_payload, &payload)
        .map_err(|_| unavailable("The completed media payload could not be finalized."))?;
    std::fs::rename(&pending_manifest, &manifest_path)
        .map_err(|_| unavailable("The completed media cache manifest could not be finalized."))?;
    Ok(payload)
}

fn hash_media_file(path: &Path) -> Result<String, BackendError> {
    let mut file = std::fs::File::open(path)
        .map_err(|_| unavailable("The completed media payload could not be verified."))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| unavailable("The completed media payload could not be verified."))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn fetch_media_to_file(
    url: &str,
    platform: Option<Platform>,
    item_id: &str,
    cancellation: &CancellationToken,
    referer: &str,
    destination: &Path,
    report_progress: &EngineProgressReporter<'_>,
    private_grant: Option<&PrivateTargetGrant>,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    report_progress(EngineProgress {
        current: 0,
        total: None,
        label: "media.downloading".into(),
    })?;
    let target = UrlPolicy.normalize_for_session(url)?;
    let item_id = item_id.to_string();
    let referer = referer.to_string();
    let destination = destination.to_path_buf();
    let token = cancellation.clone();
    let private_grant = private_grant.cloned();
    let worker_stop = CancellationToken::new();
    let worker_stop_for_fetch = worker_stop.clone();
    let (progress_sender, progress_receiver) = std::sync::mpsc::channel::<WebFetchProgress>();
    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| unavailable("The media fetch runtime could not be started."))?;
        let mut last_progress_bucket = None;
        runtime.block_on(
            WebFetchService::default().fetch_to_file(
                target,
                &UrlPolicy::default(),
                &WebFetchPolicy {
                    max_response_bytes: 1024 * 1024 * 1024,
                    total_timeout_ms: 30 * 60 * 1000,
                    content: WebFetchContent::TemporaryMedia,
                    referer: Some(referer),
                    require_https: platform.is_some(),
                    allowed_host_suffixes: platform
                        .map(trusted_platform_asset_suffixes)
                        .unwrap_or_default()
                        .iter()
                        .map(|suffix| (*suffix).into())
                        .collect(),
                    ..WebFetchPolicy::default()
                },
                private_grant.as_ref(),
                &item_id,
                &destination,
                move |progress| {
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
                || token.is_cancelled() || worker_stop_for_fetch.is_cancelled(),
            ),
        )
    });
    let mut progress_error = None;
    while !worker.is_finished() {
        while let Ok(progress) = progress_receiver.try_recv() {
            if let Err(error) = report_media_download_progress(report_progress, progress) {
                worker_stop.cancel();
                progress_error = Some(error);
                break;
            }
        }
        if progress_error.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let fetched = worker
        .join()
        .map_err(|_| unavailable("The media fetch worker stopped unexpectedly."))?;
    if let Some(error) = progress_error {
        return Err(error);
    }
    while let Ok(progress) = progress_receiver.try_recv() {
        report_media_download_progress(report_progress, progress)?;
    }
    fetched
}

fn report_media_download_progress(
    report_progress: &EngineProgressReporter<'_>,
    progress: WebFetchProgress,
) -> Result<(), BackendError> {
    report_progress(EngineProgress {
        current: progress.downloaded_bytes,
        total: progress.total_bytes,
        label: "media.downloading".into(),
    })
}

fn fetch_image(
    url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    referer: &str,
    platform: Option<Platform>,
    artifact_source: &dyn WebArtifactSource,
    private_grant: Option<&PrivateTargetGrant>,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let target = UrlPolicy.normalize_for_session(url)?;
    artifact_source.fetch(
        target,
        WebFetchPolicy {
            max_response_bytes: 8 * 1024 * 1024,
            content: WebFetchContent::Image,
            referer: Some(referer.to_string()),
            require_https: platform.is_some(),
            allowed_host_suffixes: platform
                .map(trusted_platform_asset_suffixes)
                .unwrap_or_default()
                .iter()
                .map(|suffix| (*suffix).into())
                .collect(),
            ..WebFetchPolicy::default()
        },
        private_grant,
        item_id,
        cancellation,
    )
}

fn fetch_subtitle(
    url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    referer: &str,
    platform: Option<Platform>,
    artifact_source: &dyn WebArtifactSource,
    private_grant: Option<&PrivateTargetGrant>,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let target = UrlPolicy.normalize_for_session(url)?;
    artifact_source.fetch(
        target,
        WebFetchPolicy {
            max_response_bytes: 4 * 1024 * 1024,
            content: WebFetchContent::Subtitle,
            referer: Some(referer.to_string()),
            require_https: platform.is_some(),
            allowed_host_suffixes: platform
                .map(trusted_platform_asset_suffixes)
                .unwrap_or_default()
                .iter()
                .map(|suffix| (*suffix).into())
                .collect(),
            ..WebFetchPolicy::default()
        },
        private_grant,
        item_id,
        cancellation,
    )
}

fn image_extension(content_type: &str) -> &'static str {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        _ => "jpg",
    }
}

fn image_extension_for_url(content_type: &str, url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        _ if lower.contains(".png") => "png",
        _ if lower.contains(".gif") => "gif",
        _ if lower.contains(".webp") => "webp",
        _ if lower.contains(".avif") => "avif",
        _ => "jpg",
    }
}

fn media_extension(content_type: &str, url: &str) -> &'static str {
    let lower_url = url.to_ascii_lowercase();
    match content_type.split(';').next().unwrap_or("").trim() {
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/mp4" => "m4a",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        value if value.starts_with("video/") => "mp4",
        value if value.starts_with("audio/") => "m4a",
        _ if lower_url.contains(".mp3") => "mp3",
        _ if lower_url.contains(".wav") => "wav",
        _ if lower_url.contains(".m4a") => "m4a",
        _ if lower_url.contains(".ogg") => "ogg",
        _ if lower_url.contains(".opus") => "opus",
        _ if lower_url.contains(".flac") => "flac",
        _ if lower_url.contains(".webm") => "webm",
        _ if lower_url.contains(".mov") => "mov",
        _ if lower_url.contains(".mkv") => "mkv",
        _ => "mp4",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        direct_image_result, direct_media_result, extract_html_image_urls, extract_html_media_url,
        extract_html_title, is_bilibili_video_url, is_trusted_platform_asset_url,
        load_completed_media_download, media_extension, platform_after_redirect,
        platform_image_output_is_meaningful, platform_image_requires_ocr, render_platform_markdown,
        replace_markdown_asset_reference, report_media_download_progress,
        select_primary_web_artifact, should_run_platform_image_ocr, store_completed_media_download,
        xiaohongshu_error, NetworkWebArtifactSource, WebArtifactSource,
    };
    use crate::models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode};
    use crate::services::import_v2::connectors::ConnectorFailure;
    use crate::services::import_v2::engine::{EngineOperation, EngineRequest};
    use crate::services::import_v2::platform_provider::PlatformSubtitleKind;
    use crate::services::import_v2::platform_provider::{Platform, PlatformDocument};
    use crate::services::import_v2::redaction::redact_sensitive_text;
    use crate::services::import_v2::url_policy::UrlPolicy;
    use crate::services::import_v2::web_fetch::{
        WebFetchArtifact, WebFetchPolicy, WebFetchProgress,
    };
    use crate::tasks::task_model::CancellationToken;

    #[test]
    fn network_web_fetch_is_safe_when_called_from_a_tokio_runtime() {
        let target = UrlPolicy::default()
            .normalize_for_session("https://example.com/article")
            .unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let error = runtime
            .block_on(async {
                NetworkWebArtifactSource.fetch(
                    target,
                    WebFetchPolicy::default(),
                    None,
                    "runtime-boundary",
                    &cancellation,
                )
            })
            .unwrap_err();

        assert_eq!(error.code, "IMPORT_V2_CANCELLED");
    }

    #[test]
    fn preserves_audio_and_video_container_extensions() {
        assert_eq!(
            media_extension("audio/mp4; codecs=mp4a.40.2", "https://cdn.example/a"),
            "m4a"
        );
        assert_eq!(
            media_extension("video/mp4; codecs=avc1", "https://cdn.example/a"),
            "mp4"
        );
        assert_eq!(
            media_extension("video/webm", "https://cdn.example/a"),
            "webm"
        );
    }

    #[test]
    fn forwards_builtin_media_download_bytes_to_the_engine_reporter() {
        let reported = std::sync::Mutex::new(Vec::new());
        report_media_download_progress(
            &|progress| {
                reported.lock().unwrap().push(progress);
                Ok(())
            },
            WebFetchProgress {
                downloaded_bytes: 512,
                total_bytes: Some(1024),
            },
        )
        .unwrap();
        assert_eq!(
            reported.lock().unwrap().as_slice(),
            &[crate::services::import_v2::engine::EngineProgress {
                current: 512,
                total: Some(1024),
                label: "media.downloading".into(),
            }]
        );
    }

    #[test]
    fn completed_media_download_cache_is_reusable_only_while_payload_is_intact() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        let downloaded = root.path().join("response.bin");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(&downloaded, b"downloaded-media").unwrap();
        let url = "https://cdn.example/video.mp4";
        let target = UrlPolicy::default().normalize_for_session(url).unwrap();
        let artifact = WebFetchArtifact {
            bytes: Vec::new(),
            byte_len: 16,
            final_public_url: target.public.public_url.clone(),
            final_session_target: target,
            content_type: "video/mp4".into(),
            sanitized_headers: Default::default(),
            redirects: Vec::new(),
            elapsed_ms: 5,
        };

        let payload = store_completed_media_download(&staging, &downloaded, &artifact).unwrap();
        assert!(!downloaded.exists());
        assert_eq!(std::fs::read(&payload).unwrap(), b"downloaded-media");
        let reused = load_completed_media_download(&staging, url)
            .unwrap()
            .expect("completed cache should be reusable");
        assert_eq!(reused.byte_len, artifact.byte_len);
        assert_eq!(reused.content_type, artifact.content_type);
        assert!(reused.bytes.is_empty());

        std::fs::write(&payload, b"tampered-payload").unwrap();
        assert!(load_completed_media_download(&staging, url)
            .unwrap()
            .is_none());
    }

    #[test]
    fn direct_media_preservation_failure_does_not_block_asr_continuation() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("assets"), b"directory-conflict").unwrap();
        let url = "https://cdn.example/original.mp4";
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: "media-preservation".into(),
            project_id: "project".into(),
            session_id: "session".into(),
            item_id: "item".into(),
            task_id: "task".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Video".into(),
                locator: url.into(),
                normalized_locator: Some(url.into()),
                source_identity: None,
                media_save_mode: MediaSaveMode::PreserveOriginal,
            },
            project_root: root.path().to_string_lossy().into_owned(),
            staging_root: "staging".into(),
            chained_input: None,
            local_asr_authorized: true,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: MediaSaveMode::PreserveOriginal,
        };
        let target = UrlPolicy::default().normalize_for_session(url).unwrap();
        let artifact = WebFetchArtifact {
            bytes: b"video-bytes".to_vec(),
            byte_len: 11,
            final_public_url: url.into(),
            final_session_target: target,
            content_type: "video/mp4".into(),
            sanitized_headers: Default::default(),
            redirects: Vec::new(),
            elapsed_ms: 1,
        };

        let result = direct_media_result(&request, &CancellationToken::new(), &artifact).unwrap();

        assert!(result.asset_paths.is_empty());
        assert!(result.continuation.is_some());
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("preservation failed")));
        assert_eq!(
            std::fs::read(staging.join("source.bin")).unwrap(),
            b"video-bytes"
        );
    }

    #[test]
    fn direct_image_preservation_failure_does_not_block_ocr_continuation() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("assets"), b"directory-conflict").unwrap();
        let url = "https://cdn.example/original.png";
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: "image-preservation".into(),
            project_id: "project".into(),
            session_id: "session".into(),
            item_id: "item".into(),
            task_id: "task".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Image".into(),
                locator: url.into(),
                normalized_locator: Some(url.into()),
                source_identity: None,
                media_save_mode: MediaSaveMode::PreserveOriginal,
            },
            project_root: root.path().to_string_lossy().into_owned(),
            staging_root: "staging".into(),
            chained_input: None,
            local_asr_authorized: false,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: true,
            media_save_mode: MediaSaveMode::PreserveOriginal,
        };
        let target = UrlPolicy::default().normalize_for_session(url).unwrap();
        let artifact = WebFetchArtifact {
            bytes: b"image-bytes".to_vec(),
            byte_len: 11,
            final_public_url: url.into(),
            final_session_target: target,
            content_type: "image/png".into(),
            sanitized_headers: Default::default(),
            redirects: Vec::new(),
            elapsed_ms: 1,
        };

        let result = direct_image_result(&request, &CancellationToken::new(), &artifact).unwrap();

        assert!(result.asset_paths.is_empty());
        assert!(result.continuation.is_some());
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("preservation failed")));
        assert_eq!(
            std::fs::read(staging.join("source.bin")).unwrap(),
            b"image-bytes"
        );
    }

    #[test]
    fn extracts_media_from_og_video_and_video_source_markup() {
        assert_eq!(
            extract_html_media_url(
                r#"<meta property="og:video" content="https://cdn.example/video.mp4">"#
            ),
            Some("https://cdn.example/video.mp4".into())
        );
        assert_eq!(
            extract_html_media_url(
                r#"<video><source src='https://cdn.example/video.webm'></video>"#
            ),
            Some("https://cdn.example/video.webm".into())
        );
    }

    #[test]
    fn extracts_titles_and_resolves_image_sources() {
        let html = r#"<meta property="og:title" content="Note title"><img data-src="/image.webp">"#;
        assert_eq!(extract_html_title(html).as_deref(), Some("Note title"));
        assert_eq!(
            extract_html_image_urls(html, "https://www.xiaohongshu.com/explore/1"),
            vec!["https://www.xiaohongshu.com/image.webp"]
        );
    }

    #[test]
    fn redacts_sensitive_query_and_json_values() {
        let value = redact_sensitive_text(
            r#"https://cdn.example/a?token=secret&xsec_token=xhs-secret&x=1 {"sign":"signed"}"#,
        );
        assert!(!value.contains("secret"));
        assert!(!value.contains("xhs-secret"));
        assert!(!value.contains("signed"));
        assert!(value.contains("REDACTED"));
    }

    #[test]
    fn xiaohongshu_provider_evidence_remains_valid_json_without_signed_values() {
        let document = PlatformDocument {
            platform: "xiaohongshu".into(),
            platform_id: Some("note-1".into()),
            content_type: "image_post".into(),
            canonical_url: "https://www.xiaohongshu.com/explore/note-1".into(),
            title: "笔记".into(),
            title_source: "platform".into(),
            author: None,
            published_at: None,
            description: "正文".into(),
            hashtags: Vec::new(),
            images: vec![
                "https://sns-img-qc.xhscdn.com/1.jpg?xsec_token=image-secret&sign=signature".into(),
            ],
            media_url: None,
            media_size_bytes: None,
            cover_url: None,
            subtitles: Vec::new(),
            chapters: Vec::new(),
            restricted_content: false,
        };
        let persisted = redact_sensitive_text(&serde_json::to_string_pretty(&document).unwrap());
        assert!(serde_json::from_str::<serde_json::Value>(&persisted).is_ok());
        assert!(!persisted.contains("image-secret"));
        assert!(!persisted.contains("signature"));
        assert!(!persisted.contains("xsec_token"));
        assert!(!persisted.contains("sign="));
        assert!(persisted.contains("/1.jpg"));
    }

    #[test]
    fn platform_assets_use_a_closed_host_allowlist() {
        assert!(is_trusted_platform_asset_url(
            Platform::Bilibili,
            "https://upos-sz-mirrorali.bilivideo.com/video.mp4"
        ));
        assert!(is_trusted_platform_asset_url(
            Platform::Bilibili,
            "https://809al93l.edge.mountaintoys.cn:4483/upgcxcode/video.mp4"
        ));
        assert!(!is_trusted_platform_asset_url(
            Platform::Bilibili,
            "https://edge.mountaintoys.cn.evil.example/video.mp4"
        ));
        assert!(!is_trusted_platform_asset_url(
            Platform::Bilibili,
            "http://809al93l.edge.mountaintoys.cn:4483/upgcxcode/video.mp4"
        ));
        assert!(is_trusted_platform_asset_url(
            Platform::Xiaohongshu,
            "https://sns-img-qc.xhscdn.com/image.jpg"
        ));
        assert!(is_trusted_platform_asset_url(
            Platform::Douyin,
            "https://p3-sign.douyinvod.com/video.mp4"
        ));
        assert!(is_trusted_platform_asset_url(
            Platform::Douyin,
            "https://v3-dy-o-abtest.zjcdn.com/video.mp4"
        ));
        assert!(!is_trusted_platform_asset_url(
            Platform::Douyin,
            "https://cdn.example/video.mp4"
        ));
    }

    #[test]
    fn resolved_bilibili_video_urls_are_detected_after_short_link_redirects() {
        assert!(is_bilibili_video_url(
            "https://www.bilibili.com/video/BV1N7411A7WU/"
        ));
        assert!(!is_bilibili_video_url("https://b23.tv/short-code"));
    }

    #[test]
    fn redirect_target_reclassifies_an_unknown_short_link_as_xiaohongshu() {
        assert_eq!(
            platform_after_redirect(
                None,
                "https://www.xiaohongshu.com/discovery/item/6a61c0bf000000000e034c02"
            ),
            Some(Platform::Xiaohongshu)
        );
        assert_eq!(
            platform_after_redirect(
                Some(Platform::Xiaohongshu),
                "https://www.xiaohongshu.com/explore/note-1"
            ),
            Some(Platform::Xiaohongshu)
        );
    }

    #[test]
    fn successful_bilibili_api_evidence_precedes_unreadable_page_bytes() {
        let url = "https://www.bilibili.com/video/BV1N7411A7WU/";
        let target = UrlPolicy::default().normalize_for_session(url).unwrap();
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: "fixture".into(),
            project_id: "fixture".into(),
            session_id: "fixture".into(),
            item_id: "fixture".into(),
            task_id: "fixture".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Bilibili".into(),
                locator: url.into(),
                normalized_locator: Some(url.into()),
                source_identity: None,
                media_save_mode: MediaSaveMode::ExtractOnly,
            },
            project_root: String::new(),
            staging_root: String::new(),
            chained_input: None,
            local_asr_authorized: false,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: MediaSaveMode::ExtractOnly,
        };
        let page = WebFetchArtifact {
            bytes: vec![0xff, 0xfe, 0xfd],
            byte_len: 3,
            final_public_url: url.into(),
            final_session_target: target,
            content_type: "text/html".into(),
            sanitized_headers: Default::default(),
            redirects: Vec::new(),
            elapsed_ms: 1,
        };
        let api = crate::services::import_v2::bilibili::BilibiliApiResult {
            source_body: r#"{"view":{"code":0},"player":null}"#.into(),
            document: PlatformDocument {
                platform: "bilibili".into(),
                platform_id: Some("BV1N7411A7WU".into()),
                content_type: "video".into(),
                canonical_url: url.into(),
                title: "Fixture".into(),
                title_source: "platform".into(),
                author: None,
                published_at: None,
                description: "Fixture".into(),
                hashtags: Vec::new(),
                images: Vec::new(),
                media_url: None,
                media_size_bytes: None,
                cover_url: None,
                subtitles: Vec::new(),
                chapters: Vec::new(),
                restricted_content: false,
            },
        };

        let (selected, api_is_source) =
            select_primary_web_artifact(Ok(page), Some(&api), None, &request).unwrap();

        assert!(api_is_source);
        assert_eq!(selected.bytes, api.source_body.as_bytes());
        assert_eq!(selected.content_type, "application/json");
    }

    #[test]
    fn xiaohongshu_image_posts_require_ocr_before_preview() {
        let document = PlatformDocument {
            platform: "xiaohongshu".into(),
            platform_id: Some("note-1".into()),
            content_type: "image_post".into(),
            canonical_url: "https://www.xiaohongshu.com/explore/note-1".into(),
            title: "Image post".into(),
            title_source: "platform".into(),
            author: None,
            published_at: None,
            description: "Body".into(),
            hashtags: Vec::new(),
            images: vec!["https://sns-img-qc.xhscdn.com/one.jpg".into()],
            media_url: None,
            media_size_bytes: None,
            cover_url: None,
            subtitles: Vec::new(),
            chapters: Vec::new(),
            restricted_content: false,
        };
        assert!(platform_image_requires_ocr(Some(&document), false));
        assert!(!platform_image_requires_ocr(Some(&document), true));
        assert!(!should_run_platform_image_ocr(Some(&document), false));
        assert!(should_run_platform_image_ocr(Some(&document), true));
        let mut video = document.clone();
        video.content_type = "video".into();
        video.subtitles.push(
            crate::services::import_v2::platform_provider::PlatformSubtitle {
                url: "https://sns-subtitle-s2.xhscdn.com/source.srt".into(),
                automatic: true,
                kind: PlatformSubtitleKind::PlatformAutoOriginal,
                language: Some("zh-CN".into()),
                label: Some("source".into()),
            },
        );
        assert!(
            !should_run_platform_image_ocr(Some(&video), true),
            "a subtitle-only video cover must never enter the image OCR continuation"
        );
        let mut image_only = document.clone();
        image_only.description.clear();
        assert!(!platform_image_output_is_meaningful(&image_only, 0));
        assert!(platform_image_output_is_meaningful(&image_only, 1));
    }

    #[test]
    fn ordinary_webpage_images_never_enter_platform_ocr_continuation() {
        assert!(!platform_image_requires_ocr(None, false));
        assert!(!platform_image_requires_ocr(None, true));
        assert!(!should_run_platform_image_ocr(None, true));
    }

    #[test]
    fn xiaohongshu_markdown_keeps_source_sections_and_marks_inferred_titles() {
        let document = PlatformDocument {
            platform: "xiaohongshu".into(),
            platform_id: Some("note-1".into()),
            content_type: "image_post".into(),
            canonical_url: "https://www.xiaohongshu.com/explore/note-1".into(),
            title: "正文首行".into(),
            title_source: "inferred".into(),
            author: Some("作者".into()),
            published_at: Some("2026-07-20T08:00:00Z".into()),
            description: "正文首行\n第二行".into(),
            hashtags: vec!["#知识库".into()],
            images: vec!["https://sns-webpic-qc.xhscdn.com/1.jpg".into()],
            media_url: None,
            media_size_bytes: None,
            cover_url: Some("https://sns-webpic-qc.xhscdn.com/1.jpg".into()),
            subtitles: Vec::new(),
            chapters: Vec::new(),
            restricted_content: false,
        };
        let markdown = render_platform_markdown(&document);
        for expected in [
            "## 原始正文",
            "## 话题",
            "## 图片",
            "标题来源：由原始正文首行推断",
        ] {
            assert!(markdown.contains(expected), "missing {expected}");
        }
        assert!(!markdown.starts_with("---"));
        assert!(!markdown.contains("engine_id"));
        assert!(!markdown.contains("source_id"));
        assert!(!markdown.contains("## 字幕 / 转写"));
        assert!(!markdown.contains("## 封面"));
    }

    #[test]
    fn unavailable_platform_assets_become_readable_text_not_broken_links() {
        let markdown = "## 图片\n\n1. ![第 1 张](https://cdn.example/1.jpg)\n\n## 视频 / 音频\n\n[平台媒体](https://cdn.example/video.mp4)\n";
        let markdown = replace_markdown_asset_reference(
            markdown,
            "https://cdn.example/1.jpg",
            "（原图不可用）",
        );
        let markdown = replace_markdown_asset_reference(
            &markdown,
            "https://cdn.example/video.mp4",
            "（原始媒体未保留）",
        );
        assert!(markdown.contains("1. （原图不可用）"));
        assert!(markdown.contains("## 视频 / 音频\n\n（原始媒体未保留）"));
        assert!(!markdown.contains("]()"));
    }

    #[test]
    fn xiaohongshu_failures_map_to_stable_actionable_codes() {
        assert_eq!(
            xiaohongshu_error(ConnectorFailure::Captcha).code,
            "IMPORT_WEB_CAPTCHA_REQUIRED"
        );
        assert_eq!(
            xiaohongshu_error(ConnectorFailure::LoginRequired).code,
            "IMPORT_WEB_LOGIN_REQUIRED"
        );
        assert_eq!(
            xiaohongshu_error(ConnectorFailure::Removed).code,
            "IMPORT_WEB_CONTENT_REMOVED"
        );
    }
}

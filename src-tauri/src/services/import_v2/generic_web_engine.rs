use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode};
use crate::services::import_v2::bilibili;
use crate::services::import_v2::connectors::{wechat, xiaohongshu, ConnectorFailure};
use crate::services::import_v2::engine::{
    EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::markdown_normalizer::{
    decode_text, html_to_markdown, normalize_markdown,
};
use crate::services::import_v2::media_router::{
    link_or_copy, move_staged_file, TemporaryMediaWorkspace,
};
use crate::services::import_v2::platform_provider::{extract_platform_document, Platform};
use crate::services::import_v2::redaction::redact_sensitive_text;
use crate::services::import_v2::subtitle::{parse_subtitle_segments, render_subtitle_markdown};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{
    WebFetchArtifact, WebFetchContent, WebFetchPolicy, WebFetchService,
};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;
use serde::Serialize;

#[derive(Clone)]
pub struct GenericWebEngine {
    web_targets: Arc<WebTargetStore>,
    route: &'static str,
    engine_id: &'static str,
}

impl GenericWebEngine {
    pub const fn new(
        web_targets: Arc<WebTargetStore>,
        engine_id: &'static str,
        route: &'static str,
    ) -> Self {
        Self {
            web_targets,
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
        let item_id = request.item_id.clone();
        let fetch_item_id = item_id.clone();
        let token = cancellation.clone();
        let mut fetch_policy = WebFetchPolicy::default();
        let direct_media_url = is_direct_media_locator(&request.input);
        let direct_image_url = is_direct_image_locator(&request.input);
        if direct_media_url {
            fetch_policy.content = WebFetchContent::Media;
            fetch_policy.max_response_bytes = 256 * 1024 * 1024;
        } else if direct_image_url {
            fetch_policy.content = WebFetchContent::Image;
            fetch_policy.max_response_bytes = 8 * 1024 * 1024;
        }
        let page_artifact = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| unavailable("The web fetch runtime could not be started."))?;
            runtime.block_on(WebFetchService::default().fetch(
                target,
                &UrlPolicy::default(),
                &fetch_policy,
                None,
                &fetch_item_id,
                |_| {},
                || token.is_cancelled(),
            ))
        })
        .join()
        .map_err(|_| unavailable("The web fetch worker stopped unexpectedly."))?;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let platform = Platform::from_url(
            request
                .input
                .normalized_locator
                .as_deref()
                .unwrap_or(&request.input.locator),
        );
        let bilibili_api_request = page_artifact.as_ref().ok().map(|artifact| {
            let mut resolved = request.clone();
            resolved.input.normalized_locator = Some(artifact.final_public_url.clone());
            resolved
        });
        let bilibili_api =
            if self.route == "web.bilibili.video" && platform == Some(Platform::Bilibili) {
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
        let (mut markdown, mut warnings) = platform_document
            .as_ref()
            .map(|document| {
                render_platform_markdown(
                    document,
                    self.route,
                    self.engine_id,
                    env!("CARGO_PKG_VERSION"),
                )
            })
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
        let mut successful_images = 0usize;
        for (index, image_url) in image_urls.into_iter().enumerate() {
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
            if request.media_save_mode == MediaSaveMode::ExtractOnly && !image_ocr_enabled {
                markdown =
                    replace_markdown_asset_reference(&markdown, &image_url, "（原图未保留）");
                continue;
            }
            match fetch_image(
                &image_url,
                &item_id,
                cancellation,
                &artifact.final_public_url,
            ) {
                Ok(image) => {
                    if platform.is_some_and(|platform| {
                        !is_trusted_platform_asset_url(platform, &image.final_public_url)
                    }) {
                        markdown = replace_markdown_asset_reference(
                            &markdown,
                            &image_url,
                            "锛堝浘鐗囨潵婧愭湭鑾峰厑璁革級",
                        );
                        warnings.push(
                            "Platform image redirect left the verified host allowlist.".into(),
                        );
                        continue;
                    }
                    successful_images += 1;
                    let extension = image_extension(&image.content_type);
                    if image_ocr_enabled {
                        temporary_ocr_inputs.push(stage_temporary_ocr_input(
                            &staging,
                            index,
                            extension,
                            &image.bytes,
                        )?);
                    }
                    if request.media_save_mode == MediaSaveMode::PreserveOriginal {
                        let relative = format!("assets/images/{:03}.{extension}", index + 1);
                        std::fs::create_dir_all(staging.join("assets/images")).map_err(|_| {
                            unavailable("The original image directory could not be created.")
                        })?;
                        std::fs::write(staging.join(&relative), &image.bytes)
                            .map_err(|_| unavailable("An original image could not be staged."))?;
                        markdown = markdown.replace(&image_url, &relative);
                        asset_paths.push(relative);
                    } else {
                        markdown = replace_markdown_asset_reference(
                            &markdown,
                            &image_url,
                            "（原图未保留）",
                        );
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
        if platform_document.as_ref().is_some_and(|document| {
            !platform_image_output_is_meaningful(document, successful_images)
        }) {
            return Err(BackendError::new(
                "IMPORT_WEB_MEDIA_UNAVAILABLE",
                "The Xiaohongshu image post had no text and none of its images could be localized.",
                true,
                true,
            ));
        }
        if image_ocr_enabled {
            if temporary_ocr_inputs.is_empty() {
                warnings.push(
                    "Local OCR was requested, but no platform image could be downloaded.".into(),
                );
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
                            transcript_source = Some(if subtitle.automatic {
                                "platform_auto_subtitle".into()
                            } else {
                                "platform_human_subtitle".into()
                            });
                            transcript_language = subtitle.language.clone();
                            markdown.push_str("\n\n## 字幕 / 转写\n\n");
                            markdown.push_str(&format!(
                                "> 来源：{}{}\n\n",
                                if subtitle.automatic {
                                    "平台自动字幕"
                                } else {
                                    "平台人工字幕"
                                },
                                subtitle
                                    .label
                                    .as_deref()
                                    .map(|label| format!(" · {label}"))
                                    .unwrap_or_default()
                            ));
                            markdown.push_str(&rendered);
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
                            let segments_relative = "subtitles/segments.json";
                            let serialized =
                                serde_json::to_vec_pretty(&segments).map_err(|_| {
                                    unavailable(
                                        "The normalized subtitle segments could not be serialized.",
                                    )
                                })?;
                            std::fs::write(staging.join(segments_relative), serialized).map_err(
                                |_| {
                                    unavailable(
                                        "The normalized subtitle segments could not be staged.",
                                    )
                                },
                            )?;
                            asset_paths.push(segments_relative.into());
                            transcription_ready = true;
                            break;
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
        if platform == Some(Platform::Bilibili)
            && (is_bilibili_video_locator(request)
                || is_bilibili_video_url(&artifact.final_public_url))
            && media_url.is_none()
        {
            if transcription_ready && request.media_save_mode == MediaSaveMode::ExtractOnly {
                // A verified transcript satisfies extraction-only imports;
                // no media download is needed.
            } else if request.allow_missing_transcript
                && request.media_save_mode == MediaSaveMode::ExtractOnly
            {
                warnings.push(
                    "Bilibili metadata was imported without a transcript or local media stream."
                        .into(),
                );
            } else if !transcription_ready
                && !request.local_asr_authorized
                && !request.allow_missing_transcript
            {
                return Err(BackendError::new(
                    "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
                    "Bilibili did not expose usable subtitles and no media was available for local ASR.",
                    true,
                    true,
                ));
            } else {
                return Err(BackendError::new(
                    "IMPORT_WEB_MEDIA_UNAVAILABLE",
                    "Bilibili did not expose a downloadable media stream for this video.",
                    true,
                    true,
                ));
            }
        }
        if is_supported_media_target(request) {
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
                    if !transcription_ready
                        && !request.local_asr_authorized
                        && !request.allow_missing_transcript
                    {
                        return Err(BackendError::new(
                            "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
                            "Local ASR is required because the platform did not provide usable subtitles.",
                            true,
                            true,
                        ));
                    }
                    if request.media_save_mode == MediaSaveMode::ExtractOnly
                        && (transcription_ready || request.allow_missing_transcript)
                    {
                        None
                    } else {
                        let download =
                            TemporaryMediaWorkspace::create_unique(&staging, ".media-fetch")?;
                        let download_path = download.path().join("response.bin");
                        match fetch_media_to_file(
                            &media_url,
                            &item_id,
                            cancellation,
                            request
                                .input
                                .normalized_locator
                                .as_deref()
                                .unwrap_or(&request.input.locator),
                            &download_path,
                        ) {
                            Ok(media) => Some((media, download)),
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
                    let extension = media_extension(&media.content_type, &media.final_public_url);
                    if media.byte_len == 0 {
                        return Err(unavailable("The platform media response was empty."));
                    }
                    let downloaded_path = download.path().join("response.bin");
                    let durable_media = if request.media_save_mode
                        == MediaSaveMode::PreserveOriginal
                    {
                        let relative = format!("assets/original-media.{extension}");
                        std::fs::create_dir_all(staging.join("assets")).map_err(|_| {
                            unavailable("The original media directory could not be created.")
                        })?;
                        let durable_path = staging.join(&relative);
                        move_staged_file(&downloaded_path, &durable_path)
                            .map_err(|_| unavailable("The original media could not be staged."))?;
                        markdown = markdown.replace(&media_url, &relative);
                        markdown.push_str(&format!(
                            "\n\n## Original media\n\n[Download original media]({relative})\n"
                        ));
                        asset_paths.push(relative);
                        Some(durable_path)
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
                            move_staged_file(&downloaded_path, &temporary_path).map_err(|_| {
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
    route: &str,
    engine_id: &str,
    engine_version: &str,
) -> String {
    let yaml = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
    let mut markdown = String::from("---\n");
    markdown.push_str("type: source\n");
    markdown.push_str(&format!("title: {}\n", yaml(&document.title)));
    markdown.push_str(&format!("title_source: {}\n", yaml(&document.title_source)));
    markdown.push_str(&format!("source_url: {}\n", yaml(&document.canonical_url)));
    markdown.push_str(&format!("source_platform: {}\n", yaml(&document.platform)));
    markdown.push_str(&format!("content_type: {}\n", yaml(&document.content_type)));
    markdown.push_str(&format!("route: {}\n", yaml(route)));
    markdown.push_str(&format!("engine_id: {}\n", yaml(engine_id)));
    markdown.push_str(&format!("engine_version: {}\n", yaml(engine_version)));
    if let Some(platform_id) = document.platform_id.as_deref() {
        markdown.push_str(&format!("source_id: {}\n", yaml(platform_id)));
    }
    if let Some(author) = document.author.as_deref() {
        markdown.push_str(&format!("author: {}\n", yaml(author)));
    }
    if let Some(published_at) = document.published_at.as_deref() {
        markdown.push_str(&format!("published_at: {}\n", yaml(published_at)));
    }
    markdown.push_str("---\n\n");
    markdown.push_str(&format!("# {}\n\n", document.title));
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
    markdown.push_str(&format!("- 导入路线：`{route}`\n"));
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
    if request.media_save_mode == MediaSaveMode::PreserveOriginal {
        let relative = format!("assets/original-media.{extension}");
        std::fs::create_dir_all(staging.join("assets"))
            .map_err(|_| unavailable("The original media directory could not be created."))?;
        std::fs::write(staging.join(&relative), &artifact.bytes)
            .map_err(|_| unavailable("The original media could not be staged."))?;
        markdown.push_str(&format!("\n[Download original media]({relative})\n"));
        asset_paths.push(relative);
    }
    let warnings = Vec::new();
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
    if request.media_save_mode == MediaSaveMode::PreserveOriginal {
        let relative = format!("assets/original-image.{extension}");
        std::fs::create_dir_all(staging.join("assets"))
            .map_err(|_| unavailable("The original image directory could not be created."))?;
        std::fs::write(staging.join(&relative), &artifact.bytes)
            .map_err(|_| unavailable("The original image could not be staged."))?;
        markdown.push_str(&format!("![Original image]({relative})\n"));
        asset_paths.push(relative);
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
        warnings: &[],
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
        warnings: Vec::new(),
    })
}

fn stage_temporary_ocr_input(
    staging: &Path,
    index: usize,
    extension: &str,
    bytes: &[u8],
) -> Result<String, BackendError> {
    let _ = index;
    let temporary = TemporaryMediaWorkspace::create_unique(staging, ".ocr-input")?;
    let temporary_path = temporary.path().join(format!("input.{extension}"));
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
        && document
            .is_some_and(|document| !document.images.is_empty() && document.media_url.is_none())
}

fn platform_image_output_is_meaningful(
    document: &crate::services::import_v2::platform_provider::PlatformDocument,
    successful_images: usize,
) -> bool {
    document.platform != "xiaohongshu"
        || document.content_type != "image_post"
        || !document.description.trim().is_empty()
        || successful_images > 0
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

fn is_supported_media_target(request: &EngineRequest) -> bool {
    let value = request
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&request.input.locator);
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            host == "b23.tv"
                || host == "bilibili.com"
                || host.ends_with(".bilibili.com")
                || host == "xiaohongshu.com"
                || host.ends_with(".xiaohongshu.com")
                || host == "xhslink.com"
                || host.ends_with(".xhslink.com")
                || host == "douyin.com"
                || host.ends_with(".douyin.com")
                || host == "iesdouyin.com"
                || host.ends_with(".iesdouyin.com")
        })
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
    let Some(host) = url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    let matches_suffix = |suffix: &str| host == suffix || host.ends_with(&format!(".{suffix}"));
    match platform {
        Platform::Bilibili => [
            "bilibili.com",
            "b23.tv",
            "bilivideo.com",
            "bilivideo.cn",
            "hdslb.com",
            "biliimg.com",
        ]
        .iter()
        .any(|suffix| matches_suffix(suffix)),
        Platform::Xiaohongshu => ["xiaohongshu.com", "xhslink.com", "xhscdn.com", "xhscdn.net"]
            .iter()
            .any(|suffix| matches_suffix(suffix)),
        Platform::Douyin => [
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
        ]
        .iter()
        .any(|suffix| matches_suffix(suffix)),
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

fn fetch_media_to_file(
    url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    referer: &str,
    destination: &Path,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let target = UrlPolicy.normalize_for_session(url)?;
    let item_id = item_id.to_string();
    let referer = referer.to_string();
    let destination = destination.to_path_buf();
    let token = cancellation.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| unavailable("The media fetch runtime could not be started."))?;
        runtime.block_on(WebFetchService::default().fetch_to_file(
            target,
            &UrlPolicy::default(),
            &WebFetchPolicy {
                max_response_bytes: 1024 * 1024 * 1024,
                total_timeout_ms: 30 * 60 * 1000,
                content: WebFetchContent::TemporaryMedia,
                referer: Some(referer),
                ..WebFetchPolicy::default()
            },
            None,
            &item_id,
            &destination,
            |_| {},
            || token.is_cancelled(),
        ))
    })
    .join()
    .map_err(|_| unavailable("The media fetch worker stopped unexpectedly."))?
}

fn fetch_image(
    url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    referer: &str,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let target = UrlPolicy.normalize_for_session(url)?;
    let item_id = item_id.to_string();
    let referer = referer.to_string();
    let token = cancellation.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| unavailable("The image fetch runtime could not be started."))?;
        runtime.block_on(WebFetchService::default().fetch(
            target,
            &UrlPolicy::default(),
            &WebFetchPolicy {
                max_response_bytes: 8 * 1024 * 1024,
                content: WebFetchContent::Image,
                referer: Some(referer),
                ..WebFetchPolicy::default()
            },
            None,
            &item_id,
            |_| {},
            || token.is_cancelled(),
        ))
    })
    .join()
    .map_err(|_| unavailable("The image fetch worker stopped unexpectedly."))?
}

fn fetch_subtitle(
    url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    referer: &str,
) -> Result<crate::services::import_v2::web_fetch::WebFetchArtifact, BackendError> {
    let target = UrlPolicy.normalize_for_session(url)?;
    let item_id = item_id.to_string();
    let referer = referer.to_string();
    let token = cancellation.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| unavailable("The subtitle fetch runtime could not be started."))?;
        runtime.block_on(WebFetchService::default().fetch(
            target,
            &UrlPolicy::default(),
            &WebFetchPolicy {
                max_response_bytes: 4 * 1024 * 1024,
                content: WebFetchContent::Subtitle,
                referer: Some(referer),
                ..WebFetchPolicy::default()
            },
            None,
            &item_id,
            |_| {},
            || token.is_cancelled(),
        ))
    })
    .join()
    .map_err(|_| unavailable("The subtitle fetch worker stopped unexpectedly."))?
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
        extract_html_image_urls, extract_html_media_url, extract_html_title, is_bilibili_video_url,
        is_trusted_platform_asset_url, media_extension, platform_image_output_is_meaningful,
        render_platform_markdown, replace_markdown_asset_reference, select_primary_web_artifact,
        should_run_platform_image_ocr, xiaohongshu_error, GenericWebEngine,
    };
    use crate::models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode};
    use crate::services::import_v2::connectors::ConnectorFailure;
    use crate::services::import_v2::engine::{
        validate_engine_result, EngineOperation, EngineRequest, ImportEngine,
    };
    use crate::services::import_v2::platform_provider::{Platform, PlatformDocument};
    use crate::services::import_v2::redaction::redact_sensitive_text;
    use crate::services::import_v2::url_policy::UrlPolicy;
    use crate::services::import_v2::web_fetch::WebFetchArtifact;
    use crate::services::import_v2::web_target_store::WebTargetStore;
    use crate::services::SecretService;
    use crate::tasks::task_model::CancellationToken;
    use std::sync::Arc;

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
            cover_url: None,
            subtitles: Vec::new(),
            chapters: Vec::new(),
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
            local_ocr_authorized: false,
            allow_missing_transcript: false,
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
                cover_url: None,
                subtitles: Vec::new(),
                chapters: Vec::new(),
            },
        };

        let (selected, api_is_source) =
            select_primary_web_artifact(Ok(page), Some(&api), None, &request).unwrap();

        assert!(api_is_source);
        assert_eq!(selected.bytes, api.source_body.as_bytes());
        assert_eq!(selected.content_type, "application/json");
    }

    #[test]
    #[ignore = "requires the public Bilibili API"]
    fn public_bilibili_engine_result_obeys_the_staging_contract() {
        let root = std::env::temp_dir().join(format!("bilibili-engine-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".app")).unwrap();
        let url = std::env::var("LLM_WIKI_BILIBILI_TEST_URL")
            .unwrap_or_else(|_| "https://www.bilibili.com/video/BV1N7411A7WU/".into());
        let target = UrlPolicy::default().normalize_for_session(&url).unwrap();
        let targets = Arc::new(WebTargetStore::new(SecretService::memory()));
        let reference = targets.store(&target).unwrap();
        let engine = GenericWebEngine::new(targets, "builtin.web-bilibili", "web.bilibili.video");
        let staging_root = ".app/import-sessions/session/items/item/staging";
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: "network-fixture".into(),
            project_id: "network-fixture".into(),
            session_id: "session".into(),
            item_id: "item".into(),
            task_id: "task".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "www.bilibili.com".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: MediaSaveMode::ExtractOnly,
            },
            project_root: root.to_string_lossy().into_owned(),
            staging_root: staging_root.into(),
            chained_input: None,
            local_asr_authorized: false,
            local_ocr_authorized: false,
            allow_missing_transcript: true,
            media_save_mode: MediaSaveMode::ExtractOnly,
        };

        let result = engine
            .execute(&request, &CancellationToken::new())
            .expect("metadata-only Bilibili preview should remain available without subtitles");
        assert!(
            validate_engine_result(staging_root, &result).is_ok(),
            "Bilibili result violated staging: assets={:?}, continuation={:?}",
            result.asset_paths,
            result.continuation
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn platform_image_posts_do_not_require_ocr_unless_explicitly_enabled() {
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
            cover_url: None,
            subtitles: Vec::new(),
            chapters: Vec::new(),
        };
        assert!(!should_run_platform_image_ocr(Some(&document), false));
        assert!(should_run_platform_image_ocr(Some(&document), true));
        let mut image_only = document.clone();
        image_only.description.clear();
        assert!(!platform_image_output_is_meaningful(&image_only, 0));
        assert!(platform_image_output_is_meaningful(&image_only, 1));
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
            cover_url: Some("https://sns-webpic-qc.xhscdn.com/1.jpg".into()),
            subtitles: Vec::new(),
            chapters: Vec::new(),
        };
        let markdown = render_platform_markdown(
            &document,
            "web.xiaohongshu.note",
            "builtin.web-xiaohongshu",
            "0.1.0",
        );
        for expected in [
            "type: source",
            "source_platform: \"xiaohongshu\"",
            "source_id: \"note-1\"",
            "title_source: \"inferred\"",
            "## 原始正文",
            "## 话题",
            "## 图片",
            "标题来源：由原始正文首行推断",
        ] {
            assert!(markdown.contains(expected), "missing {expected}");
        }
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

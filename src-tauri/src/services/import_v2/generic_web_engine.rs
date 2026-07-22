use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode};
use crate::services::import_v2::bilibili;
use crate::services::import_v2::connectors::wechat;
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
use crate::services::import_v2::subtitle::render_subtitle_markdown;
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
    content_kind: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at: Option<&'a str>,
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
        let bilibili_api =
            if self.route == "web.bilibili.video" && platform == Some(Platform::Bilibili) {
                match bilibili::fetch(request, cancellation) {
                    Ok(result) => result,
                    Err(error) if error.code == "IMPORT_V2_CANCELLED" => return Err(error),
                    Err(_) => None,
                }
            } else {
                None
            };
        let (artifact, api_is_source) = match (page_artifact, bilibili_api.as_ref()) {
            (Ok(artifact), _) => (artifact, false),
            (Err(_error), Some(api)) => {
                let target = UrlPolicy::default().normalize_for_session(
                    request
                        .input
                        .normalized_locator
                        .as_deref()
                        .unwrap_or(&request.input.locator),
                )?;
                (
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
                )
            }
            (Err(error), None) => return Err(error),
        };
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
        if bilibili_api.is_none() && is_platform_auth_challenge(request, &body) {
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
        let platform_document = bilibili_api
            .as_ref()
            .map(|api| api.document.clone())
            .or_else(|| {
                platform.and_then(|platform| {
                    extract_platform_document(platform, &body, &artifact.final_public_url)
                })
            });
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
        for (index, image_url) in image_urls.into_iter().enumerate() {
            if let Some(platform) = platform {
                if !is_trusted_platform_asset_url(platform, &image_url) {
                    markdown = markdown.replace(&image_url, "(platform image host not allowed)");
                    warnings.push("Platform image host was not in the verified allowlist.".into());
                    continue;
                }
            }
            if request.media_save_mode == MediaSaveMode::ExtractOnly && !image_ocr_enabled {
                markdown = markdown.replace(&image_url, "(original image not retained)");
                continue;
            }
            match fetch_image(
                &image_url,
                &item_id,
                cancellation,
                &artifact.final_public_url,
            ) {
                Ok(image) => {
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
                        let relative = format!("assets/image-{index}.{extension}");
                        std::fs::create_dir_all(staging.join("assets")).map_err(|_| {
                            unavailable("The original image directory could not be created.")
                        })?;
                        std::fs::write(staging.join(&relative), &image.bytes)
                            .map_err(|_| unavailable("An original image could not be staged."))?;
                        markdown = markdown.replace(&image_url, &relative);
                        asset_paths.push(relative);
                    } else {
                        markdown = markdown.replace(&image_url, "(original image not retained)");
                    }
                }
                Err(error) => {
                    markdown = markdown.replace(&image_url, "(original image unavailable)");
                    warnings.push(format!(
                        "Original image was not localized: {}",
                        error.message
                    ));
                }
            }
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
                        let extension = subtitle_extension(
                            &subtitle_artifact.content_type,
                            &subtitle_artifact.final_public_url,
                        );
                        if let Some(rendered) =
                            render_subtitle_markdown(&subtitle_artifact.bytes, extension)
                        {
                            markdown.push_str("\n\n## 字幕 / 转写\n\n");
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
            && is_bilibili_video_locator(request)
            && media_url.is_none()
        {
            if transcription_ready && request.media_save_mode == MediaSaveMode::ExtractOnly {
                // A verified transcript satisfies extraction-only imports;
                // no media download is needed.
            } else if !transcription_ready && !request.local_asr_authorized {
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
                    markdown = markdown.replace(&media_url, "(platform media host not allowed)");
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
                                markdown =
                                    markdown.replace(&media_url, "(original media unavailable)");
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
                        markdown = markdown.replace(&media_url, "(original media not retained)");
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
            content_kind: platform_document
                .as_ref()
                .map(|document| document.content_type.as_str()),
            author: platform_document
                .as_ref()
                .and_then(|document| document.author.as_deref()),
            published_at: platform_document
                .as_ref()
                .and_then(|document| document.published_at.as_deref()),
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
    markdown.push_str("## 来源信息\n\n");
    markdown.push_str(&format!("- 平台：{}\n", document.platform));
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
    if !document.description.trim().is_empty() {
        markdown.push_str("\n## 原始描述\n\n");
        markdown.push_str(document.description.trim());
        markdown.push('\n');
    }
    if !document.hashtags.is_empty() {
        markdown.push_str("\n## 话题\n\n");
        markdown.push_str(&document.hashtags.join(" "));
        markdown.push('\n');
    }
    if !document.images.is_empty() {
        markdown.push_str("\n## 图片\n\n");
        for (index, image) in document.images.iter().enumerate() {
            markdown.push_str(&format!("{}. ![第 {} 张]({image})\n", index + 1, index + 1));
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
        content_kind: None,
        author: None,
        published_at: None,
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
        content_kind: Some("image"),
        author: None,
        published_at: None,
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
        extract_html_image_urls, extract_html_media_url, extract_html_title,
        is_trusted_platform_asset_url, media_extension, should_run_platform_image_ocr,
    };
    use crate::services::import_v2::platform_provider::{Platform, PlatformDocument};
    use crate::services::import_v2::redaction::redact_sensitive_text;

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
    fn platform_image_posts_do_not_require_ocr_unless_explicitly_enabled() {
        let document = PlatformDocument {
            platform: "xiaohongshu".into(),
            platform_id: Some("note-1".into()),
            content_type: "image_post".into(),
            canonical_url: "https://www.xiaohongshu.com/explore/note-1".into(),
            title: "Image post".into(),
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
    }
}

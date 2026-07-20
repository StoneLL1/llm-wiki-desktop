use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::services::import_v2::connectors::{self, wechat::is_wechat_target};
use crate::services::import_v2::engine::{
    EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::markdown_normalizer::{decode_text, html_to_markdown};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{WebFetchContent, WebFetchPolicy, WebFetchService};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;

const MAX_WECHAT_IMAGES: usize = 32;
const MAX_WECHAT_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct WechatWebEngine {
    web_targets: Arc<WebTargetStore>,
}

impl WechatWebEngine {
    pub const fn new(web_targets: Arc<WebTargetStore>) -> Self {
        Self { web_targets }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WechatMetadata<'a> {
    engine_id: &'a str,
    engine_version: &'a str,
    route: &'a str,
    title: &'a str,
    author: Option<&'a str>,
    published_at: Option<&'a str>,
    public_url: &'a str,
    images: Vec<&'a str>,
}

impl ImportEngine for WechatWebEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "builtin.web-wechat".into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            route: "web.wechat.article".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        if input.kind != ImportInputKind::Url {
            return false;
        }
        is_wechat_target(
            input
                .normalized_locator
                .as_deref()
                .unwrap_or(&input.locator),
        )
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
                "The WeChat engine supports mp.weixin.qq.com URLs only.",
            ));
        }
        let target = self.web_targets.resolve(
            &request.input.locator,
            request.input.normalized_locator.as_deref(),
        )?;
        let item_id = request.item_id.clone();
        let token = cancellation.clone();
        let artifact = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| unavailable("The WeChat fetch runtime could not be started."))?;
            runtime.block_on(WebFetchService::default().fetch(
                target,
                &UrlPolicy::default(),
                &WebFetchPolicy::default(),
                None,
                &item_id,
                |_| {},
                || token.is_cancelled(),
            ))
        })
        .join()
        .map_err(|_| unavailable("The WeChat fetch worker stopped unexpectedly."))??;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }

        let body = decode_text(&artifact.bytes)?;
        let document = connectors::wechat::extract(&body, &artifact.final_public_url)
            .map_err(connector_error)?;
        let (markdown_body, mut warnings) = html_to_markdown(&document.body_html);
        if markdown_body.trim().is_empty() {
            return Err(BackendError::new(
                "IMPORT_WEB_STRUCTURE_CHANGED",
                "The WeChat article body was empty after extraction.",
                true,
                true,
            ));
        }
        let markdown = format!("# {}\n\n{}", document.title, markdown_body.trim());
        let safe_snapshot = format!(
            "<article><h1>{}</h1>{}</article>",
            document.title, document.body_html
        );
        let staging = resolve_inside(Path::new(&request.project_root), &request.staging_root)?;
        std::fs::create_dir_all(&staging)
            .map_err(|_| unavailable("The WeChat staging directory could not be created."))?;
        let descriptor = self.descriptor();
        let image_urls = document
            .image_requests
            .iter()
            .map(|image| image.public_url.as_str())
            .collect::<Vec<_>>();
        let metadata = WechatMetadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            title: &document.title,
            author: document.author.as_deref(),
            published_at: document.published_at.as_deref(),
            public_url: &document.public_url,
            images: image_urls.clone(),
        };
        warnings.push("WECHAT_SPECIALIZED_EXTRACTOR".into());
        let written = std::fs::write(staging.join("source.html"), safe_snapshot.as_bytes())
            .and_then(|_| std::fs::write(staging.join("document.md"), markdown.as_bytes()))
            .and_then(|_| {
                serde_json::to_vec_pretty(&metadata)
                    .map_err(std::io::Error::other)
                    .and_then(|bytes| std::fs::write(staging.join("metadata.json"), bytes))
            });
        if written.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(unavailable(
                "The WeChat engine could not write item staging.",
            ));
        }
        let image_count = document.image_requests.len();
        let image_requests = document.image_requests.clone();
        let image_staging = staging.clone();
        let item_id = request.item_id.clone();
        let image_token = cancellation.clone();
        let localized = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Ok(runtime) => localize_wechat_images(
                    &runtime,
                    &image_staging,
                    &item_id,
                    &image_requests,
                    &image_token,
                ),
                Err(_) => Ok(ImageLocalizationSummary {
                    failed: image_requests.len(),
                    ..ImageLocalizationSummary::default()
                }),
            }
        })
        .join()
        .map_err(|_| unavailable("The WeChat image worker stopped unexpectedly."))??;
        if localized.failed > 0 || localized.skipped > 0 {
            warnings.push("WECHAT_IMAGE_DOWNLOAD_PARTIAL".into());
        }
        let meaningful_image_coverage = if image_count > 0 {
            Some(localized.downloaded as f64 / image_count as f64)
        } else {
            None
        };
        let asset_paths = localized.asset_paths;
        Ok(EngineResult {
            source_snapshot_path: "source.html".into(),
            markdown_path: "document.md".into(),
            asset_paths,
            metadata_path: Some("metadata.json".into()),
            title: document.title,
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage,
            continuation: None,
            warnings,
        })
    }
}

#[derive(Default)]
struct ImageLocalizationSummary {
    asset_paths: Vec<String>,
    downloaded: usize,
    failed: usize,
    skipped: usize,
}

fn localize_wechat_images(
    runtime: &tokio::runtime::Runtime,
    staging: &Path,
    item_id: &str,
    requests: &[connectors::ImageRequest],
    cancellation: &CancellationToken,
) -> Result<ImageLocalizationSummary, BackendError> {
    if requests.is_empty() {
        return Ok(ImageLocalizationSummary::default());
    }
    let markdown_path = staging.join("document.md");
    let source_path = staging.join("source.html");
    let mut markdown = std::fs::read_to_string(&markdown_path)
        .map_err(|_| unavailable("The WeChat candidate could not be reopened."))?;
    let mut source = std::fs::read_to_string(&source_path)
        .map_err(|_| unavailable("The WeChat snapshot could not be reopened."))?;
    let mut summary = ImageLocalizationSummary {
        skipped: requests.len().saturating_sub(MAX_WECHAT_IMAGES),
        ..ImageLocalizationSummary::default()
    };

    for (index, image) in requests.iter().take(MAX_WECHAT_IMAGES).enumerate() {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let target = match UrlPolicy.normalize_for_session(&image.request_url) {
            Ok(target) => target,
            Err(_) => {
                summary.failed += 1;
                continue;
            }
        };
        let mut policy = WebFetchPolicy::default();
        policy.content = WebFetchContent::Image;
        policy.max_response_bytes = MAX_WECHAT_IMAGE_BYTES;
        let fetched = runtime.block_on(WebFetchService::default().fetch(
            target,
            &UrlPolicy::default(),
            &policy,
            None,
            item_id,
            |_| {},
            || cancellation.is_cancelled(),
        ));
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(_) if cancellation.is_cancelled() => return Err(cancelled()),
            Err(_) => {
                summary.failed += 1;
                continue;
            }
        };
        let Some(extension) = image_extension(&fetched.content_type) else {
            summary.failed += 1;
            continue;
        };
        let relative = format!("assets/wechat-{index}.{extension}");
        let destination = staging.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| unavailable("The WeChat asset directory could not be created."))?;
        }
        std::fs::write(&destination, fetched.bytes)
            .map_err(|_| unavailable("A WeChat image could not be written."))?;
        markdown = replace_image_reference(&markdown, &image.public_url, &relative);
        source = replace_image_reference(&source, &image.public_url, &relative);
        summary.asset_paths.push(relative);
        summary.downloaded += 1;
    }

    std::fs::write(markdown_path, markdown)
        .map_err(|_| unavailable("The localized WeChat candidate could not be written."))?;
    std::fs::write(source_path, source)
        .map_err(|_| unavailable("The localized WeChat snapshot could not be written."))?;
    Ok(summary)
}

fn replace_image_reference(content: &str, public_url: &str, relative: &str) -> String {
    content.replace(public_url, relative)
}

fn image_extension(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn connector_error(failure: connectors::ConnectorFailure) -> BackendError {
    match failure {
        connectors::ConnectorFailure::Challenge => BackendError::new(
            "IMPORT_WEB_CHALLENGE_DETECTED",
            "WeChat returned a verification page. Complete verification and retry.",
            false,
            true,
        ),
        connectors::ConnectorFailure::Captcha => BackendError::new(
            "IMPORT_WEB_CAPTCHA_REQUIRED",
            "WeChat requires a verification step before the article can be read.",
            false,
            true,
        ),
        connectors::ConnectorFailure::LoginRequired => BackendError::new(
            "IMPORT_WEB_LOGIN_REQUIRED",
            "WeChat authentication is required before the article can be read.",
            false,
            true,
        ),
        connectors::ConnectorFailure::Removed => BackendError::new(
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "The WeChat article is unavailable or has been removed.",
            false,
            true,
        ),
        connectors::ConnectorFailure::EmptyBody => BackendError::new(
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "The WeChat article did not contain readable body content.",
            true,
            true,
        ),
        connectors::ConnectorFailure::StructureChanged => BackendError::new(
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "The WeChat article structure could not be recognized.",
            true,
            true,
        ),
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
            "The WeChat staging path is outside the project root.",
        ));
    }
    Ok(path)
}

fn cancelled() -> BackendError {
    BackendError::new(
        "IMPORT_V2_CANCELLED",
        "WeChat import was cancelled.",
        true,
        false,
    )
}

fn unavailable(message: &'static str) -> BackendError {
    BackendError::new("IMPORT_V2_ENGINE_UNAVAILABLE", message, true, true)
}

#[cfg(test)]
mod tests {
    use super::{image_extension, replace_image_reference};

    #[test]
    fn image_extension_accepts_common_types_and_parameters() {
        assert_eq!(image_extension("image/jpeg; charset=binary"), Some("jpg"));
        assert_eq!(image_extension("image/webp"), Some("webp"));
        assert_eq!(image_extension("image/svg+xml"), None);
    }

    #[test]
    fn image_reference_replacement_updates_markdown_and_snapshot_content() {
        let url = "https://mmbiz.qpic.cn/image.jpg?signature=redacted";
        let content = format!("![cover]({url})\n<img src=\"{url}\">");
        let localized = replace_image_reference(&content, url, "assets/wechat-0.jpg");
        assert!(!localized.contains(url));
        assert_eq!(localized.matches("assets/wechat-0.jpg").count(), 2);
    }
}

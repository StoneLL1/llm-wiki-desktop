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
use crate::services::import_v2::web_fetch::{WebFetchPolicy, WebFetchService};
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::tasks::task_model::CancellationToken;

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
        let has_images = !image_urls.is_empty();
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
        Ok(EngineResult {
            source_snapshot_path: "source.html".into(),
            markdown_path: "document.md".into(),
            asset_paths: Vec::new(),
            metadata_path: Some("metadata.json".into()),
            title: document.title,
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: has_images.then_some(1.0),
            continuation: None,
            warnings,
        })
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

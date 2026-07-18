use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::services::import_v2::connectors::wechat;
use crate::services::import_v2::engine::{
    EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::markdown_normalizer::{
    decode_text, html_to_markdown, normalize_markdown,
};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{WebFetchPolicy, WebFetchService};
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
        input.kind == ImportInputKind::Url && !is_bilibili_target(input)
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
        let token = cancellation.clone();
        let artifact = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| unavailable("The web fetch runtime could not be started."))?;
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
        .map_err(|_| unavailable("The web fetch worker stopped unexpectedly."))??;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let body = decode_text(&artifact.bytes)?;
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
        let (markdown, warnings) = if artifact.content_type.contains("html") {
            html_to_markdown(&body)
        } else {
            (normalize_markdown(&body), Vec::new())
        };
        if markdown.trim().is_empty() {
            return Err(unavailable(
                "The web response did not contain readable text.",
            ));
        }
        let staging = resolve_inside(Path::new(&request.project_root), &request.staging_root)?;
        std::fs::create_dir_all(&staging)
            .map_err(|_| unavailable("The web item staging directory could not be created."))?;
        let descriptor = self.descriptor();
        let metadata = WebMetadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            final_public_url: &artifact.final_public_url,
            content_type: &artifact.content_type,
            redirect_count: artifact.redirects.len(),
            warnings: &warnings,
        };
        let written = std::fs::write(staging.join("source.bin"), &artifact.bytes)
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
            asset_paths: Vec::new(),
            metadata_path: Some("metadata.json".into()),
            title: request.input.display_name.clone(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
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

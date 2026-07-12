use crate::errors::{BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_OUTPUT_INVALID};
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::services::import_v2::engine::{
    EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::markdown_normalizer::{
    csv_to_gfm, decode_utf8, html_to_markdown, normalize_markdown,
};
use crate::tasks::task_model::CancellationToken;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Default)]
pub struct NativeFileEngine;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata<'a> {
    engine_id: &'a str,
    engine_version: &'a str,
    route: &'a str,
    source_name: &'a str,
    warnings: &'a [String],
}

impl ImportEngine for NativeFileEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "builtin.native-file".into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            route: "file.native".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
            && extension(&input.locator).is_some_and(|ext| {
                matches!(
                    ext.as_str(),
                    "md" | "markdown" | "txt" | "csv" | "html" | "htm"
                )
            })
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
            return Err(invalid(
                "The native file engine does not support this input.",
            ));
        }
        let root = PathBuf::from(&request.project_root);
        let source = resolve_source(&root, &request.input.locator)?;
        let staging = resolve_inside(&root, &request.staging_root)?;
        let bytes =
            std::fs::read(&source).map_err(|_| invalid("The source file could not be read."))?;
        let text = decode_utf8(&bytes)?;
        let format = extension(&request.input.locator).unwrap_or_default();
        let (markdown, warnings) = match format.as_str() {
            "csv" => (normalize_markdown(&csv_to_gfm(text)?), Vec::new()),
            "html" | "htm" => html_to_markdown(text),
            _ => (normalize_markdown(text), Vec::new()),
        };
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        std::fs::create_dir_all(&staging)
            .map_err(|_| invalid("The item staging directory could not be created."))?;
        let descriptor = self.descriptor();
        let metadata = Metadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            source_name: &request.input.display_name,
            warnings: &warnings,
        };
        let asset_paths = copy_local_images(&markdown, source.parent().unwrap_or(&root), &staging)?;
        let written = std::fs::write(staging.join("source.bin"), &bytes)
            .and_then(|_| std::fs::write(staging.join("document.md"), markdown.as_bytes()))
            .and_then(|_| {
                serde_json::to_vec_pretty(&metadata)
                    .map_err(std::io::Error::other)
                    .and_then(|bytes| std::fs::write(staging.join("metadata.json"), bytes))
            });
        if written.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(invalid("The native engine could not write item staging."));
        }
        if cancellation.is_cancelled() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(cancelled());
        }
        Ok(EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "document.md".into(),
            asset_paths,
            metadata_path: Some("metadata.json".into()),
            title: Path::new(&request.input.display_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            text_coverage: Some(1.0),
            table_cell_accuracy: (format == "csv").then_some(1.0),
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
        return Err(invalid(
            "The native engine path is outside the authorized project root.",
        ));
    }
    Ok(path)
}
fn resolve_source(root: &Path, locator: &str) -> Result<PathBuf, BackendError> {
    let candidate = Path::new(locator);
    if candidate.is_absolute() {
        return Ok(candidate.to_path_buf());
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid("The native engine source path is invalid."));
    }
    Ok(root.join(candidate))
}
fn extension(locator: &str) -> Option<String> {
    Path::new(locator)
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
}
fn copy_local_images(
    markdown: &str,
    root: &Path,
    staging: &Path,
) -> Result<Vec<String>, BackendError> {
    let mut paths = Vec::new();
    let mut rest = markdown;
    while let Some(start) = rest.find("![") {
        rest = &rest[start + 2..];
        let Some(label_end) = rest.find("](") else {
            continue;
        };
        rest = &rest[label_end + 2..];
        let Some(end) = rest.find(')') else { break };
        let destination = rest[..end]
            .split_ascii_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(['<', '>']);
        rest = &rest[end + 1..];
        if destination.is_empty()
            || destination.contains(':')
            || destination.starts_with('/')
            || destination.starts_with('\\')
        {
            continue;
        }
        let relative = Path::new(destination.split('#').next().unwrap_or(""));
        if relative
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(invalid(
                "A Markdown image path escapes its authorized source directory.",
            ));
        }
        let source = root.join(relative);
        if !source.is_file() {
            continue;
        }
        let target = staging.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| invalid("A Markdown image could not be staged."))?;
        }
        std::fs::copy(source, target)
            .map_err(|_| invalid("A Markdown image could not be staged."))?;
        paths.push(relative.to_string_lossy().replace('\\', "/"));
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}
fn invalid(message: &'static str) -> BackendError {
    BackendError::new(IMPORT_V2_ENGINE_OUTPUT_INVALID, message, false, true)
}
fn cancelled() -> BackendError {
    BackendError::new(
        IMPORT_V2_CANCELLED,
        "The import engine was cancelled.",
        true,
        false,
    )
}

use std::collections::HashSet;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::compile::{CompileConsumptionRecord, SourceVersionRef};
use crate::models::paths::ProjectContext;
use crate::services::FileStore;
use serde::Deserialize;

/// Read-only compatibility boundary for projects that predate Import V2.
/// No main-path Compile code may parse or mutate `.app/source-index.json`.
pub struct LegacyCompileSource {
    pub reference: SourceVersionRef,
    pub project_path: String,
    pub absolute_path: PathBuf,
    pub already_consumed: bool,
}

pub struct LegacyCompileDiagnostics {
    pub confirmed_sources: Vec<String>,
    pub markdown_paths: Vec<String>,
    pub empty_markdown_paths: Vec<String>,
}

pub struct CompileLegacyAdapter;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceArtifactIndex {
    #[serde(default)]
    sources: std::collections::HashMap<String, Vec<String>>,
}

impl CompileLegacyAdapter {
    pub fn exists(context: &ProjectContext) -> bool {
        context.app_dir.join("source-index.json").is_file()
    }

    pub fn list(context: &ProjectContext) -> Result<Vec<LegacyCompileSource>, BackendError> {
        let index_path = context.app_dir.join("source-index.json");
        if !index_path.is_file() {
            return Ok(Vec::new());
        }
        let index = FileStore.read_json_file::<SourceArtifactIndex>(&index_path)?;
        let mut seen = HashSet::new();
        let mut sources = Vec::new();
        let mut entries: Vec<_> = index.sources.into_iter().collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        for (source_key, mut artifacts) in entries {
            artifacts.sort();
            for project_path in artifacts {
                let normalized = project_path.replace('\\', "/");
                if !(normalized.starts_with("raw/extracted/")
                    || normalized.starts_with("wiki/sources/"))
                    || !normalized.ends_with(".md")
                    || !seen.insert(normalized.clone())
                {
                    continue;
                }
                let absolute_path = context.resolve_project_path(&normalized)?;
                let bytes = std::fs::read(&absolute_path).map_err(|error| {
                    BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
                })?;
                if String::from_utf8_lossy(&bytes).trim().is_empty() {
                    continue;
                }
                let content_hash = format!("{:x}", Sha256::digest(&bytes));
                let source_hash = format!("{:x}", Sha256::digest(source_key.as_bytes()));
                sources.push(LegacyCompileSource {
                    reference: SourceVersionRef {
                        source_id: format!("legacy-{}", &source_hash[..16]),
                        version_id: format!("legacy-{}", &content_hash[..16]),
                        content_hash,
                    },
                    project_path: normalized,
                    absolute_path,
                    already_consumed: false,
                });
            }
        }
        let promoted_root = context.wiki_dir.join("sources");
        for absolute_path in FileStore.list_markdown_files(&promoted_root)? {
            let project_path = context.to_project_relative(&absolute_path)?;
            if !seen.insert(project_path.clone()) {
                continue;
            }
            let bytes = std::fs::read(&absolute_path).map_err(|error| {
                BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
            })?;
            if String::from_utf8_lossy(&bytes).trim().is_empty() {
                continue;
            }
            let content_hash = format!("{:x}", Sha256::digest(&bytes));
            let source_hash = format!("{:x}", Sha256::digest(project_path.as_bytes()));
            sources.push(LegacyCompileSource {
                reference: SourceVersionRef {
                    source_id: format!("legacy-{}", &source_hash[..16]),
                    version_id: format!("legacy-{}", &content_hash[..16]),
                    content_hash,
                },
                project_path,
                absolute_path,
                already_consumed: false,
            });
        }
        let consumed_versions = Self::consumed_versions(context)?;
        for source in &mut sources {
            source.already_consumed = consumed_versions.contains(&source.reference);
        }
        sources.sort_by(|left, right| left.project_path.cmp(&right.project_path));
        Ok(sources)
    }

    pub fn consumed_versions(
        context: &ProjectContext,
    ) -> Result<HashSet<SourceVersionRef>, BackendError> {
        let compile_root = context.app_dir.join("compile");
        if !compile_root.is_dir() {
            return Ok(HashSet::new());
        }
        let mut paths = std::fs::read_dir(&compile_root)
            .map_err(|error| compile_consumption_error(error.to_string()))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| compile_consumption_error(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        let mut consumed = HashSet::new();
        for path in paths {
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| compile_consumption_error(error.to_string()))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(compile_consumption_error(
                    "A Compile consumption record is not a regular file.",
                ));
            }
            let record = FileStore
                .read_json_file::<CompileConsumptionRecord>(&path)
                .map_err(|error| compile_consumption_error(error.message))?;
            if record.schema_version != 1 || record.compile_task_id.trim().is_empty() {
                return Err(compile_consumption_error(
                    "A Compile consumption record has an unsupported schema.",
                ));
            }
            consumed.extend(
                record
                    .source_versions
                    .into_iter()
                    .filter(|reference| reference.source_id.starts_with("legacy-")),
            );
        }
        Ok(consumed)
    }

    pub fn diagnostics(context: &ProjectContext) -> Result<LegacyCompileDiagnostics, BackendError> {
        let index_path = context.app_dir.join("source-index.json");
        if !index_path.is_file() {
            return Ok(LegacyCompileDiagnostics {
                confirmed_sources: Vec::new(),
                markdown_paths: Vec::new(),
                empty_markdown_paths: Vec::new(),
            });
        }
        let index = FileStore.read_json_file::<SourceArtifactIndex>(&index_path)?;
        let mut confirmed_sources = index.sources.keys().cloned().collect::<Vec<_>>();
        confirmed_sources.sort();
        let mut markdown_paths = index
            .sources
            .values()
            .flatten()
            .filter(|path| {
                (path.starts_with("raw/extracted/") || path.starts_with("wiki/sources/"))
                    && path.ends_with(".md")
            })
            .cloned()
            .collect::<Vec<_>>();
        markdown_paths.sort();
        markdown_paths.dedup();
        let mut empty_markdown_paths = Vec::new();
        for relative in &markdown_paths {
            let absolute = context.resolve_project_path(relative)?;
            let content = std::fs::read_to_string(&absolute).map_err(|error| {
                BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
            })?;
            if content.trim().is_empty() {
                empty_markdown_paths.push(relative.clone());
            }
        }
        Ok(LegacyCompileDiagnostics {
            confirmed_sources,
            markdown_paths,
            empty_markdown_paths,
        })
    }

    pub fn resolve(
        context: &ProjectContext,
        requested: &[SourceVersionRef],
    ) -> Result<Vec<LegacyCompileSource>, BackendError> {
        let available = Self::list(context)?;
        let mut resolved = Vec::with_capacity(requested.len());
        for reference in requested {
            let source = available
                .iter()
                .find(|source| &source.reference == reference)
                .ok_or_else(|| {
                    BackendError::new(
                        "COMPILE_SOURCE_VERSION_INVALID",
                        "A selected legacy Source version is missing or has changed.",
                        true,
                        true,
                    )
                })?;
            resolved.push(LegacyCompileSource {
                reference: source.reference.clone(),
                project_path: source.project_path.clone(),
                absolute_path: source.absolute_path.clone(),
                already_consumed: source.already_consumed,
            });
        }
        Ok(resolved)
    }
}

fn compile_consumption_error(message: impl Into<String>) -> BackendError {
    BackendError::new("COMPILE_CONSUMPTION_INVALID", message.into(), true, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::compile::{CompileConsumptionRecord, CompileRoute};
    use std::fs;

    #[test]
    fn legacy_adapter_is_read_only_and_resolves_only_indexed_markdown() {
        let root =
            std::env::temp_dir().join(format!("compile-legacy-adapter-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::write(root.join("raw/extracted/资料.md"), "# 已确认").unwrap();
        fs::write(root.join("raw/extracted/orphan.md"), "# 未确认").unwrap();
        let index_path = root.join(".app/source-index.json");
        fs::write(
            &index_path,
            r#"{"sources":{"raw/sources/资料.txt":["raw/extracted/资料.md"]}}"#,
        )
        .unwrap();
        let before = fs::read(&index_path).unwrap();
        let before_metadata = fs::metadata(&index_path).unwrap();
        let context = ProjectContext::new("legacy-project", root.clone());

        let listed = CompileLegacyAdapter::list(&context).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].project_path, "raw/extracted/资料.md");
        let resolved =
            CompileLegacyAdapter::resolve(&context, std::slice::from_ref(&listed[0].reference))
                .unwrap();

        assert_eq!(resolved.len(), 1);
        assert!(!resolved[0].already_consumed);
        fs::create_dir_all(root.join(".app/compile")).unwrap();
        fs::write(
            root.join(".app/compile/task-1.json"),
            serde_json::to_vec_pretty(&CompileConsumptionRecord {
                schema_version: 1,
                compile_task_id: "task-1".into(),
                route: CompileRoute::Byok,
                consumed_at: "2026-07-26T00:00:00Z".into(),
                source_versions: vec![listed[0].reference.clone()],
                affected_paths: vec!["wiki/index.md".into()],
                checkpoint: None,
            })
            .unwrap(),
        )
        .unwrap();
        let repeated =
            CompileLegacyAdapter::resolve(&context, std::slice::from_ref(&listed[0].reference))
                .unwrap();
        assert!(repeated[0].already_consumed);
        assert_eq!(resolved[0].project_path, "raw/extracted/资料.md");
        assert_eq!(fs::read(&index_path).unwrap(), before);
        let after_metadata = fs::metadata(&index_path).unwrap();
        assert_eq!(after_metadata.len(), before_metadata.len());
        assert_eq!(
            after_metadata.modified().unwrap(),
            before_metadata.modified().unwrap()
        );
        assert!(!root.join("wiki").exists());
        fs::remove_dir_all(root).ok();
    }
}

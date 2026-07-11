use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::import::{
    ConflictResolution, ImportPreview, ImportedSource, SourceArtifactIndex,
};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

use super::classify_file;
use super::preview::{collect_source_files, file_hash_fast};

impl super::ImportService {
    pub fn validate_imported_source_path(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<PathBuf, BackendError> {
        let normalized = relative_path.replace('\\', "/");
        if !(normalized.starts_with("raw/sources/") || normalized.starts_with("raw/assets/")) {
            return Err(BackendError::new(
                "SOURCE_PATH_OUT_OF_SCOPE",
                "Source operations are limited to raw/sources and raw/assets.",
                true,
                true,
            ));
        }
        let path = context.resolve_project_path(&normalized)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            BackendError::new("SOURCE_NOT_FOUND", error.to_string(), true, true)
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(BackendError::new(
                "SOURCE_PATH_INVALID",
                "The source path must be a regular file and cannot be a symlink.",
                true,
                true,
            ));
        }
        Ok(path)
    }

    pub fn list_imported_sources(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<ImportedSource>, BackendError> {
        let mut paths = Vec::new();
        for root in [
            context.raw_dir.join("sources"),
            context.raw_dir.join("assets"),
        ] {
            if root.exists() {
                collect_source_files(&root, &mut paths)?;
            }
        }
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let relative = context.to_project_relative(&path)?;
                Ok(ImportedSource {
                    size_bytes: path.metadata().map(|value| value.len()).unwrap_or(0),
                    file_type: classify_file(&path),
                    path: relative,
                })
            })
            .collect()
    }

    pub fn hash_external_file(&self, path: &Path) -> Result<String, BackendError> {
        file_hash_fast(path)
    }

    pub fn read_source_index(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
    ) -> Result<SourceArtifactIndex, BackendError> {
        let path = context.app_dir.join("source-index.json");
        if !path.exists() {
            return Ok(SourceArtifactIndex::default());
        }
        file_store.read_json(context, ".app/source-index.json")
    }

    pub fn record_confirmed_sources(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        preview: &ImportPreview,
    ) -> Result<(), BackendError> {
        let mut index = self.read_source_index(context, file_store)?;
        for entry in &preview.files {
            if matches!(
                entry
                    .conflict
                    .as_ref()
                    .and_then(|conflict| conflict.resolution.as_ref()),
                Some(ConflictResolution::Skip | ConflictResolution::LinkToExisting)
            ) {
                continue;
            }
            let mut artifacts = entry.extracted_assets.clone();
            if let Some(path) = entry.extracted_text_path.clone() {
                artifacts.push(path);
            }
            artifacts.sort();
            artifacts.dedup();
            index.sources.insert(entry.archived_path.clone(), artifacts);
        }
        file_store.write_json_atomic(context, ".app/source-index.json", &index)
    }

    pub fn collect_source_paths(
        &self,
        source_paths: &[String],
    ) -> Result<Vec<PathBuf>, BackendError> {
        let mut files = Vec::new();
        for source_path in source_paths {
            collect_source_files(Path::new(source_path), &mut files)?;
        }
        Ok(files)
    }

    pub fn stage_text_source(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        source_name: &str,
        extension: &str,
        content: &str,
    ) -> Result<PathBuf, BackendError> {
        if content.trim().is_empty() {
            return Err(BackendError::new(
                "IMPORT_TEXT_EMPTY",
                "Clipboard or URL content cannot be empty.",
                true,
                true,
            ));
        }
        if content.len() > 5 * 1024 * 1024 {
            return Err(BackendError::new(
                "IMPORT_TEXT_TOO_LARGE",
                "Clipboard or extracted URL content exceeds the 5 MB limit.",
                true,
                false,
            ));
        }
        if !matches!(extension, "md" | "url") {
            return Err(BackendError::new(
                "IMPORT_STAGING_TYPE_INVALID",
                "Only Markdown clipboard and URL staging files are supported.",
                false,
                true,
            ));
        }

        let safe_stem: String = source_name
            .chars()
            .filter_map(|ch| {
                if ch.is_alphanumeric() || matches!(ch, '-' | '_') {
                    Some(ch)
                } else if ch.is_whitespace() {
                    Some('-')
                } else {
                    None
                }
            })
            .take(80)
            .collect();
        let safe_stem = safe_stem.trim_matches('-');
        let safe_stem = if safe_stem.is_empty() {
            "import"
        } else {
            safe_stem
        };
        let staging_dir = context.app_dir.join("import-staging");
        file_store.ensure_absolute_dir(&staging_dir)?;
        let path = staging_dir.join(format!(
            "{}-{}.{}",
            safe_stem,
            uuid::Uuid::new_v4(),
            extension
        ));
        file_store.write_text_absolute(&path, content)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::services::FileStore;

    use super::super::{test_support::tmp_context, ImportService};

    #[test]
    fn staged_text_sources_are_backend_named_and_project_scoped() {
        let (context, root) = tmp_context("staged-text");
        let staged = ImportService
            .stage_text_source(
                &context,
                &FileStore,
                "../../危险标题",
                "md",
                "# Clipboard\n\ncontent",
            )
            .unwrap();

        assert!(staged.starts_with(root.join(".app/import-staging")));
        assert_eq!(
            staged.extension().and_then(|value| value.to_str()),
            Some("md")
        );
        assert_eq!(
            fs::read_to_string(&staged).unwrap(),
            "# Clipboard\n\ncontent"
        );
        assert!(!staged.to_string_lossy().contains(".."));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn imported_source_management_is_scoped_to_regular_raw_files() {
        let (context, root) = tmp_context("source-management-scope");
        let source = root.join("raw/sources/markdown/资料.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "# Source").unwrap();

        let service = ImportService;
        let listed = service.list_imported_sources(&context).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "raw/sources/markdown/资料.md");
        assert!(service
            .validate_imported_source_path(&context, "raw/sources/markdown/资料.md")
            .is_ok());
        assert_eq!(
            service
                .validate_imported_source_path(&context, "wiki/index.md")
                .unwrap_err()
                .code,
            "SOURCE_PATH_OUT_OF_SCOPE"
        );
        assert!(service
            .validate_imported_source_path(&context, "raw/sources/../../purpose.md")
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

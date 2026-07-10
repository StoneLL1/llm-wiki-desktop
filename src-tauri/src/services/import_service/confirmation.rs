use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::import::{ConflictResolution, ImportFileEntry, ImportPreview};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

use super::classification::{deterministic_rename, target_archive_dir};
use super::classify_file;
use super::preview::file_hash_fast;
use super::promotion::remap_extracted_paths;
use super::source_actions::remove_project_files;

impl super::ImportService {
    pub fn confirm_import(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        preview: &ImportPreview,
    ) -> Result<(), BackendError> {
        let mut planned = Vec::new();
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

            let source = Path::new(&entry.source_path);
            if !source.is_file() {
                return Err(BackendError::new(
                    "IMPORT_SOURCE_MISSING",
                    "Source file is missing and cannot be archived.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "sourcePath": entry.source_path,
                    "archivedPath": entry.archived_path,
                })));
            }

            self.validate_confirm_entry(entry)?;

            let target = context.resolve_project_path(&entry.archived_path)?;
            if target.exists() {
                return Err(BackendError::new(
                    "IMPORT_ARCHIVE_TARGET_EXISTS",
                    "Archived target already exists and will not be overwritten.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "sourcePath": entry.source_path,
                    "archivedPath": entry.archived_path,
                })));
            }

            planned.push((entry, source.to_path_buf(), target));
        }

        let mut copied = Vec::new();
        for (entry, source, target) in &planned {
            if let Some(parent) = target.parent() {
                if let Err(err) = fs::create_dir_all(parent) {
                    rollback_import_targets(&copied);
                    return Err(BackendError::new(
                        "FILE_DIR_CREATE_FAILED",
                        err.to_string(),
                        true,
                        false,
                    ));
                }
            }
            if let Err(err) = fs::copy(source, target) {
                rollback_import_targets(&copied);
                return Err(BackendError::new(
                    "IMPORT_ARCHIVE_COPY_FAILED",
                    err.to_string(),
                    true,
                    false,
                )
                .with_details(serde_json::json!({
                    "sourcePath": entry.source_path,
                    "archivedPath": entry.archived_path,
                })));
            }
            copied.push(target.clone());
        }

        // Promote each confirmed source's browsable text from raw/extracted/
        // (staging) or the archived .md original into wiki/sources/ so the
        // verbatim original is browsable immediately, without compiling.
        let (promotion_by_archive, promoted_paths, staged_to_delete) =
            match self.promote_extracted_to_sources(context, file_store, preview) {
                Ok(value) => value,
                Err(error) => {
                    rollback_import_targets(&copied);
                    return Err(error);
                }
            };

        let remapped_preview = remap_extracted_paths(preview, &promotion_by_archive);
        if let Err(error) = self.record_confirmed_sources(context, file_store, &remapped_preview) {
            rollback_import_targets(&copied);
            remove_project_files(context, &promoted_paths);
            return Err(error);
        }

        // Promoted content now lives under wiki/sources/; the staged
        // raw/extracted/ copies existed only to back the preview. Archived
        // originals under raw/sources/ are immutable and stay in place.
        for staged in &staged_to_delete {
            if let Ok(absolute) = context.resolve_project_path(staged) {
                let _ = fs::remove_file(absolute);
            }
        }

        let staging_dir = context.app_dir.join("import-staging");
        for (_, source, _) in planned {
            if source.starts_with(&staging_dir) {
                let _ = fs::remove_file(source);
            }
        }

        Ok(())
    }

    fn validate_confirm_entry(&self, entry: &ImportFileEntry) -> Result<(), BackendError> {
        let source = Path::new(&entry.source_path);
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let file_type = classify_file(source);
        let expected_dir = target_archive_dir(&file_type);
        let current_hash = file_hash_fast(source)?;
        let original_target = format!("{}/{}", expected_dir, file_name);
        let renamed_target = format!(
            "{}/{}",
            expected_dir,
            deterministic_rename(file_name, &current_hash)
        );
        let normalized_archived = entry.archived_path.replace('\\', "/");
        let expected_target = if matches!(
            entry
                .conflict
                .as_ref()
                .and_then(|conflict| conflict.resolution.as_ref()),
            Some(ConflictResolution::Rename)
        ) || entry.renamed_from.is_some()
        {
            &renamed_target
        } else {
            &original_target
        };

        if entry.hash != current_hash {
            return Err(BackendError::new(
                "IMPORT_SOURCE_CHANGED",
                "Source file changed after preview; refresh the import preview before confirming.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "sourcePath": entry.source_path,
                "archivedPath": entry.archived_path,
            })));
        }

        if normalized_archived != expected_target.as_str() {
            return Err(BackendError::new(
                "IMPORT_ARCHIVE_PATH_INVALID",
                "Archived path does not match the backend-derived import archive route.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "sourcePath": entry.source_path,
                "archivedPath": entry.archived_path,
                "expectedPath": expected_target,
            })));
        }

        Ok(())
    }
}

fn rollback_import_targets(paths: &[PathBuf]) {
    for path in paths.iter().rev() {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn verify_project_hash(
    file_store: &FileStore,
    context: &ProjectContext,
    path: &str,
    expected: &str,
) -> Result<(), BackendError> {
    if file_store.file_hash(context, path)? != expected {
        return Err(BackendError::new(
            "CONFIRMATION_STATE_MISMATCH",
            "The original source changed after preview.",
            true,
            true,
        ));
    }
    Ok(())
}

pub(super) fn validate_artifact_paths(
    context: &ProjectContext,
    paths: &[String],
) -> Result<(), BackendError> {
    for path in paths {
        let normalized = path.replace('\\', "/");
        if !normalized.starts_with("raw/extracted/") && !normalized.starts_with("wiki/sources/") {
            return Err(BackendError::new(
                "SOURCE_ARTIFACT_PATH_INVALID",
                "Source artifacts must remain under raw/extracted or wiki/sources.",
                false,
                true,
            ));
        }
        context.resolve_project_path(&normalized)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::models::import::*;
    use crate::services::FileStore;

    use super::super::{test_support::tmp_context, ImportService};

    #[test]
    fn confirmed_import_copies_source_files_to_archive_paths() {
        let (context, root) = tmp_context("confirm-archive");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("notes.md");
        fs::write(&source_path, b"# Imported notes").unwrap();

        let request = ImportRequest {
            source_paths: vec![source_path.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        service.confirm_import(&context, &store, &preview).unwrap();

        let archived = root.join("raw/sources/markdown/notes.md");
        assert!(archived.exists());
        assert_eq!(fs::read_to_string(archived).unwrap(), "# Imported notes");

        // The verbatim Markdown original is promoted to wiki/sources/ so it is
        // browsable immediately, and source-index records that promoted path.
        let promoted = root.join("wiki/sources/notes.md");
        assert!(
            promoted.exists(),
            "markdown original must be promoted to wiki/sources/"
        );
        let promoted_body = fs::read_to_string(&promoted).unwrap();
        assert!(promoted_body.starts_with("---\n"));
        assert!(promoted_body.contains("type: source"));
        assert!(promoted_body.contains("# Imported notes"));

        let index = service.read_source_index(&context, &store).unwrap();
        assert_eq!(
            index.sources.get("raw/sources/markdown/notes.md"),
            Some(&vec!["wiki/sources/notes.md".to_string()])
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmed_import_rejects_tampered_archive_paths() {
        let (context, root) = tmp_context("confirm-tamper");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("notes.md");
        fs::write(&source_path, b"# Imported notes").unwrap();

        let request = ImportRequest {
            source_paths: vec![source_path.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let mut preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();
        preview.files[0].archived_path = "wiki/tampered.md".to_string();

        let err = service
            .confirm_import(&context, &store, &preview)
            .expect_err("tampered archive target must be rejected");
        assert_eq!(err.code, "IMPORT_ARCHIVE_PATH_INVALID");
        assert!(!root.join("wiki/tampered.md").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmed_import_rejects_source_changes_after_preview() {
        let (context, root) = tmp_context("confirm-stale");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("notes.md");
        fs::write(&source_path, b"# Before").unwrap();

        let request = ImportRequest {
            source_paths: vec![source_path.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();
        fs::write(&source_path, b"# After").unwrap();

        let err = service
            .confirm_import(&context, &store, &preview)
            .expect_err("changed source must require a refreshed preview");
        assert_eq!(err.code, "IMPORT_SOURCE_CHANGED");
        assert!(!root.join("raw/sources/markdown/notes.md").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmed_import_does_not_partially_archive_when_a_later_source_is_stale() {
        let (context, root) = tmp_context("confirm-atomic-validation");
        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        let first = source_dir.join("first.md");
        let second = source_dir.join("second.md");
        fs::write(&first, "# First").unwrap();
        fs::write(&second, "# Second").unwrap();
        let request = ImportRequest {
            source_paths: vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
            allow_duplicates: false,
            link_duplicates: false,
        };
        let preview = ImportService
            .preview_import(&context, &FileStore, &request, &[])
            .unwrap();
        fs::remove_file(&second).unwrap();

        let error = ImportService
            .confirm_import(&context, &FileStore, &preview)
            .expect_err("the batch must fail before copying any source");

        assert_eq!(error.code, "IMPORT_SOURCE_MISSING");
        assert!(!root.join("raw/sources/markdown/first.md").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_duplicates_can_link_to_existing_without_archiving() {
        let (context, root) = tmp_context("allow-dup");
        let store = FileStore;

        let md_dir = root.join("raw/sources/markdown");
        fs::create_dir_all(&md_dir).unwrap();
        let content = b"# Duplicate";
        fs::write(md_dir.join("existing.md"), content).unwrap();

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("existing.md"), content).unwrap();

        let request = ImportRequest {
            source_paths: vec![source_dir.join("existing.md").to_string_lossy().to_string()],
            allow_duplicates: true,
            link_duplicates: true,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.archived_files, 0);
        assert_eq!(preview.summary.duplicate_files, 1);
        assert_eq!(
            preview.conflicts[0].resolution,
            Some(ConflictResolution::LinkToExisting)
        );
        // existing_path should be populated from scan
        assert!(preview.conflicts[0].existing_path.is_some());

        fs::remove_dir_all(root).unwrap();
    }
}

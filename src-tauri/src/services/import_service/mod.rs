mod classification;
mod confirmation;
mod preview;
mod promotion;
mod source_actions;
mod source_catalog;

#[cfg(test)]
mod test_support;

pub use classification::classify_file;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::git::CheckpointPurpose;
use crate::models::import::{
    ConflictResolution, ConflictType, ExtractResult, ExtractionStatus, ImportConflict,
    ImportFileEntry, ImportPreview, ImportRequest, ImportSummary,
};
use crate::models::paths::ProjectContext;
use crate::services::{file_store::FileStore, GitService};

use classification::{deterministic_rename, target_archive_dir};
use promotion::remap_extracted_paths;

#[derive(Default)]
pub struct ImportService;

pub(super) type FileBackup = Vec<(PathBuf, Option<Vec<u8>>)>;

impl ImportService {
    pub fn cleanup_replacement_artifacts(
        &self,
        context: &ProjectContext,
        old_artifacts: &[String],
        new_artifacts: &[String],
    ) {
        let current: HashSet<&str> = old_artifacts.iter().map(String::as_str).collect();
        let staged_only: Vec<String> = new_artifacts
            .iter()
            .filter(|path| !current.contains(path.as_str()))
            .cloned()
            .collect();
        remove_project_files(context, &staged_only);
    }

    pub fn apply_source_delete(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        target_path: &str,
        target_hash: &str,
        artifacts: &[String],
    ) -> Result<bool, BackendError> {
        self.validate_imported_source_path(context, target_path)?;
        verify_project_hash(file_store, context, target_path, target_hash)?;
        validate_artifact_paths(context, artifacts)?;
        let mut scoped = vec![
            target_path.to_string(),
            ".app/source-index.json".to_string(),
        ];
        scoped.extend(artifacts.iter().cloned());
        let backup = backup_project_files(context, &scoped)?;
        let checkpoint = git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::HighRiskOperation,
            "Before deleting original source",
            &scoped,
        )?;
        let result = (|| {
            fs::remove_file(context.resolve_project_path(target_path)?).map_err(|error| {
                BackendError::new("SOURCE_DELETE_FAILED", error.to_string(), true, false)
            })?;
            remove_project_files(context, artifacts);
            let mut index = self.read_source_index(context, file_store)?;
            index.sources.remove(target_path);
            file_store.write_json_atomic(context, ".app/source-index.json", &index)?;
            git_service.create_scoped_checkpoint(
                context,
                CheckpointPurpose::FinalResult,
                "Delete original source",
                &scoped,
            )?;
            Ok::<(), BackendError>(())
        })();
        if let Err(error) = result {
            restore_project_files(&backup);
            let _ = git_service.unstage_paths(context, &scoped);
            return Err(error);
        }
        Ok(checkpoint.commit_hash.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_source_replace(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        target_path: &str,
        target_hash: &str,
        replacement_path: &Path,
        replacement_hash: &str,
        old_artifacts: &[String],
        new_artifacts: &[String],
    ) -> Result<bool, BackendError> {
        let operation = (|| {
            self.validate_imported_source_path(context, target_path)?;
            verify_project_hash(file_store, context, target_path, target_hash)?;
            if self.hash_external_file(replacement_path)? != replacement_hash {
                return Err(BackendError::new(
                    "CONFIRMATION_STATE_MISMATCH",
                    "The replacement source changed after preview.",
                    true,
                    true,
                ));
            }
            validate_artifact_paths(context, old_artifacts)?;
            validate_artifact_paths(context, new_artifacts)?;
            let mut before_paths = vec![
                target_path.to_string(),
                ".app/source-index.json".to_string(),
            ];
            before_paths.extend(old_artifacts.iter().cloned());
            let mut result_paths = before_paths.clone();
            result_paths.extend(new_artifacts.iter().cloned());
            let backup = backup_project_files(context, &result_paths)?;
            let checkpoint = git_service.create_scoped_checkpoint(
                context,
                CheckpointPurpose::HighRiskOperation,
                "Before replacing original source",
                &before_paths,
            )?;
            let mutation = (|| {
                fs::copy(replacement_path, context.resolve_project_path(target_path)?).map_err(
                    |error| {
                        BackendError::new("SOURCE_REPLACE_FAILED", error.to_string(), true, false)
                    },
                )?;
                let keep: HashSet<&str> = new_artifacts.iter().map(String::as_str).collect();
                let obsolete: Vec<String> = old_artifacts
                    .iter()
                    .filter(|path| !keep.contains(path.as_str()))
                    .cloned()
                    .collect();
                remove_project_files(context, &obsolete);
                let mut index = self.read_source_index(context, file_store)?;
                index
                    .sources
                    .insert(target_path.to_string(), new_artifacts.to_vec());
                file_store.write_json_atomic(context, ".app/source-index.json", &index)?;
                git_service.create_scoped_checkpoint(
                    context,
                    CheckpointPurpose::FinalResult,
                    "Replace original source",
                    &result_paths,
                )?;
                Ok::<(), BackendError>(())
            })();
            if let Err(error) = mutation {
                restore_project_files(&backup);
                let _ = git_service.unstage_paths(context, &result_paths);
                return Err(error);
            }
            Ok(checkpoint.commit_hash.is_some())
        })();
        if operation.is_err() {
            self.cleanup_replacement_artifacts(context, old_artifacts, new_artifacts);
        }
        operation
    }

    pub fn preview_import(
        &self,
        context: &ProjectContext,
        _file_store: &FileStore,
        request: &ImportRequest,
        extraction_results: &[ExtractResult],
    ) -> Result<ImportPreview, BackendError> {
        let mut entries: Vec<ImportFileEntry> = Vec::new();
        let mut conflicts: Vec<ImportConflict> = Vec::new();
        let mut archived_files: u32 = 0;
        let mut duplicate_files: u32 = 0;
        let mut renamed_files: u32 = 0;
        let mut failed_files: u32 = 0;

        let extract_map: HashMap<&String, &ExtractResult> = extraction_results
            .iter()
            .map(|r| (&r.original_name, r))
            .collect();

        // Build a set of known hashes from existing files in raw/
        let known = self.scan_existing(context)?;

        let source_paths = self.collect_source_paths(&request.source_paths)?;

        for source_path in &source_paths {
            let source_path_str = source_path.to_string_lossy().to_string();
            let file_name = source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let file_type = classify_file(source_path);

            let archive_dir = target_archive_dir(&file_type);
            let hash = file_hash_fast(source_path)
                .unwrap_or_else(|_| format!("!nohash:{}", source_path_str));
            let size_bytes = source_path.metadata().map(|m| m.len()).unwrap_or(0);

            let extract = extract_map.get(&source_path_str);

            let mut entry = ImportFileEntry {
                original_name: file_name.to_string(),
                source_path: source_path_str.clone(),
                archived_path: format!("{}/{}", archive_dir, file_name),
                file_type: file_type.clone(),
                size_bytes,
                hash: hash.clone(),
                extraction_status: extract
                    .map(|e| e.status.clone())
                    .unwrap_or(ExtractionStatus::Pending),
                extraction_error: extract.and_then(|e| e.error.clone()),
                text_preview: extract.and_then(|e| e.text_preview.clone()),
                page_count: extract.and_then(|e| e.metadata.as_ref().and_then(|m| m.page_count)),
                word_count: extract.and_then(|e| e.metadata.as_ref().and_then(|m| m.word_count)),
                metadata: extract.and_then(|e| e.metadata.clone()),
                extracted_text_path: extract.and_then(|e| e.extracted_text_path.clone()),
                extracted_assets: extract
                    .map(|e| e.extracted_assets.clone())
                    .unwrap_or_default(),
                conflict: None,
                renamed_from: None,
            };

            // Check for conflicts: exact duplicates and name collisions
            if known.contains_key(&hash) {
                let existing_path = known.get(&hash).cloned();
                let conflict = ImportConflict {
                    original_name: file_name.to_string(),
                    conflict_type: ConflictType::ExactDuplicate,
                    existing_path,
                    resolved_path: entry.archived_path.clone(),
                    existing_hash: Some(hash.clone()),
                    new_hash: hash.clone(),
                    resolution: if request.link_duplicates {
                        Some(ConflictResolution::LinkToExisting)
                    } else {
                        Some(ConflictResolution::Skip)
                    },
                };
                entry.conflict = Some(conflict.clone());
                conflicts.push(conflict);
                duplicate_files += 1;
            } else {
                // Check for name collision: same archived path but different content
                let target_path = context.root.join(&entry.archived_path);
                if target_path.exists() {
                    let existing_hash = file_hash_fast(&target_path).unwrap_or_default();
                    if existing_hash != hash {
                        let new_name = deterministic_rename(file_name, &hash);
                        let new_path = format!("{}/{}", archive_dir, new_name);
                        let conflict = ImportConflict {
                            original_name: file_name.to_string(),
                            conflict_type: ConflictType::NameCollision,
                            existing_path: Some(entry.archived_path.clone()),
                            resolved_path: new_path.clone(),
                            existing_hash: Some(existing_hash),
                            new_hash: hash.clone(),
                            resolution: Some(ConflictResolution::Rename),
                        };
                        entry.renamed_from = Some(entry.archived_path.clone());
                        entry.archived_path = new_path;
                        entry.conflict = Some(conflict.clone());
                        conflicts.push(conflict);
                        renamed_files += 1;
                    }
                }
                archived_files += 1;
            }

            if extract
                .map(|e| e.status == ExtractionStatus::Failed)
                .unwrap_or(false)
            {
                failed_files += 1;
            }

            entries.push(entry);
        }

        let summary = ImportSummary {
            total_files: source_paths.len() as u32,
            archived_files,
            duplicate_files,
            renamed_files,
            failed_files,
            conflicts_count: conflicts.len() as u32,
        };

        Ok(ImportPreview {
            files: entries,
            conflicts,
            summary,
        })
    }

    /// Walk raw/ directory and return hash → relative-path mapping.
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

    fn scan_existing(
        &self,
        context: &ProjectContext,
    ) -> Result<HashMap<String, String>, BackendError> {
        let mut map = HashMap::new();
        if context.raw_dir.exists() {
            self.collect_hashes(
                &context.raw_dir,
                &context.root,
                &context.raw_dir.join("extracted"),
                &mut map,
            )?;
        }
        Ok(map)
    }

    fn collect_hashes(
        &self,
        dir: &Path,
        raw_base: &Path,
        excluded_dir: &Path,
        map: &mut HashMap<String, String>,
    ) -> Result<(), BackendError> {
        if !dir.exists() || dir == excluded_dir {
            return Ok(());
        }
        let entries = fs::read_dir(dir).map_err(|err| {
            BackendError::new("FILE_ENUMERATE_FAILED", err.to_string(), true, false)
        })?;
        for entry in entries {
            let entry = entry.map_err(|err| {
                BackendError::new("FILE_ENUMERATE_FAILED", err.to_string(), true, false)
            })?;
            let path = entry.path();
            if path.is_dir() {
                self.collect_hashes(&path, raw_base, excluded_dir, map)?;
            } else if path.is_file() {
                if let Ok(hash) = file_hash_fast(&path) {
                    let rel = path
                        .strip_prefix(raw_base)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));
                    map.insert(hash, rel);
                }
            }
        }
        Ok(())
    }
}

fn rollback_import_targets(paths: &[PathBuf]) {
    for path in paths.iter().rev() {
        let _ = fs::remove_file(path);
    }
}

fn verify_project_hash(
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

fn validate_artifact_paths(context: &ProjectContext, paths: &[String]) -> Result<(), BackendError> {
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

fn remove_project_files(context: &ProjectContext, paths: &[String]) {
    for path in paths {
        if let Ok(absolute) = context.resolve_project_path(path) {
            let _ = fs::remove_file(absolute);
        }
    }
}

fn backup_project_files(
    context: &ProjectContext,
    paths: &[String],
) -> Result<FileBackup, BackendError> {
    paths
        .iter()
        .map(|path| {
            let absolute = context.resolve_project_path(path)?;
            let bytes = if absolute.exists() {
                Some(fs::read(&absolute).map_err(|error| {
                    BackendError::new("SOURCE_BACKUP_FAILED", error.to_string(), true, false)
                })?)
            } else {
                None
            };
            Ok((absolute, bytes))
        })
        .collect()
}

fn restore_project_files(backup: &FileBackup) {
    for (path, bytes) in backup {
        match bytes {
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(path, bytes);
            }
            None => {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn file_hash_fast(path: &Path) -> Result<String, BackendError> {
    use sha2::{Digest, Sha256};
    let metadata = fs::metadata(path)
        .map_err(|err| BackendError::new("FILE_METADATA_FAILED", err.to_string(), true, false))?;
    let max_size: u64 = 100 * 1024 * 1024; // 100 MB
    if metadata.len() > max_size {
        return Err(BackendError::new(
            "FILE_TOO_LARGE",
            format!(
                "File exceeds {} MB size limit for hashing",
                max_size / (1024 * 1024)
            ),
            true,
            false,
        ));
    }
    let bytes = fs::read(path)
        .map_err(|err| BackendError::new("FILE_READ_FAILED", err.to_string(), true, false))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::test_support::tmp_context;
    use super::*;
    use crate::models::import::{SourceArtifactIndex, SourceFileType, SourceMetadata};
    use std::collections::HashMap;

    // ── Duplicate and conflict handling tests ──

    #[test]
    fn extracted_markdown_does_not_make_source_a_duplicate() {
        let (context, root) = tmp_context("extracted-is-not-source");
        let source_root =
            std::env::temp_dir().join(format!("llm-wiki-import-external-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&source_root).unwrap();
        let source = source_root.join("研究笔记.md");
        let content = b"# Same bytes as the preview artifact";
        fs::write(&source, content).unwrap();

        let extracted_dir = root.join("raw/extracted");
        fs::create_dir_all(&extracted_dir).unwrap();
        fs::write(extracted_dir.join("preview-artifact.md"), content).unwrap();

        let preview = ImportService
            .preview_import(
                &context,
                &FileStore,
                &ImportRequest {
                    source_paths: vec![source.to_string_lossy().into_owned()],
                    allow_duplicates: false,
                    link_duplicates: false,
                },
                &[],
            )
            .unwrap();

        assert_eq!(preview.summary.archived_files, 1);
        assert_eq!(preview.summary.duplicate_files, 0);
        assert!(preview.files[0].conflict.is_none());

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_raw_source_directories_still_participate_in_duplicate_detection() {
        let (context, root) = tmp_context("legacy-raw-source");
        let legacy_dir = root.join("raw/articles");
        fs::create_dir_all(&legacy_dir).unwrap();
        let content = b"# Existing legacy source";
        fs::write(legacy_dir.join("existing.md"), content).unwrap();

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("existing.md");
        fs::write(&source, content).unwrap();

        let preview = ImportService
            .preview_import(
                &context,
                &FileStore,
                &ImportRequest {
                    source_paths: vec![source.to_string_lossy().into_owned()],
                    allow_duplicates: false,
                    link_duplicates: false,
                },
                &[],
            )
            .unwrap();

        assert_eq!(preview.summary.archived_files, 0);
        assert_eq!(preview.summary.duplicate_files, 1);
        assert_eq!(
            preview.conflicts[0].existing_path.as_deref(),
            Some("raw/articles/existing.md")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_exact_duplicates_in_preview() {
        let (context, root) = tmp_context("exact-dup");
        let store = FileStore;

        // Write a file into raw/sources/markdown/
        let md_dir = root.join("raw/sources/markdown");
        fs::create_dir_all(&md_dir).unwrap();
        let content = b"# Same content";
        fs::write(md_dir.join("existing.md"), content).unwrap();

        // Create a temp source file with the same content
        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("existing.md"), content).unwrap();

        let request = ImportRequest {
            source_paths: vec![source_dir.join("existing.md").to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.duplicate_files, 1);
        assert_eq!(preview.summary.archived_files, 0);
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(
            preview.conflicts[0].conflict_type,
            ConflictType::ExactDuplicate
        );
        assert_eq!(
            preview.conflicts[0].resolution,
            Some(ConflictResolution::Skip)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_name_collision_different_content() {
        let (context, root) = tmp_context("name-collision");
        let store = FileStore;

        let md_dir = root.join("raw/sources/markdown");
        fs::create_dir_all(&md_dir).unwrap();
        fs::write(md_dir.join("notes.md"), b"# Original content").unwrap();

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("notes.md"), b"# Different content").unwrap();

        let request = ImportRequest {
            source_paths: vec![source_dir.join("notes.md").to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.renamed_files, 1);
        assert!(preview
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::NameCollision));
        // The renamed file should have a different archived_path than the original
        let entry = &preview.files[0];
        assert_ne!(entry.archived_path, "raw/sources/markdown/notes.md");
        assert!(entry.renamed_from.is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archives_new_files_without_conflict() {
        let (context, root) = tmp_context("no-conflict");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("new-doc.pdf"), b"%PDF-1.4 fake").unwrap();

        let request = ImportRequest {
            source_paths: vec![source_dir.join("new-doc.pdf").to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.archived_files, 1);
        assert_eq!(preview.summary.duplicate_files, 0);
        assert_eq!(preview.summary.renamed_files, 0);
        assert_eq!(
            preview.files[0].archived_path,
            "raw/sources/pdfs/new-doc.pdf"
        );

        fs::remove_dir_all(root).unwrap();
    }

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

    #[test]
    fn preview_summary_counts_are_consistent() {
        let (context, root) = tmp_context("summary-counts");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("a.pdf"), b"%PDF fake").unwrap();
        fs::write(source_dir.join("b.md"), b"# markdown").unwrap();
        fs::write(source_dir.join("c.png"), b"PNG fake").unwrap();

        let request = ImportRequest {
            source_paths: vec![
                source_dir.join("a.pdf").to_string_lossy().to_string(),
                source_dir.join("b.md").to_string_lossy().to_string(),
                source_dir.join("c.png").to_string_lossy().to_string(),
            ],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.total_files, 3);
        assert_eq!(preview.summary.archived_files, 3);
        assert_eq!(preview.files.len(), 3);
        // Verify each file goes to correct directory
        assert_eq!(preview.files[0].archived_path, "raw/sources/pdfs/a.pdf");
        assert_eq!(preview.files[1].archived_path, "raw/sources/markdown/b.md");
        assert_eq!(preview.files[2].archived_path, "raw/assets/c.png");

        fs::remove_dir_all(root).unwrap();
    }

    // ── Folder import test ──

    #[test]
    fn preview_handles_folder_import_multiple_file_types() {
        let (context, root) = tmp_context("folder-import");
        let store = FileStore;

        // Simulate a folder with mixed file types
        let source_dir = root.join("import-folder");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("报告.pdf"), b"%PDF fake").unwrap();
        fs::write(source_dir.join("数据.xlsx"), b"XLSX fake").unwrap();
        fs::write(source_dir.join("幻灯片.pptx"), b"PPTX fake").unwrap();
        fs::write(source_dir.join("笔记.md"), b"# notes").unwrap();
        fs::write(source_dir.join("图片.jpg"), b"JPEG fake").unwrap();
        fs::write(source_dir.join("未知文件.xyz"), b"unknown").unwrap();
        let nested = source_dir.join("子目录");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("嵌套.txt"), b"nested").unwrap();

        let request = ImportRequest {
            source_paths: vec![source_dir.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.total_files, 7);
        assert_eq!(preview.summary.archived_files, 7);
        assert!(preview
            .files
            .iter()
            .any(|file| file.original_name == "嵌套.txt"));

        // Check each file went to the correct directory
        let pdf_entry = preview
            .files
            .iter()
            .find(|f| f.original_name == "报告.pdf")
            .unwrap();
        assert_eq!(pdf_entry.archived_path, "raw/sources/pdfs/报告.pdf");

        let xlsx_entry = preview
            .files
            .iter()
            .find(|f| f.original_name == "数据.xlsx")
            .unwrap();
        assert_eq!(xlsx_entry.archived_path, "raw/sources/sheets/数据.xlsx");

        let pptx_entry = preview
            .files
            .iter()
            .find(|f| f.original_name == "幻灯片.pptx")
            .unwrap();
        assert_eq!(pptx_entry.archived_path, "raw/sources/slides/幻灯片.pptx");

        let md_entry = preview
            .files
            .iter()
            .find(|f| f.original_name == "笔记.md")
            .unwrap();
        assert_eq!(md_entry.archived_path, "raw/sources/markdown/笔记.md");

        let img_entry = preview
            .files
            .iter()
            .find(|f| f.original_name == "图片.jpg")
            .unwrap();
        assert_eq!(img_entry.archived_path, "raw/assets/图片.jpg");

        let unknown_entry = preview
            .files
            .iter()
            .find(|f| f.original_name == "未知文件.xyz")
            .unwrap();
        assert_eq!(
            unknown_entry.archived_path,
            "raw/sources/other/未知文件.xyz"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_rejects_a_missing_source_instead_of_marking_it_ready() {
        let (context, root) = tmp_context("missing-source");
        let missing = root.join("does-not-exist").join("资料.md");
        let request = ImportRequest {
            source_paths: vec![missing.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let error = ImportService
            .preview_import(&context, &FileStore, &request, &[])
            .expect_err("a missing path must not appear as an archive-ready source");

        assert_eq!(error.code, "IMPORT_SOURCE_NOT_FOUND");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replacement_artifact_cleanup_preserves_artifacts_shared_with_the_current_source() {
        let (context, root) = tmp_context("replacement-cleanup");
        let shared = root.join("raw/extracted/shared.md");
        let staged = root.join("raw/extracted/staged.md");
        fs::create_dir_all(shared.parent().unwrap()).unwrap();
        fs::write(&shared, "shared").unwrap();
        fs::write(&staged, "staged").unwrap();

        ImportService.cleanup_replacement_artifacts(
            &context,
            &["raw/extracted/shared.md".to_string()],
            &[
                "raw/extracted/shared.md".to_string(),
                "raw/extracted/staged.md".to_string(),
            ],
        );

        assert!(shared.exists());
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_delete_removes_indexed_artifacts_and_commits_a_clean_result() {
        let (context, root) = tmp_context("source-delete");
        let source = root.join("raw/sources/markdown/source.md");
        let artifact = root.join("raw/extracted/source.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::create_dir_all(&context.app_dir).unwrap();
        fs::write(&source, "# Source").unwrap();
        fs::write(&artifact, "# Extracted").unwrap();
        let store = FileStore;
        store
            .write_json_atomic(
                &context,
                ".app/source-index.json",
                &SourceArtifactIndex {
                    sources: HashMap::from([(
                        "raw/sources/markdown/source.md".to_string(),
                        vec!["raw/extracted/source.md".to_string()],
                    )]),
                },
            )
            .unwrap();
        let git = GitService;
        git.initialize_repository(&context, "baseline").unwrap();
        let hash = store
            .file_hash(&context, "raw/sources/markdown/source.md")
            .unwrap();

        let checkpoint = ImportService
            .apply_source_delete(
                &context,
                &store,
                &git,
                "raw/sources/markdown/source.md",
                &hash,
                &["raw/extracted/source.md".to_string()],
            )
            .unwrap();

        assert!(checkpoint);
        assert!(!source.exists());
        assert!(!artifact.exists());
        assert!(ImportService
            .read_source_index(&context, &store)
            .unwrap()
            .sources
            .is_empty());
        assert!(!git.repository_status(&context).unwrap().has_changes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejected_source_replacement_cleans_only_new_staged_artifacts() {
        let (context, root) = tmp_context("source-replace-rejected");
        let source = root.join("raw/sources/markdown/source.md");
        let shared = root.join("raw/extracted/shared.md");
        let staged = root.join("raw/extracted/staged.md");
        let replacement = root.join("replacement.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(shared.parent().unwrap()).unwrap();
        fs::write(&source, "old").unwrap();
        fs::write(&shared, "shared").unwrap();
        fs::write(&staged, "staged").unwrap();
        fs::write(&replacement, "new").unwrap();
        let service = ImportService;
        let replacement_hash = service.hash_external_file(&replacement).unwrap();

        let error = service
            .apply_source_replace(
                &context,
                &FileStore,
                &GitService,
                "raw/sources/markdown/source.md",
                "stale-target-hash",
                &replacement,
                &replacement_hash,
                &["raw/extracted/shared.md".to_string()],
                &[
                    "raw/extracted/shared.md".to_string(),
                    "raw/extracted/staged.md".to_string(),
                ],
            )
            .unwrap_err();

        assert_eq!(error.code, "CONFIRMATION_STATE_MISMATCH");
        assert_eq!(fs::read_to_string(source).unwrap(), "old");
        assert!(shared.exists());
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_replace_updates_archive_artifacts_index_and_commits_a_clean_result() {
        let (context, root) = tmp_context("source-replace");
        let source = root.join("raw/sources/markdown/source.md");
        let old_artifact = root.join("raw/extracted/old.md");
        let new_artifact = root.join("raw/extracted/new.md");
        let replacement = root.join("replacement.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(old_artifact.parent().unwrap()).unwrap();
        fs::create_dir_all(&context.app_dir).unwrap();
        fs::write(&source, "old source").unwrap();
        fs::write(&old_artifact, "old extracted").unwrap();
        fs::write(&new_artifact, "new extracted").unwrap();
        fs::write(&replacement, "new source").unwrap();
        let store = FileStore;
        store
            .write_json_atomic(
                &context,
                ".app/source-index.json",
                &SourceArtifactIndex {
                    sources: HashMap::from([(
                        "raw/sources/markdown/source.md".to_string(),
                        vec!["raw/extracted/old.md".to_string()],
                    )]),
                },
            )
            .unwrap();
        let git = GitService;
        git.initialize_repository(&context, "baseline").unwrap();
        let target_hash = store
            .file_hash(&context, "raw/sources/markdown/source.md")
            .unwrap();
        let replacement_hash = ImportService.hash_external_file(&replacement).unwrap();

        let checkpoint = ImportService
            .apply_source_replace(
                &context,
                &store,
                &git,
                "raw/sources/markdown/source.md",
                &target_hash,
                &replacement,
                &replacement_hash,
                &["raw/extracted/old.md".to_string()],
                &["raw/extracted/new.md".to_string()],
            )
            .unwrap();

        assert!(checkpoint);
        assert_eq!(fs::read_to_string(source).unwrap(), "new source");
        assert!(!old_artifact.exists());
        assert!(new_artifact.exists());
        assert_eq!(
            ImportService
                .read_source_index(&context, &store)
                .unwrap()
                .sources
                .get("raw/sources/markdown/source.md"),
            Some(&vec!["raw/extracted/new.md".to_string()])
        );
        assert!(!git.repository_status(&context).unwrap().has_changes);
        fs::remove_dir_all(root).unwrap();
    }

    // ── Extraction status in preview test ──

    #[test]
    fn preview_includes_extraction_status_and_errors() {
        let (context, root) = tmp_context("extract-preview");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("good.md"), b"# Good file").unwrap();
        fs::write(source_dir.join("bad.pdf"), b"%corrupt").unwrap();

        let extract_results = vec![
            ExtractResult {
                original_name: source_dir.join("good.md").to_string_lossy().to_string(),
                file_type: SourceFileType::Markdown,
                status: ExtractionStatus::Extracted,
                error: None,
                text_preview: Some("# Good file".to_string()),
                metadata: Some(SourceMetadata {
                    title: None,
                    author: None,
                    created: None,
                    modified: None,
                    page_count: None,
                    word_count: Some(3),
                    language: None,
                }),
                extracted_text_path: Some("raw/extracted/good.txt".to_string()),
                extracted_assets: vec!["raw/extracted/good-cover.png".to_string()],
            },
            ExtractResult {
                original_name: source_dir.join("bad.pdf").to_string_lossy().to_string(),
                file_type: SourceFileType::Pdf,
                status: ExtractionStatus::Failed,
                error: Some("PDF parsing error: corrupt header".to_string()),
                text_preview: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
            },
        ];

        let request = ImportRequest {
            source_paths: vec![
                source_dir.join("good.md").to_string_lossy().to_string(),
                source_dir.join("bad.pdf").to_string_lossy().to_string(),
            ],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &extract_results)
            .unwrap();

        let good = preview
            .files
            .iter()
            .find(|f| f.original_name == "good.md")
            .unwrap();
        assert_eq!(good.extraction_status, ExtractionStatus::Extracted);
        assert_eq!(good.text_preview, Some("# Good file".to_string()));
        assert_eq!(good.word_count, Some(3));
        assert_eq!(
            good.extracted_text_path.as_deref(),
            Some("raw/extracted/good.txt")
        );
        assert_eq!(good.extracted_assets, vec!["raw/extracted/good-cover.png"]);

        let bad = preview
            .files
            .iter()
            .find(|f| f.original_name == "bad.pdf")
            .unwrap();
        assert_eq!(bad.extraction_status, ExtractionStatus::Failed);
        assert!(bad.extraction_error.as_ref().unwrap().contains("corrupt"));
        assert_eq!(bad.text_preview, None);

        assert_eq!(preview.summary.failed_files, 1);

        fs::remove_dir_all(root).unwrap();
    }

    // ── Serialization tests ──

    #[test]
    fn import_preview_serializes_with_camelcase() {
        let preview = ImportPreview {
            files: vec![ImportFileEntry {
                original_name: "test.pdf".to_string(),
                source_path: "D:/tmp/test.pdf".to_string(),
                archived_path: "raw/sources/pdfs/test.pdf".to_string(),
                file_type: SourceFileType::Pdf,
                size_bytes: 1024,
                hash: "abc123".to_string(),
                extraction_status: ExtractionStatus::Pending,
                extraction_error: None,
                text_preview: None,
                page_count: None,
                word_count: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
                conflict: None,
                renamed_from: None,
            }],
            conflicts: vec![],
            summary: ImportSummary {
                total_files: 1,
                archived_files: 1,
                duplicate_files: 0,
                renamed_files: 0,
                failed_files: 0,
                conflicts_count: 0,
            },
        };

        let json = serde_json::to_string(&preview).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["summary"]["totalFiles"], serde_json::json!(1));
        assert_eq!(
            parsed["files"][0]["originalName"],
            serde_json::json!("test.pdf")
        );
        assert_eq!(parsed["files"][0]["fileType"], serde_json::json!("pdf"));
    }
}

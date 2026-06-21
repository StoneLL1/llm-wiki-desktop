use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::git::CheckpointPurpose;
use crate::models::import::{
    ConflictResolution, ConflictType, ExtractResult, ExtractionStatus, ImportConflict,
    ImportFileEntry, ImportPreview, ImportRequest, ImportSummary, ImportedSource,
    SourceArtifactIndex, SourceFileType,
};
use crate::models::paths::ProjectContext;
use crate::services::{file_store::FileStore, GitService};

// ── Free functions: usable from other services without coupling ──

fn collect_source_files(
    path: &Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<(), BackendError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(BackendError::new(
                "IMPORT_SOURCE_NOT_FOUND",
                "The selected import source does not exist or cannot be read.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "path": path.to_string_lossy(),
                "error": error.to_string(),
            })));
        }
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if metadata.is_dir() {
        let mut entries = fs::read_dir(path)
            .map_err(|error| {
                BackendError::new("FILE_ENUMERATE_FAILED", error.to_string(), true, false)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                BackendError::new("FILE_ENUMERATE_FAILED", error.to_string(), true, false)
            })?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            collect_source_files(&entry.path(), files)?;
        }
    }
    Ok(())
}

pub fn classify_file(path: &Path) -> SourceFileType {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "pdf" => SourceFileType::Pdf,
        "doc" | "docx" | "odt" | "rtf" => SourceFileType::Document,
        "ppt" | "pptx" | "odp" => SourceFileType::Presentation,
        "xls" | "xlsx" | "ods" => SourceFileType::Spreadsheet,
        "csv" => SourceFileType::Csv,
        "md" | "markdown" => SourceFileType::Markdown,
        "txt" | "text" => SourceFileType::Text,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => SourceFileType::Image,
        "html" | "htm" => SourceFileType::Html,
        "url" => SourceFileType::Url,
        _ => SourceFileType::Unknown,
    }
}

pub fn target_archive_dir(file_type: &SourceFileType) -> &'static str {
    match file_type {
        SourceFileType::Pdf => "raw/sources/pdfs",
        SourceFileType::Document => "raw/sources/docs",
        SourceFileType::Presentation => "raw/sources/slides",
        SourceFileType::Spreadsheet => "raw/sources/sheets",
        SourceFileType::Markdown => "raw/sources/markdown",
        SourceFileType::Text => "raw/sources/markdown",
        SourceFileType::Image => "raw/assets",
        SourceFileType::Html => "raw/sources/markdown",
        SourceFileType::Csv => "raw/sources/sheets",
        SourceFileType::Url => "raw/sources/links",
        SourceFileType::Unknown => "raw/sources/other",
    }
}

pub fn deterministic_rename(original_name: &str, hash: &str) -> String {
    let stem_path = Path::new(original_name);
    let stem = stem_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = stem_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let short_hash = &hash[..8.min(hash.len())];

    if ext.is_empty() {
        format!("{}-{}", stem, short_hash)
    } else {
        format!("{}-{}.{}", stem, short_hash, ext)
    }
}

#[derive(Default)]
pub struct ImportService;

type FileBackup = Vec<(PathBuf, Option<Vec<u8>>)>;

impl ImportService {
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

        if let Err(error) = self.record_confirmed_sources(context, file_store, preview) {
            rollback_import_targets(&copied);
            return Err(error);
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
        let raw_dir = &context.raw_dir;
        if !raw_dir.exists() {
            return Ok(map);
        }
        self.collect_hashes(raw_dir, raw_dir, &mut map)?;
        Ok(map)
    }

    fn collect_hashes(
        &self,
        dir: &Path,
        raw_base: &Path,
        map: &mut HashMap<String, String>,
    ) -> Result<(), BackendError> {
        if !dir.exists() {
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
                self.collect_hashes(&path, raw_base, map)?;
            } else if path.is_file() {
                if let Ok(hash) = file_hash_fast(&path) {
                    let rel = path
                        .strip_prefix(raw_base)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| path.to_string_lossy().to_string());
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
        if !normalized.starts_with("raw/extracted/") {
            return Err(BackendError::new(
                "SOURCE_ARTIFACT_PATH_INVALID",
                "Source artifacts must remain under raw/extracted.",
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
    use super::*;
    use crate::models::import::SourceMetadata;
    use crate::models::paths::ProjectContext;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-import-{stamp}-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    // ── File classification tests ──

    #[test]
    fn classifies_pdf() {
        assert_eq!(classify_file(Path::new("doc.pdf")), SourceFileType::Pdf);
        assert_eq!(classify_file(Path::new("DOC.PDF")), SourceFileType::Pdf);
    }

    #[test]
    fn classifies_documents() {
        assert_eq!(
            classify_file(Path::new("report.docx")),
            SourceFileType::Document
        );
        assert_eq!(
            classify_file(Path::new("notes.doc")),
            SourceFileType::Document
        );
        assert_eq!(
            classify_file(Path::new("draft.odt")),
            SourceFileType::Document
        );
        assert_eq!(
            classify_file(Path::new("legacy.rtf")),
            SourceFileType::Document
        );
    }

    #[test]
    fn classifies_presentations() {
        assert_eq!(
            classify_file(Path::new("deck.pptx")),
            SourceFileType::Presentation
        );
        assert_eq!(
            classify_file(Path::new("old.ppt")),
            SourceFileType::Presentation
        );
    }

    #[test]
    fn classifies_spreadsheets() {
        assert_eq!(
            classify_file(Path::new("data.xlsx")),
            SourceFileType::Spreadsheet
        );
    }

    #[test]
    fn classifies_csv_separately() {
        assert_eq!(classify_file(Path::new("export.csv")), SourceFileType::Csv);
    }

    #[test]
    fn classifies_markdown_and_text() {
        assert_eq!(
            classify_file(Path::new("notes.md")),
            SourceFileType::Markdown
        );
        assert_eq!(classify_file(Path::new("readme.txt")), SourceFileType::Text);
    }

    #[test]
    fn classifies_images() {
        assert_eq!(classify_file(Path::new("photo.png")), SourceFileType::Image);
        assert_eq!(classify_file(Path::new("logo.svg")), SourceFileType::Image);
        assert_eq!(classify_file(Path::new("scan.jpg")), SourceFileType::Image);
    }

    #[test]
    fn classifies_html() {
        assert_eq!(classify_file(Path::new("page.html")), SourceFileType::Html);
    }

    #[test]
    fn classifies_staged_url_sources() {
        assert_eq!(classify_file(Path::new("article.url")), SourceFileType::Url);
        assert_eq!(
            target_archive_dir(&SourceFileType::Url),
            "raw/sources/links"
        );
    }

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
    fn classifies_unknown() {
        assert_eq!(
            classify_file(Path::new("sound.mp3")),
            SourceFileType::Unknown
        );
        assert_eq!(
            classify_file(Path::new("archive.zip")),
            SourceFileType::Unknown
        );
    }

    // ── Archive directory routing tests ──

    #[test]
    fn routes_pdf_to_raw_sources_pdfs() {
        assert_eq!(target_archive_dir(&SourceFileType::Pdf), "raw/sources/pdfs");
    }

    #[test]
    fn routes_docx_to_raw_sources_docs() {
        assert_eq!(
            target_archive_dir(&SourceFileType::Document),
            "raw/sources/docs"
        );
    }

    #[test]
    fn routes_pptx_to_raw_sources_slides() {
        assert_eq!(
            target_archive_dir(&SourceFileType::Presentation),
            "raw/sources/slides"
        );
    }

    #[test]
    fn routes_xlsx_to_raw_sources_sheets() {
        assert_eq!(
            target_archive_dir(&SourceFileType::Spreadsheet),
            "raw/sources/sheets"
        );
    }

    #[test]
    fn routes_md_to_raw_sources_markdown() {
        assert_eq!(
            target_archive_dir(&SourceFileType::Markdown),
            "raw/sources/markdown"
        );
    }

    #[test]
    fn routes_images_to_raw_assets() {
        assert_eq!(target_archive_dir(&SourceFileType::Image), "raw/assets");
    }

    #[test]
    fn routes_unknown_to_raw_sources_other() {
        assert_eq!(
            target_archive_dir(&SourceFileType::Unknown),
            "raw/sources/other"
        );
    }

    // ── Deterministic rename tests ──

    #[test]
    fn deterministic_rename_keeps_extension() {
        let renamed = deterministic_rename("report.pdf", "abc12345");
        assert!(renamed.starts_with("report-"));
        assert!(renamed.ends_with(".pdf"));
        assert!(renamed.contains("abc12345"));
    }

    #[test]
    fn deterministic_rename_handles_no_extension() {
        let renamed = deterministic_rename("README", "abc12345");
        assert!(renamed.starts_with("README-"));
        assert!(renamed.contains("abc12345"));
        assert!(!renamed.contains("."));
    }

    #[test]
    fn deterministic_rename_is_stable() {
        let a = deterministic_rename("file.pdf", "abc12345");
        let b = deterministic_rename("file.pdf", "abc12345");
        assert_eq!(a, b);
    }

    // ── CJK filename tests ──

    #[test]
    fn handles_cjk_filenames_in_classification() {
        assert_eq!(
            classify_file(Path::new("概念说明.md")),
            SourceFileType::Markdown
        );
        assert_eq!(
            classify_file(Path::new("数据报告.csv")),
            SourceFileType::Csv
        );
        assert_eq!(
            classify_file(Path::new("プレゼン.pptx")),
            SourceFileType::Presentation
        );
        assert_eq!(
            classify_file(Path::new("研究论文.pdf")),
            SourceFileType::Pdf
        );
    }

    #[test]
    fn cjk_deterministic_rename_preserves_unicode_stem() {
        let renamed = deterministic_rename("概念.md", "abc12345");
        assert!(renamed.starts_with("概念-"));
        assert!(renamed.ends_with(".md"));
    }

    // ── Duplicate and conflict handling tests ──

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
        let index = service.read_source_index(&context, &store).unwrap();
        assert_eq!(
            index.sources.get("raw/sources/markdown/notes.md"),
            Some(&Vec::<String>::new())
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

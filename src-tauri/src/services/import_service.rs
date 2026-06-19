use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::errors::BackendError;
use crate::models::import::{
    ConflictResolution, ConflictType, ExtractResult, ExtractionStatus, ImportConflict,
    ImportFileEntry, ImportPreview, ImportRequest, ImportSummary, SourceFileType,
};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

// ── Free functions: usable from other services without coupling ──

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

impl ImportService {
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

        for source_path_str in &request.source_paths {
            let source_path = Path::new(source_path_str);
            let file_name = source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let file_type = classify_file(source_path);

            let archive_dir = target_archive_dir(&file_type);
            let hash = file_hash_fast(source_path)
                .unwrap_or_else(|_| format!("!nohash:{}", source_path_str));
            let size_bytes = source_path.metadata().map(|m| m.len()).unwrap_or(0);

            let extract = extract_map.get(source_path_str);

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
            total_files: request.source_paths.len() as u32,
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
        _file_store: &FileStore,
        preview: &ImportPreview,
    ) -> Result<(), BackendError> {
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

            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    BackendError::new("FILE_DIR_CREATE_FAILED", err.to_string(), true, false)
                })?;
            }
            fs::copy(source, &target).map_err(|err| {
                BackendError::new("IMPORT_ARCHIVE_COPY_FAILED", err.to_string(), true, false)
                    .with_details(serde_json::json!({
                        "sourcePath": entry.source_path,
                        "archivedPath": entry.archived_path,
                    }))
            })?;
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

        let paths: Vec<String> = vec![
            "报告.pdf",
            "数据.xlsx",
            "幻灯片.pptx",
            "笔记.md",
            "图片.jpg",
            "未知文件.xyz",
        ]
        .iter()
        .map(|n| source_dir.join(n).to_string_lossy().to_string())
        .collect();

        let request = ImportRequest {
            source_paths: paths,
            allow_duplicates: false,
            link_duplicates: false,
        };

        let service = ImportService;
        let preview = service
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        assert_eq!(preview.summary.total_files, 6);
        assert_eq!(preview.summary.archived_files, 6);

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
                extracted_text_path: None,
                extracted_assets: vec![],
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

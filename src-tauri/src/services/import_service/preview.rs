use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::errors::BackendError;
use crate::models::import::{
    ConflictResolution, ConflictType, ExtractResult, ExtractionStatus, ImportConflict,
    ImportFileEntry, ImportPreview, ImportRequest, ImportSummary,
};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

use super::classification::{deterministic_rename, target_archive_dir};
use super::classify_file;

pub(super) fn collect_source_files(
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

impl super::ImportService {
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

pub(super) fn file_hash_fast(path: &Path) -> Result<String, BackendError> {
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
    use std::fs;

    use crate::models::import::*;
    use crate::services::FileStore;

    use super::super::{test_support::tmp_context, ImportService};

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

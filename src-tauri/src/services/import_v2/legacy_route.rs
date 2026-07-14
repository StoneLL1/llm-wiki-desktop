use std::path::Path;

use crate::errors::BackendError;
use crate::models::import::{
    ExtractionStatus, ImportConflict, ImportFileEntry, ImportPreview, ImportSummary,
    SourceFileType,
};
use crate::models::import_v2::{ImportInputKind, ImportItemStatus, ImportSession};

/// Compatibility projection for the existing import surface. It is a view
/// adapter only: it never writes legacy source indexes, raw files, or wiki
/// pages. The activation-aware frontend can consume the existing table shape
/// while commits continue through Import V2's typed session API.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyPreviewAdapter;

impl LegacyPreviewAdapter {
    pub fn from_session(session: &ImportSession) -> Result<ImportPreview, BackendError> {
        let mut files = Vec::with_capacity(session.items.len());
        let mut conflicts = Vec::new();
        let mut archived_files = 0_u32;
        let mut failed_files = 0_u32;

        for item in &session.items {
            let preview = item.preview.as_ref();
            let status = if preview.is_some() {
                if preview.is_some_and(|value| {
                    matches!(
                        value.quality.level,
                        crate::models::import_v2::QualityLevel::Fail
                    )
                }) {
                    ExtractionStatus::Failed
                } else {
                    ExtractionStatus::Extracted
                }
            } else if item.issue.is_some()
                || matches!(item.status, ImportItemStatus::Failed | ImportItemStatus::Cancelled)
            {
                ExtractionStatus::Failed
            } else {
                ExtractionStatus::Pending
            };
            if matches!(status, ExtractionStatus::Extracted) {
                archived_files += 1;
            }
            if matches!(status, ExtractionStatus::Failed) {
                failed_files += 1;
            }

            let (hash, size_bytes, markdown_path, assets, conflict) = if let Some(preview) = preview
            {
                let conflict = (item.status == ImportItemStatus::NeedsMerge).then(|| {
                    let value = ImportConflict {
                        original_name: item.input.display_name.clone(),
                        conflict_type: crate::models::import::ConflictType::PathConflict,
                        existing_path: None,
                        resolved_path: preview.markdown.relative_path.clone(),
                        existing_hash: None,
                        new_hash: preview.source_snapshot.sha256.clone(),
                        resolution: None,
                    };
                    conflicts.push(value.clone());
                    value
                });
                (
                    preview.source_snapshot.sha256.clone(),
                    preview.source_snapshot.size_bytes,
                    preview.markdown.relative_path.clone(),
                    preview
                        .assets
                        .iter()
                        .map(|asset| asset.relative_path.clone())
                        .collect::<Vec<_>>(),
                    conflict,
                )
            } else {
                (String::new(), 0, String::new(), Vec::new(), None)
            };
            files.push(ImportFileEntry {
                original_name: item.input.display_name.clone(),
                source_path: item.input.locator.clone(),
                archived_path: markdown_path.clone(),
                file_type: source_file_type(&item.input.kind, &item.input.locator),
                size_bytes,
                hash,
                extraction_status: status,
                extraction_error: item.issue.as_ref().map(|issue| issue.message.clone()),
                text_preview: None,
                page_count: None,
                word_count: None,
                metadata: None,
                extracted_text_path: (!markdown_path.is_empty()).then_some(markdown_path),
                extracted_assets: assets,
                conflict,
                renamed_from: None,
            });
        }

        Ok(ImportPreview {
            files,
            conflicts,
            summary: ImportSummary {
                total_files: session.items.len() as u32,
                archived_files,
                duplicate_files: 0,
                renamed_files: 0,
                failed_files,
                conflicts_count: session
                    .items
                    .iter()
                    .filter(|item| item.status == ImportItemStatus::NeedsMerge)
                    .count() as u32,
            },
            v2_session_id: Some(session.session_id.clone()),
        })
    }
}

fn source_file_type(kind: &ImportInputKind, name: &str) -> SourceFileType {
    if *kind == ImportInputKind::Url {
        return SourceFileType::Url;
    }
    match Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => SourceFileType::Pdf,
        "doc" | "docx" | "odt" | "rtf" => SourceFileType::Document,
        "ppt" | "pptx" | "odp" => SourceFileType::Presentation,
        "xls" | "xlsx" | "ods" => SourceFileType::Spreadsheet,
        "md" | "markdown" => SourceFileType::Markdown,
        "txt" => SourceFileType::Text,
        "csv" => SourceFileType::Csv,
        "html" | "htm" => SourceFileType::Html,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => SourceFileType::Image,
        _ => SourceFileType::Unknown,
    }
}

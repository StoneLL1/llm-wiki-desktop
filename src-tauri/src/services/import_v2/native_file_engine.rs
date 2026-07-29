use crate::errors::{BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_OUTPUT_INVALID};
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::models::import_v2_file::FileFormat;
use crate::models::source_package::{
    SourcePackageManifest, SourcePackageMember, SourcePackageMemberRole,
};
use crate::services::import_v2::engine::{
    EngineContinuation, EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::generic_web_engine::{NetworkWebArtifactSource, WebArtifactSource};
use crate::services::import_v2::markdown_normalizer::{
    decode_text, html_to_markdown, normalize_markdown,
};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{WebFetchContent, WebFetchPolicy};
use crate::tasks::task_model::CancellationToken;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

struct StagingCleanup {
    path: PathBuf,
    preserve: bool,
}

impl StagingCleanup {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            preserve: false,
        }
    }

    fn preserve(&mut self) {
        self.preserve = true;
    }
}

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Clone)]
pub struct NativeFileEngine {
    artifact_source: Arc<dyn WebArtifactSource>,
}

impl Default for NativeFileEngine {
    fn default() -> Self {
        Self {
            artifact_source: Arc::new(NetworkWebArtifactSource),
        }
    }
}

impl NativeFileEngine {
    pub fn new_with_artifact_source(artifact_source: Arc<dyn WebArtifactSource>) -> Self {
        Self { artifact_source }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata<'a> {
    engine_id: &'a str,
    engine_version: &'a str,
    route: &'a str,
    source_name: &'a str,
    relative_path: &'a str,
    warnings: &'a [String],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredMetadata<'a> {
    engine_id: &'a str,
    engine_version: &'a str,
    route: &'a str,
    source_name: &'a str,
    relative_path: &'a str,
    warnings: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pdf_inspection: Option<&'a crate::services::import_v2::pdf_router::PdfInspection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pdf_page_plan: Option<&'a [crate::services::import_v2::pdf_router::PdfPagePlan]>,
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
        if input.kind == ImportInputKind::ClipboardText {
            return true;
        }
        input.kind == ImportInputKind::File
            && matches!(
                Path::new(&input.locator)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str(),
                "md" | "markdown" | "mdx" | "mkd" | "txt" | "html" | "htm"
            )
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
        let identity = request
            .input
            .source_identity
            .as_ref()
            .ok_or_else(source_changed)?;
        let bytes = safe_read_source(&source, identity)?;
        let text = decode_text(&bytes)?;
        let prefix = &bytes[..bytes.len().min(8192)];
        let (format, _) =
            crate::services::import_v2::file_discovery::identify_file(&source, prefix)?;
        let allow_remote_images = format == FileFormat::Html;
        let (markdown, mut warnings) = match format {
            FileFormat::Html => html_to_markdown(&text),
            FileFormat::Markdown | FileFormat::Text => (normalize_markdown(&text), Vec::new()),
            _ => (normalize_markdown(&text), Vec::new()),
        };
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        std::fs::create_dir_all(&staging)
            .map_err(|_| invalid("The item staging directory could not be created."))?;
        let mut cleanup = StagingCleanup::new(&staging);
        let (markdown, asset_paths, resource_warnings) = copy_and_rewrite_local_assets(
            &markdown,
            source.parent().unwrap_or(&root),
            &staging,
            allow_remote_images,
            &request.item_id,
            cancellation,
            self.artifact_source.as_ref(),
        )?;
        warnings.extend(resource_warnings);
        let descriptor = self.descriptor();
        let metadata = Metadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            source_name: &request.input.display_name,
            relative_path: &request.input.display_name,
            warnings: &warnings,
        };
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
        cleanup.preserve();
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

const CSV_PACKAGE_ROWS: usize = 5_000;
const CSV_PACKAGE_THRESHOLD_ROWS: usize = 10_000;
const CSV_PACKAGE_THRESHOLD_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub struct NativeCsvPackageEngine;

impl ImportEngine for NativeCsvPackageEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "builtin.csv-package".into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            route: "file.csv-package".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let root = PathBuf::from(&request.project_root);
        let source = resolve_source(&root, &request.input.locator)?;
        let staging = resolve_inside(&root, &request.staging_root)?;
        let identity = request
            .input
            .source_identity
            .as_ref()
            .ok_or_else(source_changed)?;
        let bytes = safe_read_source(&source, identity)?;
        let text = decode_text(&bytes)?;
        let delimiter = detect_delimiter(&text);
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .flexible(true)
            .has_headers(false)
            .from_reader(text.as_bytes());
        let rows = reader
            .records()
            .map(|record| {
                record
                    .map(|record| record.iter().map(str::to_string).collect::<Vec<_>>())
                    .map_err(|_| invalid("The CSV contains an invalid record."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            return Err(invalid("The CSV contains no readable rows."));
        }
        std::fs::create_dir_all(&staging)
            .map_err(|_| invalid("The CSV staging directory could not be created."))?;
        let mut cleanup = StagingCleanup::new(&staging);
        std::fs::write(staging.join("source.bin"), &bytes)
            .map_err(|_| invalid("The CSV source snapshot could not be staged."))?;
        let package_required =
            rows.len() >= CSV_PACKAGE_THRESHOLD_ROWS || bytes.len() >= CSV_PACKAGE_THRESHOLD_BYTES;
        let mut asset_paths = Vec::new();
        if package_required {
            let pages_root = staging.join("package/pages");
            std::fs::create_dir_all(&pages_root)
                .map_err(|_| invalid("The CSV package pages could not be staged."))?;
            let mut index = format!(
                "# {}\n\n{} rows · {} columns\n\n## Row ranges\n\n",
                request.input.display_name,
                rows.len(),
                rows.iter().map(Vec::len).max().unwrap_or(0)
            );
            let mut members = Vec::new();
            for (chunk_index, chunk) in rows.chunks(CSV_PACKAGE_ROWS).enumerate() {
                if cancellation.is_cancelled() {
                    let _ = std::fs::remove_dir_all(&staging);
                    return Err(cancelled());
                }
                let start = chunk_index * CSV_PACKAGE_ROWS + 1;
                let end = start + chunk.len() - 1;
                let file_name = format!("rows-{start:06}-{end:06}.md");
                let staging_path = format!("package/pages/{file_name}");
                index.push_str(&format!("- [Rows {start}–{end}]({file_name})\n"));
                let markdown = format!("# Rows {start}–{end}\n\n{}", rows_to_gfm(chunk));
                std::fs::write(staging.join(&staging_path), markdown.as_bytes())
                    .map_err(|_| invalid("A CSV package page could not be staged."))?;
                let hash = format!("{:x}", Sha256::digest(markdown.as_bytes()));
                members.push(SourcePackageMember {
                    order: (chunk_index + 1) as u32,
                    role: SourcePackageMemberRole::RowChunk,
                    title: format!("Rows {start}–{end}"),
                    staging_path: staging_path.clone(),
                    wiki_path: String::new(),
                    baseline_path: String::new(),
                    content_hash: hash.clone(),
                    human_edit_hash: hash,
                });
                asset_paths.push(staging_path);
            }
            std::fs::write(staging.join("document.md"), index.as_bytes())
                .map_err(|_| invalid("The CSV package index could not be staged."))?;
            let index_hash = format!("{:x}", Sha256::digest(index.as_bytes()));
            members.insert(
                0,
                SourcePackageMember {
                    order: 0,
                    role: SourcePackageMemberRole::Index,
                    title: request.input.display_name.clone(),
                    staging_path: "document.md".into(),
                    wiki_path: String::new(),
                    baseline_path: String::new(),
                    content_hash: index_hash.clone(),
                    human_edit_hash: index_hash,
                },
            );
            let package = SourcePackageManifest::staging(members);
            package
                .validate_staging()
                .map_err(|_| invalid("The CSV package contract is invalid."))?;
            std::fs::write(
                staging.join("source-package.json"),
                serde_json::to_vec_pretty(&package)
                    .map_err(|_| invalid("The CSV package contract could not be serialized."))?,
            )
            .map_err(|_| invalid("The CSV package contract could not be staged."))?;
            asset_paths.push("source-package.json".into());
        } else {
            let markdown = format!("# {}\n\n{}", request.input.display_name, rows_to_gfm(&rows));
            std::fs::write(staging.join("document.md"), markdown.as_bytes())
                .map_err(|_| invalid("The CSV preview could not be staged."))?;
        }
        let descriptor = self.descriptor();
        let warnings = package_required
            .then(|| "CSV_SOURCE_PACKAGE".to_string())
            .into_iter()
            .collect::<Vec<_>>();
        let metadata = Metadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            source_name: &request.input.display_name,
            relative_path: &request.input.display_name,
            warnings: &warnings,
        };
        std::fs::write(
            staging.join("metadata.json"),
            serde_json::to_vec_pretty(&metadata)
                .map_err(|_| invalid("The CSV metadata could not be serialized."))?,
        )
        .map_err(|_| invalid("The CSV metadata could not be staged."))?;
        cleanup.preserve();
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
            table_cell_accuracy: Some(1.0),
            sheet_count_exact: Some(1.0),
            slide_count_exact: None,
            non_empty_cell_coverage: Some(1.0),
            formula_value_pairs: Some(1.0),
            meaningful_image_coverage: None,
            continuation: None,
            warnings,
        })
    }
}

/// Built-in deterministic readers for formats that used to be routed only to
/// optional capability packs. They intentionally consume the verified source
/// snapshot and write only item staging artifacts.
pub struct NativeStructuredFileEngine {
    engine_id: &'static str,
    route: &'static str,
}

impl NativeStructuredFileEngine {
    pub const fn new(engine_id: &'static str, route: &'static str) -> Self {
        Self { engine_id, route }
    }

    fn extension(&self) -> &'static str {
        match self.route {
            "pdf.text" => "pdf",
            "office.modern.docx" => "docx",
            "office.modern.xlsx" => "xlsx",
            "office.modern.pptx" => "pptx",
            _ => "",
        }
    }
}

impl ImportEngine for NativeStructuredFileEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: self.engine_id.into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            route: self.route.into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
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
                "The built-in structured file engine does not support this input.",
            ));
        }
        let root = PathBuf::from(&request.project_root);
        let staging = resolve_inside(&root, &request.staging_root)?;
        let (source, bytes, preserve_source_snapshot) =
            if let Some(chained_input) = request.chained_input.as_deref() {
                let source =
                    resolve_chained_office_artifact(&staging, chained_input, self.extension())?;
                let bytes = safe_read_chained_artifact(&staging, &source)?;
                (source, bytes, true)
            } else {
                let source = resolve_source(&root, &request.input.locator)?;
                let identity = request
                    .input
                    .source_identity
                    .as_ref()
                    .ok_or_else(source_changed)?;
                let bytes = safe_read_source(&source, identity)?;
                (source, bytes, false)
            };
        let mut pdf_inspection = None;
        let mut pdf_page_plan = None;
        let mut continuation = None;
        let mut markdown: String = match self.extension() {
            "pdf" => {
                let inspection =
                    crate::services::import_v2::pdf_router::inspect_pdf(&source, None).map_err(
                        |error| match error {
                            crate::services::import_v2::pdf_router::PdfInspectionError::PasswordRequired { .. }
                            | crate::services::import_v2::pdf_router::PdfInspectionError::InvalidPassword { .. } => {
                                BackendError::new(
                                    "IMPORT_PDF_ENCRYPTED_UNSUPPORTED",
                                    "Encrypted PDF files are currently unsupported. No source data was saved.",
                                    false,
                                    true,
                                )
                            }
                            crate::services::import_v2::pdf_router::PdfInspectionError::ActiveContentRejected => {
                                BackendError::new(
                                    "IMPORT_PDF_ACTIVE_CONTENT_REJECTED",
                                    "The PDF contains active content and was not imported.",
                                    false,
                                    true,
                                )
                            }
                            crate::services::import_v2::pdf_router::PdfInspectionError::CorruptInput => {
                                BackendError::new(
                                    "IMPORT_FILE_CORRUPT",
                                    "The PDF could not be inspected safely.",
                                    true,
                                    true,
                                )
                            }
                        },
                    )?;
                let page_plan = crate::services::import_v2::pdf_router::plan_pdf_pages(
                    &inspection,
                    crate::services::import_v2::pdf_router::PdfRouteCapabilities {
                        ocr: request.local_ocr_authorized,
                        ..Default::default()
                    },
                )
                .map_err(|_| invalid("The PDF page route could not be planned."))?;
                let requires_more_capability = page_plan.iter().any(|page| {
                    page.route
                        == crate::services::import_v2::pdf_router::PdfPageRoute::WaitingCapability
                });
                pdf_inspection = Some(inspection);
                pdf_page_plan = Some(page_plan);
                if requires_more_capability {
                    return Err(BackendError::new(
                        "IMPORT_WEB_OCR_UNAVAILABLE",
                        "Some PDF pages need local OCR before a complete Source can be created.",
                        true,
                        true,
                    ));
                }
                let page_plan = pdf_page_plan.as_deref().unwrap_or_default();
                if page_plan.iter().any(|page| {
                    page.route == crate::services::import_v2::pdf_router::PdfPageRoute::SelectiveOcr
                }) {
                    std::fs::create_dir_all(&staging).map_err(|_| {
                        invalid("The PDF selective OCR staging directory could not be created.")
                    })?;
                    let prepared = crate::services::import_v2::pdf_router::prepare_selective_ocr(
                        &source, &staging, page_plan,
                    )?;
                    continuation = Some(EngineContinuation::LocalOcr {
                        temporary_input_paths: prepared.temporary_input_paths,
                    });
                    prepared.markdown
                } else {
                    crate::services::import_v2::structured_extract::extract_pdf_markdown_from_bytes(
                        &bytes,
                    )?
                }
            }
            extension => {
                crate::services::import_v2::structured_extract::extract_ooxml_markdown_from_bytes(
                    extension, &bytes,
                )?
            }
        };
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        std::fs::create_dir_all(&staging)
            .map_err(|_| invalid("The item staging directory could not be created."))?;
        let mut cleanup = StagingCleanup::new(&staging);
        let extension = self.extension();
        let mut asset_paths = Vec::new();
        let mut sheet_count_exact = None;
        let mut slide_count_exact = None;
        let mut formula_value_pairs = None;
        let mut meaningful_image_coverage = None;
        let mut presentation_image_preservation_incomplete = false;
        if extension == "xlsx" {
            let sheets = workbook_sheets_from_markdown(&markdown)?;
            let output = crate::services::import_v2::office_postprocess::WorkbookPlan::new(
                "pending",
                "pending",
                crate::services::import_v2::office_postprocess::WorkbookOutputMode::OverviewAndSheets,
                sheets.clone(),
            )
            .render()
            .map_err(|_| invalid("The workbook output plan could not be rendered."))?;
            let package = stage_workbook_package(&staging, &request.input.display_name, &sheets)?;
            markdown = package.0;
            asset_paths = package.1;
            sheet_count_exact = Some(sheets.len() as f64);
            formula_value_pairs = sheets
                .iter()
                .flat_map(|sheet| sheet.rows.iter())
                .flatten()
                .any(|cell| cell.formula.is_some())
                .then_some(1.0);
            let _ = output;
        } else if extension == "pptx" {
            let mut slides = presentation_slides_from_markdown(&markdown);
            let mut presentation_media = stage_pptx_media(&bytes, &staging)?;
            for slide in &mut slides {
                slide.images = presentation_media
                    .images_by_slide
                    .remove(&slide.number)
                    .unwrap_or_default();
            }
            meaningful_image_coverage = presentation_media.meaningful_image_coverage();
            presentation_image_preservation_incomplete = presentation_media
                .preserved_image_references
                < presentation_media.referenced_images;
            asset_paths.extend(presentation_media.asset_paths);
            let output = crate::services::import_v2::office_postprocess::PresentationPlan::new(
                "pending", "pending", slides,
            )
            .render()
            .map_err(|_| invalid("The presentation output plan could not be rendered."))?;
            markdown = output.candidates.join("\n\n");
            slide_count_exact = Some(
                markdown
                    .lines()
                    .filter(|line| line.starts_with("## Slide "))
                    .count() as f64,
            );
        }
        let descriptor = self.descriptor();
        let mut warnings = match self.extension() {
            "pdf" => vec!["PDF_LAYOUT_NOT_EXTRACTED".to_string()],
            "docx" => vec!["OFFICE_STRUCTURED_CONTENT_NOT_EXTRACTED".to_string()],
            _ => Vec::new(),
        };
        if presentation_image_preservation_incomplete {
            warnings.push("PRESENTATION_IMAGE_PRESERVATION_INCOMPLETE".to_string());
        }
        let metadata = StructuredMetadata {
            engine_id: &descriptor.engine_id,
            engine_version: &descriptor.engine_version,
            route: &descriptor.route,
            source_name: &request.input.display_name,
            relative_path: &request.input.display_name,
            warnings: &warnings,
            pdf_inspection: pdf_inspection.as_ref(),
            pdf_page_plan: pdf_page_plan.as_deref(),
        };
        let source_write = if preserve_source_snapshot {
            let snapshot = staging.join("source.bin");
            let metadata = std::fs::symlink_metadata(&snapshot);
            if metadata.is_ok_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && !is_reparse_point(&metadata)
            }) {
                Ok(())
            } else {
                Err(std::io::Error::other(
                    "The original legacy Office snapshot is missing.",
                ))
            }
        } else {
            std::fs::write(staging.join("source.bin"), &bytes)
        };
        let written = source_write
            .and_then(|_| std::fs::write(staging.join("document.md"), markdown.as_bytes()))
            .and_then(|_| {
                serde_json::to_vec_pretty(&metadata)
                    .map_err(std::io::Error::other)
                    .and_then(|bytes| std::fs::write(staging.join("metadata.json"), bytes))
            });
        if written.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(invalid(
                "The built-in structured file engine could not write staging.",
            ));
        }
        if cancellation.is_cancelled() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(cancelled());
        }
        cleanup.preserve();
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
            // The fallback reader extracts cell text but does not verify table
            // structure, formulas, or displayed values.
            table_cell_accuracy: None,
            sheet_count_exact,
            slide_count_exact,
            non_empty_cell_coverage: None,
            formula_value_pairs,
            meaningful_image_coverage,
            continuation,
            warnings,
        })
    }
}

fn workbook_sheets_from_markdown(
    markdown: &str,
) -> Result<Vec<crate::services::import_v2::office_postprocess::Sheet>, BackendError> {
    use crate::services::import_v2::office_postprocess::{Cell, Sheet};
    let mut sheets = Vec::new();
    let mut current_name = None::<String>;
    let mut rows = Vec::<Vec<Cell>>::new();
    let flush = |sheets: &mut Vec<Sheet>, name: &mut Option<String>, rows: &mut Vec<Vec<Cell>>| {
        if let Some(name) = name.take() {
            let declared_columns = rows.iter().map(Vec::len).max().unwrap_or(0) as u32;
            if rows
                .iter()
                .any(|row| row.iter().any(|cell| !cell.value.trim().is_empty()))
            {
                sheets.push(Sheet {
                    name,
                    hidden: false,
                    rows: std::mem::take(rows),
                    declared_columns,
                });
            } else {
                rows.clear();
            }
        }
    };
    for line in markdown.lines() {
        if let Some(name) = line.strip_prefix("## ") {
            flush(&mut sheets, &mut current_name, &mut rows);
            current_name = Some(name.trim().to_string());
            continue;
        }
        if current_name.is_some()
            && line.starts_with('|')
            && line.ends_with('|')
            && !line
                .trim_matches('|')
                .split('|')
                .all(|cell| cell.trim().chars().all(|char| char == '-' || char == ':'))
        {
            rows.push(
                line.trim_matches('|')
                    .split('|')
                    .map(|value| workbook_cell_from_markdown(value.trim()))
                    .collect(),
            );
        }
    }
    flush(&mut sheets, &mut current_name, &mut rows);
    if sheets.is_empty() {
        return Err(invalid("The workbook contains no non-empty sheets."));
    }
    Ok(sheets)
}

fn workbook_cell_from_markdown(
    value: &str,
) -> crate::services::import_v2::office_postprocess::Cell {
    use crate::services::import_v2::office_postprocess::Cell;
    let value = unescape_table_cell(value);
    if let Some(rest) = value.strip_prefix("`=") {
        if let Some((formula, displayed)) = rest.split_once("` → ") {
            return Cell::formula(format!("={formula}"), displayed);
        }
    }
    Cell::value(value)
}

fn stage_workbook_package(
    staging: &Path,
    title: &str,
    sheets: &[crate::services::import_v2::office_postprocess::Sheet],
) -> Result<(String, Vec<String>), BackendError> {
    let pages_root = staging.join("package/pages");
    std::fs::create_dir_all(&pages_root)
        .map_err(|_| invalid("The workbook package pages could not be staged."))?;
    let mut index = format!(
        "# {title}\n\n{} non-empty sheets\n\n## Sheets\n\n",
        sheets.len()
    );
    let mut members = Vec::new();
    let mut assets = Vec::new();
    let mut formula_evidence = Vec::new();
    for (index_value, sheet) in sheets.iter().enumerate() {
        let file_name = format!(
            "sheet-{:03}-{}.md",
            index_value + 1,
            short_name_hash(&sheet.name)
        );
        let staging_path = format!("package/pages/{file_name}");
        index.push_str(&format!(
            "- [{}]({file_name})\n",
            escape_link_label(&sheet.name)
        ));
        let mut page = format!("# {}\n\n", sheet.name);
        page.push_str(&cells_to_gfm(&sheet.rows));
        for (row_index, row) in sheet.rows.iter().enumerate() {
            for (column_index, cell) in row.iter().enumerate() {
                if let Some(formula) = cell.formula.as_ref() {
                    formula_evidence.push(serde_json::json!({
                        "sheet": sheet.name,
                        "row": row_index + 1,
                        "column": column_index + 1,
                        "formula": formula,
                        "displayedValue": cell.value,
                    }));
                }
            }
        }
        std::fs::write(staging.join(&staging_path), page.as_bytes())
            .map_err(|_| invalid("A workbook sheet page could not be staged."))?;
        let hash = format!("{:x}", Sha256::digest(page.as_bytes()));
        members.push(SourcePackageMember {
            order: (index_value + 1) as u32,
            role: SourcePackageMemberRole::Sheet,
            title: sheet.name.clone(),
            staging_path: staging_path.clone(),
            wiki_path: String::new(),
            baseline_path: String::new(),
            content_hash: hash.clone(),
            human_edit_hash: hash,
        });
        assets.push(staging_path);
    }
    std::fs::write(staging.join("document.md"), index.as_bytes())
        .map_err(|_| invalid("The workbook package index could not be staged."))?;
    let index_hash = format!("{:x}", Sha256::digest(index.as_bytes()));
    members.insert(
        0,
        SourcePackageMember {
            order: 0,
            role: SourcePackageMemberRole::Index,
            title: title.to_string(),
            staging_path: "document.md".into(),
            wiki_path: String::new(),
            baseline_path: String::new(),
            content_hash: index_hash.clone(),
            human_edit_hash: index_hash,
        },
    );
    let package = SourcePackageManifest::staging(members);
    package
        .validate_staging()
        .map_err(|_| invalid("The workbook package contract is invalid."))?;
    std::fs::write(
        staging.join("source-package.json"),
        serde_json::to_vec_pretty(&package)
            .map_err(|_| invalid("The workbook package contract could not be serialized."))?,
    )
    .map_err(|_| invalid("The workbook package contract could not be staged."))?;
    assets.push("source-package.json".into());
    if !formula_evidence.is_empty() {
        let relative = "source-evidence/workbook-formulas.json";
        let target = staging.join(relative);
        std::fs::create_dir_all(target.parent().unwrap())
            .map_err(|_| invalid("The workbook formula evidence directory could not be staged."))?;
        std::fs::write(
            target,
            serde_json::to_vec_pretty(&formula_evidence)
                .map_err(|_| invalid("The workbook formula evidence could not be serialized."))?,
        )
        .map_err(|_| invalid("The workbook formula evidence could not be staged."))?;
        assets.push(relative.into());
    }
    Ok((index, assets))
}

fn cells_to_gfm(rows: &[Vec<crate::services::import_v2::office_postprocess::Cell>]) -> String {
    let values = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.value.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    rows_to_gfm(&values)
}

fn presentation_slides_from_markdown(
    markdown: &str,
) -> Vec<crate::services::import_v2::office_postprocess::Slide> {
    use crate::services::import_v2::office_postprocess::Slide;
    let mut slides = Vec::new();
    let normalized = format!("\n{markdown}");
    for section in normalized.split("\n## Slide ").skip(1) {
        let mut lines = section.lines();
        let number = lines
            .next()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .unwrap_or(slides.len() as u32 + 1);
        let content = lines
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let notes_prefix = format!("> Speaker notes (slide {number}): ");
        let notes = content
            .iter()
            .find_map(|line| line.strip_prefix(&notes_prefix).map(str::to_string));
        let body = content
            .iter()
            .filter(|line| !line.starts_with("> Speaker notes (slide "))
            .cloned()
            .collect::<Vec<_>>();
        let title = body
            .first()
            .cloned()
            .unwrap_or_else(|| format!("Slide {number}"));
        slides.push(Slide {
            number,
            title,
            body: body.into_iter().skip(1).collect(),
            notes,
            images: Vec::new(),
        });
    }
    if slides.is_empty() && !markdown.trim().is_empty() {
        slides.push(Slide {
            number: 1,
            title: "Slide 1".into(),
            body: vec![markdown.trim().to_string()],
            notes: None,
            images: Vec::new(),
        });
    }
    slides
}

struct StagedPresentationMedia {
    images_by_slide: std::collections::BTreeMap<
        u32,
        Vec<crate::services::import_v2::office_postprocess::SlideImage>,
    >,
    asset_paths: Vec<String>,
    referenced_images: usize,
    preserved_image_references: usize,
}

impl StagedPresentationMedia {
    fn meaningful_image_coverage(&self) -> Option<f64> {
        (self.referenced_images > 0)
            .then(|| self.preserved_image_references as f64 / self.referenced_images as f64)
    }
}

fn stage_pptx_media(bytes: &[u8], staging: &Path) -> Result<StagedPresentationMedia, BackendError> {
    use crate::services::import_v2::office_postprocess::SlideImage;
    use std::collections::{BTreeMap, HashMap};
    use std::io::{Cursor, Read};
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| invalid("The presentation media container could not be opened."))?;
    let mut media_entries = HashMap::<String, Vec<u8>>::new();
    let mut slide_xml = BTreeMap::<u32, String>::new();
    let mut relationship_xml = BTreeMap::<u32, String>::new();
    let mut presentation_xml = None;
    let mut presentation_relationship_xml = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| invalid("A presentation media entry could not be inspected."))?;
        let name = entry.name().replace('\\', "/");
        if name.split('/').any(|part| part == "..") || entry.size() > 32 * 1024 * 1024 {
            continue;
        }
        let media_name = name
            .strip_prefix("ppt/media/")
            .filter(|relative| !relative.contains('/'))
            .map(str::to_owned);
        let slide_number = presentation_slide_number(&name, "ppt/slides/slide", ".xml");
        let relationship_number =
            presentation_slide_number(&name, "ppt/slides/_rels/slide", ".xml.rels");
        let is_presentation = name == "ppt/presentation.xml";
        let is_presentation_relationship = name == "ppt/_rels/presentation.xml.rels";
        if media_name.is_none()
            && slide_number.is_none()
            && relationship_number.is_none()
            && !is_presentation
            && !is_presentation_relationship
        {
            continue;
        }
        let mut contents = Vec::new();
        entry
            .by_ref()
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut contents)
            .map_err(|_| invalid("A presentation entry could not be read."))?;
        if contents.len() > 32 * 1024 * 1024 {
            return Err(invalid(
                "A presentation entry exceeds the extraction limit.",
            ));
        }
        if let Some(media_name) = media_name {
            media_entries.insert(media_name, contents);
        } else if let Some(number) = slide_number {
            slide_xml.insert(
                number,
                String::from_utf8(contents)
                    .map_err(|_| invalid("A presentation slide XML entry is invalid."))?,
            );
        } else if let Some(number) = relationship_number {
            relationship_xml.insert(
                number,
                String::from_utf8(contents)
                    .map_err(|_| invalid("A presentation relationship entry is invalid."))?,
            );
        } else if is_presentation {
            presentation_xml = Some(
                String::from_utf8(contents)
                    .map_err(|_| invalid("The presentation XML entry is invalid."))?,
            );
        } else if is_presentation_relationship {
            presentation_relationship_xml = Some(
                String::from_utf8(contents)
                    .map_err(|_| invalid("The presentation relationship map is invalid."))?,
            );
        }
    }

    let logical_slide_numbers = presentation_xml
        .as_deref()
        .zip(presentation_relationship_xml.as_deref())
        .map(|(presentation, relationships)| {
            presentation_logical_slide_numbers(presentation, relationships)
        })
        .transpose()?;
    let has_authoritative_slide_map = logical_slide_numbers.is_some();
    let logical_slide_numbers = logical_slide_numbers.unwrap_or_default();
    let mut images_by_slide = BTreeMap::<u32, Vec<SlideImage>>::new();
    let mut staged_media = HashMap::<String, SlideImage>::new();
    let mut asset_paths = Vec::new();
    let mut referenced_images = 0_usize;
    let mut preserved_image_references = 0_usize;
    for (slide_number, slide) in slide_xml {
        let logical_slide_number = match logical_slide_numbers.get(&slide_number) {
            Some(number) => *number,
            None if has_authoritative_slide_map => continue,
            None => slide_number,
        };
        let embedded_relationship_ids = presentation_embedded_relationship_ids(&slide);
        referenced_images += embedded_relationship_ids.len();
        let Some(relationships) = relationship_xml.get(&slide_number) else {
            continue;
        };
        let relationship_targets = presentation_relationship_targets(relationships);
        let mut images = Vec::new();
        for relationship_id in embedded_relationship_ids {
            let Some(media_name) = relationship_targets.get(&relationship_id) else {
                continue;
            };
            let Some(media) = media_entries.get(media_name) else {
                continue;
            };
            let image = if let Some(image) = staged_media.get(media_name) {
                image.clone()
            } else {
                let extension = Path::new(media_name)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if !matches!(
                    extension.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff"
                ) {
                    continue;
                }
                let Some((width_px, height_px)) = image_dimensions(media) else {
                    continue;
                };
                let digest = format!("{:x}", Sha256::digest(media));
                let relative = format!("assets/presentation/{}-{}", &digest[..12], media_name);
                let target = staging.join(&relative);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|_| {
                        invalid("The presentation image directory could not be staged.")
                    })?;
                }
                std::fs::write(target, media)
                    .map_err(|_| invalid("A presentation image could not be staged."))?;
                asset_paths.push(relative.clone());
                let image = SlideImage {
                    path: relative,
                    width_px,
                    height_px,
                    decorative: false,
                };
                staged_media.insert(media_name.clone(), image.clone());
                image
            };
            preserved_image_references += 1;
            if !images
                .iter()
                .any(|existing: &SlideImage| existing.path == image.path)
            {
                images.push(image);
            }
        }
        if !images.is_empty() {
            images_by_slide.insert(logical_slide_number, images);
        }
    }
    asset_paths.sort();
    asset_paths.dedup();
    Ok(StagedPresentationMedia {
        images_by_slide,
        asset_paths,
        referenced_images,
        preserved_image_references,
    })
}

fn presentation_slide_number(name: &str, prefix: &str, suffix: &str) -> Option<u32> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

fn presentation_logical_slide_numbers(
    presentation_xml: &str,
    relationships_xml: &str,
) -> Result<std::collections::HashMap<u32, u32>, BackendError> {
    let targets = presentation_relationship_targets(relationships_xml);
    let mut reader = quick_xml::Reader::from_str(presentation_xml);
    let mut event_buf = Vec::new();
    let mut output = std::collections::HashMap::new();
    let mut logical_number = 0_u32;
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(quick_xml::events::Event::Start(event))
            | Ok(quick_xml::events::Event::Empty(event))
                if event.name().as_ref().ends_with(b"sldId") =>
            {
                let relationship_id = event.attributes().flatten().find_map(|attribute| {
                    attribute
                        .key
                        .as_ref()
                        .ends_with(b":id")
                        .then(|| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
                });
                let Some(relationship_id) = relationship_id else {
                    continue;
                };
                let Some(target) = targets.get(&relationship_id) else {
                    return Err(invalid(
                        "A presentation slide relationship could not be resolved.",
                    ));
                };
                let Some(physical_number) = presentation_slide_number(target, "slide", ".xml")
                else {
                    return Err(invalid(
                        "A presentation slide relationship target is invalid.",
                    ));
                };
                logical_number += 1;
                output.insert(physical_number, logical_number);
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => return Err(invalid("The presentation XML entry is invalid.")),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(output)
}

fn presentation_relationship_targets(xml: &str) -> std::collections::HashMap<String, String> {
    let mut targets = std::collections::HashMap::new();
    for element in xml.split('<').filter(|element| {
        element.starts_with("Relationship ") || element.starts_with("Relationship/")
    }) {
        let Some(id) = xml_attribute(element, "Id") else {
            continue;
        };
        let Some(target) = xml_attribute(element, "Target") else {
            continue;
        };
        if let Some(name) = Path::new(&target)
            .file_name()
            .and_then(|value| value.to_str())
        {
            targets.insert(id, name.to_string());
        }
    }
    targets
}

fn presentation_embedded_relationship_ids(xml: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = xml;
    while let Some(index) = rest.find("r:embed=") {
        rest = &rest[index + "r:embed=".len()..];
        let Some(quote) = rest
            .chars()
            .next()
            .filter(|value| *value == '"' || *value == '\'')
        else {
            continue;
        };
        rest = &rest[quote.len_utf8()..];
        let Some(end) = rest.find(quote) else {
            break;
        };
        let id = &rest[..end];
        if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_string());
        }
        rest = &rest[end + quote.len_utf8()..];
    }
    ids
}

fn xml_attribute(element: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some((_, value)) = element.split_once(&needle) {
            if let Some(end) = value.find(quote) {
                return Some(value[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod presentation_media_tests {
    use super::stage_pptx_media;
    use std::io::{Cursor, Write};

    #[test]
    fn pptx_images_follow_logical_presentation_order_not_part_numbers() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let image =
            include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/matrix/image.png");
        for (name, bytes) in [
            (
                "ppt/presentation.xml",
                br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="512" r:id="rIdB"/></p:sldIdLst></p:presentation>"#
                    .as_slice(),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                br#"<Relationships><Relationship Id="rIdB" Type="x/slide" Target="slides/slide7.xml"/></Relationships>"#
                    .as_slice(),
            ),
            (
                "ppt/slides/slide7.xml",
                br#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><a:blip r:embed="rIdImage"/></p:sld>"#
                    .as_slice(),
            ),
            (
                "ppt/slides/_rels/slide7.xml.rels",
                br#"<Relationships><Relationship Id="rIdImage" Type="x/image" Target="../media/image1.png"/></Relationships>"#
                    .as_slice(),
            ),
            ("ppt/media/image1.png", image.as_slice()),
        ] {
            archive.start_file(name, options).unwrap();
            archive.write_all(bytes).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let staging = tempfile::tempdir().unwrap();

        let staged = stage_pptx_media(&bytes, staging.path()).unwrap();

        assert_eq!(staged.images_by_slide.get(&1).map(Vec::len), Some(1));
        assert!(!staged.images_by_slide.contains_key(&7));
        assert_eq!(staged.asset_paths.len(), 1);
        assert_eq!(staged.meaningful_image_coverage(), Some(1.0));
    }

    #[test]
    fn pptx_orphan_slide_images_are_ignored_when_presentation_map_exists() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let image =
            include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/matrix/image.png");
        for (name, bytes) in [
            (
                "ppt/presentation.xml",
                br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="512" r:id="rIdB"/></p:sldIdLst></p:presentation>"#
                    .as_slice(),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                br#"<Relationships><Relationship Id="rIdB" Type="x/slide" Target="slides/slide7.xml"/></Relationships>"#
                    .as_slice(),
            ),
            (
                "ppt/slides/slide7.xml",
                br#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><a:p><a:r><a:t>Only logical slide</a:t></a:r></a:p></p:sld>"#
                    .as_slice(),
            ),
            (
                "ppt/slides/slide1.xml",
                br#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><a:blip r:embed="rIdOrphanImage"/></p:sld>"#
                    .as_slice(),
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                br#"<Relationships><Relationship Id="rIdOrphanImage" Type="x/image" Target="../media/orphan.png"/></Relationships>"#
                    .as_slice(),
            ),
            ("ppt/media/orphan.png", image.as_slice()),
        ] {
            archive.start_file(name, options).unwrap();
            archive.write_all(bytes).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let staging = tempfile::tempdir().unwrap();

        let staged = stage_pptx_media(&bytes, staging.path()).unwrap();

        assert!(staged.images_by_slide.is_empty());
        assert!(staged.asset_paths.is_empty());
        assert_eq!(staged.meaningful_image_coverage(), None);
        assert!(!staging.path().join("assets/presentation").exists());
    }

    #[test]
    fn pptx_image_coverage_reports_partial_and_total_preservation_loss() {
        let image =
            include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/matrix/image.png");
        for (include_preserved_image, expected_coverage, expected_assets) in
            [(true, 0.5, 1_usize), (false, 0.0, 0_usize)]
        {
            let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default();
            for (name, bytes) in [
                (
                    "ppt/presentation.xml",
                    br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="512" r:id="rIdB"/></p:sldIdLst></p:presentation>"#
                        .as_slice(),
                ),
                (
                    "ppt/_rels/presentation.xml.rels",
                    br#"<Relationships><Relationship Id="rIdB" Type="x/slide" Target="slides/slide7.xml"/></Relationships>"#
                        .as_slice(),
                ),
                (
                    "ppt/slides/slide7.xml",
                    br#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><a:blip r:embed="rIdPreserved"/><a:blip r:embed="rIdMissing"/></p:sld>"#
                        .as_slice(),
                ),
                (
                    "ppt/slides/_rels/slide7.xml.rels",
                    br#"<Relationships><Relationship Id="rIdPreserved" Type="x/image" Target="../media/preserved.png"/><Relationship Id="rIdMissing" Type="x/image" Target="../media/missing.png"/></Relationships>"#
                        .as_slice(),
                ),
            ] {
                archive.start_file(name, options).unwrap();
                archive.write_all(bytes).unwrap();
            }
            if include_preserved_image {
                archive
                    .start_file("ppt/media/preserved.png", options)
                    .unwrap();
                archive.write_all(image).unwrap();
            }
            let bytes = archive.finish().unwrap().into_inner();
            let staging = tempfile::tempdir().unwrap();

            let staged = stage_pptx_media(&bytes, staging.path()).unwrap();

            assert_eq!(staged.meaningful_image_coverage(), Some(expected_coverage));
            assert_eq!(staged.asset_paths.len(), expected_assets);
            assert_eq!(staged.referenced_images, 2);
            assert_eq!(
                staged.preserved_image_references,
                usize::from(include_preserved_image)
            );
        }
    }
}

fn unescape_table_cell(value: &str) -> String {
    value
        .replace("<br>", "\n")
        .replace("\\|", "|")
        .replace("\\\\", "\\")
}

fn short_name_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))[..12].to_string()
}

fn escape_link_label(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

pub(crate) fn resolve_inside(root: &Path, locator: &str) -> Result<PathBuf, BackendError> {
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
pub(crate) fn resolve_source(root: &Path, locator: &str) -> Result<PathBuf, BackendError> {
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

fn resolve_chained_office_artifact(
    staging: &Path,
    relative: &str,
    expected_extension: &str,
) -> Result<PathBuf, BackendError> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.extension().and_then(|value| value.to_str()) != Some(expected_extension)
    {
        return Err(invalid("The converted Office staging artifact is invalid."));
    }
    let candidate = staging.join(relative);
    let canonical_staging = staging
        .canonicalize()
        .map_err(|_| invalid("The Office staging directory could not be resolved."))?;
    let canonical = candidate
        .canonicalize()
        .map_err(|_| invalid("The converted Office artifact could not be resolved."))?;
    if !canonical.starts_with(&canonical_staging) {
        return Err(invalid(
            "The converted Office artifact escaped item staging.",
        ));
    }
    Ok(canonical)
}

fn safe_read_chained_artifact(staging: &Path, source: &Path) -> Result<Vec<u8>, BackendError> {
    let metadata = std::fs::symlink_metadata(source)
        .map_err(|_| invalid("The converted Office artifact could not be inspected."))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || metadata.len() > 64 * 1024 * 1024
    {
        return Err(invalid(
            "The converted Office artifact is not a safe bounded file.",
        ));
    }
    let canonical_staging = staging
        .canonicalize()
        .map_err(|_| invalid("The Office staging directory could not be resolved."))?;
    if !source.starts_with(&canonical_staging) {
        return Err(invalid(
            "The converted Office artifact escaped item staging.",
        ));
    }
    std::fs::read(source).map_err(|_| invalid("The converted Office artifact could not be read."))
}

pub(crate) fn safe_read_source(
    path: &Path,
    identity: &crate::models::import_v2::SourceIdentity,
) -> Result<Vec<u8>, BackendError> {
    let link = std::fs::symlink_metadata(path)
        .map_err(|_| invalid("The source file could not be inspected."))?;
    if link.file_type().is_symlink() || is_reparse_point(&link) || !link.is_file() {
        return Err(BackendError::new(
            "IMPORT_FILE_SOURCE_CHANGED",
            "The selected source changed and must be scanned again.",
            true,
            true,
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| invalid("The source file could not be resolved."))?;
    if canonical.to_string_lossy() != identity.canonical_path {
        return Err(source_changed());
    }
    if canonical
        != path
            .canonicalize()
            .map_err(|_| invalid("The source file could not be resolved."))?
    {
        return Err(BackendError::new(
            "IMPORT_FILE_SOURCE_CHANGED",
            "The selected source changed and must be scanned again.",
            true,
            true,
        ));
    }
    let file = std::fs::File::open(&canonical)
        .map_err(|_| invalid("The source file could not be read."))?;
    let before = file
        .metadata()
        .map_err(|_| invalid("The source file could not be inspected."))?;
    if before.len() != identity.size_bytes {
        return Err(source_changed());
    }
    if before.len() > 64 * 1024 * 1024 {
        return Err(invalid("The source file exceeds the ingestion size limit."));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    use std::io::Read;
    file.take(before.len() + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("The source file could not be read."))?;
    let after = std::fs::metadata(&canonical)
        .map_err(|_| invalid("The source file changed while it was being read."))?;
    if bytes.len() as u64 != before.len() || before.len() != after.len() {
        return Err(BackendError::new(
            "IMPORT_FILE_SOURCE_CHANGED",
            "The selected source changed and must be scanned again.",
            true,
            true,
        ));
    }
    let prefix = &bytes[..bytes.len().min(8192)];
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let magic = format!("{:x}", Sha256::digest(prefix));
    if hash != identity.sha256 || magic != identity.magic {
        return Err(source_changed());
    }
    Ok(bytes)
}

fn source_changed() -> BackendError {
    BackendError::new(
        "IMPORT_FILE_SOURCE_CHANGED",
        "The selected source changed and must be scanned again.",
        true,
        true,
    )
}
#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}
fn copy_and_rewrite_local_assets(
    markdown: &str,
    root: &Path,
    staging: &Path,
    allow_remote_images: bool,
    item_id: &str,
    cancellation: &CancellationToken,
    artifact_source: &dyn WebArtifactSource,
) -> Result<(String, Vec<String>, Vec<String>), BackendError> {
    let mut paths = Vec::new();
    let mut replacements = Vec::new();
    let mut warnings = Vec::new();
    let canonical_root = root
        .canonicalize()
        .map_err(|_| invalid("The Markdown resource root could not be resolved."))?;
    for resource in markdown_resources(markdown) {
        let destination = resource.destination.as_str();
        if destination.is_empty() {
            continue;
        }
        if destination.starts_with("https://") || destination.starts_with("http://") {
            if !allow_remote_images || !resource.image {
                continue;
            }
            if cancellation.is_cancelled() {
                return Err(cancelled());
            }
            match fetch_remote_image(destination, item_id, cancellation, artifact_source, staging) {
                Ok((relative, bytes)) => {
                    let target = staging.join(&relative);
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).map_err(|_| {
                            invalid("A remote HTML image directory could not be staged.")
                        })?;
                    }
                    std::fs::write(&target, bytes)
                        .map_err(|_| invalid("A remote HTML image could not be staged."))?;
                    let stable = relative.to_string_lossy().replace('\\', "/");
                    paths.push(stable.clone());
                    replacements.push((destination.to_string(), stable));
                }
                Err(_) => {
                    warnings.push("IMPORT_REMOTE_IMAGE_UNAVAILABLE".to_string());
                    replacements.push((
                        destination.to_string(),
                        "assets/remote-image-unavailable".to_string(),
                    ));
                }
            }
            continue;
        }
        if destination.contains(':')
            || destination.starts_with('/')
            || destination.starts_with('\\')
        {
            continue;
        }
        let mut relative = PathBuf::new();
        let mut escapes_source_root = false;
        for part in Path::new(destination.split('#').next().unwrap_or("")).components() {
            match part {
                Component::Normal(part) => relative.push(part),
                Component::CurDir => {}
                // Keep the Markdown text intact, but do not copy a resource
                // outside the source document's directory into staging.
                Component::ParentDir => escapes_source_root = true,
                Component::RootDir | Component::Prefix(_) => escapes_source_root = true,
            }
        }
        if escapes_source_root || relative.as_os_str().is_empty() {
            continue;
        }
        let source = root.join(&relative);
        let metadata = match std::fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(invalid("A Markdown resource could not be inspected.")),
        };
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(invalid(
                "A Markdown resource must not be a symlink or reparse point.",
            ));
        }
        if !metadata.is_file() {
            continue;
        }
        let canonical_source = source
            .canonicalize()
            .map_err(|_| invalid("A Markdown resource could not be resolved."))?;
        if !canonical_source.starts_with(&canonical_root) {
            return Err(invalid(
                "A Markdown resource path escapes its authorized source directory.",
            ));
        }
        let staged_relative = Path::new("assets").join(&relative);
        let target = staging.join(&staged_relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| invalid("A Markdown resource could not be staged."))?;
        }
        std::fs::copy(canonical_source, target)
            .map_err(|_| invalid("A Markdown resource could not be staged."))?;
        let staged = staged_relative.to_string_lossy().replace('\\', "/");
        paths.push(staged.clone());
        replacements.push((destination.to_string(), staged));
    }
    paths.sort();
    paths.dedup();
    let mut rewritten = markdown.to_string();
    for (original, stable) in replacements {
        rewritten = rewritten.replace(&format!("]({original})"), &format!("]({stable})"));
        rewritten = rewritten.replace(&format!("](<{original}>)"), &format!("]({stable})"));
        rewritten = rewritten.replace(&format!("]: {original}"), &format!("]: {stable}"));
        rewritten = rewritten.replace(&format!("]:\t{original}"), &format!("]:\t{stable}"));
    }
    warnings.sort();
    warnings.dedup();
    Ok((rewritten, paths, warnings))
}

struct MarkdownResource {
    destination: String,
    image: bool,
}

fn markdown_resources(markdown: &str) -> Vec<MarkdownResource> {
    let mut resources = Vec::new();
    let mut offset = 0;
    while let Some(relative_end) = markdown[offset..].find("](") {
        let end = offset + relative_end;
        let destination_start = end + 2;
        let Some(relative_close) = markdown[destination_start..].find(')') else {
            break;
        };
        let close = destination_start + relative_close;
        let destination = markdown[destination_start..close]
            .split_ascii_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(['<', '>']);
        let opening = markdown[..end].rfind('[');
        let image = opening.is_some_and(|opening| {
            opening > 0 && markdown.as_bytes().get(opening - 1) == Some(&b'!')
        });
        if !destination.is_empty() {
            resources.push(MarkdownResource {
                destination: destination.to_string(),
                image,
            });
        }
        offset = close + 1;
    }

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('[') else {
            continue;
        };
        let Some((_, destination)) = rest.split_once("]:") else {
            continue;
        };
        let destination = destination
            .trim()
            .split_ascii_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(['<', '>']);
        if !destination.is_empty() {
            resources.push(MarkdownResource {
                destination: destination.to_string(),
                image: reference_is_image(markdown, trimmed),
            });
        }
    }
    resources
}

fn reference_is_image(markdown: &str, definition: &str) -> bool {
    let Some(label_end) = definition.find("]:") else {
        return false;
    };
    let label = &definition[1..label_end];
    markdown.contains(&format!("][{label}]")) && markdown.contains("![")
}

fn fetch_remote_image(
    url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    artifact_source: &dyn WebArtifactSource,
    staging: &Path,
) -> Result<(PathBuf, Vec<u8>), BackendError> {
    let target = UrlPolicy.normalize_for_session(url)?;
    let artifact = artifact_source.fetch(
        target,
        WebFetchPolicy {
            max_response_bytes: 8 * 1024 * 1024,
            content: WebFetchContent::Image,
            ..WebFetchPolicy::default()
        },
        item_id,
        cancellation,
    )?;
    let extension = image_extension(&artifact.content_type)
        .ok_or_else(|| invalid("A remote HTML image has an unsupported content type."))?;
    let (width, height) = image_dimensions(&artifact.bytes)
        .ok_or_else(|| invalid("A remote HTML image could not be inspected."))?;
    if width < 32 || height < 32 || u64::from(width) * u64::from(height) < 4_096 {
        return Err(invalid("A remote HTML image is not meaningful content."));
    }
    let digest = format!("{:x}", Sha256::digest(&artifact.bytes));
    let relative = Path::new("assets")
        .join("remote")
        .join(format!("{digest}.{extension}"));
    let target = staging.join(&relative);
    if target.exists() {
        return Ok((relative, artifact.bytes));
    }
    Ok((relative, artifact.bytes))
}

fn image_extension(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        "image/tiff" => Some("tiff"),
        _ => None,
    }
}

fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        return Some((
            u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
            u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
        ));
    }
    if bytes.starts_with(b"BM") && bytes.len() >= 26 {
        let width = i32::from_le_bytes(bytes[18..22].try_into().ok()?);
        let height = i32::from_le_bytes(bytes[22..26].try_into().ok()?);
        return (width != 0 && height != 0)
            .then_some((width.unsigned_abs(), height.unsigned_abs()));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        if bytes.get(12..16) == Some(b"VP8X") && bytes.len() >= 30 {
            let width = 1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]);
            let height = 1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]);
            return Some((width, height));
        }
        if bytes.get(12..16) == Some(b"VP8L") && bytes.len() >= 25 {
            let packed = u32::from_le_bytes(bytes[21..25].try_into().ok()?);
            return Some(((packed & 0x3fff) + 1, ((packed >> 14) & 0x3fff) + 1));
        }
        if bytes.get(12..16) == Some(b"VP8 ") && bytes.len() >= 30 {
            return Some((
                u16::from_le_bytes(bytes[26..28].try_into().ok()?) as u32 & 0x3fff,
                u16::from_le_bytes(bytes[28..30].try_into().ok()?) as u32 & 0x3fff,
            ));
        }
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut offset = 2;
        while offset + 9 < bytes.len() {
            if bytes[offset] != 0xff {
                offset += 1;
                continue;
            }
            let marker = bytes[offset + 1];
            offset += 2;
            if marker == 0xd8 || marker == 0xd9 || (0xd0..=0xd7).contains(&marker) {
                continue;
            }
            let length =
                u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize;
            if length < 2 || offset + length > bytes.len() {
                return None;
            }
            if matches!(
                marker,
                0xc0 | 0xc1
                    | 0xc2
                    | 0xc3
                    | 0xc5
                    | 0xc6
                    | 0xc7
                    | 0xc9
                    | 0xca
                    | 0xcb
                    | 0xcd
                    | 0xce
                    | 0xcf
            ) && length >= 7
            {
                return Some((
                    u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().ok()?) as u32,
                    u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().ok()?) as u32,
                ));
            }
            offset += length;
        }
    }
    None
}

fn detect_delimiter(text: &str) -> u8 {
    [b',', b'\t', b';']
        .into_iter()
        .max_by_key(|delimiter| {
            text.lines()
                .take(4)
                .map(|line| {
                    line.as_bytes()
                        .iter()
                        .filter(|byte| **byte == *delimiter)
                        .count()
                })
                .sum::<usize>()
        })
        .unwrap_or(b',')
}

fn rows_to_gfm(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1);
    let mut output = String::new();
    for (index, row) in rows.iter().enumerate() {
        output.push('|');
        for column in 0..columns {
            output.push(' ');
            output.push_str(&escape_table_cell(
                row.get(column).map(String::as_str).unwrap_or(""),
            ));
            output.push_str(" |");
        }
        output.push('\n');
        if index == 0 {
            output.push('|');
            for _ in 0..columns {
                output.push_str(" --- |");
            }
            output.push('\n');
        }
    }
    output
}

fn escape_table_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(['\r', '\n'], "<br>")
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

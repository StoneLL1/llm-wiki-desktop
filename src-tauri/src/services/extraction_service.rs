use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::errors::BackendError;
use crate::models::import::{ExtractResult, ExtractionStatus, SourceFileType, SourceMetadata};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

#[derive(Default)]
pub struct ExtractionService;

impl ExtractionService {
    pub fn extract_text(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        source_path: &Path,
        output_dir: &Path,
    ) -> Result<ExtractResult, BackendError> {
        let original_name = source_path.to_string_lossy().to_string();
        let file_type = super::import_service::classify_file(source_path);

        if !source_path.exists() {
            return Ok(ExtractResult {
                original_name,
                file_type,
                status: ExtractionStatus::Failed,
                error: Some("File not found.".to_string()),
                text_preview: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
            });
        }
        ensure_source_file_size(source_path)?;

        match &file_type {
            SourceFileType::Markdown | SourceFileType::Text | SourceFileType::Url => {
                // Direct text extraction
                let content = fs::read_to_string(source_path).map_err(|err| {
                    BackendError::new("EXTRACT_READ_FAILED", err.to_string(), true, false)
                        .with_details(serde_json::json!({ "path": original_name }))
                })?;

                let word_count = count_words(&content);
                let preview = take_preview(&content, 500);
                let extracted_text_path =
                    write_extracted_text(context, file_store, source_path, output_dir, &content)?;

                Ok(ExtractResult {
                    original_name,
                    file_type: file_type.clone(),
                    status: ExtractionStatus::Extracted,
                    error: None,
                    text_preview: Some(preview),
                    metadata: Some(SourceMetadata {
                        title: None,
                        author: None,
                        created: None,
                        modified: None,
                        page_count: None,
                        word_count: Some(word_count),
                        language: None,
                    }),
                    extracted_text_path: Some(extracted_text_path),
                    extracted_assets: vec![],
                })
            }
            SourceFileType::Csv => {
                let markdown = csv_to_markdown(source_path)?;
                build_text_result(
                    context,
                    file_store,
                    source_path,
                    output_dir,
                    &original_name,
                    SourceFileType::Csv,
                    &markdown,
                    None,
                )
            }
            SourceFileType::Html => {
                // Simple text extraction from HTML (full parser adapter in follow-up tasks)
                let content = fs::read_to_string(source_path).map_err(|err| {
                    BackendError::new("EXTRACT_READ_FAILED", err.to_string(), true, false)
                        .with_details(serde_json::json!({ "path": original_name }))
                })?;

                let text = strip_html_tags(&content);
                let word_count = count_words(&text);
                let preview = take_preview(&text, 500);
                let extracted_text_path =
                    write_extracted_text(context, file_store, source_path, output_dir, &text)?;

                Ok(ExtractResult {
                    original_name,
                    file_type: file_type.clone(),
                    status: ExtractionStatus::Extracted,
                    error: None,
                    text_preview: Some(preview),
                    metadata: Some(SourceMetadata {
                        title: Some(extract_html_title(&content)),
                        author: None,
                        created: None,
                        modified: None,
                        page_count: None,
                        word_count: Some(word_count),
                        language: None,
                    }),
                    extracted_text_path: Some(extracted_text_path),
                    extracted_assets: vec![],
                })
            }
            SourceFileType::Pdf => {
                // PDF text extraction via a pure-Rust parser. OCR / visual
                // understanding of scanned or image-only PDFs is handled later
                // by the compile Agent/Skill — the import layer only preserves
                // the original file plus any extractable text layer.
                extract_pdf(context, file_store, source_path, output_dir, &original_name)
            }
            SourceFileType::Document
            | SourceFileType::Presentation
            | SourceFileType::Spreadsheet => {
                // DOCX/PPTX/XLSX (and legacy .doc/.ppt/.xls when they slip
                // through classification) are OOXML zip containers; we extract
                // the text-bearing XML parts. Legacy binary formats have no
                // pure-Rust reader in scope and degrade to Failed with a clear
                // reason rather than Unsupported, so the preview still shows
                // the file was attempted.
                extract_ooxml(
                    context,
                    file_store,
                    source_path,
                    output_dir,
                    &original_name,
                    &file_type,
                )
            }
            SourceFileType::Image => {
                // Images are supported archive-only sources. OCR and visual
                // understanding intentionally remain compile-time concerns.
                Ok(ExtractResult {
                    original_name,
                    file_type: file_type.clone(),
                    status: ExtractionStatus::Extracted,
                    error: None,
                    text_preview: None,
                    metadata: Some(SourceMetadata {
                        title: None,
                        author: None,
                        created: None,
                        modified: None,
                        page_count: None,
                        word_count: None,
                        language: None,
                    }),
                    extracted_text_path: None,
                    extracted_assets: vec![],
                })
            }
            _ => Ok(ExtractResult {
                original_name,
                file_type: file_type.clone(),
                status: ExtractionStatus::Unsupported,
                error: None,
                text_preview: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
            }),
        }
    }

    pub fn extract_batch(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        source_paths: &[String],
        output_dir: &Path,
    ) -> Vec<ExtractResult> {
        source_paths
            .iter()
            .map(|path_str| {
                let path = Path::new(path_str);
                self.extract_text(context, file_store, path, output_dir)
                    .unwrap_or_else(|err| {
                        let ft = super::import_service::classify_file(path);
                        ExtractResult {
                            original_name: path_str.clone(),
                            file_type: ft,
                            status: ExtractionStatus::Failed,
                            error: Some(format!("[{}] {}", err.code, err.message)),
                            text_preview: None,
                            metadata: None,
                            extracted_text_path: None,
                            extracted_assets: vec![],
                        }
                    })
            })
            .collect()
    }
}

pub fn count_words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

pub fn take_preview(text: &str, max_chars: usize) -> String {
    let cleaned = text.replace('\r', "");
    if cleaned.len() <= max_chars {
        return cleaned.to_string();
    }

    let mut end = max_chars;
    while end > 0 && !cleaned.is_char_boundary(end) {
        end -= 1;
    }

    let preview = &cleaned[..end];
    format!("{}…", preview)
}

pub fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Collapse whitespace
    let collapsed: String = result
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    collapsed
}

fn csv_to_markdown(source_path: &Path) -> Result<String, BackendError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(source_path)
        .map_err(|error| {
            BackendError::new("EXTRACT_CSV_READ_FAILED", error.to_string(), true, false)
                .with_details(serde_json::json!({ "path": source_path.to_string_lossy() }))
        })?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| {
            BackendError::new("EXTRACT_CSV_PARSE_FAILED", error.to_string(), true, false)
                .with_details(serde_json::json!({ "path": source_path.to_string_lossy() }))
        })?;
        rows.push(record.iter().map(markdown_table_cell).collect::<Vec<_>>());
    }
    Ok(rows_to_markdown_table(&rows, true))
}

fn markdown_table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
        .trim()
        .to_string()
}

fn rows_to_markdown_table(rows: &[Vec<String>], first_row_is_header: bool) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let mut normalized = rows.to_vec();
    for row in &mut normalized {
        row.resize(width, String::new());
    }
    let header = if first_row_is_header && !normalized.is_empty() {
        normalized.remove(0)
    } else {
        (1..=width).map(|index| format!("Column {index}")).collect()
    };
    let mut markdown = String::new();
    push_markdown_row(&mut markdown, &header);
    push_markdown_row(&mut markdown, &vec!["---".to_string(); width]);
    for row in normalized {
        push_markdown_row(&mut markdown, &row);
    }
    markdown
}

fn push_markdown_row(output: &mut String, row: &[String]) {
    output.push_str("| ");
    output.push_str(&row.join(" | "));
    output.push_str(" |\n");
}

pub fn extract_html_title(html: &str) -> String {
    let lower = html.to_lowercase();
    // Find <title> or <title ...>, allowing attributes/whitespace inside the opening tag.
    let tag_start = match lower.find("<title") {
        Some(i) => i,
        None => return String::new(),
    };
    let after_tag = &lower[tag_start + 6..]; // skip "<title"
    let (content, content_start_offset) = if let Some(rest) = after_tag.strip_prefix('>') {
        // Simple <title> — skip the '>' and use rest as content
        (rest, 1)
    } else if after_tag.starts_with(' ')
        || after_tag.starts_with('\t')
        || after_tag.starts_with('\n')
    {
        // <title lang="en"> — skip attributes until '>'
        match after_tag.find('>') {
            Some(end) => (&after_tag[end + 1..], end + 1),
            None => return String::new(),
        }
    } else {
        return String::new();
    };
    let content_start = tag_start + 6 + content_start_offset;
    if let Some(end) = content.find("</title>") {
        return html[content_start..content_start + end].trim().to_string();
    }
    String::new()
}

/// Extract a PDF's text layer via the pure-Rust `pdf-extract` parser.
///
/// The import layer only preserves the extractable text — OCR / visual
/// understanding of scanned or image-only PDFs is the compile Agent's job, so a
/// PDF with no text layer is surfaced as `Failed` with a clear reason (still
/// distinct from `Unsupported`).
fn extract_pdf(
    context: &ProjectContext,
    file_store: &FileStore,
    source_path: &Path,
    output_dir: &Path,
    original_name: &str,
) -> Result<ExtractResult, BackendError> {
    let pages = pdf_extract::extract_text_by_pages(source_path);
    match pages {
        Ok(pages) if !pages.is_empty() && pages.iter().any(|p| !p.trim().is_empty()) => {
            let page_count = pages.len() as u32;
            let text = pages
                .iter()
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            build_text_result(
                context,
                file_store,
                source_path,
                output_dir,
                original_name,
                SourceFileType::Pdf,
                &text,
                Some(page_count),
            )
        }
        Ok(_) => {
            // Empty vector or all-blank pages: the file has no extractable
            // text layer (likely scanned). Not a parse failure — OCR belongs
            // to the compile step.
            Ok(no_text_layer_result(original_name, SourceFileType::Pdf))
        }
        Err(error) => Ok(ExtractResult {
            original_name: original_name.to_string(),
            file_type: SourceFileType::Pdf,
            status: ExtractionStatus::Failed,
            error: Some(format!("PDF parsing failed: {error}")),
            text_preview: None,
            metadata: None,
            extracted_text_path: None,
            extracted_assets: vec![],
        }),
    }
}

/// Extract text from OOXML containers (DOCX/PPTX/XLSX), which are zip archives
/// of XML parts. Legacy binary `.doc/.ppt/.xls` have no in-scope pure-Rust
/// reader and degrade to `Failed` with a clear reason.
fn extract_ooxml(
    context: &ProjectContext,
    file_store: &FileStore,
    source_path: &Path,
    output_dir: &Path,
    original_name: &str,
    file_type: &SourceFileType,
) -> Result<ExtractResult, BackendError> {
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    // Legacy binary Office formats (.doc/.ppt/.xls) are OLE compound files,
    // not zip containers — the OOXML reader below would reject them. Surface a
    // clear failure instead of Unsupported so the preview shows an attempt.
    if matches!(ext.as_str(), "doc" | "ppt" | "xls") {
        let target = match ext.as_str() {
            "doc" => "docx",
            "ppt" => "pptx",
            "xls" => "xlsx",
            other => other,
        };
        return Ok(ExtractResult {
            original_name: original_name.to_string(),
            file_type: file_type.clone(),
            status: ExtractionStatus::Failed,
            error: Some(format!(
                "Legacy binary .{ext} is not supported by the OOXML text adapter. Convert to .{target} for text extraction."
            )),
            text_preview: None,
            metadata: None,
            extracted_text_path: None,
            extracted_assets: vec![],
        });
    }

    let file = fs::File::open(source_path).map_err(|error| {
        BackendError::new("EXTRACT_READ_FAILED", error.to_string(), true, false)
            .with_details(serde_json::json!({ "path": source_path.to_string_lossy() }))
    })?;
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => {
            return Ok(ExtractResult {
                original_name: original_name.to_string(),
                file_type: file_type.clone(),
                status: ExtractionStatus::Failed,
                error: Some(format!("Not a valid Office (zip) container: {error}")),
                text_preview: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
            });
        }
    };
    validate_archive_limits(&mut archive)?;

    let (text, page_count) = match file_type {
        SourceFileType::Document => (read_docx_text(&mut archive)?, None),
        SourceFileType::Presentation => {
            let (text, slide_count) = read_pptx_text(&mut archive)?;
            (text, Some(slide_count))
        }
        SourceFileType::Spreadsheet => (read_xlsx_text(&mut archive)?, None),
        other => {
            return Ok(ExtractResult {
                original_name: original_name.to_string(),
                file_type: other.clone(),
                status: ExtractionStatus::Unsupported,
                error: Some("No OOXML text adapter for this file type.".to_string()),
                text_preview: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
            });
        }
    };

    if text.trim().is_empty() {
        return Ok(no_text_layer_result(original_name, file_type.clone()));
    }

    build_text_result(
        context,
        file_store,
        source_path,
        output_dir,
        original_name,
        file_type.clone(),
        &text,
        page_count,
    )
}

/// Collect text runs from `word/document.xml` (DOCX): `<w:t>` element bodies.
fn read_docx_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<String, BackendError> {
    let mut buf = String::new();
    for name in [
        "word/document.xml",
        "word/footnotes.xml",
        "word/endnotes.xml",
    ] {
        if let Ok(mut entry) = archive.by_name(name) {
            ensure_entry_size(&entry)?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(io_read_err)?;
            let markdown = docx_xml_to_markdown(&xml)?;
            if !markdown.trim().is_empty() {
                if !buf.is_empty() {
                    buf.push_str("\n\n");
                }
                buf.push_str(markdown.trim());
            }
        }
    }
    Ok(buf)
}

fn docx_xml_to_markdown(xml: &str) -> Result<String, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut output = String::new();
    let mut paragraph = String::new();
    let mut heading_level = None;
    let mut is_list = false;
    let mut in_text = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut cell = String::new();
    let mut row = Vec::new();
    let mut table = Vec::new();
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"p" => {
                    paragraph.clear();
                    heading_level = None;
                    is_list = false;
                }
                b"pStyle" => heading_level = heading_level_from_attributes(&event),
                b"numPr" => is_list = true,
                b"t" => in_text = true,
                b"tbl" => {
                    in_table = true;
                    table.clear();
                }
                b"tr" => row.clear(),
                b"tc" => {
                    in_cell = true;
                    cell.clear();
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => match local_name(event.name().as_ref()) {
                b"pStyle" => heading_level = heading_level_from_attributes(&event),
                b"numPr" => is_list = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_text => {
                let value = text.unescape().map_err(xml_err)?;
                paragraph.push_str(&value);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"t" => in_text = false,
                b"p" => {
                    let value = paragraph.trim();
                    if in_cell {
                        if !cell.is_empty() && !value.is_empty() {
                            cell.push_str("<br>");
                        }
                        cell.push_str(value);
                    } else if !in_table && !value.is_empty() {
                        if let Some(level) = heading_level {
                            output.push_str(&"#".repeat(level));
                            output.push(' ');
                        } else if is_list {
                            output.push_str("- ");
                        }
                        output.push_str(value);
                        output.push_str("\n\n");
                    }
                }
                b"tc" => {
                    row.push(markdown_table_cell(&cell));
                    in_cell = false;
                }
                b"tr" => {
                    if !row.is_empty() {
                        table.push(std::mem::take(&mut row));
                    }
                }
                b"tbl" => {
                    output.push_str(&rows_to_markdown_table(&table, true));
                    output.push('\n');
                    in_table = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(output.trim().to_string())
}

fn heading_level_from_attributes(event: &quick_xml::events::BytesStart<'_>) -> Option<usize> {
    let value = event.attributes().flatten().find_map(|attribute| {
        (local_name(attribute.key.as_ref()) == b"val")
            .then(|| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
    })?;
    let lower = value.to_ascii_lowercase();
    let digits = lower.strip_prefix("heading")?.trim();
    digits.parse::<usize>().ok().map(|level| level.clamp(1, 6))
}

/// Collect slide text from `ppt/slides/slideN.xml` (PPTX): `<a:t>` bodies.
/// Returns the joined text and the slide count (used as the page count).
fn read_pptx_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<(String, u32), BackendError> {
    let names = archive_names(archive);
    let mut slides: Vec<&String> = names
        .iter()
        .filter(|n| {
            let n = n.as_str();
            n.starts_with("ppt/slides/slide") && n.ends_with(".xml")
        })
        .collect();
    slides.sort_by_key(|a| slide_index(a));
    let slide_count = slides.len() as u32;
    let mut buf = String::new();
    for (index, name) in slides.into_iter().enumerate() {
        if let Ok(mut entry) = archive.by_name(name) {
            ensure_entry_size(&entry)?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(io_read_err)?;
            let slide_text = pptx_slide_to_markdown(&xml)?;
            buf.push_str(&format!("## Slide {}\n", index + 1));
            if !slide_text.trim().is_empty() {
                buf.push_str(slide_text.trim());
                buf.push('\n');
            }
            buf.push('\n');
        }
    }
    Ok((buf, slide_count))
}

fn pptx_slide_to_markdown(xml: &str) -> Result<String, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut output = String::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    let mut is_bullet = false;
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"p" => {
                    paragraph.clear();
                    is_bullet = false;
                }
                b"t" => in_text = true,
                b"buChar" | b"buAutoNum" => is_bullet = true,
                _ => {}
            },
            Ok(Event::Empty(event))
                if matches!(local_name(event.name().as_ref()), b"buChar" | b"buAutoNum") =>
            {
                is_bullet = true;
            }
            Ok(Event::Text(text)) if in_text => {
                paragraph.push_str(&text.unescape().map_err(xml_err)?);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"t" => in_text = false,
                b"p" if !paragraph.trim().is_empty() => {
                    if is_bullet {
                        output.push_str("- ");
                    }
                    output.push_str(paragraph.trim());
                    output.push('\n');
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(output)
}

/// Collect cell values from an XLSX: shared strings (`xl/sharedStrings.xml`,
/// `<t>` bodies) plus inline worksheet strings (`xl/worksheets/sheetN.xml`,
/// `<v>` numeric/string bodies and `<is><t>` inline strings).
fn read_xlsx_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<String, BackendError> {
    let mut shared: Vec<String> = Vec::new();
    if let Ok(mut entry) = archive.by_name("xl/sharedStrings.xml") {
        ensure_entry_size(&entry)?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml).map_err(io_read_err)?;
        shared = read_shared_strings(&xml)?;
    }

    let names = archive_names(archive);
    let mut sheets: Vec<&String> = names
        .iter()
        .filter(|n| {
            let n = n.as_str();
            n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")
        })
        .collect();
    sheets.sort_by_key(|a| sheet_index(a));

    let mut buf = String::new();
    for name in sheets {
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        ensure_entry_size(&entry)?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml).map_err(io_read_err)?;

        let rows = read_xlsx_rows(&xml, &shared)?;
        buf.push_str(&format!("## Sheet {}\n\n", sheet_index(name)));
        buf.push_str(&rows_to_markdown_table(&rows, true));
        buf.push('\n');
    }
    Ok(buf)
}

fn read_shared_strings(xml: &str) -> Result<Vec<String>, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_text = false;
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"si" => {
                    in_si = true;
                    current.clear();
                }
                b"t" if in_si => in_text = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_text => {
                current.push_str(&text.unescape().map_err(xml_err)?);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"t" => in_text = false,
                b"si" => {
                    strings.push(std::mem::take(&mut current));
                    in_si = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(strings)
}

fn read_xlsx_rows(xml: &str, shared: &[String]) -> Result<Vec<Vec<String>>, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell_column = 0usize;
    let mut cell_type = String::new();
    let mut cell_value = String::new();
    let mut in_value = false;
    let mut in_inline_text = false;
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"row" => row.clear(),
                b"c" => {
                    cell_type.clear();
                    cell_value.clear();
                    cell_column = row.len();
                    for attribute in event.attributes().flatten() {
                        match local_name(attribute.key.as_ref()) {
                            b"r" => {
                                cell_column = cell_column_index(&String::from_utf8_lossy(
                                    attribute.value.as_ref(),
                                ))?;
                            }
                            b"t" => {
                                cell_type =
                                    String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
                            }
                            _ => {}
                        }
                    }
                }
                b"v" => in_value = true,
                b"t" => in_inline_text = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_value || in_inline_text => {
                cell_value.push_str(&text.unescape().map_err(xml_err)?);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"v" => in_value = false,
                b"t" => in_inline_text = false,
                b"c" => {
                    row.resize(cell_column + 1, String::new());
                    let value = if cell_type == "s" {
                        let index = cell_value.trim().parse::<usize>().map_err(|_| {
                            BackendError::new(
                                "EXTRACT_XLSX_SHARED_STRING_INVALID",
                                "An XLSX shared-string cell contains an invalid index.",
                                true,
                                false,
                            )
                        })?;
                        shared.get(index).cloned().ok_or_else(|| {
                            BackendError::new(
                                "EXTRACT_XLSX_SHARED_STRING_INVALID",
                                "An XLSX shared-string index is out of range.",
                                true,
                                false,
                            )
                        })?
                    } else {
                        cell_value.clone()
                    };
                    row[cell_column] = markdown_table_cell(&value);
                }
                b"row" => rows.push(std::mem::take(&mut row)),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(rows)
}

fn cell_column_index(reference: &str) -> Result<usize, BackendError> {
    let letters = reference
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .collect::<Vec<_>>();
    let column = letters.iter().try_fold(0usize, |value, byte| {
        value
            .checked_mul(26)
            .and_then(|value| value.checked_add(usize::from(byte.to_ascii_uppercase() - b'A' + 1)))
    });
    match column {
        Some(1..=16_384) if !letters.is_empty() => Ok(column.unwrap() - 1),
        _ => Err(BackendError::new(
            "EXTRACT_XLSX_CELL_REFERENCE_INVALID",
            "An XLSX cell reference exceeds Excel column limits.",
            true,
            false,
        )),
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn archive_names<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
        .collect()
}

fn slide_index(name: &str) -> u32 {
    let trimmed = name
        .trim_start_matches("ppt/slides/slide")
        .trim_end_matches(".xml");
    trimmed.parse().unwrap_or(0)
}

fn sheet_index(name: &str) -> u32 {
    let trimmed = name
        .trim_start_matches("xl/worksheets/sheet")
        .trim_end_matches(".xml");
    trimmed.parse().unwrap_or(0)
}

fn io_read_err(error: std::io::Error) -> BackendError {
    BackendError::new("EXTRACT_READ_FAILED", error.to_string(), true, false)
}

fn xml_err(error: quick_xml::Error) -> BackendError {
    BackendError::new("EXTRACT_XML_PARSE_FAILED", error.to_string(), true, false)
}

/// Upper bound on a single decompressed OOXML entry. Guards against zip bombs
/// (a tiny compressed entry decompressing to gigabytes) by refusing to buffer
/// an entry larger than this into memory before parsing.
const MAX_SOURCE_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OOXML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OOXML_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OOXML_ENTRIES: usize = 4_096;

fn ensure_source_file_size(path: &Path) -> Result<(), BackendError> {
    let size = fs::metadata(path).map_err(io_read_err)?.len();
    if size > MAX_SOURCE_FILE_BYTES {
        return Err(BackendError::new(
            "EXTRACT_SOURCE_TOO_LARGE",
            "The source file exceeds the 64 MiB extraction limit.",
            true,
            false,
        )
        .with_details(serde_json::json!({ "path": path.to_string_lossy(), "sizeBytes": size })));
    }
    Ok(())
}

fn validate_archive_limits<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<(), BackendError> {
    if archive.len() > MAX_OOXML_ENTRIES {
        return Err(BackendError::new(
            "EXTRACT_ARCHIVE_TOO_LARGE",
            "The Office archive contains too many entries.",
            true,
            false,
        ));
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            BackendError::new(
                "EXTRACT_ARCHIVE_READ_FAILED",
                error.to_string(),
                true,
                false,
            )
        })?;
        ensure_entry_size(&entry)?;
        total = total.checked_add(entry.size()).ok_or_else(|| {
            BackendError::new(
                "EXTRACT_ARCHIVE_TOO_LARGE",
                "The Office archive size overflowed the extraction limit.",
                true,
                false,
            )
        })?;
        if total > MAX_OOXML_TOTAL_BYTES {
            return Err(BackendError::new(
                "EXTRACT_ARCHIVE_TOO_LARGE",
                "The Office archive expands beyond the 64 MiB extraction limit.",
                true,
                false,
            ));
        }
    }
    Ok(())
}

/// Reject a zip entry whose declared uncompressed size exceeds the safety cap.
fn ensure_entry_size(entry: &zip::read::ZipFile<'_>) -> Result<(), BackendError> {
    ensure_entry_size_value(entry.size())
}

fn ensure_entry_size_value(size: u64) -> Result<(), BackendError> {
    if size > MAX_OOXML_ENTRY_BYTES {
        return Err(BackendError::new(
            "EXTRACT_ENTRY_TOO_LARGE",
            "An Office XML part is too large to extract safely.",
            true,
            false,
        ));
    }
    Ok(())
}

/// Build the standard `Extracted` result from a fully-extracted text buffer,
/// writing the text to `raw/extracted/` and counting words.
#[allow(clippy::too_many_arguments)]
fn build_text_result(
    context: &ProjectContext,
    file_store: &FileStore,
    source_path: &Path,
    output_dir: &Path,
    original_name: &str,
    file_type: SourceFileType,
    text: &str,
    page_count: Option<u32>,
) -> Result<ExtractResult, BackendError> {
    let word_count = count_words(text);
    let preview = take_preview(text, 500);
    let extracted_text_path =
        write_extracted_text(context, file_store, source_path, output_dir, text)?;
    Ok(ExtractResult {
        original_name: original_name.to_string(),
        file_type,
        status: ExtractionStatus::Extracted,
        error: None,
        text_preview: Some(preview),
        metadata: Some(SourceMetadata {
            title: None,
            author: None,
            created: None,
            modified: None,
            page_count,
            word_count: Some(word_count),
            language: None,
        }),
        extracted_text_path: Some(extracted_text_path),
        extracted_assets: vec![],
    })
}

/// A file with no extractable text layer (scanned PDF, image-only Office
/// doc). Surfaced as `Failed` with an explicit reason pointing to the compile
/// Agent — distinct from `Unsupported`, so the preview reflects that the file
/// was parsed and simply has no text, not that no adapter exists.
fn no_text_layer_result(original_name: &str, file_type: SourceFileType) -> ExtractResult {
    ExtractResult {
        original_name: original_name.to_string(),
        file_type,
        status: ExtractionStatus::Failed,
        error: Some(
            "No extractable text layer found. OCR / visual understanding is handled by the compile Agent."
                .to_string(),
        ),
        text_preview: None,
        metadata: None,
        extracted_text_path: None,
        extracted_assets: vec![],
    }
}

fn write_extracted_text(
    context: &ProjectContext,
    file_store: &FileStore,
    source_path: &Path,
    output_dir: &Path,
    text: &str,
) -> Result<String, BackendError> {
    file_store.ensure_absolute_dir(output_dir)?;

    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("source");
    let mut hasher = Sha256::new();
    hasher.update(source_path.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(text.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let filename = format!("{}-{}.md", sanitize_filename(stem), &hash[..8]);
    let output_path = output_dir.join(filename);

    let relative = output_path
        .strip_prefix(&context.root)
        .map_err(|_| {
            BackendError::new(
                "EXTRACT_OUTPUT_PATH_INVALID",
                "Extracted text path must remain inside the project root.",
                false,
                true,
            )
        })?
        .to_string_lossy()
        .replace('\\', "/");

    if !relative.starts_with("raw/extracted/") {
        return Err(BackendError::new(
            "EXTRACT_OUTPUT_PATH_INVALID",
            "Extracted text must be written under raw/extracted.",
            false,
            true,
        )
        .with_details(serde_json::json!({ "path": relative })));
    }

    if output_path.exists() {
        let existing = fs::read_to_string(&output_path).map_err(|error| {
            BackendError::new("EXTRACT_OUTPUT_READ_FAILED", error.to_string(), true, false)
                .with_details(serde_json::json!({ "path": relative }))
        })?;
        if existing != text {
            return Err(BackendError::new(
                "EXTRACT_OUTPUT_CONFLICT",
                "The extracted Markdown was externally edited and will not be overwritten during preview.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": relative })));
        }
        return Ok(relative);
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)
    {
        Ok(mut file) => file.write_all(text.as_bytes()).map_err(|error| {
            BackendError::new(
                "EXTRACT_OUTPUT_WRITE_FAILED",
                error.to_string(),
                true,
                false,
            )
            .with_details(serde_json::json!({ "path": relative }))
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read_to_string(&output_path).map_err(|read_error| {
                BackendError::new(
                    "EXTRACT_OUTPUT_READ_FAILED",
                    read_error.to_string(),
                    true,
                    false,
                )
                .with_details(serde_json::json!({ "path": relative }))
            })?;
            if existing != text {
                return Err(BackendError::new(
                    "EXTRACT_OUTPUT_CONFLICT",
                    "The extracted Markdown changed while previewing and will not be overwritten.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": relative })));
            }
        }
        Err(error) => {
            return Err(BackendError::new(
                "EXTRACT_OUTPUT_WRITE_FAILED",
                error.to_string(),
                true,
                false,
            )
            .with_details(serde_json::json!({ "path": relative })));
        }
    }
    Ok(relative)
}

fn sanitize_filename(stem: &str) -> String {
    let sanitized: String = stem
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => ch,
        })
        .collect();
    if sanitized.trim().is_empty() {
        "source".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import::ExtractionStatus;
    use crate::models::paths::ProjectContext;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-extract-{stamp}-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    // ── Text extraction tests ──

    #[test]
    fn extracts_markdown_text() {
        let (context, root) = tmp_context("md-extract");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("test.md");
        fs::write(&source, "# Hello\n\nThis is a **markdown** file.\n").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert!(result.text_preview.unwrap().contains("Hello"));
        assert!(result.metadata.unwrap().word_count.unwrap() > 0);
        let extracted_path = result.extracted_text_path.unwrap();
        assert!(extracted_path.starts_with("raw/extracted/"));
        assert!(root.join(extracted_path).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_txt_text() {
        let (context, root) = tmp_context("txt-extract");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("notes.txt");
        fs::write(&source, "Plain text content\nwith multiple lines.\n").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert!(result.text_preview.unwrap().contains("Plain text"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracted_artifact_is_markdown_and_cjk_safe() {
        let (context, root) = tmp_context("markdown-artifact");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("研究资料.txt");
        fs::write(&source, "第一段内容\nsecond paragraph\n").unwrap();

        let result = ExtractionService
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        let relative = result.extracted_text_path.unwrap();
        assert!(relative.starts_with("raw/extracted/研究资料-"));
        assert!(relative.ends_with(".md"));
        assert!(!relative.contains('\\'));
        assert_eq!(
            fs::read_to_string(root.join(&relative)).unwrap(),
            "第一段内容\nsecond paragraph\n"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_does_not_overwrite_an_externally_edited_extracted_markdown() {
        let (context, root) = tmp_context("extracted-conflict");
        let source = root.join("notes.txt");
        let out_dir = root.join("raw/extracted");
        fs::write(&source, "original source").unwrap();
        let first = ExtractionService
            .extract_text(&context, &FileStore, &source, &out_dir)
            .unwrap();
        let extracted = root.join(first.extracted_text_path.unwrap());
        fs::write(&extracted, "external edit").unwrap();

        let error = ExtractionService
            .extract_text(&context, &FileStore, &source, &out_dir)
            .unwrap_err();

        assert_eq!(error.code, "EXTRACT_OUTPUT_CONFLICT");
        assert_eq!(fs::read_to_string(extracted).unwrap(), "external edit");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_csv_text() {
        let (context, root) = tmp_context("csv-extract");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("data.csv");
        fs::write(&source, "name,age,city\nAlice,30,NYC\nBob,25,LA\n").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert!(result.text_preview.unwrap().contains("Alice"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn csv_is_converted_to_markdown_table() {
        let (context, root) = tmp_context("csv-markdown");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("people.csv");
        fs::write(
            &source,
            "name,note\nAlice,\"hello, world\"\nBob,\"left | right\"\n",
        )
        .unwrap();

        let result = ExtractionService
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();
        let markdown = fs::read_to_string(root.join(result.extracted_text_path.unwrap())).unwrap();

        assert!(markdown.contains("| name | note |"));
        assert!(markdown.contains("| --- | --- |"));
        assert!(markdown.contains("| Alice | hello, world |"));
        assert!(markdown.contains("| Bob | left \\| right |"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_html_stripping_tags() {
        let (context, root) = tmp_context("html-extract");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("page.html");
        fs::write(
            &source,
            "<html><head><title>Test Page</title></head><body><h1>Hello</h1><p>World</p></body></html>",
        )
        .unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        let preview = result.text_preview.unwrap();
        assert!(preview.contains("Hello"));
        assert!(preview.contains("World"));
        assert!(!preview.contains("<h1>"));
        assert!(!preview.contains("<html>"));

        let meta = result.metadata.unwrap();
        assert_eq!(meta.title, Some("Test Page".to_string()));

        fs::remove_dir_all(root).unwrap();
    }

    // ── PDF / Office adapter tests ──

    #[test]
    fn corrupt_pdf_extraction_fails_with_a_clear_reason_not_unsupported() {
        // A non-PDF byte payload cannot be parsed by the PDF adapter. It must
        // surface as Failed (an explicit parse attempt), never Unsupported —
        // PRD-IMP-001 requires the preview to distinguish "tried and failed"
        // from "no adapter".
        let (context, root) = tmp_context("pdf-corrupt");
        let store = FileStore;
        let source = root.join("doc.pdf");
        fs::write(&source, b"not actually a pdf").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Failed);
        assert_ne!(result.status, ExtractionStatus::Unsupported);
        assert!(result.error.unwrap().to_lowercase().contains("pdf"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_docx_extraction_fails_not_unsupported() {
        // Bytes that are not a zip container cannot be an OOXML document.
        let (context, root) = tmp_context("docx-invalid");
        let store = FileStore;
        let source = root.join("report.docx");
        fs::write(&source, b"fake docx content").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Failed);
        assert_ne!(result.status, ExtractionStatus::Unsupported);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_ooxml_container_is_rejected_before_archive_read() {
        let (context, root) = tmp_context("ooxml-container-limit");
        let source = root.join("huge.docx");
        let file = fs::File::create(&source).unwrap();
        file.set_len(MAX_SOURCE_FILE_BYTES + 1).unwrap();

        let error = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap_err();

        assert_eq!(error.code, "EXTRACT_SOURCE_TOO_LARGE");
        assert!(!root.join("raw/extracted").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_binary_office_formats_fail_with_conversion_hint() {
        let (context, root) = tmp_context("legacy-doc");
        let store = FileStore;
        let source = root.join("legacy.doc");
        fs::write(&source, b"D0CF11E0A1B11AE1 legacy ole bytes").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Failed);
        let error = result.error.unwrap();
        assert!(error.contains("Convert to .docx"), "error was: {error}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_xls_hint_targets_xlsx_not_xlx() {
        // Regression: a naive `trim_end_matches('s')` produced ".xlx" for the
        // .xls case. Each legacy extension must map to its real OOXML target.
        let (context, root) = tmp_context("legacy-xls");
        let store = FileStore;
        for (ext, target) in [("doc", "docx"), ("ppt", "pptx"), ("xls", "xlsx")] {
            let source = root.join(format!("legacy.{ext}"));
            fs::write(&source, b"legacy ole bytes").unwrap();
            let result = ExtractionService
                .extract_text(&context, &store, &source, &root)
                .unwrap();
            assert_eq!(result.status, ExtractionStatus::Failed);
            let error = result.error.unwrap();
            assert!(
                error.contains(&format!("Convert to .{target}")),
                "{ext} hint should target .{target}, error was: {error}"
            );
            assert!(!error.contains(".xlx"), ".xls must not produce .xlx");
            let _ = fs::remove_file(&source);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xlsx_numeric_cell_is_not_misread_as_a_shared_string() {
        // Regression (BLOCKER): a numeric cell whose <v> value happens to be a
        // valid shared-string index must emit the literal number, not the
        // shared string. With 3 shared strings, a numeric cell <v>1</v> would
        // have been mis-emitted as the 2nd shared string ("World") before the
        // cell-type-aware fix.
        let (context, root) = tmp_context("xlsx-numeric");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("data.xlsx");
        let shared = vec!["Hello".to_string(), "World".to_string(), "Pi".to_string()];
        // A1: shared string index 1 -> "World". B1: numeric 1 (NOT shared).
        let sheet = r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="s"><v>1</v></c><c r="B1"><v>1</v></c></row>
</sheetData>
</worksheet>"#;
        fs::write(&source, sample_xlsx(&shared, sheet)).unwrap();

        let result = ExtractionService
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        let preview = result.text_preview.unwrap();
        // Shared-string cell resolved to "World".
        assert!(preview.contains("World"));
        // Numeric cell "1" must appear literally; the preview must NOT contain
        // a second "World" (which the buggy version would have emitted).
        assert_eq!(preview.matches("World").count(), 1);
        assert!(preview.contains('1'));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xlsx_inline_string_cells_are_extracted() {
        // Regression: inline-string cells (t="inlineStr", <is><t>...</t></is>)
        // were silently dropped because <is> was not <v>. They must now emit
        // their text alongside shared-string and numeric cells.
        let (context, root) = tmp_context("xlsx-inline");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("data.xlsx");
        let shared = vec!["Shared".to_string()];
        let sheet = r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="inlineStr"><is><t>Inline</t></is></c></row>
</sheetData>
</worksheet>"#;
        fs::write(&source, sample_xlsx(&shared, sheet)).unwrap();

        let result = ExtractionService
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        let preview = result.text_preview.unwrap();
        assert!(preview.contains("Shared"));
        assert!(preview.contains("Inline"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xlsx_rejects_out_of_range_shared_string_indexes() {
        let (context, root) = tmp_context("xlsx-shared-range");
        let source = root.join("bad-shared.xlsx");
        let sheet = r#"<worksheet xmlns="x"><sheetData><row r="1"><c r="A1" t="s"><v>99</v></c></row></sheetData></worksheet>"#;
        fs::write(&source, sample_xlsx(&["only".to_string()], sheet)).unwrap();

        let error = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap_err();

        assert_eq!(error.code, "EXTRACT_XLSX_SHARED_STRING_INVALID");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xlsx_rejects_columns_beyond_excel_limits() {
        let error = cell_column_index("ZZZZZ1").unwrap_err();
        assert_eq!(error.code, "EXTRACT_XLSX_CELL_REFERENCE_INVALID");
    }

    #[test]
    fn images_are_successfully_archived_without_fake_text_extraction() {
        let (context, root) = tmp_context("image-archive");
        let source = root.join("diagram.png");
        fs::write(&source, b"not decoded during import").unwrap();

        let result = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert_eq!(result.extracted_text_path, None);
        assert!(result.metadata.is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn docx_text_is_extracted_to_raw_extracted() {
        let (context, root) = tmp_context("docx-real");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("report.docx");
        fs::write(&source, sample_docx("Hello Wiki", "Second paragraph here.")).unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        let preview = result.text_preview.unwrap();
        assert!(preview.contains("Hello Wiki"));
        assert!(preview.contains("Second paragraph"));
        let meta = result.metadata.unwrap();
        assert!(meta.word_count.unwrap() >= 4);
        // DOCX page count is unreliable — None per PRD-IMP-004 "pages or words".
        assert_eq!(meta.page_count, None);
        let extracted_path = result.extracted_text_path.unwrap();
        assert!(extracted_path.starts_with("raw/extracted/"));
        assert!(root.join(&extracted_path).exists());
        let on_disk = fs::read_to_string(root.join(extracted_path)).unwrap();
        assert!(on_disk.contains("Hello Wiki"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pdf_text_layer_is_extracted_to_markdown() {
        let (context, root) = tmp_context("pdf-text-layer");
        let source = root.join("paper.pdf");
        fs::write(&source, sample_text_pdf(Some("Hello PDF Wiki"))).unwrap();

        let result = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert!(result.text_preview.unwrap().contains("Hello PDF Wiki"));
        assert_eq!(result.metadata.unwrap().page_count, Some(1));
        assert!(result.extracted_text_path.unwrap().ends_with(".md"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scanned_pdf_has_actionable_ocr_handoff_without_fake_markdown() {
        let (context, root) = tmp_context("pdf-scanned");
        let source = root.join("scan.pdf");
        fs::write(&source, sample_text_pdf(None)).unwrap();

        let result = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Failed);
        assert_ne!(result.status, ExtractionStatus::Unsupported);
        assert!(result.error.unwrap().contains("OCR"));
        assert_eq!(result.extracted_text_path, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_ooxml_entry_is_rejected_before_read() {
        let error = ensure_entry_size_value(MAX_OOXML_ENTRY_BYTES + 1).unwrap_err();
        assert_eq!(error.code, "EXTRACT_ENTRY_TOO_LARGE");
    }

    #[test]
    fn docx_preserves_headings_lists_and_tables_as_markdown() {
        let (context, root) = tmp_context("docx-structure");
        let source = root.join("structured.docx");
        let document = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Research</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First item</w:t></w:r></w:p>
<w:p><w:r><w:t>Normal paragraph.</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Alpha</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>42</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:body></w:document>"#;
        fs::write(
            &source,
            zip_to_bytes(&[
                ("[Content_Types].xml", "<Types/>"),
                ("word/document.xml", document),
            ]),
        )
        .unwrap();

        let result = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap();
        let markdown = fs::read_to_string(root.join(result.extracted_text_path.unwrap())).unwrap();

        assert!(markdown.contains("# Research"));
        assert!(markdown.contains("- First item"));
        assert!(markdown.contains("Normal paragraph."));
        assert!(markdown.contains("| Name | Value |"));
        assert!(markdown.contains("| --- | --- |"));
        assert!(markdown.contains("| Alpha | 42 |"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pptx_text_is_extracted_with_slide_count_as_page_count() {
        let (context, root) = tmp_context("pptx-real");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("deck.pptx");
        let slides = vec!["Intro slide".to_string(), "Details slide".to_string()];
        fs::write(&source, sample_pptx(&slides)).unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        let preview = result.text_preview.unwrap();
        assert!(preview.contains("Intro slide"));
        assert!(preview.contains("Details slide"));
        let meta = result.metadata.unwrap();
        // Slide count is a meaningful "page" surrogate for presentations.
        assert_eq!(meta.page_count, Some(2));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pptx_emits_numbered_slide_sections_and_bullets() {
        let (context, root) = tmp_context("pptx-structure");
        let source = root.join("deck.pptx");
        let slide_one = r#"<p:sld xmlns:a="a" xmlns:p="p"><p:cSld><a:p><a:r><a:t>Intro</a:t></a:r></a:p><a:p><a:pPr><a:buChar char="•"/></a:pPr><a:r><a:t>Key point</a:t></a:r></a:p></p:cSld></p:sld>"#;
        let slide_two = r#"<p:sld xmlns:a="a" xmlns:p="p"><p:cSld><a:p><a:r><a:t>Details</a:t></a:r></a:p></p:cSld></p:sld>"#;
        fs::write(
            &source,
            zip_to_bytes(&[
                ("[Content_Types].xml", "<Types/>"),
                ("ppt/slides/slide2.xml", slide_two),
                ("ppt/slides/slide1.xml", slide_one),
            ]),
        )
        .unwrap();

        let result = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap();
        let markdown = fs::read_to_string(root.join(result.extracted_text_path.unwrap())).unwrap();

        assert!(markdown.starts_with("## Slide 1\n"));
        assert!(markdown.contains("Intro\n"));
        assert!(markdown.contains("- Key point"));
        assert!(markdown.contains("## Slide 2\nDetails"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xlsx_cell_values_are_extracted_including_shared_strings() {
        let (context, root) = tmp_context("xlsx-real");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");
        let source = root.join("data.xlsx");
        // Two shared strings; sheet1 references index 0 (Hello) and a numeric 42.
        let shared = vec!["Hello".to_string(), "World".to_string()];
        let sheet = r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row>
</sheetData>
</worksheet>"#;
        fs::write(&source, sample_xlsx(&shared, sheet)).unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &out_dir)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        let preview = result.text_preview.unwrap();
        assert!(preview.contains("Hello"));
        assert!(preview.contains("42"));
        let meta = result.metadata.unwrap();
        assert!(meta.word_count.unwrap() >= 2);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn xlsx_preserves_missing_columns_in_markdown_table() {
        let (context, root) = tmp_context("xlsx-layout");
        let source = root.join("layout.xlsx");
        let sheet = r#"<worksheet xmlns="x"><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1"><v>7</v></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>Beta</t></is></c><c r="B2"><v>1</v></c></row>
</sheetData></worksheet>"#;
        fs::write(&source, sample_xlsx(&["Name".to_string()], sheet)).unwrap();

        let result = ExtractionService
            .extract_text(&context, &FileStore, &source, &root.join("raw/extracted"))
            .unwrap();
        let markdown = fs::read_to_string(root.join(result.extracted_text_path.unwrap())).unwrap();

        assert!(markdown.contains("## Sheet 1"));
        assert!(markdown.contains("| Name |  | 7 |"));
        assert!(markdown.contains("| --- | --- | --- |"));
        assert!(markdown.contains("| Beta | 1 |  |"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_text_layer_result_routes_ocr_to_compile_agent() {
        // Directly exercises the empty-text-layer branch: a PDF/Office doc that
        // parses but yields no text must surface Failed (not Unsupported) with
        // a reason pointing OCR to the compile Agent.
        let result = no_text_layer_result("scan.pdf", SourceFileType::Pdf);
        assert_eq!(result.status, ExtractionStatus::Failed);
        assert_ne!(result.status, ExtractionStatus::Unsupported);
        assert_eq!(result.file_type, SourceFileType::Pdf);
        assert!(result.error.unwrap().contains("compile Agent"));
    }

    // ── Batch extraction tests ──

    #[test]
    fn batch_extraction_continues_on_failure() {
        let (context, root) = tmp_context("batch-extract");
        let store = FileStore;
        let out_dir = root.join("raw/extracted");

        let good = root.join("good.md");
        let bad = PathBuf::from("/nonexistent/file.pdf");

        fs::write(&good, "# Good content\n").unwrap();

        let service = ExtractionService;
        let results = service.extract_batch(
            &context,
            &store,
            &[
                good.to_string_lossy().to_string(),
                bad.to_string_lossy().to_string(),
            ],
            &out_dir,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, ExtractionStatus::Extracted);
        // The nonexistent file should be Failed, not crash the batch
        assert_eq!(results[1].status, ExtractionStatus::Failed);
        assert!(results[1].error.as_ref().unwrap().contains("not found"));

        fs::remove_dir_all(root).unwrap();
    }

    // ── Utility function tests ──

    #[test]
    fn count_words_handles_multilingual_text() {
        assert_eq!(count_words("Hello world"), 2);
        assert_eq!(count_words("你好世界"), 1);
        assert_eq!(count_words("Hello 世界"), 2);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn take_preview_truncates_at_max_chars() {
        let text = "a".repeat(1000);
        let preview = take_preview(&text, 100);
        assert!(preview.len() <= 104); // 100 + "…"
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn take_preview_does_not_truncate_short_text() {
        let text = "Short text";
        let preview = take_preview(text, 500);
        assert_eq!(preview, "Short text");
    }

    #[test]
    fn strip_html_tags_removes_all_tags() {
        let html = "<div><p>Hello <b>World</b></p></div>";
        let text = strip_html_tags(html);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn extract_html_title_finds_title() {
        let html = "<html><head><title>My Page</title></head><body></body></html>";
        assert_eq!(extract_html_title(html), "My Page");
    }

    #[test]
    fn extract_html_title_returns_empty_when_missing() {
        assert_eq!(extract_html_title("<html><body>No title</body></html>"), "");
    }

    #[test]
    fn extract_html_title_handles_attributes() {
        let html = r#"<html><head><title lang="en">My Page</title></head><body></body></html>"#;
        assert_eq!(extract_html_title(html), "My Page");
    }

    #[test]
    fn extract_html_title_handles_multiline_attributes() {
        let html = "<html><head><title\n  data-page=\"home\"\n>My Page</title></head></html>";
        assert_eq!(extract_html_title(html), "My Page");
    }

    // ── Sample file generators (minimal valid OOXML / PDF fixtures) ──

    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn zip_to_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, content) in entries {
                zip.start_file(name, opts).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    fn sample_text_pdf(text: Option<&str>) -> Vec<u8> {
        let stream = text
            .map(|value| format!("BT /F1 12 Tf 72 720 Td ({value}) Tj ET"))
            .unwrap_or_default();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    /// Build a minimal valid DOCX with the given paragraphs as `word/document.xml`.
    fn sample_docx(first: &str, second: &str) -> Vec<u8> {
        let document = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t>{first}</w:t></w:r></w:p>
<w:p><w:r><w:t>{second}</w:t></w:r></w:p>
</w:body>
</w:document>"#
        );
        zip_to_bytes(&[
            (
                "[Content_Types].xml",
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>",
            ),
            ("word/document.xml", &document),
        ])
    }

    /// Build a minimal valid PPTX with one slide per text entry.
    fn sample_pptx(slides: &[String]) -> Vec<u8> {
        let mut owned_names: Vec<String> = Vec::new();
        let mut entries: Vec<(String, String)> = Vec::new();
        entries.push((
            "[Content_Types].xml".to_string(),
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .to_string(),
        ));
        for (i, text) in slides.iter().enumerate() {
            let name = format!("ppt/slides/slide{}.xml", i + 1);
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld>
</p:sld>"#
            );
            owned_names.push(name.clone());
            entries.push((name, xml));
        }
        let refs: Vec<(&str, &str)> = entries
            .iter()
            .map(|(name, content)| (name.as_str(), content.as_str()))
            .collect();
        zip_to_bytes(&refs)
    }

    /// Build a minimal valid XLSX with shared strings and one worksheet.
    fn sample_xlsx(shared: &[String], sheet_xml: &str) -> Vec<u8> {
        let count = shared.len();
        let mut sst = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{count}\" uniqueCount=\"{count}\">"
        );
        for s in shared {
            sst.push_str(&format!("<si><t>{s}</t></si>"));
        }
        sst.push_str("</sst>");
        zip_to_bytes(&[
            (
                "[Content_Types].xml",
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>",
            ),
            ("xl/sharedStrings.xml", &sst),
            ("xl/worksheets/sheet1.xml", sheet_xml),
        ])
    }
}

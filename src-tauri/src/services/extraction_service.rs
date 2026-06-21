use std::fs;
use std::io::{Cursor, Read};
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

        match &file_type {
            SourceFileType::Markdown
            | SourceFileType::Text
            | SourceFileType::Csv
            | SourceFileType::Url => {
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
            _ => {
                // Images, unknown: nothing to extract in MVP
                Ok(ExtractResult {
                    original_name,
                    file_type: file_type.clone(),
                    status: ExtractionStatus::Unsupported,
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
    let bytes = match fs::read(source_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(ExtractResult {
                original_name: original_name.to_string(),
                file_type: file_type.clone(),
                status: ExtractionStatus::Failed,
                error: Some(format!("Failed to read file: {error}")),
                text_preview: None,
                metadata: None,
                extracted_text_path: None,
                extracted_assets: vec![],
            });
        }
    };

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

    let cursor = Cursor::new(bytes);
    let mut archive = match ZipArchive::new(cursor) {
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
    for name in ["word/document.xml", "word/footnotes.xml", "word/endnotes.xml"] {
        if let Ok(mut entry) = archive.by_name(name) {
            ensure_entry_size(&entry)?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(io_read_err)?;
            buf.push_str(&collect_element_text(&xml, &["w:t"]));
            buf.push('\n');
        }
    }
    Ok(buf)
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
    for name in slides {
        if let Ok(mut entry) = archive.by_name(name) {
            ensure_entry_size(&entry)?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(io_read_err)?;
            let slide_text = collect_element_text(&xml, &["a:t"]);
            if !slide_text.trim().is_empty() {
                buf.push_str(&slide_text);
                buf.push_str("\n\n");
            }
        }
    }
    Ok((buf, slide_count))
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
        shared = collect_element_text_split(&xml, &["t"]);
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

        let mut reader = Reader::from_str(&xml);
        reader.config_mut().trim_text(true);
        // Stack of element local names (owned bytes, because the reader reuses
        // its read buffer) so Text events know their enclosing element. A
        // separate `cell_shared` flag tracks whether the current `<c>` cell is
        // `t="s"` (shared-string index): `<v>` bodies are only resolved against
        // `shared` when this is true, otherwise the literal value is emitted.
        // Without this check a numeric cell whose value happens to be a valid
        // shared-string index would be mis-emitted.
        let mut element_stack: Vec<Vec<u8>> = Vec::new();
        let mut cell_shared = false;
        let mut buf_small = Vec::new();
        loop {
            match reader.read_event_into(&mut buf_small) {
                Ok(Event::Start(e)) => {
                    let local = e.name().as_ref().to_vec();
                    if local == b"c" {
                        // Cell type lives on the <c> element's `t` attribute.
                        cell_shared = e.attributes().flatten().any(|attr| {
                            attr.key.as_ref() == b"t" && attr.value.as_ref() == b"s"
                        });
                    }
                    element_stack.push(local);
                }
                Ok(Event::Empty(e)) => {
                    if e.name().as_ref() == b"c" {
                        cell_shared = false;
                    }
                }
                Ok(Event::Text(t)) => {
                    let text = t.unescape().map_err(xml_err)?.into_owned();
                    if text.trim().is_empty() {
                        continue;
                    }
                    let inside_v = element_stack
                        .last()
                        .map(|n| n.as_slice() == b"v")
                        .unwrap_or(false);
                    let inside_t = element_stack
                        .last()
                        .map(|n| n.as_slice() == b"t")
                        .unwrap_or(false);
                    if inside_v {
                        if cell_shared {
                            if let Ok(idx) = text.trim().parse::<usize>() {
                                if let Some(shared) = shared.get(idx) {
                                    buf.push_str(shared);
                                    buf.push('\t');
                                    continue;
                                }
                            }
                            // Shared-string index out of range: emit nothing
                            // rather than a misleading raw index.
                            continue;
                        }
                        // Numeric (or t="str"/t="e") cell: emit the literal.
                        buf.push_str(&text);
                        buf.push('\t');
                    } else if inside_t {
                        // <is><t>...</t></is> inline string, or stray <t>: emit
                        // the text so inline-string cells are not dropped.
                        buf.push_str(&text);
                        buf.push('\t');
                    }
                }
                Ok(Event::End(e)) => {
                    if e.name().as_ref() == b"c" {
                        cell_shared = false;
                    }
                    element_stack.pop();
                }
                Ok(Event::Eof) => break,
                Err(error) => return Err(xml_err(error)),
                _ => {}
            }
        }
        if !buf.is_empty() && !buf.ends_with('\n') {
            buf.push('\n');
        }
    }
    Ok(buf)
}

/// Collect the inner text of every occurrence of the given element names,
/// joined with a space per element to keep words separable.
fn collect_element_text(xml: &str, names: &[&str]) -> String {
    let parts = collect_element_text_split(xml, names);
    parts.join(" ")
}

/// Collect the inner text of the given elements, one String per occurrence.
fn collect_element_text_split(xml: &str, names: &[&str]) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut depth_in_target = 0u32;
    let mut current = String::new();
    let mut out = Vec::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if depth_in_target > 0 || names_contains(names, e.name().as_ref()) {
                    if depth_in_target == 0 {
                        depth_in_target = 1;
                    } else {
                        depth_in_target += 1;
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                if depth_in_target == 0 && names_contains(names, e.name().as_ref()) {
                    out.push(String::new());
                }
            }
            Ok(Event::Text(t)) if depth_in_target > 0 => {
                if let Ok(text) = t.unescape() {
                    if !current.is_empty() && !text.trim().is_empty() {
                        current.push(' ');
                    }
                    current.push_str(text.trim());
                }
            }
            Ok(Event::End(_)) if depth_in_target > 0 => {
                depth_in_target -= 1;
                if depth_in_target == 0
                    && !current.is_empty() {
                        out.push(std::mem::take(&mut current));
                    }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}

fn names_contains(names: &[&str], local: &[u8]) -> bool {
    names.iter().any(|n| {
        let n = n.as_bytes();
        // Match the local name even when namespaced (e.g. "w:t" vs bare "t").
        local.ends_with(n)
            && (local.len() == n.len() || local.get(local.len() - n.len() - 1) == Some(&b':'))
    })
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
const MAX_OOXML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// Reject a zip entry whose declared uncompressed size exceeds the safety cap.
fn ensure_entry_size(entry: &zip::read::ZipFile<'_>) -> Result<(), BackendError> {
    if entry.size() > MAX_OOXML_ENTRY_BYTES {
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
    let filename = format!("{}-{}.txt", sanitize_filename(stem), &hash[..8]);
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

    file_store.write_text_absolute(&output_path, text)?;
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
        let shared = vec![
            "Hello".to_string(),
            "World".to_string(),
            "Pi".to_string(),
        ];
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

    use std::io::Write as _;
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
            ("[Content_Types].xml", "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"),
            ("word/document.xml", &document),
        ])
    }

    /// Build a minimal valid PPTX with one slide per text entry.
    fn sample_pptx(slides: &[String]) -> Vec<u8> {
        let mut owned_names: Vec<String> = Vec::new();
        let mut entries: Vec<(String, String)> = Vec::new();
        entries.push(
            (
                "[Content_Types].xml".to_string(),
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>".to_string(),
            ),
        );
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
            ("[Content_Types].xml", "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"),
            ("xl/sharedStrings.xml", &sst),
            ("xl/worksheets/sheet1.xml", sheet_xml),
        ])
    }
}

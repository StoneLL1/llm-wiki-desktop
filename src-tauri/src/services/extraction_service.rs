use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

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
            SourceFileType::Markdown | SourceFileType::Text | SourceFileType::Csv => {
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
            SourceFileType::Pdf
            | SourceFileType::Document
            | SourceFileType::Presentation
            | SourceFileType::Spreadsheet => {
                // These formats require external parser adapters.
                // MVP: report unsupported with clear status — per-file failure,
                // batch must continue.
                let ft = file_type.clone();
                Ok(ExtractResult {
                    original_name,
                    file_type: file_type.clone(),
                    status: ExtractionStatus::Unsupported,
                    error: Some(format!(
                        "Parser adapter not yet available for {:?}. Install a parser adapter or re-import after adapter support lands.",
                        ft
                    )),
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
        let source = root.join("test.md");
        fs::write(&source, "# Hello\n\nThis is a **markdown** file.\n").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
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
        let source = root.join("notes.txt");
        fs::write(&source, "Plain text content\nwith multiple lines.\n").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert!(result.text_preview.unwrap().contains("Plain text"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_csv_text() {
        let (context, root) = tmp_context("csv-extract");
        let store = FileStore;
        let source = root.join("data.csv");
        fs::write(&source, "name,age,city\nAlice,30,NYC\nBob,25,LA\n").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Extracted);
        assert!(result.text_preview.unwrap().contains("Alice"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_html_stripping_tags() {
        let (context, root) = tmp_context("html-extract");
        let store = FileStore;
        let source = root.join("page.html");
        fs::write(
            &source,
            "<html><head><title>Test Page</title></head><body><h1>Hello</h1><p>World</p></body></html>",
        )
        .unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
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

    // ── Unsupported format tests ──

    #[test]
    fn pdf_extraction_returns_unsupported_in_mvp() {
        let (context, root) = tmp_context("pdf-extract");
        let store = FileStore;
        let source = root.join("doc.pdf");
        fs::write(&source, b"%PDF-1.4 fake pdf content").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Unsupported);
        assert!(result.error.unwrap().contains("adapter"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn docx_extraction_returns_unsupported_in_mvp() {
        let (context, root) = tmp_context("docx-extract");
        let store = FileStore;
        let source = root.join("report.docx");
        fs::write(&source, b"fake docx content").unwrap();

        let service = ExtractionService;
        let result = service
            .extract_text(&context, &store, &source, &root)
            .unwrap();

        assert_eq!(result.status, ExtractionStatus::Unsupported);

        fs::remove_dir_all(root).unwrap();
    }

    // ── Batch extraction tests ──

    #[test]
    fn batch_extraction_continues_on_failure() {
        let (context, root) = tmp_context("batch-extract");
        let store = FileStore;

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
            &root,
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
}

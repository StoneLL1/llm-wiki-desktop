use std::path::Path;

use crate::models::import::SourceFileType;

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

pub(super) fn target_archive_dir(file_type: &SourceFileType) -> &'static str {
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

pub(super) fn deterministic_rename(original_name: &str, hash: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

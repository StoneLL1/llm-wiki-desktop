//! Sources-as-extracted-originals: integration tests for confirm-time
//! promotion to `wiki/sources/` and compile protection of that subtree.
//!
//! These run via the real Rust services (no Tauri/GUI DLLs), like `mvp_flow`.

use llm_wiki_desktop_lib::models::compile::{CompileFile, CompileManifest};
use llm_wiki_desktop_lib::models::import::{
    ExtractionStatus, ImportFileEntry, ImportPreview, ImportSummary, SourceFileType,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::{CompileService, FileStore, ImportService};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn make_context(slug: &str) -> (ProjectContext, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("sources-promo-{}-{}", slug, uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("wiki/sources")).unwrap();
    fs::create_dir_all(root.join("raw/extracted")).unwrap();
    fs::create_dir_all(root.join("raw/sources/markdown")).unwrap();
    fs::create_dir_all(root.join("raw/sources/pdfs")).unwrap();
    fs::create_dir_all(root.join(".app")).unwrap();
    (ProjectContext::new("project", root.clone()), root)
}

/// Build an ImportFileEntry whose `hash` matches the source file on disk.
fn entry(
    source_path: &str,
    archived_path: &str,
    file_type: SourceFileType,
    extracted_text_path: Option<&str>,
) -> ImportFileEntry {
    let bytes = fs::read(source_path).unwrap();
    ImportFileEntry {
        original_name: PathBuf::from(source_path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        source_path: source_path.to_string(),
        archived_path: archived_path.to_string(),
        file_type,
        size_bytes: bytes.len() as u64,
        hash: sha256_hex(&bytes),
        extraction_status: ExtractionStatus::Extracted,
        extraction_error: None,
        text_preview: None,
        page_count: None,
        word_count: None,
        metadata: None,
        extracted_text_path: extracted_text_path.map(str::to_string),
        extracted_assets: Vec::new(),
        conflict: None,
        renamed_from: None,
    }
}

fn empty_summary(n: u32) -> ImportSummary {
    ImportSummary {
        total_files: n,
        archived_files: n,
        duplicate_files: 0,
        renamed_files: 0,
        failed_files: 0,
        conflicts_count: 0,
    }
}

#[test]
fn markdown_import_is_promoted_verbatim_to_wiki_sources() {
    let (context, root) = make_context("md-promo");
    let store = FileStore;
    let src = root.join("import").join("notes.md");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(&src, b"# Imported notes\n\nSome body.").unwrap();

    let entry = entry(
        src.to_string_lossy().as_ref(),
        "raw/sources/markdown/notes.md",
        SourceFileType::Markdown,
        None,
    );
    let preview = ImportPreview {
        files: vec![entry],
        conflicts: Vec::new(),
        summary: empty_summary(1),
        v2_session_id: None,
    };

    ImportService
        .confirm_import(&context, &store, &preview)
        .unwrap();

    let promoted = root.join("wiki/sources/notes.md");
    assert!(promoted.exists(), "markdown original must be promoted");
    let body = fs::read_to_string(&promoted).unwrap();
    assert!(body.starts_with("---\n"), "frontmatter must lead the page");
    assert!(body.contains("type: source"));
    assert!(body.contains("sources: \"notes.md\""));
    assert!(
        body.contains("# Imported notes\n\nSome body."),
        "verbatim original body must be preserved"
    );

    let index = ImportService.read_source_index(&context, &store).unwrap();
    assert_eq!(
        index.sources.get("raw/sources/markdown/notes.md"),
        Some(&vec!["wiki/sources/notes.md".to_string()]),
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn extracted_text_import_is_promoted_and_transient_staging_removed() {
    let (context, root) = make_context("extracted-promo");
    let store = FileStore;
    let src = root.join("import").join("report.pdf");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(&src, b"%fake pdf bytes%").unwrap();
    // Staged extracted Markdown backing the preview.
    let staged = "raw/extracted/report-deadbeef.md";
    fs::write(root.join(staged), "# Extracted report\n\nKey facts.").unwrap();

    let entry = entry(
        src.to_string_lossy().as_ref(),
        "raw/sources/pdfs/report.pdf",
        SourceFileType::Pdf,
        Some(staged),
    );
    let preview = ImportPreview {
        files: vec![entry],
        conflicts: Vec::new(),
        summary: empty_summary(1),
        v2_session_id: None,
    };

    ImportService
        .confirm_import(&context, &store, &preview)
        .unwrap();

    let promoted = root.join("wiki/sources/report.md");
    assert!(promoted.exists(), "extracted text must be promoted");
    let body = fs::read_to_string(&promoted).unwrap();
    assert!(body.contains("type: source"));
    assert!(body.contains("# Extracted report"));

    // Staging is transient: the staged raw/extracted copy is removed.
    assert!(
        !root.join(staged).exists(),
        "staged raw/extracted file must be removed after promotion"
    );
    // The immutable archived original stays in place.
    assert!(root.join("raw/sources/pdfs/report.pdf").exists());

    let index = ImportService.read_source_index(&context, &store).unwrap();
    assert_eq!(
        index.sources.get("raw/sources/pdfs/report.pdf"),
        Some(&vec!["wiki/sources/report.md".to_string()]),
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn cjk_original_name_is_preserved_in_promoted_filename() {
    let (context, root) = make_context("cjk");
    let store = FileStore;
    let src = root.join("import").join("研究笔记.md");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(&src, "# 笔记内容").unwrap();

    let entry = entry(
        src.to_string_lossy().as_ref(),
        "raw/sources/markdown/研究笔记.md",
        SourceFileType::Markdown,
        None,
    );
    let preview = ImportPreview {
        files: vec![entry],
        conflicts: Vec::new(),
        summary: empty_summary(1),
        v2_session_id: None,
    };

    ImportService
        .confirm_import(&context, &store, &preview)
        .unwrap();

    // CJK chars are alphanumeric (Unicode-aware), so the stem is preserved.
    assert!(
        root.join("wiki/sources/研究笔记.md").exists(),
        "CJK stem must be preserved verbatim"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn colliding_clean_names_get_numeric_suffix() {
    let (context, root) = make_context("collision");
    let store = FileStore;

    // Two distinct sources whose original names both stem to "alpha".
    let md_src = root.join("import").join("alpha.md");
    let pdf_src = root.join("import").join("alpha.pdf");
    fs::create_dir_all(root.join("import")).unwrap();
    fs::write(&md_src, b"# Alpha markdown").unwrap();
    fs::write(&pdf_src, b"fake pdf").unwrap();
    let staged = "raw/extracted/alpha-cafebabe.md";
    fs::write(root.join(staged), "# Alpha extracted").unwrap();

    let md_entry = entry(
        md_src.to_string_lossy().as_ref(),
        "raw/sources/markdown/alpha.md",
        SourceFileType::Markdown,
        None,
    );
    let pdf_entry = entry(
        pdf_src.to_string_lossy().as_ref(),
        "raw/sources/pdfs/alpha.pdf",
        SourceFileType::Pdf,
        Some(staged),
    );
    let preview = ImportPreview {
        files: vec![md_entry, pdf_entry],
        conflicts: Vec::new(),
        summary: empty_summary(2),
        v2_session_id: None,
    };

    ImportService
        .confirm_import(&context, &store, &preview)
        .unwrap();

    assert!(
        root.join("wiki/sources/alpha.md").exists(),
        "first promotion takes the base name"
    );
    assert!(
        root.join("wiki/sources/alpha-2.md").exists(),
        "second promotion gets a -2 suffix"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn validate_manifest_rejects_writes_under_wiki_sources() {
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "i"),
            CompileFile::new("wiki/overview.md", "o"),
            CompileFile::new("wiki/log.md", "l"),
            CompileFile::new("wiki/sources/secret.md", "should be rejected"),
        ],
        deletions: vec![],
        summary: "compile".into(),
    };
    let err = CompileService::validate_manifest(&manifest).expect_err("must reject");
    assert_eq!(err.code, "COMPILE_PROTECTED_PATH");
}

#[test]
fn validate_manifest_rejects_deletions_under_wiki_sources() {
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "i"),
            CompileFile::new("wiki/overview.md", "o"),
            CompileFile::new("wiki/log.md", "l"),
        ],
        deletions: vec!["wiki/sources/secret.md".to_string()],
        summary: "compile".into(),
    };
    let err = CompileService::validate_manifest(&manifest).expect_err("must reject");
    assert_eq!(err.code, "COMPILE_PROTECTED_PATH");
}

#[test]
fn manifest_from_workspace_excludes_wiki_sources_from_files_and_deletions() {
    let root = std::env::temp_dir().join(format!("sources-ws-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("wiki/sources")).unwrap();
    fs::create_dir_all(root.join("wiki/entities")).unwrap();
    fs::write(root.join("wiki/index.md"), "# Index").unwrap();
    fs::write(root.join("wiki/overview.md"), "# Overview").unwrap();
    fs::write(root.join("wiki/log.md"), "# Log").unwrap();
    fs::write(root.join("wiki/sources/keep.md"), "imported original").unwrap();
    fs::write(root.join("wiki/entities/e.md"), "entity").unwrap();

    // An existing sources page is in the snapshot (so it could look like a
    // deletion if dropped); the manifest must neither emit it as a file nor
    // compute a deletion for it.
    let original_paths = std::collections::HashMap::from([
        ("wiki/index.md".to_string(), "index-hash".to_string()),
        (
            "wiki/sources/keep.md".to_string(),
            "source-hash".to_string(),
        ),
    ]);
    let manifest = CompileService::manifest_from_workspace(&root, &original_paths).unwrap();

    assert!(
        !manifest
            .files
            .iter()
            .any(|f| f.path.starts_with("wiki/sources/")),
        "no sources page may appear in the manifest files"
    );
    assert!(
        !manifest
            .deletions
            .iter()
            .any(|p| p.starts_with("wiki/sources/")),
        "no sources page may be scheduled for deletion"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn markdown_starting_with_horizontal_rule_is_not_misread_as_frontmatter() {
    let (context, root) = make_context("hr-rule");
    let store = FileStore;
    // Opens with `---` used as a thematic rule, not frontmatter. The body must
    // be preserved verbatim rather than being split into invalid YAML.
    let src = root.join("import").join("rules.md");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(&src, "---\nIntro prose line\n\n---\n\nAfter the rule").unwrap();

    let entry = entry(
        src.to_string_lossy().as_ref(),
        "raw/sources/markdown/rules.md",
        SourceFileType::Markdown,
        None,
    );
    let preview = ImportPreview {
        files: vec![entry],
        conflicts: Vec::new(),
        summary: empty_summary(1),
        v2_session_id: None,
    };

    ImportService
        .confirm_import(&context, &store, &preview)
        .unwrap();

    let body = fs::read_to_string(root.join("wiki/sources/rules.md")).unwrap();
    assert!(
        body.starts_with("---\ntype: source\n"),
        "page must open with the injected frontmatter"
    );
    assert!(body.contains("Intro prose line"));
    assert!(body.contains("After the rule"));
    // The verbatim original (including its leading horizontal rule) is
    // preserved as the page body, not split into invalid frontmatter.
    assert!(body.contains("---\nIntro prose line\n\n---\n\nAfter the rule"));
    fs::remove_dir_all(root).ok();
}

#[test]
fn extracted_markdown_files_reads_both_promoted_and_legacy_paths() {
    let (context, root) = make_context("enum");
    // Promoted original + legacy staging entry, both in the index.
    fs::write(root.join("wiki/sources/promoted.md"), "promoted body").unwrap();
    fs::write(root.join("raw/extracted/legacy.md"), "legacy body").unwrap();
    fs::write(
        root.join(".app/source-index.json"),
        r#"{"sources":{
            "raw/sources/markdown/promoted.md":["wiki/sources/promoted.md"],
            "raw/sources/pdfs/legacy.pdf":["raw/extracted/legacy.md"]
        }}"#,
    )
    .unwrap();

    let files = CompileService::extracted_markdown_files(&context).unwrap();
    let rels: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert!(rels.contains(&"wiki/sources/promoted.md".to_string()));
    assert!(rels.contains(&"raw/extracted/legacy.md".to_string()));

    fs::remove_dir_all(root).ok();
}

use std::fs;

use llm_wiki_desktop_lib::errors::{IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_OUTPUT_INVALID};
use llm_wiki_desktop_lib::models::import_v2::{ArtifactKind, ImportInput, ImportInputKind};
use llm_wiki_desktop_lib::services::import_v2::engine::{
    EngineOperation, EngineRequest, ImportEngine,
};
use llm_wiki_desktop_lib::services::import_v2::native_file_engine::NativeFileEngine;
use llm_wiki_desktop_lib::services::import_v2::quality_gate::QualityGate;
use llm_wiki_desktop_lib::tasks::task_model::CancellationToken;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn request(root: &TempDir, name: &str) -> EngineRequest {
    let path = root.path().join(name);
    let source_identity =
        fs::read(&path).ok().map(
            |bytes| llm_wiki_desktop_lib::models::import_v2::SourceIdentity {
                canonical_path: path.canonicalize().unwrap().to_string_lossy().into_owned(),
                size_bytes: bytes.len() as u64,
                modified_nanos: None,
                file_id: None,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                magic: format!("{:x}", Sha256::digest(&bytes[..bytes.len().min(8192)])),
            },
        );
    EngineRequest {
        protocol_version: "2.0".into(),
        request_id: "request-1".into(),
        project_id: "project-1".into(),
        session_id: "session-1".into(),
        item_id: "item-1".into(),
        task_id: "task-1".into(),
        operation: EngineOperation::Extract,
        input: ImportInput {
            kind: ImportInputKind::File,
            display_name: name.into(),
            locator: name.into(),
            normalized_locator: None,
            source_identity,
            media_save_mode: Default::default(),
        },
        project_root: root.path().to_string_lossy().into_owned(),
        staging_root: "staging".into(),
        chained_input: None,
        local_asr_authorized: false,
        local_ocr_authorized: false,
        allow_missing_transcript: false,
        media_save_mode: Default::default(),
    }
}

fn run(
    root: &TempDir,
    name: &str,
    bytes: &[u8],
) -> (
    llm_wiki_desktop_lib::services::import_v2::engine::EngineResult,
    String,
) {
    fs::write(root.path().join(name), bytes).unwrap();
    let result = NativeFileEngine::default()
        .execute(&request(root, name), &CancellationToken::new())
        .unwrap();
    let markdown = fs::read_to_string(root.path().join("staging/document.md")).unwrap();
    (result, markdown)
}

#[test]
fn descriptor_and_supported_extensions_are_stable() {
    let engine = NativeFileEngine::default();
    let descriptor = engine.descriptor();
    assert_eq!(descriptor.engine_id, "builtin.native-file");
    assert_eq!(descriptor.route, "file.native");
    for name in [
        "a.md",
        "a.markdown",
        "a.mdx",
        "a.mkd",
        "a.txt",
        "a.csv",
        "a.html",
        "a.htm",
    ] {
        assert!(engine.supports(&request(&TempDir::new().unwrap(), name).input));
    }
    assert!(!engine.supports(&request(&TempDir::new().unwrap(), "a.docx").input));
}

#[test]
fn markdown_snapshot_is_byte_exact_while_candidate_is_utf8_lf() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("images")).unwrap();
    fs::write(root.path().join("images/a.png"), b"png").unwrap();
    let original = b"\xEF\xBB\xBF---\r\ntitle: \xE8\xB5\x84\xE6\x96\x99\r\n---\r\n# Heading\r\n\r\n| A | B |\r\n|---|---|\r\n| x | y |\r\n\r\n![relative](images/a.png)\r\n";
    let (result, markdown) = run(&root, "资料.md", original);
    assert_eq!(
        fs::read(root.path().join("staging/source.bin")).unwrap(),
        original
    );
    assert!(!markdown.starts_with('\u{feff}'));
    assert!(!markdown.contains('\r'));
    assert!(markdown.starts_with("---\ntitle: 资料\n---\n"));
    assert!(markdown.contains("| A | B |"));
    assert!(markdown.contains("![relative](images/a.png)"));
    assert_eq!(result.metadata_path.as_deref(), Some("metadata.json"));
    let preview = QualityGate::default()
        .evaluate(&root.path().join("staging"), &result)
        .unwrap();
    assert!(preview
        .assets
        .iter()
        .any(|artifact| artifact.kind == ArtifactKind::Metadata));
}

#[test]
fn markdown_normalizes_dot_image_paths_and_keeps_escaping_links_non_fatal() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("images")).unwrap();
    fs::write(root.path().join("images/a.png"), b"png").unwrap();
    let (result, markdown) = run(
        &root,
        "paths.md",
        b"![local](./images/a.png)\n![external](../images/outside.png)\n",
    );

    assert!(markdown.contains("![local](./images/a.png)"));
    assert!(markdown.contains("![external](../images/outside.png)"));
    assert_eq!(result.asset_paths, vec!["images/a.png"]);
}

#[test]
fn markdown_accepts_gb18030_and_utf16_sources() {
    let root = TempDir::new().unwrap();
    let (gb18030, _, had_errors) = encoding_rs::GB18030.encode("# 标题\n");
    assert!(!had_errors);
    let (_, markdown) = run(&root, "legacy.md", &gb18030);
    assert_eq!(markdown, "# 标题\n");

    let (_, markdown) = run(&root, "utf16.md", b"\xff\xfe#\0 \0t\0i\0t\0l\0e\0\n\0");
    assert_eq!(markdown, "# title\n");
}

#[test]
fn csv_quotes_newlines_and_pipes_as_gfm_without_silent_loss() {
    let root = TempDir::new().unwrap();
    let (_, markdown) = run(
        &root,
        "table.csv",
        b"name,note\r\nAlice,\"line 1\nline 2\"\r\nBob,\"a|b\"\r\n",
    );
    assert!(markdown.contains("| name | note |"));
    assert!(markdown.contains("line 1<br>line 2"));
    assert!(markdown.contains("a\\|b"));
}

#[test]
fn local_html_removes_executable_content_and_emits_typed_warnings() {
    let root = TempDir::new().unwrap();
    let html = br#"<html><head><style>secret{}</style><script>alert(1)</script></head><body onload='x()'><h1>Safe</h1><p>Hello <a href='javascript:alert(1)'>bad</a> <a href='vbscript:x'>bad2</a><img src='data:text/html;base64,PHNjcmlwdD4=' onerror='x'></p><ul><li>One</li></ul></body></html>"#;
    let (result, markdown) = run(&root, "local.html", html);
    assert!(markdown.contains("# Safe"));
    assert!(markdown.contains("Hello"));
    for forbidden in [
        "alert(1)",
        "secret{}",
        "onload",
        "onerror",
        "javascript:",
        "vbscript:",
        "data:text/html",
    ] {
        assert!(!markdown.to_ascii_lowercase().contains(forbidden));
    }
    assert!(result.warnings.contains(&"HTML_SCRIPT_REMOVED".into()));
    assert!(result.warnings.contains(&"HTML_STYLE_REMOVED".into()));
    assert!(result
        .warnings
        .contains(&"HTML_EVENT_HANDLER_REMOVED".into()));
    assert!(result.warnings.contains(&"HTML_UNSAFE_URI_REMOVED".into()));
    let preview = QualityGate::default()
        .evaluate(&root.path().join("staging"), &result)
        .unwrap();
    assert!(!preview.quality.warnings.is_empty());
}

#[test]
fn invalid_utf8_and_pre_cancel_leave_no_staging_artifacts() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("bad.txt"), [0xff, 0xfe, 0x00]).unwrap();
    let error = NativeFileEngine::default()
        .execute(&request(&root, "bad.txt"), &CancellationToken::new())
        .unwrap_err();
    assert_eq!(error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID);
    assert!(!root.path().join("staging").exists());

    fs::write(root.path().join("cancel.md"), b"# no write").unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let error = NativeFileEngine::default()
        .execute(&request(&root, "cancel.md"), &token)
        .unwrap_err();
    assert_eq!(error.code, IMPORT_V2_CANCELLED);
    assert!(!root.path().join("staging").exists());
}

#[test]
fn rejects_same_length_source_swap_after_discovery() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("swap.md"), b"# trusted\n").unwrap();
    let request = request(&root, "swap.md");
    fs::write(root.path().join("swap.md"), b"# hostile\n").unwrap();
    let error = NativeFileEngine::default()
        .execute(&request, &CancellationToken::new())
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_FILE_SOURCE_CHANGED");
    assert!(!root.path().join("staging").exists());
}

#[cfg(unix)]
#[test]
fn rejects_markdown_images_that_are_symlinks() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("images")).unwrap();
    fs::write(outside.path().join("secret.png"), b"secret").unwrap();
    symlink(
        outside.path().join("secret.png"),
        root.path().join("images/secret.png"),
    )
    .unwrap();

    let error = NativeFileEngine::default()
        .execute(
            &request_with_bytes(&root, "note.md", b"![secret](images/secret.png)\n"),
            &CancellationToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID);
    assert!(!root.path().join("staging/images/secret.png").exists());
}

#[cfg(unix)]
fn request_with_bytes(root: &TempDir, name: &str, bytes: &[u8]) -> EngineRequest {
    fs::write(root.path().join(name), bytes).unwrap();
    request(root, name)
}

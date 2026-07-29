use std::fs;

use llm_wiki_desktop_lib::errors::{IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_OUTPUT_INVALID};
use llm_wiki_desktop_lib::models::import_v2::{ArtifactKind, ImportInput, ImportInputKind};
use llm_wiki_desktop_lib::services::import_v2::engine::{
    EngineOperation, EngineRequest, ImportEngine,
};
use llm_wiki_desktop_lib::services::import_v2::generic_web_engine::WebArtifactSource;
use llm_wiki_desktop_lib::services::import_v2::native_file_engine::{
    NativeCsvPackageEngine, NativeFileEngine, NativeStructuredFileEngine,
};
use llm_wiki_desktop_lib::services::import_v2::quality_gate::QualityGate;
use llm_wiki_desktop_lib::services::import_v2::url_policy::SessionWebTarget;
use llm_wiki_desktop_lib::services::import_v2::web_fetch::{WebFetchArtifact, WebFetchPolicy};
use llm_wiki_desktop_lib::tasks::task_model::CancellationToken;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::sync::Arc;
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
        asr_probe_only: false,
        asr_profile: None,
        recognition_language: None,
        selected_subtitle: None,
        local_ocr_authorized: false,
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
        "a.html",
        "a.htm",
    ] {
        assert!(engine.supports(&request(&TempDir::new().unwrap(), name).input));
    }
    assert!(!engine.supports(&request(&TempDir::new().unwrap(), "a.csv").input));
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
    assert!(markdown.contains("![relative](assets/images/a.png)"));
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

    assert!(markdown.contains("![local](assets/images/a.png)"));
    assert!(markdown.contains("![external](../images/outside.png)"));
    assert_eq!(result.asset_paths, vec!["assets/images/a.png"]);
}

#[test]
fn markdown_copies_inline_and_reference_style_attachments() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("files")).unwrap();
    fs::write(root.path().join("files/appendix.pdf"), b"%PDF-attachment").unwrap();
    fs::write(root.path().join("files/data.csv"), b"name\nvalue\n").unwrap();
    let (result, markdown) = run(
        &root,
        "attachments.md",
        b"[Appendix](files/appendix.pdf)\n[Data][data]\n\n[data]: files/data.csv\n",
    );

    assert!(markdown.contains("[Appendix](assets/files/appendix.pdf)"));
    assert!(markdown.contains("[data]: assets/files/data.csv"));
    assert_eq!(
        result.asset_paths,
        vec![
            "assets/files/appendix.pdf".to_string(),
            "assets/files/data.csv".to_string(),
        ]
    );
    assert_eq!(
        fs::read(root.path().join("staging/assets/files/data.csv")).unwrap(),
        b"name\nvalue\n"
    );
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
    fs::write(
        root.path().join("table.csv"),
        b"name,note\r\nAlice,\"line 1\nline 2\"\r\nBob,\"a|b\"\r\n",
    )
    .unwrap();
    NativeCsvPackageEngine
        .execute(&request(&root, "table.csv"), &CancellationToken::new())
        .unwrap();
    let markdown = fs::read_to_string(root.path().join("staging/document.md")).unwrap();
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

struct FixtureImageSource {
    image: Vec<u8>,
    cancel: bool,
}

impl WebArtifactSource for FixtureImageSource {
    fn fetch(
        &self,
        target: SessionWebTarget,
        _policy: WebFetchPolicy,
        _item_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<WebFetchArtifact, llm_wiki_desktop_lib::errors::BackendError> {
        if self.cancel {
            cancellation.cancel();
            return Err(llm_wiki_desktop_lib::errors::BackendError::new(
                IMPORT_V2_CANCELLED,
                "cancelled by fixture",
                true,
                false,
            ));
        }
        Ok(WebFetchArtifact {
            bytes: self.image.clone(),
            byte_len: self.image.len() as u64,
            final_public_url: target.public.public_url.clone(),
            final_session_target: target,
            content_type: "image/png".into(),
            sanitized_headers: BTreeMap::new(),
            redirects: Vec::new(),
            elapsed_ms: 0,
        })
    }
}

#[test]
fn local_html_archives_meaningful_remote_images_without_third_party_links() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("remote.html"),
        br#"<html><body><h1>Article</h1><img alt="diagram" src="https://cdn.example.test/diagram.png"></body></html>"#,
    )
    .unwrap();
    let image =
        include_bytes!("../../tests/fixtures/import-v2/local/batch3/matrix/image.png").to_vec();
    let engine = NativeFileEngine::new_with_artifact_source(Arc::new(FixtureImageSource {
        image,
        cancel: false,
    }));

    let result = engine
        .execute(&request(&root, "remote.html"), &CancellationToken::new())
        .unwrap();
    let markdown = fs::read_to_string(root.path().join("staging/document.md")).unwrap();

    assert!(!markdown.contains("cdn.example.test"));
    assert_eq!(result.asset_paths.len(), 1);
    assert!(result.asset_paths[0].starts_with("assets/remote/"));
    assert!(root
        .path()
        .join("staging")
        .join(&result.asset_paths[0])
        .is_file());
}

#[test]
fn cancellation_during_remote_resource_fetch_removes_partial_staging() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("cancel-remote.html"),
        br#"<html><body><h1>Article</h1><img alt="diagram" src="https://cdn.example.test/diagram.png"></body></html>"#,
    )
    .unwrap();
    let engine = NativeFileEngine::new_with_artifact_source(Arc::new(FixtureImageSource {
        image: Vec::new(),
        cancel: true,
    }));
    let token = CancellationToken::new();

    let error = engine
        .execute(&request(&root, "cancel-remote.html"), &token)
        .unwrap_err();

    assert_eq!(error.code, IMPORT_V2_CANCELLED);
    assert!(!root.path().join("staging").exists());
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
fn mid_write_failure_removes_partial_staging_tree() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("files")).unwrap();
    fs::write(root.path().join("files/appendix.pdf"), b"%PDF-attachment").unwrap();
    fs::write(
        root.path().join("partial.md"),
        b"[Appendix](files/appendix.pdf)\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("staging")).unwrap();
    fs::write(root.path().join("staging/assets"), b"directory collision").unwrap();

    NativeFileEngine::default()
        .execute(&request(&root, "partial.md"), &CancellationToken::new())
        .unwrap_err();

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

#[test]
fn pptx_total_image_loss_is_reported_by_engine_and_quality_gate() {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, xml) in [
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="512" r:id="rIdB"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships><Relationship Id="rIdB" Type="x/slide" Target="slides/slide7.xml"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide7.xml",
            r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><a:p><a:r><a:t>Readable slide text</a:t></a:r></a:p><a:blip r:embed="rIdMissing"/></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide7.xml.rels",
            r#"<Relationships><Relationship Id="rIdMissing" Type="x/image" Target="../media/missing.png"/></Relationships>"#,
        ),
    ] {
        archive.start_file(name, options).unwrap();
        archive.write_all(xml.as_bytes()).unwrap();
    }
    let bytes = archive.finish().unwrap().into_inner();
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("missing-image.pptx"), bytes).unwrap();

    let result = NativeStructuredFileEngine::new("builtin.office-pptx", "office.modern.pptx")
        .execute(
            &request(&root, "missing-image.pptx"),
            &CancellationToken::new(),
        )
        .unwrap();

    assert_eq!(result.meaningful_image_coverage, Some(0.0));
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning == "PRESENTATION_IMAGE_PRESERVATION_INCOMPLETE"));
    let preview = QualityGate::default()
        .evaluate(&root.path().join("staging"), &result)
        .unwrap();
    assert!(preview
        .quality
        .warnings
        .iter()
        .any(|warning| warning == "LOW_MEANINGFUL_IMAGE_COVERAGE"));
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

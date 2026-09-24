//! Input fidelity through production discovery, parser, transaction and restart.
use llm_wiki_desktop_lib::{
    models::{
        import_v2::{
            CommitImportSessionRequest, CommitItemDecision, ImportItemStatus,
            ImportMediaAuthorizationKind, ImportRecoveryAction, ImportResourceMode,
        },
        import_v2_file::FileScanPolicy,
        paths::ProjectContext,
        task::TaskType,
    },
    services::{
        import_v2::{
            file_discovery::{new_import_inputs, FileDiscoveryService},
            ImportV2Service,
        },
        FileStore, GitService, SecretService,
    },
    tasks::TaskService,
};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

fn all_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if root.exists() {
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(all_files(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

fn import_and_reopen(
    name: &str,
    original: &[u8],
    packs: &[(&Path, &str)],
    ocr: bool,
) -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("知识库");
    let incoming = temp.path().join("输入");
    fs::create_dir_all(project.join(".app")).unwrap();
    fs::create_dir_all(&incoming).unwrap();
    let input = incoming.join(name);
    fs::write(&input, original).unwrap();
    let context = ProjectContext::new("fidelity", project.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    for (pack, route) in packs {
        register_pack(&service, pack, route);
    }
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = {
        let scan = FileDiscoveryService
            .scan(
                &context,
                &[input.clone()],
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();
        assert_eq!(scan.files.len(), 1, "{:?}", scan.skipped);
        let inputs = new_import_inputs(&session, scan.files);
        service
            .add_inputs(&context, &files, &session.session_id, inputs)
            .unwrap()
    };
    let item = &session.items[0];
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            project.clone(),
            "Input fidelity".into(),
            true,
        )
        .unwrap();
    let mut prepared = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id,
        )
        .unwrap();
    if ocr {
        assert!(
            matches!(
                prepared.status,
                ImportItemStatus::WaitingAuthorization
                    | ImportItemStatus::WaitingCapability
                    | ImportItemStatus::PreviewReady
            ),
            "{prepared:?}"
        );
        service
            .authorize_media_for_session(
                &context,
                &files,
                &session.session_id,
                &item.item_id,
                ImportMediaAuthorizationKind::Ocr,
                None,
                None,
            )
            .unwrap();
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                project.clone(),
                "OCR".into(),
                true,
            )
            .unwrap();
        prepared = service
            .run_item_with_recovery(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
                Some(&ImportRecoveryAction::EnableOcr),
            )
            .unwrap();
    }
    assert_eq!(
        prepared.status,
        ImportItemStatus::PreviewReady,
        "{:?}",
        prepared.issue
    );
    let preview = prepared.preview.unwrap();
    let batch = service
        .commit_items(
            &context,
            &files,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: project.to_string_lossy().into(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id: item.item_id.clone(),
                    resolution: preview.resolution.and_then(|r| r.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(batch.committed_count, 1, "{batch:?}");
    assert_eq!(batch.failed_count, 0);
    drop(service);
    let restarted = ImportV2Service::with_secret_service(SecretService::memory());
    let reopened = restarted
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    assert_eq!(reopened.items[0].status, ImportItemStatus::Completed);
    assert_eq!(
        fs::read(&input).unwrap(),
        original,
        "input must remain byte exact"
    );
    assert!(
        all_files(&project.join("raw"))
            .iter()
            .any(|path| fs::read(path).unwrap() == original),
        "committed evidence must contain byte exact original"
    );
    let markdown = all_files(&project.join("wiki/sources"))
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .map(|p| fs::read_to_string(p).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for line in markdown.lines().filter(|line| line.starts_with("![")) {
        let relative = line.split("](").nth(1).unwrap().trim_end_matches(')');
        let resolved = llm_wiki_desktop_lib::services::import_v2::source_registry::SourceRegistry::resolve_wiki_asset_path(
            &context, &files, batch.items[0].wiki_path.as_deref().unwrap(), relative).unwrap();
        assert_eq!(
            fs::read(resolved).unwrap(),
            include_bytes!("../../tests/fixtures/import-v2/local/batch3/image-with-text.png")
        );
    }
    (temp, markdown)
}

fn register_pack(service: &ImportV2Service, root: &Path, route: &str) {
    use llm_wiki_desktop_lib::services::import_v2::capability_pack::{
        CapabilityPackManifest, ResolvedCapabilityPack,
    };
    use sha2::{Digest, Sha256};
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for key in [
        "payloadStatus",
        "buildProvenance",
        "execution",
        "distributionNote",
    ] {
        json.as_object_mut().unwrap().remove(key);
    }
    let mut manifest: CapabilityPackManifest = serde_json::from_value(json).unwrap();
    if route == "pack.office-legacy" {
        manifest.entrypoint = "python3".into();
        manifest.entrypoint_args = vec!["runner/office_legacy_pack.py".into()];
    }
    let entrypoint = root.join(&manifest.entrypoint).canonicalize().unwrap();
    let extensions = if route == "pack.office-legacy" {
        vec!["doc", "xls", "ppt"]
    } else {
        vec!["png", "tif", "tiff", "pdf"]
    };
    service
        .register_capability_pack(
            ResolvedCapabilityPack {
                manifest,
                root: root.canonicalize().unwrap(),
                entrypoint_sha256: format!("{:x}", Sha256::digest(fs::read(&entrypoint).unwrap())),
                entrypoint,
            },
            route.into(),
            extensions.into_iter().map(str::to_string).collect(),
            std::time::Duration::from_secs(180),
        )
        .unwrap();
}

fn office_image(text: &str) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let image = include_bytes!("../../tests/fixtures/import-v2/local/batch3/image-with-text.png");
    let xml = format!(
        r#"<w:document xmlns:w="w" xmlns:a="a" xmlns:r="r"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p><w:p><w:r><w:drawing><a:blip r:embed="rImage"/></w:drawing></w:r></w:p></w:body></w:document>"#
    );
    for (path, bytes) in [("[Content_Types].xml", b"<Types/>".as_slice()), ("word/document.xml", xml.as_bytes()), ("word/_rels/document.xml.rels", br#"<Relationships><Relationship Id="rImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/screenshot.png"/></Relationships>"#.as_slice()), ("word/media/screenshot.png", image.as_slice())] {
        zip.start_file(path, zip::write::SimpleFileOptions::default()).unwrap(); zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn word_ordinary_illustration_is_preserved_without_ocr_through_commit() {
    let original = office_image("Native prose explains this ordinary illustration.");
    let (temp, source) = import_and_reopen("含图 Word.docx", &original, &[], false);
    assert!(source.contains("Native prose"));
    assert!(source.contains("![Document image]"));
    let expected =
        include_bytes!("../../tests/fixtures/import-v2/local/batch3/image-with-text.png");
    assert!(all_files(&temp.path().join("知识库/raw"))
        .iter()
        .any(|path| fs::read(path).unwrap() == expected));
}

#[test]
fn mixed_pdf_preserves_readable_pages_and_marks_missing_pages_after_commit() {
    let original =
        include_bytes!("../../tests/fixtures/import-v2/local/batch3/mixed-text-scan.pdf");
    let (_temp, source) = import_and_reopen("混合.pdf", original, &[], false);
    assert!(source.contains("## Page 1"));
    assert!(source.contains("Page 2 needs OCR"));
}

#[test]
#[ignore = "requires isolated real OCR/LibreOffice payload at W7_ACCEPTANCE_ROOT"]
fn real_legacy_office_runner_native_parser_commit_reopen() {
    let root = PathBuf::from(std::env::var("W7_ACCEPTANCE_ROOT").unwrap());
    for extension in ["doc", "xls", "ppt"] {
        let original = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../tests/fixtures/import-v2/local/batch3/matrix/legacy.{extension}"
        )))
        .unwrap();
        let (_temp, source) = import_and_reopen(
            &format!("旧文档.{extension}"),
            &original,
            &[(&root.join("legacy"), "pack.office-legacy")],
            false,
        );
        assert!(!source.contains("modern Office extraction is pending"));
        assert!(
            source.contains("Batch 3") || source.contains("Workbook"),
            "{source}"
        );
    }
}

#[test]
#[ignore = "requires isolated real OCR payload at W7_ACCEPTANCE_ROOT"]
fn real_office_screenshot_and_multipage_tiff_ocr_commit_reopen() {
    let root = PathBuf::from(std::env::var("W7_ACCEPTANCE_ROOT").unwrap());
    let ocr_pack = std::env::var("W7_ACCEPTANCE_OCR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("ocr"));
    for (name, original) in [
        ("截图.docx", office_image("")),
        ("多页.tiff", fs::read(root.join("two-pages.tiff")).unwrap()),
        ("截图.pptx", fs::read(root.join("screenshot.pptx")).unwrap()),
    ] {
        let (_temp, source) =
            import_and_reopen(name, &original, &[(&ocr_pack, "ocr.cjk-accurate")], true);
        assert!(source.contains("BATCH 3 OCR TEXTIMAGE"), "{name}: {source}");
        assert!(!source.contains("<!-- OCR_"));
        assert!(!source.contains("Mean confidence:"));
        if name.ends_with("tiff") {
            assert!(source.contains("SECOND PAGE"), "{source}");
            assert!(source.contains("## Page 2"), "{source}");
        }
        eprintln!("W7 real sample {name}: {source}");
    }
}

#[test]
fn textless_pdf_and_office_screenshots_wait_without_saving_sources() {
    let mut pdf = lopdf::Document::load_mem(include_bytes!(
        "../../tests/fixtures/import-v2/local/batch3/mixed-text-scan.pdf"
    ))
    .unwrap();
    pdf.delete_pages(&[1]);
    let mut scan = Vec::new();
    pdf.save_to(&mut scan).unwrap();
    for (name, bytes) in [("scan.pdf", scan), ("screenshot.docx", office_image(""))] {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("knowledge/.app")).unwrap();
        let input = temp.path().join(name);
        fs::write(&input, bytes).unwrap();
        let context = ProjectContext::new("no-empty-source", temp.path().join("knowledge"));
        let service = ImportV2Service::with_secret_service(SecretService::memory());
        let session = service
            .create_session(&context, &FileStore, ImportResourceMode::Balanced)
            .unwrap();
        let scan = FileDiscoveryService
            .scan(
                &context,
                &[input],
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();
        assert_eq!(scan.files.len(), 1, "{scan:?}");
        let session = service
            .add_inputs(
                &context,
                &FileStore,
                &session.session_id,
                new_import_inputs(&session, scan.files),
            )
            .unwrap();
        let tasks = TaskService::default();
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                context.root.clone(),
                "No text".into(),
                true,
            )
            .unwrap();
        let item = service
            .run_item(
                &context,
                &FileStore,
                &tasks,
                &session.session_id,
                &session.items[0].item_id,
                &task.id,
            )
            .unwrap();
        assert!(
            matches!(
                item.status,
                ImportItemStatus::WaitingAuthorization | ImportItemStatus::WaitingCapability
            ),
            "{item:?}"
        );
        assert!(item.preview.is_none());
        assert!(!context.root.join("raw").exists());
        assert!(!context.root.join("wiki/sources").exists());
    }
}

#[test]
#[ignore = "requires isolated real OCR payload and mixed PDF at W7_ACCEPTANCE_ROOT"]
fn real_mixed_pdf_preserves_native_text_and_inserts_only_recognized_page() {
    let root = PathBuf::from(std::env::var("W7_ACCEPTANCE_ROOT").unwrap());
    let pack = std::env::var("W7_ACCEPTANCE_OCR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("ocr"));
    let original = fs::read(root.join("readable-and-scan.pdf")).unwrap();
    let (_temp, source) =
        import_and_reopen("混合.pdf", &original, &[(&pack, "ocr.cjk-accurate")], true);
    assert!(source.contains("trustworthy native text layer"));
    assert!(source.contains("## Page 1"));
    assert!(source.contains("## Page 2"));
    assert!(
        source
            .split_whitespace()
            .collect::<String>()
            .contains("BATCH3OCRTEXTIMAGE"),
        "{source}"
    );
    assert!(!source.contains("needs OCR"));
    assert!(!source.contains("<!-- OCR_"));
    eprintln!("W7 real mixed PDF: {source}");
}

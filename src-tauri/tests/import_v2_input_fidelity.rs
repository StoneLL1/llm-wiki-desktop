//! Input fidelity through production discovery, parser, transaction and restart.
use llm_wiki_desktop_lib::{
    models::{
        import_v2::{
            CommitImportSessionRequest, CommitItemDecision, ImportItemStatus, ImportResourceMode,
        },
        import_v2_file::{FileScanPolicy, FileSkipReason},
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

fn import_and_reopen(name: &str, original: &[u8], clipboard: bool) -> (tempfile::TempDir, String) {
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
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = if clipboard {
        service
            .add_text_input(
                &context,
                &files,
                &session.session_id,
                name,
                std::str::from_utf8(original).unwrap(),
            )
            .unwrap()
    } else {
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
    let prepared = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id,
        )
        .unwrap();
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
    (temp, markdown)
}

fn office(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in std::iter::once(&("[Content_Types].xml", "<Types/>")).chain(entries.iter()) {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn pasted_comma_prose_and_markdown_keep_text_semantics_after_commit() {
    for text in [
        "Hello, world",
        "# Hello, world\n\nA paragraph, with commas.",
    ] {
        let (_temp, source) = import_and_reopen("粘贴.md", text.as_bytes(), true);
        assert!(source.contains(text), "{source}");
    }
}

#[test]
fn text_prefix_encoding_boundaries_preserve_full_content_after_commit() {
    let text = format!("{}中文结尾", "a".repeat(8191));
    let (_temp, source) = import_and_reopen("中文边界.txt", text.as_bytes(), false);
    assert!(source.contains(&text));
    let utf16_text = format!("{}🦀中文", "a".repeat(4094));
    let utf16 = [
        vec![0xff, 0xfe],
        utf16_text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect(),
    ]
    .concat();
    let (_temp, source) = import_and_reopen("UTF16边界.txt", &utf16, false);
    assert!(source.contains(&utf16_text));
    let gb_text = format!("{}中文", "a".repeat(8191));
    let (gb, _, errors) = encoding_rs::GB18030.encode(&gb_text);
    assert!(!errors);
    let (_temp, source) = import_and_reopen("GB18030边界.txt", &gb, false);
    assert!(source.contains(&gb_text));
}

#[test]
fn quoted_csv_delimiters_newlines_and_empty_cells_survive_commit() {
    let (_temp, simple) =
        import_and_reopen("简单.csv", b"name,description\nAlice,\"a;b;c;d\"\n", false);
    assert!(simple.contains("| name | description |"), "{simple}");
    assert!(simple.contains("| Alice | a;b;c;d |"), "{simple}");
    let csv = "\u{feff}name,description,empty\nAlice,\"a;b;c;d\",\nBob,\"comma, quoted \"\"word\"\"\nnext line\",end\n";
    let (_temp, source) = import_and_reopen("表格.csv", csv.as_bytes(), false);
    assert!(
        source.contains("| name | description | empty |"),
        "{source}"
    );
    assert!(source.contains("| Alice | a;b;c;d |  |"), "{source}");
    assert!(
        source.contains("comma, quoted \"word\"<br>next line"),
        "{source}"
    );
}

#[test]
fn explicit_csv_single_record_and_ragged_semicolon_rows_keep_their_columns() {
    for (csv, expected) in [
        ("name;age", "| name | age |"),
        ("name;age\nAlice;20;extra\nBob;30", "| Alice | 20 | extra |"),
        ("name\nAlice;20\nBob;30", "| Alice | 20 |"),
    ] {
        let (_temp, source) = import_and_reopen("不齐.csv", csv.as_bytes(), false);
        assert!(source.contains(expected), "{source}");
    }
}

#[test]
fn xlsx_cells_and_formula_evidence_preserve_pipes_without_markdown_roundtrip() {
    let bytes = office(&[
        ("xl/workbook.xml", "<workbook/>"),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet><sheetData><row><c r="A1" t="inlineStr"><is><t>a|b</t></is></c><c r="B1" t="inlineStr"><is><t>c</t></is></c></row><row><c r="A2" t="str"><f>"a|b"</f><v>a|b</v></c><c r="B2" t="inlineStr"><is><t>adjacent</t></is></c></row></sheetData></worksheet>"#,
        ),
    ]);
    let (temp, source) = import_and_reopen("数据.xlsx", &bytes, false);
    assert!(source.contains("| a\\|b | c |"), "{source}");
    assert!(source.contains("| a\\|b | adjacent |"), "{source}");
    let evidence = all_files(&temp.path().join("知识库/raw"))
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "workbook-formulas.json"))
        .map(|p| fs::read_to_string(p).unwrap())
        .collect::<Vec<_>>();
    assert!(!evidence.is_empty());
    for json in evidence {
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value[0]["formula"], "=\"a|b\"");
        assert_eq!(value[0]["displayedValue"], "a|b");
        assert_eq!(value[0]["column"], 1);
    }
}

#[test]
fn docx_run_spaces_tabs_and_linebreaks_survive_commit() {
    let bytes = office(&[(
        "word/document.xml",
        r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t xml:space="preserve">Hello </w:t></w:r><w:r><w:t>world</w:t><w:tab/><w:t>tabbed</w:t><w:br/><w:t>next line</w:t></w:r></w:p><w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p></w:body></w:document>"#,
    )]);
    let (_temp, source) = import_and_reopen("文档.docx", &bytes, false);
    assert!(
        source.contains("Hello world\ttabbed\nnext line\n\nSecond paragraph"),
        "{source}"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_folder_member_is_reported_without_losing_readable_files() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let incoming = temp.path().join("输入");
    fs::create_dir(&incoming).unwrap();
    fs::write(incoming.join("可读.txt"), "可读内容").unwrap();
    let unreadable = incoming.join("不可读.txt");
    fs::write(&unreadable, "private").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0)).unwrap();
    let context = ProjectContext::new("scan", temp.path().join("知识库"));
    let scan = FileDiscoveryService.scan(
        &context,
        &[incoming.clone()],
        FileScanPolicy::default(),
        |_| {},
        || false,
    );
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
    let scan = scan.unwrap();
    assert_eq!(scan.files.len(), 1);
    assert_eq!(scan.files[0].display_name, "可读.txt");
    assert_eq!(scan.skipped.len(), 1);
    assert_eq!(scan.skipped[0].reason, FileSkipReason::Unreadable);
    let (_committed, source) = import_and_reopen("可读.txt", "可读内容".as_bytes(), false);
    assert!(source.contains("可读内容"));
    assert_eq!(
        FileDiscoveryService
            .scan(
                &context,
                &[incoming],
                FileScanPolicy::default(),
                |_| {},
                || true
            )
            .unwrap_err()
            .code,
        "IMPORT_FILE_SCAN_CANCELLED"
    );
}

#[test]
fn unavailable_local_html_image_preserves_article_after_commit() {
    let html = b"<html><body><article><h1>Article</h1><p>The complete article remains readable when its image is unavailable.</p><img alt='Missing illustration' src='http://127.0.0.1:1/missing.png'></article></body></html>";
    let (_temp, source) = import_and_reopen("文章.html", html, false);
    assert!(source.contains("IMPORT_REMOTE_IMAGE_UNAVAILABLE"));
    assert!(source.contains("[Image unavailable: Missing illustration]"));
    assert!(source.contains("The complete article remains readable"));
    assert!(!source.contains("assets/remote-image-unavailable"));
    assert!(!source.contains("![Missing illustration]"));
}

#[test]
fn cancellation_during_identity_is_not_downgraded_to_a_file_skip() {
    use std::cell::Cell;
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("取消.txt");
    fs::write(&input, "a".repeat(2 * 1024 * 1024)).unwrap();
    let context = ProjectContext::new("cancel", temp.path().join("知识库"));
    let checks = Cell::new(0);
    let result = FileDiscoveryService.scan(
        &context,
        &[input],
        FileScanPolicy::default(),
        |_| {},
        || {
            checks.set(checks.get() + 1);
            checks.get() >= 3
        },
    );
    assert_eq!(result.unwrap_err().code, "IMPORT_FILE_SCAN_CANCELLED");
}

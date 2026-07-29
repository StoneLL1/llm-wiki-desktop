use std::{
    fs,
    time::{Duration, Instant},
};

use llm_wiki_desktop_lib::models::{
    import_v2::{ImportInput, ImportInputKind, ImportResourceMode, SourceIdentity},
    import_v2_file::{FileFormat, FileScanPolicy},
    paths::ProjectContext,
    task::TaskType,
};
use llm_wiki_desktop_lib::services::import_v2::{
    engine::{EngineOperation, EngineRequest, ImportEngine},
    file_discovery::FileDiscoveryService,
    file_router::{CapabilitySnapshot, FileRoutePlanner},
    native_file_engine::NativeCsvPackageEngine,
    ImportV2Service,
};
use llm_wiki_desktop_lib::services::{FileStore, SecretService};
use llm_wiki_desktop_lib::tasks::task_model::CancellationToken;
use llm_wiki_desktop_lib::tasks::TaskService;
use sha2::{Digest, Sha256};

const RELEASE_REPORT: &str = include_str!("../../docs/qa/import-v2-file-ingestion.md");

#[test]
fn every_declared_format_has_a_stable_deterministic_route_contract() {
    let capabilities = CapabilitySnapshot {
        document_standard: true,
        office_legacy: true,
        office_oxide_installed: false,
        office_oxide_qualified: false,
        agent_available: false,
    };
    for format in [
        FileFormat::Markdown,
        FileFormat::Docx,
        FileFormat::Doc,
        FileFormat::Pdf,
        FileFormat::Xlsx,
        FileFormat::Xls,
        FileFormat::Pptx,
        FileFormat::Ppt,
    ] {
        let routes = FileRoutePlanner::deterministic_routes(format, capabilities);
        assert!(!routes.is_empty(), "missing route for {format:?}");
        assert!(routes.iter().all(|route| !route.starts_with("agent.")));
    }
}

#[test]
fn source_manifests_pin_versions_and_licenses_but_admit_missing_release_payloads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for pack in [
        "document-standard",
        "office-legacy",
        "office-oxide",
        "document-layout",
        "ocr-basic",
        "ocr-cjk-accurate",
        "media-runtime",
        "asr-whisper",
    ] {
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("capabilities").join(pack).join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(value["packId"], pack);
        assert!(value["version"].as_str().is_some_and(|v| !v.is_empty()));
        let license = value["licenseExpression"]
            .as_str()
            .unwrap()
            .to_ascii_uppercase();
        assert!(!license.contains("AGPL") && !license.contains("NONCOMMERCIAL"));
        assert_eq!(value["protocolVersion"], "2");
        // Repository manifests are planning/source manifests, not installable release evidence.
        assert_eq!(value["compressedBytes"], 0);
        assert_eq!(value["installedBytes"], 0);
    }
}

#[test]
fn ten_thousand_entry_scan_yields_first_batch_and_pre_cancel_within_local_contract() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let input = temp.path().join("input");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&input).unwrap();
    for index in 0..10_000 {
        fs::write(input.join(format!("entry-{index:05}.md")), b"# fixture").unwrap();
    }
    let context = ProjectContext::new("release-gate", project);
    let started = Instant::now();
    let mut first_batch = None;
    let result = FileDiscoveryService
        .scan(
            &context,
            &[input.clone()],
            FileScanPolicy::default(),
            |_| {
                if first_batch.is_none() {
                    first_batch = Some(started.elapsed())
                }
            },
            || false,
        )
        .unwrap();
    assert_eq!(result.files.len(), 10_000);
    assert!(first_batch.unwrap() < Duration::from_secs(1));

    let cancel_started = Instant::now();
    let cancelled = FileDiscoveryService.scan(
        &context,
        &[input],
        FileScanPolicy::default(),
        |_| {},
        || true,
    );
    assert!(cancelled.is_err());
    assert!(cancel_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn report_is_a_fail_closed_evidence_matrix_not_a_release_claim() {
    for format in [
        "Markdown", "DOCX", "DOC", "PDF", "XLSX", "XLS", "PPTX", "PPT",
    ] {
        assert!(
            RELEASE_REPORT.contains(format),
            "missing format evidence: {format}"
        );
    }
    for threat in [
        "path traversal",
        "archive bomb",
        "malicious HTML",
        "PDF actions",
        "macro/ActiveX",
        "Prompt Injection",
        "password secrecy",
        "timeout",
        "child process",
        "cancellation",
        "crash recovery",
        "repeated import",
        "partial success",
    ] {
        assert!(
            RELEASE_REPORT.contains(threat),
            "missing threat evidence: {threat}"
        );
    }
    for platform in ["Windows", "macOS", "Linux"] {
        assert!(RELEASE_REPORT.contains(platform));
    }
    assert!(RELEASE_REPORT.contains("UNVERIFIED — RELEASE BLOCKER"));
    assert!(RELEASE_REPORT.contains("actual package size: unavailable"));
    assert!(!RELEASE_REPORT.contains("READY TO RELEASE"));
    let _: ImportInputKind = ImportInputKind::File;
}

#[test]
fn real_large_csv_cancellation_leaves_no_partial_package() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let input = root.join("inputs/large.csv");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/import-v2/local/batch3/large.csv"),
        &input,
    )
    .unwrap();
    let bytes = fs::read(&input).unwrap();
    let request = EngineRequest {
        protocol_version: "2.0".into(),
        request_id: "cancel-request".into(),
        project_id: "cancel-large-csv".into(),
        project_root: root.to_string_lossy().into_owned(),
        session_id: "session".into(),
        item_id: "item".into(),
        task_id: "task".into(),
        operation: EngineOperation::Extract,
        staging_root: ".app/staging/cancelled".into(),
        input: ImportInput {
            kind: ImportInputKind::File,
            display_name: "large.csv".into(),
            locator: input.to_string_lossy().into_owned(),
            normalized_locator: Some(format!(
                "file:{}",
                input.to_string_lossy().replace('\\', "/")
            )),
            source_identity: Some(SourceIdentity {
                canonical_path: input.canonicalize().unwrap().to_string_lossy().into_owned(),
                size_bytes: bytes.len() as u64,
                modified_nanos: None,
                file_id: None,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                magic: format!("{:x}", Sha256::digest(&bytes[..bytes.len().min(8192)])),
            }),
            media_save_mode: Default::default(),
        },
        chained_input: None,
        local_asr_authorized: false,
        asr_probe_only: false,
        asr_profile: None,
        recognition_language: None,
        selected_subtitle: None,
        local_ocr_authorized: false,
        media_save_mode: Default::default(),
    };
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = NativeCsvPackageEngine
        .execute(&request, &cancellation)
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_CANCELLED");
    assert!(!root.join(".app/staging/cancelled").exists());
}

#[test]
fn clipboard_session_copy_is_removed_on_skip_and_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("clipboard-retention", root.clone());
    let files = FileStore;
    let tasks = TaskService::default();
    let service = ImportV2Service::with_secret_service(SecretService::memory());

    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_text_input(&context, &files, &session.session_id, "skip.md", "# skip")
        .unwrap();
    let item = &session.items[0];
    let skip_path = context.resolve_project_path(&item.input.locator).unwrap();
    assert!(skip_path.is_file());
    service
        .skip_item(&context, &files, &tasks, &session.session_id, &item.item_id)
        .unwrap();
    assert!(!skip_path.exists());

    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_text_input(
            &context,
            &files,
            &session.session_id,
            "cancel.md",
            "# cancel",
        )
        .unwrap();
    let item = &session.items[0];
    let cancel_path = context.resolve_project_path(&item.input.locator).unwrap();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root,
            "cancel clipboard fixture".into(),
            true,
        )
        .unwrap();
    tasks.cancel_task(&task.id).unwrap();
    assert!(cancel_path.is_file());
    let error = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id,
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_CANCELLED");
    assert!(!cancel_path.exists());
}

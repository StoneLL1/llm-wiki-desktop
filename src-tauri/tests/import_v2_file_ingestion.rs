use std::{fs, time::{Duration, Instant}};

use llm_wiki_desktop_lib::models::{
    import_v2::ImportInputKind,
    import_v2_file::{FileFormat, FileScanPolicy},
    paths::ProjectContext,
};
use llm_wiki_desktop_lib::services::import_v2::{
    file_discovery::FileDiscoveryService,
    file_router::{CapabilitySnapshot, FileRoutePlanner},
};

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
        FileFormat::Markdown, FileFormat::Docx, FileFormat::Doc, FileFormat::Pdf,
        FileFormat::Xlsx, FileFormat::Xls, FileFormat::Pptx, FileFormat::Ppt,
    ] {
        let routes = FileRoutePlanner::deterministic_routes(format, capabilities);
        assert!(!routes.is_empty(), "missing route for {format:?}");
        assert!(routes.iter().all(|route| !route.starts_with("agent.")));
    }
}

#[test]
fn source_manifests_pin_versions_and_licenses_but_admit_missing_release_payloads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for pack in [
        "document-standard", "office-legacy", "office-oxide", "document-layout",
        "ocr-basic", "ocr-cjk-accurate", "media-runtime", "asr-whisper",
    ] {
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("capabilities").join(pack).join("manifest.json")).unwrap(),
        ).unwrap();
        assert_eq!(value["packId"], pack);
        assert!(value["version"].as_str().is_some_and(|v| !v.is_empty()));
        let license = value["licenseExpression"].as_str().unwrap().to_ascii_uppercase();
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
    let result = FileDiscoveryService.scan(
        &context, &[input.clone()], FileScanPolicy::default(),
        |_| if first_batch.is_none() { first_batch = Some(started.elapsed()) },
        || false,
    ).unwrap();
    assert_eq!(result.files.len(), 10_000);
    assert!(first_batch.unwrap() < Duration::from_secs(1));

    let cancel_started = Instant::now();
    let cancelled = FileDiscoveryService.scan(
        &context, &[input], FileScanPolicy::default(), |_| {}, || true,
    );
    assert!(cancelled.is_err());
    assert!(cancel_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn report_is_a_fail_closed_evidence_matrix_not_a_release_claim() {
    for format in ["Markdown", "DOCX", "DOC", "PDF", "XLSX", "XLS", "PPTX", "PPT"] {
        assert!(RELEASE_REPORT.contains(format), "missing format evidence: {format}");
    }
    for threat in [
        "path traversal", "archive bomb", "malicious HTML", "PDF actions", "macro/ActiveX",
        "Prompt Injection", "password secrecy", "timeout", "child process", "cancellation",
        "crash recovery", "repeated import", "partial success",
    ] {
        assert!(RELEASE_REPORT.contains(threat), "missing threat evidence: {threat}");
    }
    for platform in ["Windows", "macOS", "Linux"] {
        assert!(RELEASE_REPORT.contains(platform));
    }
    assert!(RELEASE_REPORT.contains("UNVERIFIED — RELEASE BLOCKER"));
    assert!(RELEASE_REPORT.contains("actual package size: unavailable"));
    assert!(!RELEASE_REPORT.contains("READY TO RELEASE"));
    let _: ImportInputKind = ImportInputKind::File;
}

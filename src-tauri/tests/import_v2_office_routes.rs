use llm_wiki_desktop_lib::models::import_v2_file::FileFormat;
use llm_wiki_desktop_lib::services::import_v2::file_router::{
    AttemptOutcome, CapabilitySnapshot, FileRoutePlanner, QualityFloor, RouteFailure,
};

#[test]
fn modern_office_routes_are_explicit_and_deterministic() {
    let capabilities = CapabilitySnapshot {
        document_standard: true,
        office_oxide_installed: true,
        office_oxide_qualified: true,
        agent_available: true,
    };
    for (format, primary) in [
        (FileFormat::Docx, "office.modern.docx"),
        (FileFormat::Xlsx, "office.modern.xlsx"),
        (FileFormat::Pptx, "office.modern.pptx"),
    ] {
        let attempts = FileRoutePlanner::plan(format, capabilities);
        assert_eq!(
            attempts.iter().map(|a| a.route).collect::<Vec<_>>(),
            vec![
                primary,
                "pack.markitdown",
                "pack.office-oxide",
                "agent.office",
            ]
        );
        assert_eq!(attempts[0].quality_floor, QualityFloor::ModernOffice);
        assert_eq!(attempts[1].required_pack, Some("document-standard"));
        assert_eq!(attempts[2].required_pack, Some("office-oxide"));
    }
}

#[test]
fn missing_qualification_disables_oxide_even_when_installed() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot = CapabilitySnapshot::from_installation(
        true,
        true,
        &temp.path().join("office-oxide-qualification.json"),
        "x86_64-pc-windows-msvc",
        false,
    );
    let attempts = FileRoutePlanner::plan(FileFormat::Docx, snapshot);
    assert!(!attempts.iter().any(|a| a.route == "pack.office-oxide"));
}

#[test]
fn qualification_contract_is_platform_specific_and_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("qualification.json");
    std::fs::write(
        &path,
        r#"{
      "schemaVersion":1,"criticalAssertionsPassed":true,"securityBlockers":0,
      "fuzzBlockers":0,"qualifiedTargetTriples":["x86_64-pc-windows-msvc"]
    }"#,
    )
    .unwrap();
    let windows =
        CapabilitySnapshot::from_installation(false, true, &path, "x86_64-pc-windows-msvc", false);
    let linux = CapabilitySnapshot::from_installation(
        false,
        true,
        &path,
        "x86_64-unknown-linux-gnu",
        false,
    );
    assert!(
        windows.office_oxide_qualified,
        "contract fixture should enable its declared target"
    );
    assert!(
        !linux.office_oxide_qualified,
        "one-platform fixture is not cross-platform evidence"
    );
}

#[test]
fn fallback_requires_typed_failure_or_quality_rejection_and_records_every_attempt() {
    let mut record = FileRoutePlanner::record(
        "office.modern.xlsx",
        AttemptOutcome::Failed(RouteFailure::UnsupportedFeature {
            feature: "external_workbook_link".into(),
        }),
    );
    assert!(record.allows_fallback());
    assert_eq!(record.route, "office.modern.xlsx");
    record.outcome = AttemptOutcome::QualityRejected {
        actual: 0.94,
        required: 0.98,
    };
    assert!(record.allows_fallback());
    record.outcome = AttemptOutcome::Succeeded;
    assert!(!record.allows_fallback());
}

#[test]
fn quality_floor_encodes_golden_corpus_structure_contracts() {
    let floor = QualityFloor::ModernOffice.requirements();
    assert_eq!(floor.minimum_text_coverage, 0.98);
    assert!(floor.require_exact_unit_count);
    assert!(floor.require_ordered_structure);
    assert!(floor.require_tables);
    assert!(floor.require_images);
    assert!(floor.require_notes_or_footnotes);
    assert!(floor.require_formula_and_display_value);
}

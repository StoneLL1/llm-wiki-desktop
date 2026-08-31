use std::sync::atomic::AtomicBool;

use llm_wiki_desktop_lib::app_state::ProjectRegistry;
use llm_wiki_desktop_lib::models::layout::{
    inspect_native_layout, resolve_layout, NativeLayoutState, ProjectLayout,
};
use llm_wiki_desktop_lib::models::project::{ProjectFormat, ProjectHealth};
use llm_wiki_desktop_lib::services::assess_project_folder;

fn legacy_native(root: &std::path::Path, missing: &str) {
    for directory in [
        ".app/chats",
        ".app/tasks",
        "raw/sources/pdfs",
        "raw/sources/docs",
        "raw/sources/slides",
        "raw/sources/sheets",
        "raw/sources/markdown",
        "raw/sources/links",
        "raw/sources/other",
        "raw/extracted",
        "raw/assets",
        "wiki/entities",
        "wiki/concepts",
        "wiki/sources",
        "wiki/queries",
        "wiki/synthesis",
        "wiki/comparisons",
        "exports/html",
        "skills",
    ] {
        if directory != missing && !directory.starts_with(&format!("{missing}/")) {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
    }
    std::fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();
    std::fs::write(root.join("schema.md"), "# Schema\n").unwrap();
    std::fs::write(root.join("wiki/legacy-page.md"), "# Legacy\n").unwrap();
}

#[test]
fn legacy_native_layout_inspection_agrees_on_a_repairable_missing_task_root() {
    let root = tempfile::tempdir().unwrap();
    legacy_native(root.path(), ".app/tasks");

    let layout = resolve_layout(root.path()).unwrap();
    assert_eq!(layout.layout.app_state_root.as_deref(), Some(".app"));
    assert_eq!(layout.layout.evidence_root.as_deref(), Some("raw"));
    assert_eq!(layout.layout.wiki_write_root.as_deref(), Some("wiki"));
    assert!(matches!(
        inspect_native_layout(root.path()).state,
        NativeLayoutState::RepairableLegacy { .. }
    ));
    assert!(!ProjectRegistry::is_strict_native_layout(root.path()));
}

#[test]
fn native_legacy_repairable_state_advertises_a_repair_plan() {
    let root = tempfile::tempdir().unwrap();
    legacy_native(root.path(), "raw/sources");
    let config = tempfile::tempdir().unwrap();

    let assessment = assess_project_folder(
        root.path().to_string_lossy().as_ref(),
        config.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(assessment.format, ProjectFormat::NativeLegacy);
    assert_eq!(assessment.health, ProjectHealth::Repairable);
    assert!(assessment.repair_available);
}

#[test]
fn import_and_source_paths_are_fully_layout_derived_for_compatible_vaults() {
    let mut layout = ProjectLayout::native();
    layout.app_state_root = Some(".app/compat".into());
    layout.import_state_root = Some(".app/compat/import-sessions".into());
    layout.source_state_root = Some(".app/compat/sources".into());
    layout.evidence_root = Some(".app/compat/evidence".into());
    layout.source_write_root = Some("Notes/Sources".into());

    let import = layout.import_paths().unwrap();
    assert_eq!(
        import.item_staging("会话 A", "item-1").unwrap(),
        ".app/compat/import-sessions/会话 A/items/item-1/staging"
    );
    assert_eq!(
        import.history_preview("batch-1", "item-1").unwrap(),
        ".app/compat/import-history-previews/batch-1/item-1.md"
    );
    assert_eq!(
        import.recovery_journal_root(),
        ".app/compat/import-v2-journal"
    );

    let source = layout.source_paths().unwrap();
    assert_eq!(
        source.manifest("source-1").unwrap(),
        ".app/compat/sources/source-1.json"
    );
    assert_eq!(source.index(), ".app/compat/source-index-v2.json");
    assert_eq!(
        source.local_evidence_root("source-1", "version-1").unwrap(),
        ".app/compat/evidence/sources/source-1/version-1"
    );
    assert_eq!(
        source.baseline("source-1", "version-1").unwrap(),
        ".app/compat/source-artifacts/source-1/version-1/baseline.md"
    );
    assert_eq!(
        source.local_markdown("研究笔记.md").unwrap(),
        "Notes/Sources/local/研究笔记.md"
    );
}

#[test]
fn batch_a_expected_red_witnesses_cannot_be_silenced_by_test_controls() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for relative in [
        "project_layout_authority_contract.rs",
        "project_legacy_repair.rs",
        "workflow_compatible_layout.rs",
        "import_v2_scale_contract.rs",
        "import_v2_file_discovery.rs",
        "import_v2_file_orchestration.rs",
    ] {
        let source = std::fs::read_to_string(root.join(relative)).unwrap();
        let forbidden = [["ig", "nore"].concat(), ["should", "_panic"].concat()];
        for forbidden in forbidden {
            assert!(
                !source.contains(&forbidden),
                "{relative} contains {forbidden}"
            );
        }
    }
    let frontend_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
    for relative in [
        "features/import/importScaleContract.test.ts",
        "stores/importStore.test.ts",
        "stores/taskStore.test.ts",
        "types/project.contract.test.ts",
        "types/importV2.test.ts",
    ] {
        let source = std::fs::read_to_string(frontend_root.join(relative)).unwrap();
        for forbidden in [[".", "skip"].concat(), [".", "only"].concat()] {
            assert!(
                !source.contains(&forbidden),
                "{relative} contains {forbidden}"
            );
        }
    }
}

#[test]
fn expected_red_evidence_is_unique_valid_json_and_keeps_green_targets_visible() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("tests/fixtures/batch-a-expected-red.json");
    let raw = std::fs::read_to_string(path).unwrap();
    assert_eq!(raw.matches("\"baseline\"").count(), 1);
    let evidence: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(evidence["baseline"]["discoveryCallbacksFor100Files"], 100);
    assert_eq!(evidence["baseline"]["operationTasksFor10000Items"], 10_000);
    assert_eq!(
        evidence["baseline"]["frontendStorePublicationsFor10000TerminalItems"],
        10_000
    );
    assert_eq!(
        evidence["greenTargets"]["maxOperationTasksFor10000Items"],
        1
    );
    assert_eq!(evidence["greenTargets"]["maxItemWritesForOneItemUpdate"], 1);

    let witnesses = evidence["witnesses"].as_array().unwrap();
    let expected_witnesses = [
        (
            "project_layout_authority_contract.rs",
            "legacy_native_layout_inspection_agrees_on_a_repairable_missing_task_root",
            true,
        ),
        (
            "project_layout_authority_contract.rs",
            "native_legacy_repairable_state_advertises_a_repair_plan",
            true,
        ),
        (
            "import_v2_file_discovery.rs",
            "batch_a_expected_red_counts_one_discovery_callback_per_file",
            true,
        ),
        (
            "import_v2_scale_contract.rs",
            "expected_red_single_item_update_rewrites_every_persisted_item_file",
            true,
        ),
        (
            "../../src/features/import/importScaleContract.test.ts",
            "records one frontend store publication per terminal item under the current path",
            false,
        ),
    ];
    assert_eq!(witnesses.len(), expected_witnesses.len());
    for (index, (expected_file, expected_test, is_rust)) in expected_witnesses.iter().enumerate() {
        let witness = &witnesses[index];
        assert_eq!(witness["file"], *expected_file);
        assert_eq!(witness["test"], *expected_test);
        let source = std::fs::read_to_string(manifest.join("tests").join(expected_file))
            .unwrap()
            .replace("\r\n", "\n");
        let declaration = if *is_rust {
            format!("#[test]\nfn {expected_test}")
        } else {
            format!("it(\"{expected_test}\"")
        };
        assert!(
            source.contains(&declaration),
            "missing executable expected-red witness {expected_test} in {expected_file}"
        );
    }
    for witness in witnesses {
        let file = witness["file"].as_str().unwrap();
        let test = witness["test"].as_str().unwrap();
        let source = std::fs::read_to_string(manifest.join("tests").join(file)).unwrap();
        assert!(
            source.contains(test),
            "missing required expected-red witness {test} in {file}"
        );
    }
}

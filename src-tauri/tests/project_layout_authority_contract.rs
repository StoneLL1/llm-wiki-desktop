use std::sync::atomic::AtomicBool;

use llm_wiki_desktop_lib::app_state::ProjectRegistry;
use llm_wiki_desktop_lib::models::layout::resolve_layout;
use llm_wiki_desktop_lib::models::project::{ProjectFormat, ProjectHealth};
use llm_wiki_desktop_lib::services::assess_project_folder;

fn legacy_native(root: &std::path::Path, missing: &str) {
    for directory in [".app", "raw", "wiki", "exports", "skills"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    std::fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();
    std::fs::write(root.join("schema.md"), "# Schema\n").unwrap();
    std::fs::write(root.join("wiki/legacy-page.md"), "# Legacy\n").unwrap();
    if missing != "raw/sources" {
        std::fs::create_dir_all(root.join("raw/sources")).unwrap();
    }
    if missing != ".app/tasks" {
        std::fs::create_dir_all(root.join(".app/tasks")).unwrap();
    }
}

#[test]
fn expected_red_layout_and_registry_disagree_for_legacy_task_root() {
    let root = tempfile::tempdir().unwrap();
    legacy_native(root.path(), ".app/tasks");

    let layout = resolve_layout(root.path()).unwrap();
    assert_eq!(layout.layout.app_state_root.as_deref(), Some(".app"));
    assert_eq!(layout.layout.evidence_root.as_deref(), Some("raw"));
    assert_eq!(layout.layout.wiki_write_root.as_deref(), Some("wiki"));
    assert!(
        !ProjectRegistry::is_strict_native_layout(root.path()),
        "this witness must turn green in Batch B when native inspection is unified"
    );
}

#[test]
fn expected_red_native_legacy_is_repairable_without_a_repair_plan() {
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
    assert!(
        !assessment.repair_available,
        "this witness must become a positive repair-plan contract in Batch B"
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
            assert!(!source.contains(&forbidden), "{relative} contains {forbidden}");
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
            assert!(!source.contains(&forbidden), "{relative} contains {forbidden}");
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
    assert_eq!(evidence["baseline"]["frontendStorePublicationsFor10000TerminalItems"], 10_000);
    assert_eq!(evidence["greenTargets"]["maxOperationTasksFor10000Items"], 1);
    assert_eq!(evidence["greenTargets"]["maxItemWritesForOneItemUpdate"], 1);

    let witnesses = evidence["witnesses"].as_array().unwrap();
    let expected_witnesses = [
        ("project_layout_authority_contract.rs", "expected_red_layout_and_registry_disagree_for_legacy_task_root", true),
        ("project_layout_authority_contract.rs", "expected_red_native_legacy_is_repairable_without_a_repair_plan", true),
        ("workflow_compatible_layout.rs", "expected_red_compatible_enablement_has_no_task_state_root", true),
        ("import_v2_file_discovery.rs", "batch_a_expected_red_counts_one_discovery_callback_per_file", true),
        ("import_v2_scale_contract.rs", "expected_red_single_item_update_rewrites_every_persisted_item_file", true),
        ("import_v2_file_orchestration.rs", "batch_a_expected_red_task_service_creates_one_backend_task_per_item_at_scale", true),
        ("../../src/features/import/importScaleContract.test.ts", "records one frontend store publication per terminal item under the current path", false),
    ];
    assert_eq!(witnesses.len(), expected_witnesses.len());
    for (index, (expected_file, expected_test, is_rust)) in expected_witnesses.iter().enumerate() {
        let witness = &witnesses[index];
        assert_eq!(witness["file"], *expected_file);
        assert_eq!(witness["test"], *expected_test);
        let source = std::fs::read_to_string(manifest.join("tests").join(expected_file)).unwrap();
        let declaration = if *is_rust {
            format!("#[test]\nfn {expected_test}")
        } else {
            format!("it(\"{expected_test}\"")
        };
        assert!(source.contains(&declaration), "missing executable expected-red witness {expected_test} in {expected_file}");
    }
    for witness in witnesses {
        let file = witness["file"].as_str().unwrap();
        let test = witness["test"].as_str().unwrap();
        let source = std::fs::read_to_string(manifest.join("tests").join(file)).unwrap();
        assert!(source.contains(test), "missing required expected-red witness {test} in {file}");
    }
}

use std::fs;

use llm_wiki_desktop_lib::models::import_backend_activation::{
    ActivationConfirmation, ImportBackend,
};
use llm_wiki_desktop_lib::models::import_v2_migration::{
    LegacyInventory, MigrationPlan, MigrationReport, MigrationStatus, MigrationSummary,
    IMPORT_V2_MIGRATION_SCHEMA_VERSION,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::activation::ImportV2ActivationService;
use llm_wiki_desktop_lib::services::import_v2::migration::{
    ExternalToolLicenseEvidence, MigrationReadinessEvidence, PackageGateEvidence,
    REQUIRED_IMPORT_V2_CONTRACT,
};
use llm_wiki_desktop_lib::services::import_v2::ImportV2Service;
use llm_wiki_desktop_lib::services::GitService;
use tempfile::tempdir;

fn readiness() -> MigrationReadinessEvidence {
    MigrationReadinessEvidence {
        core_recovery_passed: true,
        package_gates: ["core", "file", "web", "agent"]
            .into_iter()
            .map(|package| PackageGateEvidence {
                package: package.into(),
                contract_version: REQUIRED_IMPORT_V2_CONTRACT.into(),
                release_gate_passed: true,
            })
            .collect(),
        fixture_matrix_passed: true,
        idempotence_passed: true,
        legacy_immutability_passed: true,
        long_task_recovery_passed: true,
        license_evidence: vec![ExternalToolLicenseEvidence {
            name: "builtin".into(),
            license: "MIT".into(),
            version: "1".into(),
            platform: "windows".into(),
            hash_or_signature: "sha256:abc".into(),
            size_bytes: 1,
            fallback: "native".into(),
        }],
    }
}

fn report() -> MigrationReport {
    let plan = MigrationPlan {
        plan_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
        v2_index_fingerprint: "MISSING".into(),
        inventory_fingerprint: "inventory".into(),
        candidates: Vec::new(),
        summary: MigrationSummary::default(),
    };
    let inventory = LegacyInventory {
        schema_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
        project_identity: "project".into(),
        fingerprint: "inventory".into(),
        records: Vec::new(),
        warnings: Vec::new(),
        scanned_files: Vec::new(),
    };
    let mut report = MigrationReport::from_plan(&plan, &inventory).unwrap();
    report.status = MigrationStatus::Applied;
    report.required_confirmation = false;
    report
}

#[test]
fn activation_requires_readiness_and_preserves_legacy_state() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app")).unwrap();
    fs::write(root.join(".app/source-index.json"), b"legacy-index").unwrap();
    let legacy = fs::read(root.join(".app/source-index.json")).unwrap();
    let context = ProjectContext::new("activation-test", root.to_path_buf());
    let report = report();
    let readiness = readiness();
    let service = ImportV2ActivationService::default();
    let confirmation = ActivationConfirmation {
        report_fingerprint: report.plan_fingerprint.clone(),
        token: service.confirmation_token(&report, "1.0.0"),
        acknowledge_no_git_rollback: true,
    };
    let result = service
        .activate(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &report,
            &readiness,
            "1.0.0",
            confirmation,
        )
        .unwrap();
    assert_eq!(result.record.active_backend, ImportBackend::V2);
    assert_eq!(
        fs::read(root.join(".app/source-index.json")).unwrap(),
        legacy
    );
    assert!(root
        .join(".app/import-v2-migration/activation.json")
        .exists());
    assert!(ImportV2ActivationService::legacy_mutation_guard(&context).is_err());
}

#[test]
fn activation_refuses_incomplete_gate_and_duplicate_activation() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("activation-test", root.to_path_buf());
    let report = report();
    let service = ImportV2ActivationService::default();
    let mut bad = readiness();
    bad.fixture_matrix_passed = false;
    let confirmation = ActivationConfirmation {
        report_fingerprint: report.plan_fingerprint.clone(),
        token: service.confirmation_token(&report, "1.0.0"),
        acknowledge_no_git_rollback: true,
    };
    let error = service
        .activate(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &report,
            &bad,
            "1.0.0",
            confirmation.clone(),
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_ACTIVATION_NOT_READY");

    service
        .activate(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &report,
            &readiness(),
            "1.0.0",
            confirmation,
        )
        .unwrap();
    let error = service
        .activate(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &report,
            &readiness(),
            "1.0.0",
            ActivationConfirmation {
                report_fingerprint: report.plan_fingerprint.clone(),
                token: service.confirmation_token(&report, "1.0.0"),
                acknowledge_no_git_rollback: true,
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_ACTIVATION_ALREADY_EXISTS");
}

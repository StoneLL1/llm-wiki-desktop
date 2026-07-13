use serde_json::json;

use llm_wiki_desktop_lib::commands::import_v2_migration::{
    ApplyImportV2MigrationRequest, GetImportV2MigrationStatusRequest,
    PlanImportV2MigrationRequest, ScanImportV2MigrationRequest,
};
use llm_wiki_desktop_lib::models::import_v2_migration::LegacyInventory;

#[test]
fn migration_command_requests_keep_camel_case_and_explicit_confirmation() {
    let scan = ScanImportV2MigrationRequest {
        project_id: "project-1".into(),
        project_root_path: "D:/Wiki/项目".into(),
    };
    assert_eq!(serde_json::to_value(scan).unwrap()["projectRootPath"], "D:/Wiki/项目");

    let plan = PlanImportV2MigrationRequest {
        project_id: "project-1".into(),
        project_root_path: "D:/Wiki/项目".into(),
        inventory: LegacyInventory {
            schema_version: 1,
            project_identity: "project-identity".into(),
            fingerprint: "inventory".into(),
            records: Vec::new(),
            warnings: Vec::new(),
            scanned_files: Vec::new(),
        },
    };
    assert_eq!(serde_json::to_value(plan).unwrap()["inventory"]["schemaVersion"], 1);

    let status = GetImportV2MigrationStatusRequest {
        project_id: "project-1".into(),
        project_root_path: "D:/Wiki/项目".into(),
    };
    assert_eq!(serde_json::to_value(status).unwrap()["projectId"], "project-1");

    let apply = ApplyImportV2MigrationRequest {
        project_id: "project-1".into(),
        project_root_path: "D:/Wiki/项目".into(),
        plan: serde_json::from_value(json!({
            "planVersion": 1,
            "v2IndexFingerprint": "MISSING",
            "inventoryFingerprint": "inventory",
            "candidates": [],
            "summary": {"total":0,"automaticLinks":0,"proposedRecords":0,"conflicts":0,"legacyUnmanaged":0,"warnings":0}
        })).unwrap(),
        confirmation: serde_json::from_value(json!({
            "planFingerprint": "plan",
            "token": "token",
            "acknowledgeNoGitRollback": true
        })).unwrap(),
    };
    let value = serde_json::to_value(apply).unwrap();
    assert!(value.get("confirmation").is_some());
    assert!(value.get("confirm").is_none());
}

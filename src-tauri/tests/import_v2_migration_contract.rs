use std::path::PathBuf;

use llm_wiki_desktop_lib::models::import_v2_migration::{
    LegacyRecord, MatchConfidence, MigrationCandidate, MigrationDecision, MigrationPlan,
    MigrationScanRequest, MigrationSummary,
};

fn record() -> LegacyRecord {
    LegacyRecord {
        record_id: "legacy-1".into(),
        stable_source_id: Some("source-1".into()),
        original_path: Some("raw/说明.md".into()),
        destination_path: Some("wiki/说明.md".into()),
        original_sha256: Some("hash-1".into()),
        normalized_url: None,
        recorded_content_sha256: Some("hash-1".into()),
        metadata_path: ".app/import-history/legacy-1.json".into(),
    }
}

#[test]
fn contracts_use_stable_camel_case_json() {
    let request = MigrationScanRequest {
        project_root: PathBuf::from("D:/Wiki/项目"),
    };
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(json["projectRoot"], "D:/Wiki/项目");

    let plan = MigrationPlan {
        plan_version: 1,
        v2_index_fingerprint: "v2-index-1".into(),
        inventory_fingerprint: "inventory-1".into(),
        candidates: vec![MigrationCandidate {
            candidate_id: "candidate-1".into(),
            record: record(),
            decision: MigrationDecision::LinkExisting {
                source_id: "source-1".into(),
                confidence: MatchConfidence::ExactStableSourceId,
            },
            evidence: vec!["stable_source_id".into()],
        }],
        summary: MigrationSummary::default(),
    };
    let json = serde_json::to_value(plan).unwrap();
    assert_eq!(json["inventoryFingerprint"], "inventory-1");
    assert_eq!(json["candidates"][0]["decision"]["kind"], "linkExisting");
    assert_eq!(json["candidates"][0]["decision"]["sourceId"], "source-1");
    assert!(json["candidates"][0]["decision"].get("source_id").is_none());
}

#[test]
fn invalid_plan_rejects_unknown_schema_paths_duplicates_and_unsupported_links() {
    let mut plan = MigrationPlan {
        plan_version: 99,
        v2_index_fingerprint: "v2-index-1".into(),
        inventory_fingerprint: "inventory-1".into(),
        candidates: vec![MigrationCandidate {
            candidate_id: "candidate-1".into(),
            record: record(),
            decision: MigrationDecision::LinkExisting {
                source_id: "source-1".into(),
                confidence: MatchConfidence::ExactStableSourceId,
            },
            evidence: Vec::new(),
        }],
        summary: MigrationSummary::default(),
    };
    let error = plan.validate().unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_MIGRATION_SCHEMA_UNSUPPORTED");

    plan.plan_version = 1;
    plan.candidates[0].record.destination_path = Some("../outside.md".into());
    let error = plan.validate().unwrap_err();
    assert_eq!(error.code, "PATH_TRAVERSAL");

    plan.candidates[0].record.destination_path = Some("wiki/说明.md".into());
    plan.candidates[0].evidence = vec!["stable_source_id".into()];
    plan.candidates.push(plan.candidates[0].clone());
    let error = plan.validate().unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_MIGRATION_DUPLICATE_CANDIDATE");
}

#[test]
fn link_existing_requires_structured_match_evidence() {
    let mut candidate = MigrationCandidate {
        candidate_id: "candidate-1".into(),
        record: record(),
        decision: MigrationDecision::LinkExisting {
            source_id: "source-1".into(),
            confidence: MatchConfidence::ExactHashUniqueDestination,
        },
        evidence: Vec::new(),
    };
    assert!(candidate.validate().is_err());
    candidate.evidence = vec!["content_hash".into(), "destination_path".into()];
    assert!(candidate.validate().is_ok());
}

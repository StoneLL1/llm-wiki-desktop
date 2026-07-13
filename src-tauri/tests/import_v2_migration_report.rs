use llm_wiki_desktop_lib::models::import_v2_migration::{
    LegacyInventory, LegacyRecord, MigrationCandidate, MigrationDecision, MigrationPlan,
    MigrationReport, MigrationSummary, MigrationStatus, IMPORT_V2_MIGRATION_SCHEMA_VERSION,
};

fn candidate(id: &str, decision: MigrationDecision) -> MigrationCandidate {
    MigrationCandidate {
        candidate_id: id.into(),
        record: LegacyRecord {
            record_id: id.into(),
            stable_source_id: None,
            original_path: Some(format!("raw/{id}.md")),
            destination_path: Some(format!("wiki/{id}.md")),
            original_sha256: Some(format!("hash-{id}")),
            normalized_url: None,
            recorded_content_sha256: None,
            metadata_path: ".app/source-index.json".into(),
        },
        decision,
        evidence: vec!["test_evidence".into()],
    }
}

#[test]
fn dry_run_report_is_inspectable_and_lists_immutable_boundaries() {
    let plan = MigrationPlan {
        plan_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
        inventory_fingerprint: "inventory-fingerprint".into(),
        candidates: vec![
            candidate(
                "link",
                MigrationDecision::LinkExisting {
                    source_id: "source-1".into(),
                    confidence: llm_wiki_desktop_lib::models::import_v2_migration::MatchConfidence::ExactStableSourceId,
                },
            ),
            candidate(
                "create",
                MigrationDecision::CreateV2Record {
                    proposed_source_id: "migrated-1".into(),
                },
            ),
            candidate(
                "conflict",
                MigrationDecision::Conflict {
                    candidates: vec!["source-a".into(), "source-b".into()],
                    reason: "contradictory evidence".into(),
                },
            ),
            candidate(
                "unmanaged",
                MigrationDecision::LegacyUnmanaged {
                    reason: "missing identity".into(),
                },
            ),
        ],
        summary: MigrationSummary {
            total: 4,
            automatic_links: 1,
            proposed_records: 1,
            conflicts: 1,
            legacy_unmanaged: 1,
            warnings: 1,
        },
    };
    let inventory = LegacyInventory {
        schema_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
        fingerprint: "inventory-fingerprint".into(),
        records: plan.candidates.iter().map(|candidate| candidate.record.clone()).collect(),
        warnings: vec![llm_wiki_desktop_lib::models::import_v2_migration::MigrationWarning {
            code: "MIGRATION_LEGACY_METADATA_CORRUPT".into(),
            message: "Sensitive fields omitted.".into(),
            relative_path: Some(".app/import-history/bad.json".into()),
            redacted: true,
        }],
        scanned_files: Vec::new(),
    };
    let report = MigrationReport::from_plan(&plan, &inventory).unwrap();
    let json = report.canonical_json().unwrap();
    assert_eq!(json["inventoryFingerprint"], "inventory-fingerprint");
    assert_eq!(json["status"], "dry_run_ready");
    assert_eq!(json["automaticLinks"].as_array().unwrap().len(), 1);
    assert!(json["affectedMetadataPaths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == ".app/source-index-v2.json"));
    assert!(json["untouchedContentPaths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == "raw/"));
    let markdown = report.to_markdown();
    assert!(markdown.contains("Confirmation required"));
    assert!(markdown.contains("Rollback"));
    assert!(!markdown.contains("Sensitive fields omitted."));
    assert!(!markdown.contains("D:/"));
    assert_eq!(report.status, MigrationStatus::DryRunReady);
}

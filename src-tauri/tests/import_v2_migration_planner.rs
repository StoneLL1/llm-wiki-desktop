use std::collections::BTreeMap;

use llm_wiki_desktop_lib::models::import_v2_migration::{
    LegacyInventory, LegacyRecord, MatchConfidence, MigrationDecision,
    IMPORT_V2_MIGRATION_SCHEMA_VERSION,
};
use llm_wiki_desktop_lib::services::import_v2::migration::{
    DefaultMigrationPlanner, MigrationPlanner,
};
use llm_wiki_desktop_lib::services::import_v2::source_registry::{SourceIndex, SourcePointer};

fn inventory(records: Vec<LegacyRecord>) -> LegacyInventory {
    LegacyInventory {
        schema_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
        project_identity: "project-test".into(),
        fingerprint: "inventory-test".into(),
        records,
        warnings: Vec::new(),
        scanned_files: Vec::new(),
    }
}

fn record(id: &str, hash: Option<&str>, destination: Option<&str>) -> LegacyRecord {
    LegacyRecord {
        record_id: id.into(),
        stable_source_id: None,
        original_path: Some(format!("raw/{id}.md")),
        destination_path: destination.map(str::to_string),
        original_sha256: hash.map(str::to_string),
        normalized_url: None,
        recorded_content_sha256: None,
        metadata_path: ".app/source-index.json".into(),
    }
}

fn index(hash: &str, source_id: &str) -> SourceIndex {
    let pointer = SourcePointer {
        source_id: source_id.into(),
        version_id: "version-1".into(),
    };
    SourceIndex {
        schema_version: 2,
        by_content_hash: BTreeMap::from([(hash.into(), pointer)]),
        by_locator: BTreeMap::new(),
    }
}

#[test]
fn exact_stable_source_id_and_content_identity_links() {
    let mut legacy = record("legacy-1", Some("hash-1"), Some("wiki/one.md"));
    legacy.stable_source_id = Some("source-1".into());
    let plan = DefaultMigrationPlanner
        .plan(&inventory(vec![legacy]), &index("hash-1", "source-1"))
        .unwrap();
    assert!(matches!(
        plan.candidates[0].decision,
        MigrationDecision::LinkExisting {
            confidence: MatchConfidence::ExactStableSourceId,
            ..
        }
    ));
}

#[test]
fn exact_hash_requires_unique_destination_and_does_not_use_names() {
    let plan = DefaultMigrationPlanner
        .plan(
            &inventory(vec![record("legacy-1", Some("hash-1"), Some("wiki/one.md"))]),
            &index("hash-1", "source-1"),
        )
        .unwrap();
    assert!(matches!(
        plan.candidates[0].decision,
        MigrationDecision::LinkExisting {
            confidence: MatchConfidence::ExactHashUniqueDestination,
            ..
        }
    ));

    let no_identity = record("same-title", None, Some("wiki/one.md"));
    let plan = DefaultMigrationPlanner
        .plan(&inventory(vec![no_identity]), &SourceIndex::default_v2())
        .unwrap();
    assert!(matches!(
        plan.candidates[0].decision,
        MigrationDecision::LegacyUnmanaged { .. }
    ));
}

#[test]
fn duplicate_hashes_and_case_only_paths_are_conflicts() {
    let records = vec![
        record("one", Some("same-hash"), Some("wiki/Readme.md")),
        record("two", Some("same-hash"), Some("wiki/readme.md")),
    ];
    let plan = DefaultMigrationPlanner
        .plan(&inventory(records), &index("same-hash", "source-1"))
        .unwrap();
    assert!(plan.candidates.iter().all(|candidate| matches!(
        candidate.decision,
        MigrationDecision::Conflict { .. }
    )));
}

#[test]
fn hash_and_url_pointing_to_different_sources_never_guess() {
    let mut source_index = index("hash-1", "source-hash");
    source_index.by_locator.insert(
        "https://example.com/article".into(),
        SourcePointer {
            source_id: "source-url".into(),
            version_id: "version-url".into(),
        },
    );
    let mut legacy = record("web-1", Some("hash-1"), Some("wiki/article.md"));
    legacy.normalized_url = Some("https://example.com/article".into());
    let plan = DefaultMigrationPlanner
        .plan(&inventory(vec![legacy]), &source_index)
        .unwrap();
    assert!(matches!(
        plan.candidates[0].decision,
        MigrationDecision::Conflict { .. }
    ));
}

#[test]
fn exact_hash_and_normalized_url_can_link_without_filename_evidence() {
    let mut source_index = index("hash-1", "source-1");
    source_index.by_locator.insert(
        "https://example.com/article".into(),
        SourcePointer {
            source_id: "source-1".into(),
            version_id: "version-1".into(),
        },
    );
    let mut legacy = record("web-1", Some("hash-1"), None);
    legacy.normalized_url = Some("https://example.com/article#section".into());
    let plan = DefaultMigrationPlanner
        .plan(&inventory(vec![legacy]), &source_index)
        .unwrap();
    assert!(matches!(
        plan.candidates[0].decision,
        MigrationDecision::LinkExisting {
            confidence: MatchConfidence::ExactHashNormalizedUrl,
            ..
        }
    ));
}

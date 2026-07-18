use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use url::Url;

use crate::errors::{BackendError, IMPORT_V2_SOURCE_INDEX_INVALID};
use crate::models::import_v2::IMPORT_V2_SCHEMA_VERSION;
use crate::models::import_v2_migration::{
    LegacyInventory, LegacyRecord, MatchConfidence, MigrationCandidate, MigrationDecision,
    MigrationPlan, MigrationSummary, IMPORT_V2_MIGRATION_SCHEMA_VERSION,
};
use crate::services::import_v2::source_registry::SourceIndex;

pub trait MigrationPlanner: Send + Sync {
    fn plan(
        &self,
        inventory: &LegacyInventory,
        v2_index: &SourceIndex,
    ) -> Result<MigrationPlan, BackendError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultMigrationPlanner;

impl MigrationPlanner for DefaultMigrationPlanner {
    fn plan(
        &self,
        inventory: &LegacyInventory,
        v2_index: &SourceIndex,
    ) -> Result<MigrationPlan, BackendError> {
        inventory.validate()?;
        if v2_index.schema_version != IMPORT_V2_SCHEMA_VERSION {
            return Err(BackendError::new(
                IMPORT_V2_SOURCE_INDEX_INVALID,
                "The V2 source index schema is not supported by migration.",
                false,
                true,
            ));
        }
        let hash_counts = count_hashes(&inventory.records);
        let destination_counts = count_destinations(&inventory.records);
        let mut candidates = inventory
            .records
            .iter()
            .map(|record| plan_record(record, v2_index, &hash_counts, &destination_counts))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
        let summary = summarize(&candidates, inventory.warnings.len());
        let plan = MigrationPlan {
            plan_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
            v2_index_fingerprint: fingerprint_source_index(v2_index),
            inventory_fingerprint: inventory.fingerprint.clone(),
            candidates,
            summary,
        };
        plan.validate()?;
        Ok(plan)
    }
}

pub fn fingerprint_source_index(index: &SourceIndex) -> String {
    let bytes = serde_json::to_vec(index).unwrap_or_default();
    digest(&bytes)
}

fn plan_record(
    record: &LegacyRecord,
    index: &SourceIndex,
    hash_counts: &BTreeMap<String, usize>,
    destination_counts: &BTreeMap<String, usize>,
) -> MigrationCandidate {
    let hash = record
        .recorded_content_sha256
        .as_deref()
        .or(record.original_sha256.as_deref())
        .filter(|value| !value.trim().is_empty());
    let hash_pointer = hash.and_then(|value| index.by_content_hash.get(value));
    let url = record
        .normalized_url
        .as_deref()
        .and_then(normalize_public_url);
    let url_pointer = url.as_deref().and_then(|value| index.by_locator.get(value));
    let destination_key = record.destination_path.as_deref().map(case_fold_path);
    let duplicate_hash = hash
        .and_then(|value| hash_counts.get(value))
        .is_some_and(|count| *count > 1);
    let duplicate_destination = destination_key
        .as_deref()
        .and_then(|value| destination_counts.get(value))
        .is_some_and(|count| *count > 1);

    let (decision, evidence) = if let (Some(stable_id), Some(hash_pointer), Some(_hash)) =
        (record.stable_source_id.as_deref(), hash_pointer, hash)
    {
        if stable_id == hash_pointer.source_id && !duplicate_destination {
            (
                MigrationDecision::LinkExisting {
                    source_id: stable_id.into(),
                    confidence: MatchConfidence::ExactStableSourceId,
                },
                vec!["stable_source_id".into(), "content_hash".into()],
            )
        } else if stable_id != hash_pointer.source_id {
            conflict(
                vec![stable_id.into(), hash_pointer.source_id.clone()],
                "stable source id and content hash resolve to different sources",
            )
        } else {
            conflict(
                vec![record.destination_path.clone().unwrap_or_default()],
                "destination path collides case-insensitively",
            )
        }
    } else if let (Some(_hash), Some(pointer)) = (hash, hash_pointer) {
        if duplicate_hash {
            conflict(
                vec![pointer.source_id.clone()],
                "content hash is shared by multiple legacy records",
            )
        } else if let Some(url_pointer) = url_pointer {
            if url_pointer.source_id == pointer.source_id {
                (
                    MigrationDecision::LinkExisting {
                        source_id: pointer.source_id.clone(),
                        confidence: MatchConfidence::ExactHashNormalizedUrl,
                    },
                    vec!["content_hash".into(), "normalized_public_url".into()],
                )
            } else {
                conflict(
                    vec![pointer.source_id.clone(), url_pointer.source_id.clone()],
                    "content hash and normalized URL resolve to different sources",
                )
            }
        } else if duplicate_destination || record.destination_path.is_none() {
            conflict(
                vec![pointer.source_id.clone()],
                if duplicate_destination {
                    "destination path collides case-insensitively"
                } else {
                    "exact hash lacks a unique destination path"
                },
            )
        } else {
            (
                MigrationDecision::LinkExisting {
                    source_id: pointer.source_id.clone(),
                    confidence: MatchConfidence::ExactHashUniqueDestination,
                },
                vec!["content_hash".into(), "destination_path".into()],
            )
        }
    } else if let (Some(_hash), Some(url_pointer)) = (hash, url_pointer) {
        conflict(
            vec![url_pointer.source_id.clone()],
            "normalized URL has no matching content identity",
        )
    } else if hash.is_some() && record.destination_path.is_some() && !duplicate_destination {
        (
            MigrationDecision::CreateV2Record {
                proposed_source_id: proposed_source_id(record),
            },
            vec!["legacy_content_hash".into(), "destination_path".into()],
        )
    } else {
        (
            MigrationDecision::LegacyUnmanaged {
                reason: "insufficient unique identity evidence".into(),
            },
            vec!["identity_evidence_missing".into()],
        )
    };

    MigrationCandidate {
        candidate_id: candidate_id(record),
        record: record.clone(),
        decision,
        evidence,
    }
}

fn conflict(candidates: Vec<String>, reason: &str) -> (MigrationDecision, Vec<String>) {
    (
        MigrationDecision::Conflict {
            candidates,
            reason: reason.into(),
        },
        vec!["conflicting_identity_evidence".into()],
    )
}

fn count_hashes(records: &[LegacyRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for hash in records.iter().filter_map(|record| {
        record
            .recorded_content_sha256
            .as_deref()
            .or(record.original_sha256.as_deref())
    }) {
        *counts.entry(hash.to_string()).or_insert(0) += 1;
    }
    counts
}

fn count_destinations(records: &[LegacyRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for destination in records
        .iter()
        .filter_map(|record| record.destination_path.as_deref())
    {
        *counts.entry(case_fold_path(destination)).or_insert(0) += 1;
    }
    counts
}

fn summarize(candidates: &[MigrationCandidate], warnings: usize) -> MigrationSummary {
    let mut summary = MigrationSummary {
        total: candidates.len(),
        warnings,
        ..MigrationSummary::default()
    };
    for candidate in candidates {
        match candidate.decision {
            MigrationDecision::LinkExisting { .. } => summary.automatic_links += 1,
            MigrationDecision::CreateV2Record { .. } => summary.proposed_records += 1,
            MigrationDecision::Conflict { .. } => summary.conflicts += 1,
            MigrationDecision::LegacyUnmanaged { .. } => summary.legacy_unmanaged += 1,
        }
    }
    summary
}

fn candidate_id(record: &LegacyRecord) -> String {
    let bytes = serde_json::to_vec(record).unwrap_or_default();
    format!("candidate-{}", digest(&bytes)[..24].to_string())
}

fn proposed_source_id(record: &LegacyRecord) -> String {
    let bytes = serde_json::to_vec(record).unwrap_or_default();
    format!("migrated-{}", digest(&bytes)[..24].to_string())
}

fn case_fold_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

fn normalize_public_url(value: &str) -> Option<String> {
    let mut url = Url::parse(value.trim()).ok()?;
    url.set_fragment(None);
    Some(url.to_string())
}

fn digest(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

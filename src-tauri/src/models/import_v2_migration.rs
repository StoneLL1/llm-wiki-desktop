use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{BackendError, PATH_ABSOLUTE_NOT_ALLOWED, PATH_INVALID, PATH_TRAVERSAL};

pub const IMPORT_V2_MIGRATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationScanRequest {
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LegacyRecord {
    pub record_id: String,
    #[serde(default)]
    pub stable_source_id: Option<String>,
    #[serde(default)]
    pub original_path: Option<String>,
    #[serde(default)]
    pub destination_path: Option<String>,
    #[serde(default)]
    pub original_sha256: Option<String>,
    #[serde(default)]
    pub normalized_url: Option<String>,
    #[serde(default)]
    pub recorded_content_sha256: Option<String>,
    pub metadata_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationWarning {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyInventory {
    pub schema_version: u32,
    pub project_identity: String,
    pub fingerprint: String,
    pub records: Vec<LegacyRecord>,
    pub warnings: Vec<MigrationWarning>,
    #[serde(default)]
    pub scanned_files: Vec<LegacyFileEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyFileEvidence {
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub modified_nanos: Option<u128>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MatchConfidence {
    ExactStableSourceId,
    ExactHashUniqueDestination,
    ExactHashNormalizedUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MigrationDecision {
    LinkExisting {
        #[serde(rename = "sourceId")]
        source_id: String,
        confidence: MatchConfidence,
    },
    CreateV2Record {
        #[serde(rename = "proposedSourceId")]
        proposed_source_id: String,
    },
    LegacyUnmanaged { reason: String },
    Conflict {
        candidates: Vec<String>,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCandidate {
    pub candidate_id: String,
    pub record: LegacyRecord,
    pub decision: MigrationDecision,
    /// Stable evidence codes, never a prose-only explanation.
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationSummary {
    pub total: usize,
    pub automatic_links: usize,
    pub proposed_records: usize,
    pub conflicts: usize,
    pub legacy_unmanaged: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub plan_version: u32,
    pub v2_index_fingerprint: String,
    pub inventory_fingerprint: String,
    pub candidates: Vec<MigrationCandidate>,
    pub summary: MigrationSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    NotScanned,
    DryRunReady,
    AwaitingConfirmation,
    Applying,
    Applied,
    VerificationFailed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationConfirmation {
    pub plan_fingerprint: String,
    pub token: String,
    #[serde(default)]
    pub acknowledge_no_git_rollback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationApplyResult {
    pub status: MigrationStatus,
    pub plan_fingerprint: String,
    pub applied_candidate_ids: Vec<String>,
    pub report_relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<crate::models::git::GitCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatusSnapshot {
    pub status: MigrationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<MigrationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub report_version: u32,
    pub plan_version: u32,
    pub plan_fingerprint: String,
    pub inventory_fingerprint: String,
    pub status: MigrationStatus,
    pub summary: MigrationSummary,
    pub automatic_links: Vec<MigrationCandidate>,
    pub proposed_records: Vec<MigrationCandidate>,
    pub conflicts: Vec<MigrationCandidate>,
    pub legacy_unmanaged: Vec<MigrationCandidate>,
    pub warnings: Vec<MigrationWarning>,
    pub affected_metadata_paths: Vec<String>,
    pub untouched_content_paths: Vec<String>,
    pub rollback_statement: String,
    pub required_confirmation: bool,
}

impl LegacyInventory {
    pub fn validate(&self) -> Result<(), BackendError> {
        if self.schema_version != IMPORT_V2_MIGRATION_SCHEMA_VERSION {
            return Err(schema_error("inventory"));
        }
        for record in &self.records {
            record.validate()?;
        }
        for file in &self.scanned_files {
            validate_project_relative(&file.relative_path)?;
        }
        Ok(())
    }
}

impl LegacyRecord {
    pub fn validate(&self) -> Result<(), BackendError> {
        if self.record_id.trim().is_empty() {
            return Err(BackendError::new(
                PATH_INVALID,
                "Legacy record id cannot be empty.",
                false,
                true,
            ));
        }
        validate_optional_project_relative(self.original_path.as_deref())?;
        validate_optional_project_relative(self.destination_path.as_deref())?;
        validate_project_relative(&self.metadata_path)
    }
}

impl MigrationCandidate {
    pub fn validate(&self) -> Result<(), BackendError> {
        if self.candidate_id.trim().is_empty() {
            return Err(BackendError::new(
                PATH_INVALID,
                "Migration candidate id cannot be empty.",
                false,
                true,
            ));
        }
        self.record.validate()?;
        if matches!(self.decision, MigrationDecision::LinkExisting { .. })
            && self.evidence.is_empty()
        {
            return Err(BackendError::new(
                "IMPORT_V2_MIGRATION_LINK_EVIDENCE_REQUIRED",
                "An existing-source link requires machine-readable evidence.",
                false,
                true,
            ));
        }
        Ok(())
    }
}

impl MigrationPlan {
    pub fn validate(&self) -> Result<(), BackendError> {
        if self.plan_version != IMPORT_V2_MIGRATION_SCHEMA_VERSION {
            return Err(schema_error("plan"));
        }
        if self.inventory_fingerprint.trim().is_empty() {
            return Err(BackendError::new(
                PATH_INVALID,
                "Inventory fingerprint cannot be empty.",
                false,
                true,
            ));
        }
        if self.v2_index_fingerprint.trim().is_empty() {
            return Err(BackendError::new(
                PATH_INVALID,
                "V2 index fingerprint cannot be empty.",
                false,
                true,
            ));
        }
        let mut ids = BTreeSet::new();
        for candidate in &self.candidates {
            if !ids.insert(candidate.candidate_id.clone()) {
                return Err(BackendError::new(
                    "IMPORT_V2_MIGRATION_DUPLICATE_CANDIDATE",
                    "Migration candidate ids must be unique.",
                    false,
                    true,
                ));
            }
            candidate.validate()?;
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        let mut digest = Sha256::new();
        digest.update(bytes);
        digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl MigrationReport {
    pub fn from_plan(
        plan: &MigrationPlan,
        inventory: &LegacyInventory,
    ) -> Result<Self, BackendError> {
        inventory.validate()?;
        plan.validate()?;
        if plan.inventory_fingerprint != inventory.fingerprint {
            return Err(BackendError::new(
                "IMPORT_V2_MIGRATION_PLAN_STALE",
                "The migration plan does not match the scanned inventory.",
                true,
                true,
            ));
        }
        let mut automatic_links = Vec::new();
        let mut proposed_records = Vec::new();
        let mut conflicts = Vec::new();
        let mut legacy_unmanaged = Vec::new();
        for candidate in &plan.candidates {
            let candidate = redact_candidate(candidate);
            match &candidate.decision {
                MigrationDecision::LinkExisting { .. } => automatic_links.push(candidate),
                MigrationDecision::CreateV2Record { .. } => proposed_records.push(candidate),
                MigrationDecision::Conflict { .. } => conflicts.push(candidate),
                MigrationDecision::LegacyUnmanaged { .. } => legacy_unmanaged.push(candidate),
            }
        }
        let warnings = inventory
            .warnings
            .iter()
            .map(|warning| MigrationWarning {
                code: warning.code.clone(),
                message: if warning.redacted {
                    "Sensitive legacy metadata was omitted from this report.".into()
                } else {
                    "Legacy evidence requires review before apply.".into()
                },
                relative_path: warning.relative_path.clone(),
                redacted: warning.redacted,
            })
            .collect();
        Ok(Self {
            report_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
            plan_version: plan.plan_version,
            plan_fingerprint: plan.fingerprint(),
            inventory_fingerprint: plan.inventory_fingerprint.clone(),
            status: MigrationStatus::DryRunReady,
            summary: plan.summary.clone(),
            automatic_links,
            proposed_records,
            conflicts,
            legacy_unmanaged,
            warnings,
            affected_metadata_paths: vec![
                ".app/source-index-v2.json".into(),
                ".app/import-v2-migration/report.json".into(),
            ],
            untouched_content_paths: vec![
                "raw/".into(),
                "wiki/".into(),
                ".app/source-index.json".into(),
                ".app/import-history/".into(),
                ".app/tasks/".into(),
            ],
            rollback_statement: "Rollback is release-based: open the prior application release, which reads the preserved legacy metadata and ignores V2 metadata.".into(),
            required_confirmation: true,
        })
    }

    pub fn canonical_json(&self) -> Result<serde_json::Value, BackendError> {
        serde_json::to_value(self).map_err(|error| {
            BackendError::new("JSON_SERIALIZE_FAILED", error.to_string(), false, true)
        })
    }

    pub fn to_markdown(&self) -> String {
        format!(
            "# Import V2 migration dry run\n\n- Inventory fingerprint: `{}`\n- Status: `{}`\n- Automatic links: {}\n- Proposed V2 records: {}\n- Conflicts: {}\n- Legacy unmanaged: {}\n- Warnings: {}\n- Confirmation required: {}\n\n## Metadata that may be written after confirmation\n\n{}\n\n## Guaranteed untouched\n\n{}\n\n## Rollback\n\n{}\n",
            self.inventory_fingerprint,
            serde_json::to_string(&self.status).unwrap_or_else(|_| "unknown".into()),
            self.summary.automatic_links,
            self.summary.proposed_records,
            self.summary.conflicts,
            self.summary.legacy_unmanaged,
            self.summary.warnings,
            if self.required_confirmation { "yes" } else { "no" },
            self.affected_metadata_paths
                .iter()
                .map(|path| format!("- `{path}`"))
                .collect::<Vec<_>>()
                .join("\n"),
            self.untouched_content_paths
                .iter()
                .map(|path| format!("- `{path}`"))
                .collect::<Vec<_>>()
                .join("\n"),
            self.rollback_statement,
        )
    }
}

fn redact_candidate(candidate: &MigrationCandidate) -> MigrationCandidate {
    let mut candidate = candidate.clone();
    candidate.record.normalized_url = candidate
        .record
        .normalized_url
        .as_deref()
        .and_then(|value| {
            let mut url = url::Url::parse(value).ok()?;
            url.set_username("").ok()?;
            url.set_password(None).ok()?;
            url.set_query(None);
            url.set_fragment(None);
            Some(url.to_string())
        });
    candidate.decision = match candidate.decision {
        MigrationDecision::LinkExisting {
            source_id,
            confidence,
        } => MigrationDecision::LinkExisting {
            source_id: safe_identifier(&source_id),
            confidence,
        },
        MigrationDecision::CreateV2Record { proposed_source_id } => {
            MigrationDecision::CreateV2Record {
                proposed_source_id: safe_identifier(&proposed_source_id),
            }
        }
        MigrationDecision::LegacyUnmanaged { .. } => MigrationDecision::LegacyUnmanaged {
            reason: "unmanaged legacy evidence requires review".into(),
        },
        MigrationDecision::Conflict { candidates, .. } => MigrationDecision::Conflict {
            candidates: candidates.iter().map(|value| safe_identifier(value)).collect(),
            reason: "conflicting identity evidence requires review".into(),
        },
    };
    candidate
}

fn safe_identifier(value: &str) -> String {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        value.into()
    } else {
        "[REDACTED]".into()
    }
}

pub fn validate_project_relative(value: &str) -> Result<(), BackendError> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.trim().is_empty() || normalized.starts_with('/') || path.is_absolute() {
        return Err(BackendError::new(
            PATH_ABSOLUTE_NOT_ALLOWED,
            "Migration metadata paths must be project-relative.",
            false,
            true,
        ));
    }
    if normalized.len() >= 2
        && normalized.as_bytes()[1] == b':'
        && normalized.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err(BackendError::new(
            PATH_ABSOLUTE_NOT_ALLOWED,
            "Migration metadata paths must be project-relative.",
            false,
            true,
        ));
    }
    if path.components().any(|component| component == Component::ParentDir)
        || normalized.split('/').any(|part| part == "..")
    {
        return Err(BackendError::new(
            PATH_TRAVERSAL,
            "Migration metadata paths cannot escape the project.",
            false,
            true,
        ));
    }
    Ok(())
}

fn validate_optional_project_relative(value: Option<&str>) -> Result<(), BackendError> {
    if let Some(value) = value {
        validate_project_relative(value)?;
    }
    Ok(())
}

fn schema_error(kind: &str) -> BackendError {
    BackendError::new(
        "IMPORT_V2_MIGRATION_SCHEMA_UNSUPPORTED",
        format!("Unsupported migration {kind} schema version."),
        false,
        true,
    )
}

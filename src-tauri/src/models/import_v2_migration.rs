use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    pub fingerprint: String,
    pub records: Vec<LegacyRecord>,
    pub warnings: Vec<MigrationWarning>,
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
}

impl LegacyInventory {
    pub fn validate(&self) -> Result<(), BackendError> {
        if self.schema_version != IMPORT_V2_MIGRATION_SCHEMA_VERSION {
            return Err(schema_error("inventory"));
        }
        for record in &self.records {
            record.validate()?;
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VersionOperationKind {
    LintFix,
    WikiUpdate,
    AgentRepair,
    ChatEdit,
    PageChange,
    SourceChange,
    ManualSnapshot,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VersionOperationState {
    Prepared,
    Applied,
    Restoring,
    Restored,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionOperationSummary {
    pub operation_id: String,
    pub kind: VersionOperationKind,
    pub created_at: String,
    pub state: VersionOperationState,
    pub file_count: usize,
    pub task_id: Option<String>,
}

/// The durable write intent. Summary files are a rebuildable projection only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionOperation {
    pub schema_version: u32,
    pub summary: VersionOperationSummary,
    pub project_identity: String,
    pub identity_revision: String,
    pub git_id: String,
    pub before: String,
    pub after: Option<String>,
    pub before_hashes: BTreeMap<String, Option<String>>,
    pub expected_hashes: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub restoration_id: Option<String>,
    #[serde(default)]
    pub restored_from: Option<String>,
    #[serde(default)]
    pub source_deletion: Option<SourceDeletionVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDeletionVersion {
    pub source_id: String,
    pub version_id: String,
    pub wiki_path: String,
    pub title: String,
    pub artifact_kinds: BTreeMap<String, String>,
    pub by_content_hash: BTreeMap<String, crate::models::source::SourcePointer>,
    pub by_locator: BTreeMap<String, crate::models::source::SourcePointer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionHistoryPage {
    pub operations: Vec<VersionOperationSummary>,
    pub next_cursor: Option<String>,
    pub unreadable_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionFileDiff {
    pub path: String,
    pub before_text: Option<String>,
    pub after_text: Option<String>,
    pub before_bytes: usize,
    pub after_bytes: usize,
    pub binary: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRestorePreview {
    pub operation_id: String,
    pub paths: Vec<String>,
    pub conflicts: Vec<String>,
    pub already_restored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum VersionHistoryMutation {
    Enable,
    Save {
        current_hashes: BTreeMap<String, Option<String>>,
    },
    Restore {
        operation_id: String,
        record_hash: String,
        current_hashes: BTreeMap<String, Option<String>>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionHistoryStatus {
    pub enabled: bool,
    pub git: crate::models::git::GitRepositoryStatus,
    pub git_version: String,
}

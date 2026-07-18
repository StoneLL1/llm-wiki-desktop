use serde::{Deserialize, Serialize};

use crate::models::task::TaskProgress;

pub const IMPORT_V2_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportResourceMode {
    Balanced,
    Performance,
    Saver,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportInputKind {
    File,
    Folder,
    Url,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportSessionStatus {
    Draft,
    Processing,
    WaitingForConfirmation,
    PartiallyCommitted,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemStatus {
    Queued,
    Inspecting,
    WaitingCapability,
    WaitingLogin,
    Extracting,
    Validating,
    PreviewReady,
    NeedsMerge,
    Committing,
    Completed,
    Paused,
    Cancelled,
    Skipped,
    Failed,
}

impl ImportItemStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        use ImportItemStatus::*;
        matches!(
            (self, next),
            (Queued, Inspecting | Cancelled | Skipped)
                | (
                    Inspecting,
                    WaitingCapability | WaitingLogin | Extracting | Failed | Cancelled
                )
                | (WaitingCapability, Extracting | Cancelled | Skipped | Failed)
                | (WaitingLogin, Extracting | Cancelled | Skipped | Failed)
                | (Extracting, WaitingLogin | Validating | Failed | Cancelled)
                | (Validating, PreviewReady | Failed | Cancelled)
                | (PreviewReady, NeedsMerge | Committing | Skipped | Cancelled)
                | (NeedsMerge, PreviewReady | Committing | Skipped | Cancelled)
                | (Committing, Completed | Failed)
                | (Paused, Inspecting | Extracting | Cancelled)
                | (Cancelled | Skipped, Inspecting)
                | (Failed, Inspecting | Skipped | Cancelled)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportStage {
    Inspect,
    Route,
    Extract,
    Validate,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityLevel {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    SourceSnapshot,
    Markdown,
    Image,
    Attachment,
    Subtitle,
    Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportInput {
    pub kind: ImportInputKind,
    pub display_name: String,
    pub locator: String,
    pub normalized_locator: Option<String>,
    /// Immutable discovery-time fingerprint. Older sessions deserialize without it,
    /// but file ingestion requires it before reading user-controlled content.
    #[serde(default)]
    pub source_identity: Option<SourceIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceIdentity {
    pub canonical_path: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub modified_nanos: Option<u128>,
    #[serde(default)]
    pub file_id: Option<String>,
    pub sha256: String,
    pub magic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QualityMetric {
    pub code: String,
    pub actual: f64,
    pub minimum: f64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub level: QualityLevel,
    pub metrics: Vec<QualityMetric>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_count_exact: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_count_exact: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_empty_cell_coverage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula_value_pairs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaningful_image_coverage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportArtifact {
    pub kind: ArtifactKind,
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttemptRecord {
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub stage: ImportStage,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub outcome: AttemptOutcome,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssue {
    pub code: String,
    pub message: String,
    pub stage: ImportStage,
    pub retryable: bool,
    pub user_action_required: bool,
    #[serde(default)]
    pub recovery_actions: Vec<ImportRecoveryAction>,
    #[serde(default)]
    pub available_actions: Vec<crate::models::import_v2_agent::AgentRecoveryAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportRecoveryAction {
    InstallCapability,
    Retry,
    SwitchParser,
    EnableOcr,
    InvokeAgent,
    Skip,
    ViewLog,
    RetryRoute,
    SwitchRoute,
    BeginLogin,
    AuthorizePrivateTarget,
    InstallBrowserCapability,
    InstallMediaCapability,
    AuthorizeLocalAsr,
}

impl ImportIssue {
    pub fn for_web_code(code: &str, stage: ImportStage) -> Self {
        use ImportRecoveryAction::*;
        let (retryable, user_action_required, mut recovery_actions) = match code {
            "IMPORT_WEB_LOGIN_REQUIRED"
            | "IMPORT_WEB_CHALLENGE_DETECTED"
            | "IMPORT_WEB_CAPTCHA_REQUIRED" => (false, true, vec![BeginLogin]),
            "IMPORT_V2_URL_REJECTED" | "IMPORT_V2_REDIRECT_REJECTED" => (false, true, vec![Skip]),
            "IMPORT_V2_PRIVATE_TARGET_BLOCKED" => (false, true, vec![AuthorizePrivateTarget]),
            "IMPORT_V2_RESPONSE_TOO_LARGE" => (false, true, vec![SwitchRoute]),
            "IMPORT_V2_CONNECTOR_RATE_LIMITED" => (true, false, vec![RetryRoute, SwitchRoute]),
            "IMPORT_WEB_STRUCTURE_CHANGED" => (true, true, vec![SwitchRoute, InvokeAgent]),
            "IMPORT_WEB_SUBTITLE_UNAVAILABLE" => (
                true,
                true,
                vec![AuthorizeLocalAsr, InstallMediaCapability, InvokeAgent],
            ),
            "IMPORT_ASR_ENGINE_UNAVAILABLE" | "IMPORT_ASR_ENGINE_INTEGRITY_FAILED" => {
                (true, true, vec![InstallMediaCapability, RetryRoute])
            }
            "IMPORT_ASR_TIMEOUT" | "IMPORT_ASR_ENGINE_FAILED" | "IMPORT_ASR_OUTPUT_INVALID" => {
                (true, true, vec![RetryRoute, InstallMediaCapability])
            }
            code if code.starts_with("IMPORT_ASR_") => {
                (false, true, vec![InstallMediaCapability, InvokeAgent])
            }
            "IMPORT_V2_ENGINE_UNAVAILABLE" => (true, true, vec![InstallBrowserCapability]),
            _ => (true, false, vec![RetryRoute, SwitchRoute]),
        };
        recovery_actions.extend([Skip, ViewLog]);
        Self {
            code: code.into(),
            message: "Web import could not be completed.".into(),
            stage,
            retryable,
            user_action_required,
            recovery_actions,
            available_actions: Vec::new(),
        }
    }
    pub fn for_file_code(code: &str, stage: ImportStage) -> Self {
        use ImportRecoveryAction::*;
        let (retryable, user_action_required, mut recovery_actions) = match code {
            "IMPORT_FILE_CAPABILITY_MISSING" => (true, true, vec![InstallCapability, Retry]),
            "IMPORT_FILE_PASSWORD_REQUIRED" => (true, true, vec![Retry]),
            "IMPORT_FILE_CORRUPT" => (false, true, vec![SwitchParser, InvokeAgent]),
            "IMPORT_FILE_RESOURCE_LIMIT" => (true, true, vec![Retry]),
            "IMPORT_FILE_CONVERSION_FAILED" | "IMPORT_FILE_PARSE_FAILED" => {
                (true, true, vec![Retry, SwitchParser, InvokeAgent])
            }
            "IMPORT_FILE_QUALITY_FAILED" => {
                (true, true, vec![EnableOcr, SwitchParser, InvokeAgent])
            }
            "IMPORT_FILE_CANCELLED" => (true, false, vec![Retry]),
            _ => (true, false, vec![Retry, InvokeAgent]),
        };
        recovery_actions.extend([Skip, ViewLog]);
        Self {
            code: code.into(),
            message: "File import could not be completed.".into(),
            stage,
            retryable,
            user_action_required,
            recovery_actions,
            available_actions: Vec::new(),
        }
    }

    pub fn for_commit_code(code: &str) -> Self {
        use ImportRecoveryAction::*;
        Self {
            code: code.into(),
            message: "Import result could not be committed.".into(),
            stage: ImportStage::Commit,
            retryable: false,
            user_action_required: true,
            recovery_actions: vec![ViewLog],
            available_actions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewArtifact {
    pub markdown: ImportArtifact,
    pub assets: Vec<ImportArtifact>,
    pub source_snapshot: ImportArtifact,
    pub quality: QualityReport,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub item_id: String,
    pub input: ImportInput,
    pub status: ImportItemStatus,
    pub selected: bool,
    pub task_id: Option<String>,
    pub progress: Option<TaskProgress>,
    pub attempts: Vec<AttemptRecord>,
    pub preview: Option<ImportPreviewArtifact>,
    pub issue: Option<ImportIssue>,
}

impl ImportItem {
    pub fn queued(item_id: &str, input: ImportInput) -> Self {
        Self {
            item_id: item_id.to_string(),
            input,
            status: ImportItemStatus::Queued,
            selected: true,
            task_id: None,
            progress: None,
            attempts: Vec::new(),
            preview: None,
            issue: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSession {
    pub schema_version: u32,
    pub session_id: String,
    pub project_id: String,
    pub status: ImportSessionStatus,
    pub resource_mode: ImportResourceMode,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_task_id: Option<String>,
    pub items: Vec<ImportItem>,
}

impl ImportSession {
    pub fn new(session_id: &str, project_id: &str, resource_mode: ImportResourceMode) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            schema_version: IMPORT_V2_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            status: ImportSessionStatus::Draft,
            resource_mode,
            created_at: now.clone(),
            updated_at: now,
            discovery_task_id: None,
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitConflictAction {
    CreateNew,
    KeepWiki,
    ApplyMergedCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitItemDecision {
    pub item_id: String,
    pub conflict_action: Option<CommitConflictAction>,
    pub expected_wiki_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitImportSessionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    #[serde(default)]
    pub batch_task_id: Option<String>,
    pub decisions: Vec<CommitItemDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemCommitResult {
    pub item_id: String,
    pub source_id: Option<String>,
    pub version_id: Option<String>,
    pub wiki_path: Option<String>,
    pub committed: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchResult {
    pub batch_id: String,
    pub session_id: String,
    /// Stable creation time for pagination; unlike file mtime it does not
    /// change when the batch record is updated after each item.
    #[serde(default)]
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_task_id: Option<String>,
    pub committed_count: u32,
    pub failed_count: u32,
    pub items: Vec<ImportItemCommitResult>,
    /// Immutable presentation snapshot captured when the commit batch runs.
    /// Older history files omit this field and are served through the legacy
    /// session fallback until they are naturally replaced by a new batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_snapshot: Option<ImportSession>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn import_v2_contract_is_versioned_and_camel_case() {
        let session = ImportSession::new("session-1", "project-1", ImportResourceMode::Balanced);
        let value = serde_json::to_value(session).unwrap();
        assert_eq!(value["schemaVersion"], json!(2));
        assert_eq!(value["sessionId"], json!("session-1"));
        assert_eq!(value["resourceMode"], json!("balanced"));
        assert!(value.get("session_id").is_none());
    }

    #[test]
    fn item_state_machine_rejects_preview_to_complete_shortcut() {
        assert!(ImportItemStatus::Queued.can_transition_to(&ImportItemStatus::Inspecting));
        assert!(ImportItemStatus::Validating.can_transition_to(&ImportItemStatus::PreviewReady));
        assert!(!ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Completed));
        assert!(ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Committing));
    }

    #[test]
    fn missing_web_subtitles_require_explicit_local_asr_authorization() {
        let issue =
            ImportIssue::for_web_code("IMPORT_WEB_SUBTITLE_UNAVAILABLE", ImportStage::Extract);
        assert!(issue.user_action_required);
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::AuthorizeLocalAsr));
    }

    #[test]
    fn asr_failures_preserve_actionable_recovery_codes() {
        let unavailable =
            ImportIssue::for_web_code("IMPORT_ASR_ENGINE_UNAVAILABLE", ImportStage::Extract);
        assert_eq!(unavailable.code, "IMPORT_ASR_ENGINE_UNAVAILABLE");
        assert!(unavailable.user_action_required);
        assert!(unavailable
            .recovery_actions
            .contains(&ImportRecoveryAction::InstallMediaCapability));
        let timeout = ImportIssue::for_web_code("IMPORT_ASR_TIMEOUT", ImportStage::Extract);
        assert_eq!(timeout.code, "IMPORT_ASR_TIMEOUT");
        assert!(timeout
            .recovery_actions
            .contains(&ImportRecoveryAction::RetryRoute));
    }
}

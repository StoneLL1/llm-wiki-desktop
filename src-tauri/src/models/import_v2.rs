use serde::{Deserialize, Serialize};

use crate::models::task::{TaskProgress, TaskStatus};

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
    ClipboardText,
}

/// Controls whether URL media is retained under `raw/assets` after import.
/// The source page/API snapshot and extracted Markdown are retained in both
/// modes; only the original media payload is optional.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaSaveMode {
    PreserveOriginal,
    ExtractOnly,
}

impl Default for MediaSaveMode {
    fn default() -> Self {
        Self::ExtractOnly
    }
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

/// Small, read-only control record for foreground session discovery. Batch 4
/// can extend this DTO with revisioned counts without changing the full
/// session read contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionOverview {
    pub schema_version: u32,
    pub session_id: String,
    pub project_id: String,
    pub status: ImportSessionStatus,
    pub resource_mode: ImportResourceMode,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_task_id: Option<String>,
    pub item_count: u64,
    pub semantic_revision: u64,
    pub selection_revision: u64,
    pub confirmation_digest: String,
    pub counts: ImportSessionCounts,
    pub status_counts: ImportSessionStatusCounts,
    pub selection: ImportSelectionSummary,
    pub index_state: ImportSessionIndexState,
    #[serde(default)]
    pub action_groups: Vec<ImportSessionActionGroup>,
    #[serde(default)]
    pub unresolved_count: u64,
    #[serde(default)]
    pub remaining_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_task: Option<ImportOperationTaskSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionStatusCounts {
    pub queued: u64,
    pub inspecting: u64,
    pub waiting_capability: u64,
    pub waiting_login: u64,
    pub waiting_authorization: u64,
    pub extracting: u64,
    pub validating: u64,
    pub preview_ready: u64,
    pub needs_merge: u64,
    pub committing: u64,
    pub completed: u64,
    pub paused: u64,
    pub cancelled: u64,
    pub skipped: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportSessionActionGroupKind {
    Login,
    Ocr,
    Asr,
    Capability,
    Conflict,
    Resume,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionActionGroup {
    pub group_key: String,
    pub kind: ImportSessionActionGroupKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    pub item_count: u64,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportOperationTaskSummary {
    pub task_id: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportSessionIndexState {
    Ready,
    RebuildRequired,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionCounts {
    pub all: u64,
    pub active: u64,
    pub ready: u64,
    pub needs_action: u64,
    pub failed: u64,
    pub completed: u64,
    pub waiting: u64,
    pub processed: u64,
    pub cancelled: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSelectionSummary {
    pub selected: u64,
    pub new_sources: u64,
    pub updates: u64,
    pub warnings: u64,
    pub pending: u64,
    pub restricted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemPageFilter {
    All,
    Active,
    Ready,
    NeedsAction,
    Failed,
    Completed,
}

impl Default for ImportItemPageFilter {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemPage {
    pub session_id: String,
    pub snapshot_revision: u64,
    pub items: Vec<ImportItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportUserState {
    Discovering,
    Processing,
    NeedsAction,
    Ready,
    Committing,
    Committed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportItemResolution {
    NewSource,
    ExactDuplicateSkip {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "candidateHash")]
        candidate_hash: String,
        #[serde(rename = "currentHash")]
        current_hash: String,
        #[serde(rename = "targetVersionId")]
        target_version_id: String,
    },
    SameSourceNewVersion {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "candidateHash")]
        candidate_hash: String,
        #[serde(rename = "currentHash")]
        current_hash: String,
        #[serde(rename = "targetVersionId")]
        target_version_id: String,
    },
    KeepCurrentSource {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "candidateHash")]
        candidate_hash: String,
        #[serde(rename = "currentHash")]
        current_hash: String,
        #[serde(rename = "targetVersionId")]
        target_version_id: String,
    },
    ApplyImportCandidate {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "candidateHash")]
        candidate_hash: String,
        #[serde(rename = "currentHash")]
        current_hash: String,
        #[serde(rename = "targetVersionId")]
        target_version_id: String,
    },
    ManualMerge {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "candidateHash")]
        candidate_hash: String,
        #[serde(rename = "currentHash")]
        current_hash: String,
        #[serde(rename = "targetVersionId")]
        target_version_id: String,
        #[serde(rename = "mergedHash")]
        merged_hash: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportResolutionKind {
    NewSource,
    ExactDuplicate,
    SameSourceNewVersion,
    NeedsThreeWayMerge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResolutionBinding {
    pub source_id: String,
    pub candidate_hash: String,
    pub current_hash: String,
    pub target_version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResolutionContext {
    pub kind: ImportResolutionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ImportResolutionBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_resolution: Option<ImportItemResolution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_wiki_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportThreeWayMergeContext {
    pub resolution: ImportResolutionContext,
    pub baseline_markdown: String,
    pub current_markdown: String,
    pub candidate_markdown: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportPrimaryAction {
    Retry,
    SignIn,
    Authorize,
    InstallCapability,
    EnableOcr,
    AuthorizeLocalAsr,
    InvokeLocalAgent,
    Review,
    Resolve,
    Resume,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssueDiagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserIssue {
    pub code: String,
    pub title: String,
    pub data_safety: String,
    pub primary_action: Option<ImportPrimaryAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImportIssueDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVersionChange {
    pub source_id: String,
    pub version_id: String,
    pub wiki_path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateResult {
    pub item_id: String,
    pub source_id: String,
    pub version_id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemFailure {
    pub item_id: String,
    pub input_label: String,
    pub issue: UserIssue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCompletion {
    pub session_id: String,
    pub batch_id: String,
    pub new_sources: Vec<SourceVersionChange>,
    pub updated_sources: Vec<SourceVersionChange>,
    pub duplicate_skips: Vec<DuplicateResult>,
    pub warnings: Vec<UserIssue>,
    pub failures: Vec<ItemFailure>,
}

impl ImportCompletion {
    pub fn empty(session_id: impl Into<String>, batch_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            batch_id: batch_id.into(),
            new_sources: Vec::new(),
            updated_sources: Vec::new(),
            duplicate_skips: Vec::new(),
            warnings: Vec::new(),
            failures: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemStatus {
    Queued,
    Inspecting,
    WaitingCapability,
    WaitingLogin,
    WaitingAuthorization,
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
                    WaitingCapability
                        | WaitingLogin
                        | WaitingAuthorization
                        | Extracting
                        | Paused
                        | Failed
                        | Cancelled
                )
                | (
                    WaitingCapability,
                    Inspecting | Extracting | Cancelled | Skipped | Failed
                )
                | (
                    WaitingLogin,
                    Inspecting | Extracting | Cancelled | Skipped | Failed
                )
                | (
                    WaitingAuthorization,
                    Inspecting | Extracting | Cancelled | Skipped | Failed
                )
                | (
                    Extracting,
                    WaitingCapability
                        | WaitingLogin
                        | WaitingAuthorization
                        | Validating
                        | Paused
                        | Failed
                        | Cancelled
                )
                | (
                    Validating,
                    PreviewReady | NeedsMerge | Paused | Failed | Cancelled
                )
                | (
                    PreviewReady,
                    Inspecting | NeedsMerge | Committing | Skipped | Cancelled
                )
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
    SourceEvidence,
    Markdown,
    Image,
    Attachment,
    Subtitle,
    Transcript,
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
    #[serde(default)]
    pub media_save_mode: MediaSaveMode,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePageType {
    Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFrontmatter {
    #[serde(rename = "type")]
    pub page_type: SourcePageType,
    pub source_id: String,
    pub version_id: String,
    pub source_kind: String,
    pub title: String,
    pub imported_at: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_content_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub quality: QualityReport,
    pub restricted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFrontmatterValidationError {
    pub field: String,
    pub code: String,
}

pub fn validate_source_frontmatter(
    frontmatter: &SourceFrontmatter,
    expected_content_hash: &str,
) -> Result<(), SourceFrontmatterValidationError> {
    for (field, value) in [
        ("sourceId", frontmatter.source_id.as_str()),
        ("versionId", frontmatter.version_id.as_str()),
        ("sourceKind", frontmatter.source_kind.as_str()),
        ("title", frontmatter.title.as_str()),
        ("importedAt", frontmatter.imported_at.as_str()),
        ("contentHash", frontmatter.content_hash.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(SourceFrontmatterValidationError {
                field: field.into(),
                code: "required".into(),
            });
        }
    }
    if frontmatter.content_hash.len() != 64
        || !frontmatter
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(SourceFrontmatterValidationError {
            field: "contentHash".into(),
            code: "invalid_sha256".into(),
        });
    }
    if frontmatter.content_hash != expected_content_hash {
        return Err(SourceFrontmatterValidationError {
            field: "contentHash".into(),
            code: "manifest_mismatch".into(),
        });
    }
    Ok(())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitle_candidates: Vec<String>,
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
    InstallOcrCapability,
    AuthorizeLocalAsr,
    SelectSubtitle,
}

impl ImportIssue {
    pub fn for_web_code(code: &str, stage: ImportStage) -> Self {
        use ImportRecoveryAction::*;
        let (retryable, user_action_required, mut recovery_actions) = match code {
            "IMPORT_WEB_LOGIN_REQUIRED"
            | "IMPORT_WEB_CHALLENGE_DETECTED"
            | "IMPORT_WEB_CAPTCHA_REQUIRED" => (false, true, vec![BeginLogin]),
            "IMPORT_WEB_ACCOUNT_PERMISSION_DENIED" => (false, true, vec![]),
            "IMPORT_V2_URL_REJECTED" | "IMPORT_V2_REDIRECT_REJECTED" => (false, true, vec![Skip]),
            "IMPORT_V2_PRIVATE_TARGET_BLOCKED" => (false, true, vec![AuthorizePrivateTarget]),
            "IMPORT_V2_RESPONSE_TOO_LARGE" => (false, true, vec![SwitchRoute]),
            "IMPORT_V2_CONNECTOR_RATE_LIMITED" => (true, false, vec![RetryRoute, SwitchRoute]),
            "IMPORT_WEB_CONTENT_REMOVED" => (false, true, vec![]),
            "IMPORT_WEB_MEDIA_HOST_UNSUPPORTED" => {
                (true, true, vec![SwitchRoute, InstallMediaCapability])
            }
            "IMPORT_WEB_MEDIA_UNAVAILABLE" => {
                (true, true, vec![InstallBrowserCapability, SwitchRoute])
            }
            "IMPORT_WEB_STRUCTURE_CHANGED" => (true, true, vec![SwitchRoute, InvokeAgent]),
            "IMPORT_WEB_SUBTITLE_UNAVAILABLE" => (
                true,
                true,
                vec![AuthorizeLocalAsr, InstallMediaCapability, InvokeAgent],
            ),
            "IMPORT_WEB_OCR_UNAVAILABLE" => (true, true, vec![InstallOcrCapability, InvokeAgent]),
            "IMPORT_WEB_PLATFORM_CAPABILITY_MISSING" => {
                (true, true, vec![InstallBrowserCapability, InvokeAgent])
            }
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
        for action in [Skip, ViewLog] {
            if !recovery_actions.contains(&action) {
                recovery_actions.push(action);
            }
        }
        let message = if code == "IMPORT_WEB_ACCOUNT_PERMISSION_DENIED" {
            "The current account cannot access this content."
        } else {
            "Web import could not be completed."
        };
        Self {
            code: code.into(),
            message: message.into(),
            stage,
            retryable,
            user_action_required,
            recovery_actions,
            available_actions: Vec::new(),
            subtitle_candidates: Vec::new(),
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
            "IMPORT_FILE_QUALITY_FAILED" => (
                true,
                true,
                vec![InstallOcrCapability, EnableOcr, SwitchParser, InvokeAgent],
            ),
            "IMPORT_FILE_CANCELLED" => (true, false, vec![Retry]),
            "IMPORT_FILE_SUBTITLE_AMBIGUOUS" => (true, true, vec![SelectSubtitle]),
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
            subtitle_candidates: Vec::new(),
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
            subtitle_candidates: Vec::new(),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<ImportResolutionContext>,
    /// Staging-bound merged Markdown. `ManualMerge.mergedHash` must match
    /// this immutable artifact before commit can advance the Source version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_merge: Option<ImportArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub item_id: String,
    #[serde(default)]
    pub item_revision: u64,
    pub input: ImportInput,
    pub status: ImportItemStatus,
    pub selected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_subtitle: Option<String>,
    pub task_id: Option<String>,
    pub progress: Option<TaskProgress>,
    pub attempts: Vec<AttemptRecord>,
    pub preview: Option<ImportPreviewArtifact>,
    pub issue: Option<ImportIssue>,
    /// Durable access metadata only. Connector cookies and browser profile
    /// paths must never be serialized into an import session.
    #[serde(default)]
    pub authenticated_retry: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_identity_summary: Option<String>,
    #[serde(default)]
    pub restricted_content: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restricted_identity_summary: Option<String>,
}

impl ImportItem {
    pub fn queued(item_id: &str, input: ImportInput) -> Self {
        Self {
            item_id: item_id.to_string(),
            item_revision: 1,
            input,
            status: ImportItemStatus::Queued,
            selected: true,
            selected_subtitle: None,
            task_id: None,
            progress: None,
            attempts: Vec::new(),
            preview: None,
            issue: None,
            authenticated_retry: false,
            authenticated_identity_summary: None,
            restricted_content: false,
            restricted_identity_summary: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCollectionRelation {
    pub relation_id: String,
    pub source_url: String,
    pub platform: String,
    pub title: String,
    pub child_item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ImportCollectionChildRelation>,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCollectionChildRelation {
    pub item_id: String,
    pub canonical_url: String,
    pub discovery_fingerprint: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_authorizations: Vec<ImportMediaAuthorization>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collection_relations: Vec<ImportCollectionRelation>,
    pub items: Vec<ImportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionPatchCounts {
    pub total: u64,
    pub processed: u64,
    pub succeeded: u64,
    pub waiting: u64,
    pub failed: u64,
    pub cancelled: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSessionPatchEvent {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub batch_id: String,
    pub items: Vec<ImportItem>,
    pub counts: ImportSessionPatchCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportMediaAuthorizationKind {
    Ocr,
    Asr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportAsrProfile {
    Fast,
    Balanced,
    Accurate,
}

impl Default for ImportAsrProfile {
    fn default() -> Self {
        Self::Balanced
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportMediaAuthorization {
    pub item_id: String,
    pub kind: ImportMediaAuthorizationKind,
    pub authorized_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_profile: Option<ImportAsrProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Immutable execution facts captured when an Import worker claims an item.
/// Canonical item/session JSON remains authoritative; workers must discard
/// results when `expected_item_revision` or the durable task claim changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemAuthorizationSnapshot {
    pub local_ocr_authorized: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_profile: Option<ImportAsrProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognition_language: Option<String>,
    pub local_asr_authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportWorkItemSnapshot {
    pub item_id: String,
    pub expected_item_revision: u64,
    pub input: ImportInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_subtitle: Option<String>,
    pub media_authorization: ImportItemAuthorizationSnapshot,
    pub authenticated_retry: bool,
    pub resource_mode: ImportResourceMode,
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
            media_authorizations: Vec::new(),
            collection_relations: Vec::new(),
            items: Vec::new(),
        }
    }

    pub fn has_media_authorization(
        &self,
        item_id: &str,
        kind: ImportMediaAuthorizationKind,
    ) -> bool {
        self.media_authorizations
            .iter()
            .any(|authorization| authorization.item_id == item_id && authorization.kind == kind)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitItemDecision {
    pub item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<ImportItemResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitImportSessionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    #[serde(default)]
    pub batch_task_id: Option<String>,
    #[serde(default)]
    pub acknowledge_restricted_content: bool,
    #[serde(default)]
    pub expected_selection_revision: Option<u64>,
    #[serde(default)]
    pub expected_confirmation_digest: Option<String>,
    #[serde(default)]
    pub decisions: Vec<CommitItemDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportCommitDisposition {
    NewSource,
    UpdatedSource,
    DuplicateSkipped,
    KeptCurrent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemCommitResult {
    pub item_id: String,
    pub source_id: Option<String>,
    pub version_id: Option<String>,
    pub wiki_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposition: Option<ImportCommitDisposition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<UserIssue>,
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
    /// Authoritative production summary used by the completion surface and
    /// Import History. Kept optional so pre-Batch-2 history remains readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<ImportCompletion>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn image_url_ocr_failure_offers_the_ocr_capability_installer() {
        let issue = ImportIssue::for_web_code("IMPORT_WEB_OCR_UNAVAILABLE", ImportStage::Extract);
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::InstallOcrCapability));
    }

    #[test]
    fn web_issue_recovery_actions_are_unique() {
        for code in [
            "IMPORT_WEB_LOGIN_REQUIRED",
            "IMPORT_WEB_ACCOUNT_PERMISSION_DENIED",
            "IMPORT_V2_URL_REJECTED",
            "IMPORT_V2_PRIVATE_TARGET_BLOCKED",
            "IMPORT_V2_RESPONSE_TOO_LARGE",
            "IMPORT_V2_CONNECTOR_RATE_LIMITED",
            "IMPORT_WEB_CONTENT_REMOVED",
            "IMPORT_WEB_MEDIA_HOST_UNSUPPORTED",
            "IMPORT_WEB_MEDIA_UNAVAILABLE",
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
            "IMPORT_WEB_OCR_UNAVAILABLE",
            "IMPORT_WEB_PLATFORM_CAPABILITY_MISSING",
            "IMPORT_V2_ENGINE_UNAVAILABLE",
        ] {
            let issue = ImportIssue::for_web_code(code, ImportStage::Extract);
            for (index, action) in issue.recovery_actions.iter().enumerate() {
                assert!(
                    !issue.recovery_actions[..index].contains(action),
                    "{code} repeats {action:?}"
                );
            }
            assert_eq!(
                issue.recovery_actions.last(),
                Some(&ImportRecoveryAction::ViewLog),
                "{code}"
            );
        }
    }

    #[test]
    fn attempt_error_code_is_additive_and_legacy_safe() {
        let legacy = r#"{
            "route":"web.generic.readability",
            "engineId":"builtin.web-http",
            "engineVersion":"0.1.0",
            "stage":"extract",
            "startedAt":"2026-08-07T00:00:00Z",
            "completedAt":null,
            "outcome":"failed",
            "warnings":[]
        }"#;
        let mut attempt: AttemptRecord = serde_json::from_str(legacy).unwrap();
        assert_eq!(attempt.error_code, None);

        attempt.error_code = Some("IMPORT_V2_PRIVATE_TARGET_BLOCKED".into());
        assert_eq!(
            serde_json::to_value(attempt).unwrap()["errorCode"],
            "IMPORT_V2_PRIVATE_TARGET_BLOCKED"
        );
    }

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
    fn media_authorization_is_bound_to_one_session_only() {
        let mut authorized = ImportSession::new(
            "session-authorized",
            "project-1",
            ImportResourceMode::Balanced,
        );
        authorized
            .media_authorizations
            .push(ImportMediaAuthorization {
                item_id: "item-1".into(),
                kind: ImportMediaAuthorizationKind::Ocr,
                authorized_at: "2026-07-26T00:00:00Z".into(),
                asr_profile: None,
                language: None,
            });
        let next_session =
            ImportSession::new("session-next", "project-1", ImportResourceMode::Balanced);
        assert!(authorized.has_media_authorization("item-1", ImportMediaAuthorizationKind::Ocr));
        assert!(!next_session.has_media_authorization("item-1", ImportMediaAuthorizationKind::Ocr));
        assert!(serde_json::to_string(&next_session)
            .unwrap()
            .find("mediaAuthorizations")
            .is_none());
    }

    #[test]
    fn item_state_machine_rejects_preview_to_complete_shortcut() {
        assert!(ImportItemStatus::Queued.can_transition_to(&ImportItemStatus::Inspecting));
        assert!(
            ImportItemStatus::Inspecting.can_transition_to(&ImportItemStatus::WaitingAuthorization)
        );
        assert!(
            ImportItemStatus::WaitingAuthorization.can_transition_to(&ImportItemStatus::Inspecting)
        );
        assert!(
            ImportItemStatus::WaitingCapability.can_transition_to(&ImportItemStatus::Inspecting)
        );
        assert!(
            ImportItemStatus::Extracting.can_transition_to(&ImportItemStatus::WaitingCapability)
        );
        assert!(ImportItemStatus::Validating.can_transition_to(&ImportItemStatus::PreviewReady));
        assert!(!ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Completed));
        assert!(ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Committing));
        assert!(ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Inspecting));
    }

    #[test]
    fn missing_web_subtitles_offer_asr_capability_recovery() {
        let issue =
            ImportIssue::for_web_code("IMPORT_WEB_SUBTITLE_UNAVAILABLE", ImportStage::Extract);
        assert!(issue.user_action_required);
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::InstallMediaCapability));
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::AuthorizeLocalAsr));
        assert!(!issue.recovery_actions.is_empty());
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

    #[test]
    fn removed_platform_content_is_not_presented_as_retryable() {
        let issue = ImportIssue::for_web_code("IMPORT_WEB_CONTENT_REMOVED", ImportStage::Extract);
        assert!(!issue.retryable);
        assert_eq!(
            issue.recovery_actions,
            vec![ImportRecoveryAction::Skip, ImportRecoveryAction::ViewLog]
        );
    }

    #[test]
    fn authenticated_account_permission_denial_never_loops_back_to_login() {
        let issue =
            ImportIssue::for_web_code("IMPORT_WEB_ACCOUNT_PERMISSION_DENIED", ImportStage::Extract);
        assert!(!issue.retryable);
        assert_eq!(
            issue.message,
            "The current account cannot access this content."
        );
        assert!(!issue
            .recovery_actions
            .contains(&ImportRecoveryAction::BeginLogin));
        assert_eq!(
            issue.recovery_actions,
            vec![ImportRecoveryAction::Skip, ImportRecoveryAction::ViewLog]
        );
    }

    #[test]
    fn missing_platform_route_offers_capability_and_local_agent_recovery() {
        let issue =
            ImportIssue::for_web_code("IMPORT_WEB_PLATFORM_CAPABILITY_MISSING", ImportStage::Route);
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::InstallBrowserCapability));
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::InvokeAgent));
    }

    #[test]
    fn frozen_batch_zero_contract_serializes_for_typescript_consumers() {
        let contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../../test-fixtures/import-v2-batch0-contract.json"
        ))
        .unwrap();
        let input_kinds = [
            ImportInputKind::File,
            ImportInputKind::Folder,
            ImportInputKind::Url,
            ImportInputKind::ClipboardText,
        ];
        assert_eq!(
            serde_json::to_value(input_kinds).unwrap(),
            contract["inputKinds"]
        );
        let states = [
            ImportUserState::Discovering,
            ImportUserState::Processing,
            ImportUserState::NeedsAction,
            ImportUserState::Ready,
            ImportUserState::Committing,
            ImportUserState::Committed,
            ImportUserState::Failed,
        ];
        assert_eq!(
            serde_json::to_value(states).unwrap(),
            contract["userStates"]
        );

        let primary_actions = [
            ImportPrimaryAction::Retry,
            ImportPrimaryAction::SignIn,
            ImportPrimaryAction::Authorize,
            ImportPrimaryAction::InstallCapability,
            ImportPrimaryAction::EnableOcr,
            ImportPrimaryAction::AuthorizeLocalAsr,
            ImportPrimaryAction::InvokeLocalAgent,
            ImportPrimaryAction::Review,
            ImportPrimaryAction::Resolve,
            ImportPrimaryAction::Resume,
        ];
        assert_eq!(
            serde_json::to_value(primary_actions).unwrap(),
            contract["primaryActions"]
        );

        let candidate_hash = "a".repeat(64);
        let current_hash = "b".repeat(64);
        let source_id = "src_a".to_string();
        let target_version_id = "ver_b".to_string();
        let resolutions = vec![
            ImportItemResolution::NewSource,
            ImportItemResolution::ExactDuplicateSkip {
                source_id: source_id.clone(),
                candidate_hash: candidate_hash.clone(),
                current_hash: current_hash.clone(),
                target_version_id: target_version_id.clone(),
            },
            ImportItemResolution::SameSourceNewVersion {
                source_id: source_id.clone(),
                candidate_hash: candidate_hash.clone(),
                current_hash: current_hash.clone(),
                target_version_id: target_version_id.clone(),
            },
            ImportItemResolution::KeepCurrentSource {
                source_id: source_id.clone(),
                candidate_hash: candidate_hash.clone(),
                current_hash: current_hash.clone(),
                target_version_id: target_version_id.clone(),
            },
            ImportItemResolution::ApplyImportCandidate {
                source_id: source_id.clone(),
                candidate_hash: candidate_hash.clone(),
                current_hash: current_hash.clone(),
                target_version_id: target_version_id.clone(),
            },
            ImportItemResolution::ManualMerge {
                source_id,
                candidate_hash,
                current_hash,
                target_version_id,
                merged_hash: "c".repeat(64),
            },
        ];
        assert_eq!(
            serde_json::to_value(resolutions).unwrap(),
            contract["resolutions"]
        );

        let completion: ImportCompletion =
            serde_json::from_value(contract["completion"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(completion).unwrap(),
            contract["completion"]
        );

        let frontmatter: SourceFrontmatter =
            serde_json::from_value(contract["sourceFrontmatter"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(frontmatter).unwrap(),
            contract["sourceFrontmatter"]
        );
    }

    #[test]
    fn source_frontmatter_contract_is_secret_free_and_validates_manifest_hash() {
        let hash = "c".repeat(64);
        let frontmatter = SourceFrontmatter {
            page_type: SourcePageType::Source,
            source_id: "src_a".into(),
            version_id: "ver_a".into(),
            source_kind: "local_document".into(),
            title: "研究资料".into(),
            imported_at: "2026-07-25T00:00:00Z".into(),
            content_hash: hash.clone(),
            platform: None,
            canonical_url: None,
            platform_content_id: None,
            author: None,
            published_at: None,
            language: Some("zh-CN".into()),
            quality: QualityReport {
                level: QualityLevel::Pass,
                metrics: Vec::new(),
                warnings: Vec::new(),
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
            },
            restricted: false,
        };
        validate_source_frontmatter(&frontmatter, &hash).unwrap();
        let value = serde_json::to_value(&frontmatter).unwrap();
        assert_eq!(value["type"], "source");
        assert_eq!(value["sourceId"], "src_a");
        assert_eq!(value["platformContentId"], serde_json::Value::Null);
        let encoded = serde_json::to_string(&value).unwrap();
        for forbidden in ["cookie", "token", "staging", "sessionId", "engineId"] {
            assert!(!encoded.contains(forbidden));
        }
        for forbidden in ["cookie", "token", "stagingPath", "sessionId", "engineId"] {
            let mut forbidden_value = value.clone();
            forbidden_value[forbidden] = json!("must-not-be-accepted");
            assert!(
                serde_json::from_value::<SourceFrontmatter>(forbidden_value).is_err(),
                "Source frontmatter must reject {forbidden}"
            );
        }
        assert_eq!(
            validate_source_frontmatter(&frontmatter, &"d".repeat(64))
                .unwrap_err()
                .code,
            "manifest_mismatch"
        );
    }

    #[test]
    fn legacy_cjk_session_preserves_explicit_media_mode_while_new_default_is_extract_only() {
        let session: ImportSession = serde_json::from_value(json!({
            "schemaVersion": 2,
            "sessionId": "session-cjk",
            "projectId": "project-cjk",
            "status": "draft",
            "resourceMode": "balanced",
            "createdAt": "2026-07-25T00:00:00Z",
            "updatedAt": "2026-07-25T00:00:00Z",
            "items": [{
                "itemId": "item-cjk",
                "input": {
                    "kind": "url",
                    "displayName": "访谈视频：研发复盘.mp4",
                    "locator": "https://example.com/video",
                    "normalizedLocator": "https://example.com/video",
                    "mediaSaveMode": "preserve_original"
                },
                "status": "queued",
                "selected": false,
                "taskId": null,
                "progress": null,
                "attempts": [],
                "preview": null,
                "issue": null
            }]
        }))
        .unwrap();
        assert_eq!(
            session.items[0].input.display_name,
            "访谈视频：研发复盘.mp4"
        );
        assert_eq!(
            session.items[0].input.media_save_mode,
            MediaSaveMode::PreserveOriginal
        );

        let input: ImportInput = serde_json::from_value(json!({
            "kind": "url",
            "displayName": "新远程媒体",
            "locator": "https://example.com/new",
            "normalizedLocator": null
        }))
        .unwrap();
        assert_eq!(input.media_save_mode, MediaSaveMode::ExtractOnly);
    }

    #[test]
    fn remote_media_defaults_to_extract_only_without_retaining_the_original_payload() {
        assert_eq!(MediaSaveMode::default(), MediaSaveMode::ExtractOnly);
        let value = serde_json::to_value(MediaSaveMode::default()).unwrap();
        assert_eq!(value, "extract_only");
    }
}

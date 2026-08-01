use serde::{Deserialize, Serialize};

use crate::models::agent::AgentKind;
use crate::models::confirmation::{PendingActionType, RiskLevel};
use crate::models::llm::LlmProviderKind;
use crate::models::task::TaskStatus;

pub const WORKFLOW_SCHEMA_VERSION: u32 = 1;

fn workflow_schema_version() -> u32 {
    WORKFLOW_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowKind {
    UpdateWiki,
    HealthCheck,
    GenerateContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDisplayStatus {
    Queued,
    Running,
    WaitingForConfirmation,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl From<&TaskStatus> for WorkflowDisplayStatus {
    fn from(status: &TaskStatus) -> Self {
        match status {
            TaskStatus::Queued => Self::Queued,
            TaskStatus::Running | TaskStatus::Cancelling => Self::Running,
            TaskStatus::WaitingForConfirmation => Self::WaitingForConfirmation,
            TaskStatus::Succeeded => Self::Completed,
            TaskStatus::Failed => Self::Failed,
            TaskStatus::Cancelled => Self::Cancelled,
            TaskStatus::Interrupted => Self::Interrupted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkflowRoute {
    Local {
        route_revision: String,
    },
    Agent {
        agent: AgentKind,
        model: Option<String>,
        route_revision: String,
    },
    Byok {
        provider: LlmProviderKind,
        model: String,
        route_revision: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkflowRouteSelection {
    Agent { agent: AgentKind },
    Byok { provider: LlmProviderKind },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSourceVersionRef {
    pub source_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateWikiMode {
    ChangedSources,
    FullRecompile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckMode {
    LocalQuick,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowArtifactType {
    BeautifulRead,
    KnowledgeCard,
    ConceptMap,
    ProjectReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkflowScope {
    UpdateWiki {
        mode: UpdateWikiMode,
        source_versions: Vec<WorkflowSourceVersionRef>,
    },
    HealthCheck {
        mode: HealthCheckMode,
    },
    GenerateContent {
        artifact_type: WorkflowArtifactType,
        page_paths: Vec<String>,
        output_path: Option<String>,
    },
}

/// Bounded, non-secret execution facts that may be reused for a linked retry.
/// Free-form prompts, credentials, source text, model output, raw arguments,
/// and temporary paths have no representation in this persisted type.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowExecutionOptions {
    pub preparation_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_target_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restricted_content_acknowledgement_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_provider_acknowledgement_revision: Option<String>,
}

impl WorkflowExecutionOptions {
    pub fn validate(&self) -> Result<(), String> {
        validate_revision("preparationRevision", &self.preparation_revision)?;
        if let Some(hash) = self.existing_target_hash.as_deref() {
            if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("existingTargetHash must be a SHA-256 hex digest".into());
            }
        }
        if let Some(revision) = self.restricted_content_acknowledgement_revision.as_deref() {
            validate_revision("restrictedContentAcknowledgementRevision", revision)?;
        }
        if let Some(revision) = self.remote_provider_acknowledgement_revision.as_deref() {
            validate_revision("remoteProviderAcknowledgementRevision", revision)?;
        }
        Ok(())
    }
}

fn validate_revision(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("{label} is not a bounded backend revision"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStageStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Waiting,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCountProgress {
    pub current: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkflowCandidateReference {
    TaskOwned { candidate_id: String },
    ProjectRelative { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPendingAction {
    pub id: String,
    pub action_type: PendingActionType,
    pub risk_level: RiskLevel,
    pub affected_paths: Vec<String>,
    pub candidate: Option<WorkflowCandidateReference>,
    pub expires_at: Option<String>,
    pub checkpoint_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStage {
    pub id: String,
    pub ordinal: u32,
    pub status: WorkflowStageStatus,
    pub label_key: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub current_item: Option<String>,
    pub progress: Option<WorkflowCountProgress>,
    pub decision: Option<WorkflowPendingAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBaselineSummary {
    pub fingerprint: String,
    pub captured_at: String,
    pub item_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowProjectTrust {
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFilesystemAccess {
    Writable,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPersistenceMode {
    Persistent,
    MemoryOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowGitState {
    Clean,
    Dirty,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectAccessSummary {
    pub project_id: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub trust: WorkflowProjectTrust,
    pub filesystem_access: WorkflowFilesystemAccess,
    pub persistence: WorkflowPersistenceMode,
    pub git_state: WorkflowGitState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPrerequisiteAction {
    OpenOrCreateProject,
    TrustProject,
    MakeWritable,
    ConfigureGit,
    ResolveDirtyGit,
    ImportSources,
    UpdateWiki,
    ConfigureExecutionRoute,
    ChooseExecutionRoute,
    PrepareAgain,
    AcknowledgeRemoteProvider,
    AcknowledgeRestrictedContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPrerequisite {
    pub code: String,
    pub message_key: String,
    pub blocking: bool,
    pub action: WorkflowPrerequisiteAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowGitPolicy {
    NotRequired,
    RequiredBeforeWrite,
    RequiredBeforeOverwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputSummary {
    pub label_key: String,
    pub location: Option<String>,
    pub may_change_wiki: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPreparation {
    #[serde(default = "workflow_schema_version")]
    pub schema_version: u32,
    pub preparation_id: String,
    pub preparation_revision: String,
    pub project_access: WorkflowProjectAccessSummary,
    pub kind: WorkflowKind,
    pub scope: WorkflowScope,
    pub baseline: WorkflowBaselineSummary,
    pub route: Option<WorkflowRoute>,
    pub prerequisites: Vec<WorkflowPrerequisite>,
    pub output: WorkflowOutputSummary,
    pub git_policy: WorkflowGitPolicy,
    pub requires_scope_confirmation: bool,
    pub quick_rerun_eligible: bool,
    #[serde(default)]
    pub available_source_versions: Vec<WorkflowSourceVersionRef>,
    #[serde(default)]
    pub available_wiki_pages: Vec<String>,
    #[serde(default)]
    pub available_routes: Vec<WorkflowRouteSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkflowResult {
    UpdateWiki {
        created: u64,
        updated: u64,
        skipped: u64,
        deleted: u64,
        conflicted: u64,
        affected_paths: Vec<String>,
        checkpoint_hash: Option<String>,
        final_commit: Option<String>,
    },
    HealthCheck {
        report_id: Option<String>,
        persistent: bool,
        error_count: u64,
        warning_count: u64,
        info_count: u64,
    },
    GenerateContent {
        artifact_type: WorkflowArtifactType,
        record_id: Option<String>,
        output_paths: Vec<String>,
        validation_passed: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowErrorSummary {
    pub code: String,
    pub message_key: String,
    pub recoverable: bool,
    pub user_action_required: bool,
    pub suggested_action: Option<WorkflowPrerequisiteAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRetryLink {
    pub attempt_of: String,
    pub attempt_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    #[serde(default = "workflow_schema_version")]
    pub schema_version: u32,
    pub task_id: String,
    pub project_id: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub kind: WorkflowKind,
    pub display_status: WorkflowDisplayStatus,
    pub scope: WorkflowScope,
    pub route: Option<WorkflowRoute>,
    pub fingerprint: String,
    pub baseline_fingerprint: String,
    pub stages: Vec<WorkflowStage>,
    pub current_stage_id: Option<String>,
    pub queue_position: Option<u32>,
    #[serde(default)]
    pub continuation_required: bool,
    #[serde(default)]
    pub retry: Option<WorkflowRetryLink>,
    #[serde(default)]
    pub pending_action: Option<WorkflowPendingAction>,
    #[serde(default)]
    pub result: Option<WorkflowResult>,
    #[serde(default)]
    pub error: Option<WorkflowErrorSummary>,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancellable: bool,
    pub undo_cancel_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOverviewState {
    Ready,
    NeedsPrerequisite,
    Queued,
    Running,
    WaitingForConfirmation,
    Failed,
    Interrupted,
    UpToDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOverviewRow {
    pub kind: WorkflowKind,
    pub state: WorkflowOverviewState,
    pub recommended: bool,
    pub active_task_id: Option<String>,
    pub last_completed_at: Option<String>,
    pub prerequisite: Option<WorkflowPrerequisite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowsOverview {
    #[serde(default = "workflow_schema_version")]
    pub schema_version: u32,
    pub project_access: Option<WorkflowProjectAccessSummary>,
    pub rows: Vec<WorkflowOverviewRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStartOutcome {
    Created { run: WorkflowRun },
    Existing { run: WorkflowRun },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunPage {
    pub runs: Vec<WorkflowRun>,
    pub next_cursor: Option<String>,
}

/// Private persisted execution state attached to one generic task snapshot.
/// It deliberately mirrors only bounded, non-secret workflow facts; task
/// lifecycle timestamps and project id remain owned by `BackendTask`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionState {
    #[serde(default = "workflow_schema_version")]
    pub schema_version: u32,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub kind: WorkflowKind,
    pub scope: WorkflowScope,
    #[serde(default)]
    pub execution_options: WorkflowExecutionOptions,
    pub route: Option<WorkflowRoute>,
    pub fingerprint: String,
    pub baseline_fingerprint: String,
    pub stages: Vec<WorkflowStage>,
    pub current_stage_id: Option<String>,
    pub queue_position: Option<u32>,
    #[serde(default)]
    pub continuation_required: bool,
    #[serde(default)]
    pub retry: Option<WorkflowRetryLink>,
    #[serde(default)]
    pub pending_action: Option<WorkflowPendingAction>,
    #[serde(default)]
    pub result: Option<WorkflowResult>,
    #[serde(default)]
    pub error: Option<WorkflowErrorSummary>,
    #[serde(default)]
    pub cancelled_from_queue: bool,
    #[serde(default)]
    pub undo_cancel_until: Option<String>,
}

impl WorkflowExecutionState {
    pub fn to_run(&self, task: &crate::models::task::BackendTask) -> Option<WorkflowRun> {
        Some(WorkflowRun {
            schema_version: self.schema_version,
            task_id: task.id.clone(),
            project_id: task.project_id.clone()?,
            canonical_identity_key: self.canonical_identity_key.clone(),
            identity_revision: self.identity_revision.clone(),
            kind: self.kind.clone(),
            display_status: WorkflowDisplayStatus::from(&task.status),
            scope: self.scope.clone(),
            route: self.route.clone(),
            fingerprint: self.fingerprint.clone(),
            baseline_fingerprint: self.baseline_fingerprint.clone(),
            stages: self.stages.clone(),
            current_stage_id: self.current_stage_id.clone(),
            queue_position: self.queue_position,
            continuation_required: self.continuation_required,
            retry: self.retry.clone(),
            pending_action: self.pending_action.clone(),
            result: self.result.clone(),
            error: self.error.clone(),
            started_at: task.started_at.clone(),
            updated_at: task.updated_at.clone(),
            completed_at: task.completed_at.clone(),
            cancellable: task.cancellable,
            undo_cancel_until: self.undo_cancel_until.clone(),
        })
    }
}

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

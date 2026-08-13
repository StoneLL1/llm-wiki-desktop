use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::models::agent::AgentKind;
use crate::models::confirmation::{PendingActionType, RiskLevel};
use crate::models::lint::{
    AgentLintRepairFinding, AgentLintRepairOutcome, AgentLintRepairRoundSummary, WikiLintSkillRef,
};
use crate::models::llm::LlmProviderKind;
use crate::models::task::TaskStatus;

pub const WORKFLOW_SCHEMA_VERSION: u32 = 2;

fn workflow_schema_version() -> u32 {
    WORKFLOW_SCHEMA_VERSION
}

fn legacy_workflow_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowKind {
    UpdateWiki,
    HealthCheck,
    GenerateContent,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOperationKind {
    #[default]
    BuiltIn,
    AgentLintRepair,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkflowOperation {
    #[default]
    BuiltIn,
    AgentLintRepair {
        preparation_id: String,
        preparation_revision: String,
        report_id: String,
        selection_revision: String,
        selected_finding_ids: Vec<String>,
        selected_findings: Vec<AgentLintRepairFinding>,
        skill: WikiLintSkillRef,
        authorized_path_hashes: BTreeMap<String, Option<String>>,
        expected_git_head: String,
    },
}

impl WorkflowOperation {
    pub fn kind(&self) -> WorkflowOperationKind {
        match self {
            Self::BuiltIn => WorkflowOperationKind::BuiltIn,
            Self::AgentLintRepair { .. } => WorkflowOperationKind::AgentLintRepair,
        }
    }

    fn validate(&self) -> Result<(), String> {
        let Self::AgentLintRepair {
            preparation_id,
            preparation_revision,
            report_id,
            selection_revision,
            selected_finding_ids,
            selected_findings,
            skill,
            authorized_path_hashes,
            expected_git_head,
        } = self
        else {
            return Ok(());
        };
        for (label, value) in [
            ("preparationId", preparation_id),
            ("preparationRevision", preparation_revision),
            ("reportId", report_id),
            ("selectionRevision", selection_revision),
        ] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(format!("{label} is not a bounded operation fact"));
            }
        }
        if selected_finding_ids.is_empty()
            || selected_finding_ids.len() > 100
            || selected_finding_ids
                .iter()
                .any(|value| value.trim().is_empty())
        {
            return Err("Agent lint repair requires 1..=100 selected finding ids".into());
        }
        let mut sorted_findings = selected_finding_ids.clone();
        sorted_findings.sort();
        if sorted_findings != *selected_finding_ids
            || sorted_findings.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err("Agent lint repair finding ids must be sorted and unique".into());
        }
        if selected_findings.is_empty()
            || selected_findings.len() != selected_finding_ids.len()
            || selected_findings.len() > 100
            || selected_findings
                .iter()
                .map(|finding| finding.id.as_str())
                .ne(selected_finding_ids.iter().map(String::as_str))
        {
            return Err(
                "Agent lint repair finding snapshots must exactly match selected finding ids"
                    .into(),
            );
        }
        if selected_findings.iter().any(|finding| {
            finding.id.len() > 1_024
                || finding.path.trim().is_empty()
                || finding.path.len() > 1_024
                || finding.message.trim().is_empty()
                || finding.message.chars().count() > 8_192
                || finding
                    .evidence
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > 8_192)
                || finding
                    .suggested_action
                    .as_ref()
                    .is_some_and(|value| value.chars().count() > 8_192)
        }) {
            return Err("Agent lint repair finding snapshots are not bounded".into());
        }
        if !skill.is_builtin() {
            return Err("Agent lint repair must use the pinned built-in Skill".into());
        }
        if authorized_path_hashes.is_empty()
            || authorized_path_hashes.len() > 256
            || authorized_path_hashes.iter().any(|(path, hash)| {
                path.trim().is_empty()
                    || path.len() > 1_024
                    || hash.as_ref().is_some_and(|value| {
                        value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
            })
        {
            return Err("Agent lint repair authorized path hashes are invalid".into());
        }
        if !(7..=64).contains(&expected_git_head.len())
            || !expected_git_head
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Agent lint repair expected Git HEAD is invalid".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkflowRunnerKey {
    pub kind: WorkflowKind,
    pub operation: WorkflowOperationKind,
}

impl WorkflowRunnerKey {
    pub fn new(kind: WorkflowKind, operation: WorkflowOperationKind) -> Self {
        Self { kind, operation }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    #[serde(default)]
    pub operation: WorkflowOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation_fingerprint: Option<String>,
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
        self.operation.validate()?;
        if let Some(fingerprint) = self.preparation_fingerprint.as_deref() {
            validate_revision("preparationFingerprint", fingerprint)?;
        }
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

pub(crate) fn validate_workflow_execution_contract(
    kind: &WorkflowKind,
    scope: &WorkflowScope,
    route: Option<&WorkflowRoute>,
    execution_options: &WorkflowExecutionOptions,
) -> Result<(), String> {
    execution_options.validate()?;
    if !matches!(
        execution_options.operation,
        WorkflowOperation::AgentLintRepair { .. }
    ) {
        return Ok(());
    }
    if kind != &WorkflowKind::HealthCheck {
        return Err("Agent lint repair must remain a Health Check operation".into());
    }
    if !matches!(
        scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete
        }
    ) {
        return Err("Agent lint repair requires Complete Health scope".into());
    }
    if !matches!(route, Some(WorkflowRoute::Agent { .. })) {
        return Err("Agent lint repair requires an Agent route".into());
    }
    Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDecisionCounts {
    pub created: u32,
    pub modified: u32,
    pub overwritten: u32,
    pub deleted: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFileDiffKind {
    #[default]
    TwoWay,
    ThreeWay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFileDiff {
    #[serde(default)]
    pub file_id: String,
    pub path: String,
    #[serde(default)]
    pub diff_bytes: usize,
    pub diff: Option<String>,
    #[serde(default)]
    pub kind: WorkflowFileDiffKind,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFileDiffPage {
    pub file_id: String,
    pub path: String,
    pub kind: WorkflowFileDiffKind,
    pub diff: String,
    pub next_cursor: Option<usize>,
    pub truncated: bool,
}

/// Read-only review data hydrated from the backend-owned confirmation
/// registry. It is deliberately absent from persisted workflow execution
/// state so frontend detail reads cannot become a continuation payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDecisionReview {
    pub reason: String,
    pub counts: WorkflowDecisionCounts,
    pub user_edits_detected: bool,
    pub file_diffs: Vec<WorkflowFileDiff>,
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

fn default_workflow_persistence_mode() -> WorkflowPersistenceMode {
    // A recovered workflow necessarily came from a persisted task snapshot.
    // New memory-only runs always set the mode explicitly at creation time.
    WorkflowPersistenceMode::Persistent
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPersistenceTransition {
    Unchanged,
    DowngradedToMemoryOnly,
    UpgradedToPersistent,
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
#[serde(rename_all = "camelCase")]
pub struct WorkflowHealthCoverageSummary {
    pub mode: HealthCheckMode,
    pub scanned_pages: u64,
    pub deep_covered_pages: Option<u64>,
    pub deep_truncated: bool,
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
        #[serde(default)]
        coverage: Option<WorkflowHealthCoverageSummary>,
        #[serde(default)]
        findings_by_type: BTreeMap<String, u64>,
    },
    GenerateContent {
        artifact_type: WorkflowArtifactType,
        record_id: Option<String>,
        output_paths: Vec<String>,
        #[serde(default)]
        artifact_count: Option<u64>,
        validation_passed: bool,
    },
    AgentLintRepair {
        outcome: AgentLintRepairOutcome,
        resolved_finding_ids: Vec<String>,
        unresolved_finding_ids: Vec<String>,
        introduced_finding_ids: Vec<String>,
        skipped_finding_ids: Vec<String>,
        rounds: Vec<AgentLintRepairRoundSummary>,
        affected_paths: Vec<String>,
        affected_path_hashes: BTreeMap<String, Option<String>>,
        checkpoint_hash: Option<String>,
        final_commit: Option<String>,
        diff_available: bool,
        rollback_available: bool,
        #[serde(default)]
        index_refresh_warnings: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowProjectMutationState {
    NotModified,
    Modified,
    RolledBack,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowErrorSummary {
    pub code: String,
    pub message_key: String,
    pub recoverable: bool,
    pub user_action_required: bool,
    pub suggested_action: Option<WorkflowPrerequisiteAction>,
    #[serde(default)]
    pub project_mutation_state: WorkflowProjectMutationState,
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
    #[serde(default)]
    pub operation: WorkflowOperation,
    pub display_status: WorkflowDisplayStatus,
    pub scope: WorkflowScope,
    pub route: Option<WorkflowRoute>,
    pub fingerprint: String,
    pub baseline_fingerprint: String,
    #[serde(default = "default_workflow_persistence_mode")]
    pub persistence: WorkflowPersistenceMode,
    #[serde(default)]
    pub persistence_transition: Option<WorkflowPersistenceTransition>,
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
    pub decision_review: Option<WorkflowDecisionReview>,
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
    #[serde(default)]
    pub active_continuation_required: bool,
    pub last_completed_at: Option<String>,
    #[serde(default)]
    pub last_completed_task_id: Option<String>,
    pub prerequisite: Option<WorkflowPrerequisite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowHealthContextSummary {
    pub task_id: String,
    pub completed_at: String,
    pub error_count: u64,
    pub warning_count: u64,
    pub info_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowArtifactContextSummary {
    pub task_id: String,
    pub completed_at: String,
    pub artifact_type: WorkflowArtifactType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowQueueContextItem {
    pub task_id: String,
    pub kind: WorkflowKind,
    pub operation: WorkflowOperation,
    pub queue_position: Option<u32>,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowContextSummary {
    pub pending_source_count: usize,
    pub last_health: Option<WorkflowHealthContextSummary>,
    pub recent_artifact: Option<WorkflowArtifactContextSummary>,
    pub queue_count: usize,
    pub queued_runs: Vec<WorkflowQueueContextItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowsOverview {
    #[serde(default = "workflow_schema_version")]
    pub schema_version: u32,
    pub project_access: Option<WorkflowProjectAccessSummary>,
    pub rows: Vec<WorkflowOverviewRow>,
    #[serde(default)]
    pub recent_runs: Vec<WorkflowRunSummary>,
    #[serde(default)]
    pub context_summary: Option<WorkflowContextSummary>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunHistoryPage {
    pub runs: Vec<WorkflowRunSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorkflowRunOutcomeSummary {
    UpdateWiki {
        created: u64,
        updated: u64,
        skipped: u64,
    },
    HealthCheck {
        error_count: u64,
        warning_count: u64,
        info_count: u64,
    },
    GenerateContent {
        artifact_type: WorkflowArtifactType,
        artifact_count: u64,
        validation_passed: bool,
    },
    AgentLintRepair {
        outcome: AgentLintRepairOutcome,
        resolved_count: u64,
        unresolved_count: u64,
        introduced_count: u64,
        index_refresh_stale: bool,
    },
}

impl From<&WorkflowResult> for WorkflowRunOutcomeSummary {
    fn from(result: &WorkflowResult) -> Self {
        match result {
            WorkflowResult::UpdateWiki {
                created,
                updated,
                skipped,
                ..
            } => Self::UpdateWiki {
                created: *created,
                updated: *updated,
                skipped: *skipped,
            },
            WorkflowResult::HealthCheck {
                error_count,
                warning_count,
                info_count,
                ..
            } => Self::HealthCheck {
                error_count: *error_count,
                warning_count: *warning_count,
                info_count: *info_count,
            },
            WorkflowResult::GenerateContent {
                artifact_type,
                artifact_count,
                output_paths,
                validation_passed,
                ..
            } => Self::GenerateContent {
                artifact_type: artifact_type.clone(),
                artifact_count: artifact_count.unwrap_or(output_paths.len() as u64),
                validation_passed: *validation_passed,
            },
            WorkflowResult::AgentLintRepair {
                outcome,
                resolved_finding_ids,
                unresolved_finding_ids,
                introduced_finding_ids,
                index_refresh_warnings,
                ..
            } => Self::AgentLintRepair {
                outcome: *outcome,
                resolved_count: resolved_finding_ids.len() as u64,
                unresolved_count: unresolved_finding_ids.len() as u64,
                introduced_count: introduced_finding_ids.len() as u64,
                index_refresh_stale: !index_refresh_warnings.is_empty(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunSummary {
    #[serde(default = "workflow_schema_version")]
    pub schema_version: u32,
    pub task_id: String,
    pub project_id: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub kind: WorkflowKind,
    pub operation: WorkflowOperation,
    pub display_status: WorkflowDisplayStatus,
    pub retry: Option<WorkflowRetryLink>,
    #[serde(default)]
    pub outcome: Option<WorkflowRunOutcomeSummary>,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl From<&WorkflowRun> for WorkflowRunSummary {
    fn from(run: &WorkflowRun) -> Self {
        Self {
            schema_version: run.schema_version,
            task_id: run.task_id.clone(),
            project_id: run.project_id.clone(),
            canonical_identity_key: run.canonical_identity_key.clone(),
            identity_revision: run.identity_revision.clone(),
            kind: run.kind.clone(),
            operation: run.operation.clone(),
            display_status: run.display_status.clone(),
            retry: run.retry.clone(),
            outcome: run.result.as_ref().map(WorkflowRunOutcomeSummary::from),
            started_at: run.started_at.clone(),
            updated_at: run.updated_at.clone(),
            completed_at: run.completed_at.clone(),
        }
    }
}

/// Private persisted execution state attached to one generic task snapshot.
/// It deliberately mirrors only bounded, non-secret workflow facts; task
/// lifecycle timestamps and project id remain owned by `BackendTask`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionState {
    #[serde(default = "legacy_workflow_schema_version")]
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
    #[serde(default = "default_workflow_persistence_mode")]
    pub persistence: WorkflowPersistenceMode,
    #[serde(default)]
    pub persistence_transition: Option<WorkflowPersistenceTransition>,
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
            operation: self.execution_options.operation.clone(),
            display_status: WorkflowDisplayStatus::from(&task.status),
            scope: self.scope.clone(),
            route: self.route.clone(),
            fingerprint: self.fingerprint.clone(),
            baseline_fingerprint: self.baseline_fingerprint.clone(),
            persistence: self.persistence.clone(),
            persistence_transition: self.persistence_transition,
            stages: self.stages.clone(),
            current_stage_id: self.current_stage_id.clone(),
            queue_position: self.queue_position,
            continuation_required: self.continuation_required,
            retry: self.retry.clone(),
            pending_action: self.pending_action.clone(),
            decision_review: None,
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

#[cfg(test)]
mod ui3_contract_tests {
    use super::*;

    #[test]
    fn additive_ui3_result_fields_keep_legacy_payloads_readable() {
        let health: WorkflowResult = serde_json::from_value(serde_json::json!({
            "kind": "health_check",
            "reportId": "report-a",
            "persistent": true,
            "errorCount": 1,
            "warningCount": 2,
            "infoCount": 3
        }))
        .unwrap();
        assert!(matches!(
            health,
            WorkflowResult::HealthCheck {
                coverage: None,
                findings_by_type,
                ..
            } if findings_by_type.is_empty()
        ));

        let generated: WorkflowResult = serde_json::from_value(serde_json::json!({
            "kind": "generate_content",
            "artifactType": "project_report",
            "recordId": null,
            "outputPaths": ["exports/report.html"],
            "validationPassed": true
        }))
        .unwrap();
        assert!(matches!(
            generated,
            WorkflowResult::GenerateContent {
                artifact_count: None,
                ..
            }
        ));
    }

    #[test]
    fn additive_ui3_review_and_error_fields_default_fail_closed() {
        let diff: WorkflowFileDiff = serde_json::from_value(serde_json::json!({
            "path": "wiki/a.md",
            "diff": "candidate"
        }))
        .unwrap();
        assert_eq!(diff.kind, WorkflowFileDiffKind::TwoWay);

        let error: WorkflowErrorSummary = serde_json::from_value(serde_json::json!({
            "code": "FAILED",
            "messageKey": "failed",
            "recoverable": true,
            "userActionRequired": false,
            "suggestedAction": null
        }))
        .unwrap();
        assert_eq!(
            error.project_mutation_state,
            WorkflowProjectMutationState::Unknown
        );
    }

    #[test]
    fn workflow_history_summary_exposes_only_a_bounded_typed_outcome() {
        let run: WorkflowRun = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "taskId": "task-a",
            "projectId": "project-a",
            "canonicalIdentityKey": "identity-a",
            "identityRevision": "revision-a",
            "kind": "update_wiki",
            "displayStatus": "completed",
            "scope": { "kind": "update_wiki", "mode": "changed_sources", "sourceVersions": [] },
            "route": null,
            "fingerprint": "fingerprint-a",
            "baselineFingerprint": "baseline-a",
            "stages": [],
            "currentStageId": null,
            "queuePosition": null,
            "retry": null,
            "pendingAction": null,
            "result": {
                "kind": "update_wiki",
                "created": 2,
                "updated": 3,
                "skipped": 1,
                "deleted": 0,
                "conflicted": 0,
                "affectedPaths": ["wiki/private-and-long-path.md"],
                "checkpointHash": "abc123",
                "finalCommit": "def456"
            },
            "error": null,
            "startedAt": "2026-08-10T08:00:00Z",
            "updatedAt": "2026-08-10T08:01:30Z",
            "completedAt": "2026-08-10T08:01:30Z",
            "cancellable": false,
            "undoCancelUntil": null
        }))
        .unwrap();

        let payload = serde_json::to_value(WorkflowRunSummary::from(&run)).unwrap();
        assert_eq!(payload["outcome"]["kind"], "update_wiki");
        assert_eq!(payload["outcome"]["created"], 2);
        assert_eq!(payload["outcome"]["updated"], 3);
        assert_eq!(payload["outcome"]["skipped"], 1);
        assert!(payload.to_string().len() < 1_024);
        assert!(!payload.to_string().contains("private-and-long-path"));
        assert!(!payload.to_string().contains("checkpointHash"));
    }
}

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::compile::{CompileChangeSummary, CompileManifest};
use crate::models::confirmation::{
    ActionPreview, AgentLintRepairReviewBinding, ConfirmationClaimDisposition,
    ConfirmationExecution, ConfirmationRegistry, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::lint::{
    AgentLintRepairCorrelation, AgentLintRepairFinding, AgentLintRepairOutcome,
    AgentLintRepairRequest, AgentLintRepairRoundOutput, AgentLintRepairRoundSummary,
    WikiLintSkillRef, WIKI_LINT_SCHEMA_VERSION,
};
use crate::models::paths::ProjectContext;
use crate::models::workflow::{
    WorkflowCandidateReference, WorkflowErrorSummary, WorkflowKind, WorkflowOperation,
    WorkflowOperationKind, WorkflowPendingAction, WorkflowPrerequisiteAction,
    WorkflowProjectMutationState, WorkflowResult, WorkflowRoute, WorkflowRun,
};
use crate::services::WorkflowExternalLaunchPermit;
use crate::services::{
    AgentService, BookmarkService, CompileService, FileStore, GitService, LintService,
    SearchService, SettingsService,
};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

use super::super::fingerprint::{agent_lint_repair_attestation_digest, canonical_json, hex_sha256};
use super::super::{WorkflowCoordinator, WorkflowRunner, WorkflowStageSink};
use super::update_wiki::{
    task_owned_candidate_decision_review, task_owned_candidate_file_diff_page,
    TaskOwnedCandidateReviewSource,
};

const CREATE_CHECKPOINT: &str = "create_checkpoint";
const FINALIZE_REPAIR: &str = "finalize_repair";
const REPAIR_DESCRIPTOR: &str = "agent-lint-repair-candidate.json";
const REPAIR_DESCRIPTOR_SCHEMA_VERSION: u32 = 1;
const MAX_REPAIR_ROUNDS: u8 = 3;

type StartCallback = dyn Fn(WorkflowRun) + Send + Sync;

pub struct AgentLintRepairRunner {
    start_callback: Arc<StartCallback>,
}

impl AgentLintRepairRunner {
    pub fn new(callback: impl Fn(WorkflowRun) + Send + Sync + 'static) -> Self {
        Self {
            start_callback: Arc::new(callback),
        }
    }
}

impl WorkflowRunner for AgentLintRepairRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::HealthCheck
    }

    fn operation_kind(&self) -> WorkflowOperationKind {
        WorkflowOperationKind::AgentLintRepair
    }

    fn start(&self, run: WorkflowRun) {
        (self.start_callback)(run);
    }
}

pub struct AgentLintRepairExecutionServices<'a> {
    pub agent_service: &'a AgentService,
    pub lint_service: &'a LintService,
    pub git_service: &'a GitService,
    pub file_store: &'a FileStore,
    pub bookmark_service: &'a BookmarkService,
    pub search_service: &'a SearchService,
    pub confirmation_registry: &'a ConfirmationRegistry,
    pub settings_service: &'a SettingsService,
    pub task_service: &'a TaskService,
    pub coordinator: &'a WorkflowCoordinator,
}

#[derive(Debug, Clone)]
pub struct AgentLintRepairRoundExecution {
    pub request: AgentLintRepairRequest,
    pub output: AgentLintRepairRoundOutput,
    pub manifest: CompileManifest,
    pub changes: CompileChangeSummary,
    pub baseline: HashMap<String, String>,
}

/// Execute one candidate-only repair round. The closure owns only the exact
/// task workspace and returns structured stdout; this helper never mutates the
/// project and always lets the active H2 lease remove its candidate root.
pub fn execute_agent_lint_repair_round_with<F>(
    context: &ProjectContext,
    task_id: &str,
    request: AgentLintRepairRequest,
    mut execute: F,
) -> Result<AgentLintRepairRoundExecution, BackendError>
where
    F: FnMut(&Path, &str) -> Result<String, BackendError>,
{
    let lease = LintService::create_repair_workspace(context, task_id, &request)?;
    let prompt = LintService::build_agent_lint_repair_prompt(&request)?;
    let raw = execute(lease.workspace(), &prompt)?;
    let output = LintService::parse_agent_lint_repair_round_output(&raw, &request)?;
    let candidate = LintService::validate_repair_workspace(context, &lease, &request, &output)?;
    Ok(AgentLintRepairRoundExecution {
        request,
        output,
        manifest: candidate.manifest,
        changes: candidate.changes,
        baseline: lease.baseline().clone(),
    })
}

pub fn run_agent_lint_repair_with_round_executor<A, F, G>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    language: &str,
    mut authorize_boundary: A,
    mut execute_round: F,
) -> Option<WorkflowRun>
where
    A: FnMut() -> Result<G, BackendError>,
    F: FnMut(&AgentLintRepairRequest, &Path, &str) -> Result<String, BackendError>,
{
    match execute_agent_lint_repair(
        context,
        &run,
        services,
        language,
        &mut authorize_boundary,
        &mut execute_round,
    ) {
        Ok(AgentLintRepairRunOutcome::Finished(next)) => next,
        Ok(AgentLintRepairRunOutcome::Waiting) => None,
        Err(error) => finish_repair_error(context, &run, services, error),
    }
}

pub fn run_agent_lint_repair_authorized<A>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    language: &str,
    is_default: bool,
    authorize_boundary: A,
) -> Option<WorkflowRun>
where
    A: FnMut() -> Result<WorkflowExternalLaunchPermit, BackendError>,
{
    let (agent, expected_route_revision) = match &run.route {
        Some(WorkflowRoute::Agent {
            agent,
            route_revision,
            ..
        }) => (*agent, route_revision.clone()),
        _ => {
            return finish_repair_error(
                context,
                &run,
                services,
                repair_error(
                    "LINT_AGENT_ROUTE_REQUIRED",
                    "Agent lint repair requires one exact Agent route.",
                    WorkflowProjectMutationState::NotModified,
                ),
            )
        }
    };
    let authority = RefCell::new(authorize_boundary);
    let execution_task_id = run.task_id.clone();
    run_agent_lint_repair_with_round_executor(
        context,
        run,
        services,
        language,
        || authority.borrow_mut()()?.begin(),
        |_, workspace, prompt| {
            let prepared = services
                .agent_service
                .prepare_lint_repair(agent, is_default, workspace, prompt)?;
            if prepared.route_revision() != expected_route_revision {
                return Err(repair_error(
                    "LINT_AGENT_ROUTE_CHANGED",
                    "The lint repair Agent route changed before launch.",
                    WorkflowProjectMutationState::NotModified,
                ));
            }
            let _publication = authority.borrow_mut()()?.begin()?;
            services.agent_service.run_prepared_lint_streaming(
                &prepared,
                services.task_service,
                &execution_task_id,
            )
        },
    )
}

pub struct AgentLintRepairConfirmationFailure {
    pub error: BackendError,
    pub next: Option<WorkflowRun>,
}

pub fn confirm_agent_lint_repair_review_with_round_executor<A, F, G>(
    context: &ProjectContext,
    task_id: &str,
    services: &AgentLintRepairExecutionServices<'_>,
    language: &str,
    mut authorize_boundary: A,
    mut execute_round: F,
) -> Result<(WorkflowRun, Option<WorkflowRun>), AgentLintRepairConfirmationFailure>
where
    A: FnMut() -> Result<G, BackendError>,
    F: FnMut(&AgentLintRepairRequest, &Path, &str) -> Result<String, BackendError>,
{
    let run = services
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| AgentLintRepairConfirmationFailure {
            error: repair_error(
                "TASK_NOT_FOUND",
                "Agent lint repair task not found.",
                WorkflowProjectMutationState::NotModified,
            ),
            next: None,
        })?;
    let (mut descriptor, _, _) = load_attested_descriptor(task_id, &run, services)
        .map_err(|error| AgentLintRepairConfirmationFailure { error, next: None })?;
    let (action, execution) = repair_review_action_and_execution(context, &run, &descriptor)
        .map_err(|error| AgentLintRepairConfirmationFailure { error, next: None })?;
    services
        .confirmation_registry
        .claim_exact_execution(&action.id, &execution)
        .map_err(|error| AgentLintRepairConfirmationFailure { error, next: None })?;
    let disposition = services
        .confirmation_registry
        .finish_claim_with_disposition(&action.id, true)
        .map_err(|error| AgentLintRepairConfirmationFailure { error, next: None })?;
    if disposition == ConfirmationClaimDisposition::CancelRequested {
        let result =
            match settle_repair_failure(context, &run, services, AgentLintRepairOutcome::Cancelled)
            {
                Ok(RepairFailureSettlement::Terminal(result)) => result,
                Ok(RepairFailureSettlement::Committed(_)) => {
                    let error = stale_candidate_error();
                    return Err(AgentLintRepairConfirmationFailure { error, next: None });
                }
                Err(error) => {
                    let next = finish_repair_error(context, &run, services, error.clone());
                    return Err(AgentLintRepairConfirmationFailure { error, next });
                }
            };
        let _ = discard_agent_lint_repair_candidate(task_id);
        let next = services
            .coordinator
            .finish_cancelled_and_claim_next_with_result(
                services.task_service,
                task_id,
                Some(result),
            )
            .ok()
            .and_then(|(_, next)| next);
        return Err(AgentLintRepairConfirmationFailure {
            error: repair_error(
                "LINT_REPAIR_CONFIRMATION_CANCELLED",
                "The repair review was cancelled before apply.",
                WorkflowProjectMutationState::NotModified,
            ),
            next,
        });
    }
    if let Err(message) = services
        .task_service
        .begin_confirmed_workflow_apply(task_id)
    {
        let error = task_error(message);
        let next = finish_repair_error(context, &run, services, error.clone());
        return Err(AgentLintRepairConfirmationFailure { error, next });
    }
    let round = descriptor
        .pending_round
        .as_ref()
        .map(|round| round.round)
        .unwrap_or_default();
    let review_stage = round_stage("review_risk", round);
    if let Err(message) = services
        .task_service
        .complete_workflow_stage(task_id, &review_stage)
    {
        let error = task_error(message);
        let next = finish_repair_error(context, &run, services, error.clone());
        return Err(AgentLintRepairConfirmationFailure { error, next });
    }
    let apply_stage = round_stage("apply_changes", round);
    let recheck_stage = round_stage("recheck_lint", round);
    let result = apply_pending_round(
        context,
        &run,
        services,
        &mut descriptor,
        &apply_stage,
        &recheck_stage,
        &mut authorize_boundary,
    )
    .and_then(|_| {
        let WorkflowOperation::AgentLintRepair {
            selected_findings,
            authorized_path_hashes,
            ..
        } = &run.operation
        else {
            unreachable!("descriptor loading validated the operation")
        };
        continue_agent_lint_repair_rounds(
            context,
            &run,
            services,
            language,
            selected_findings,
            authorized_path_hashes,
            &mut descriptor,
            &mut authorize_boundary,
            &mut execute_round,
        )
    });
    match result {
        Ok(AgentLintRepairRunOutcome::Finished(next)) => {
            let completed = services
                .task_service
                .get_workflow_run(task_id)
                .ok_or_else(|| AgentLintRepairConfirmationFailure {
                    error: repair_error(
                        "TASK_NOT_FOUND",
                        "Agent lint repair task disappeared after confirmation.",
                        WorkflowProjectMutationState::Unknown,
                    ),
                    next: next.clone(),
                })?;
            Ok((completed, next))
        }
        Ok(AgentLintRepairRunOutcome::Waiting) => {
            let waiting = services
                .task_service
                .get_workflow_run(task_id)
                .ok_or_else(|| AgentLintRepairConfirmationFailure {
                    error: repair_error(
                        "TASK_NOT_FOUND",
                        "Agent lint repair task disappeared while publishing the next review.",
                        WorkflowProjectMutationState::Unknown,
                    ),
                    next: None,
                })?;
            Ok((waiting, None))
        }
        Err(error) => {
            let next = finish_repair_error(context, &run, services, error.clone());
            Err(AgentLintRepairConfirmationFailure { error, next })
        }
    }
}

pub fn rollback_and_discard_agent_lint_repair_candidate(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
) -> Result<WorkflowResult, BackendError> {
    let result =
        match settle_repair_failure(context, run, services, AgentLintRepairOutcome::Cancelled)? {
            RepairFailureSettlement::Terminal(result) => result,
            RepairFailureSettlement::Committed(_) => return Err(stale_candidate_error()),
        };
    discard_agent_lint_repair_candidate(&run.task_id)?;
    Ok(result)
}

pub fn confirm_agent_lint_repair_review_authorized<A>(
    context: &ProjectContext,
    task_id: &str,
    services: &AgentLintRepairExecutionServices<'_>,
    language: &str,
    is_default: bool,
    authorize_boundary: A,
) -> Result<(WorkflowRun, Option<WorkflowRun>), AgentLintRepairConfirmationFailure>
where
    A: FnMut() -> Result<WorkflowExternalLaunchPermit, BackendError>,
{
    let run = services
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| AgentLintRepairConfirmationFailure {
            error: repair_error(
                "TASK_NOT_FOUND",
                "Agent lint repair task not found.",
                WorkflowProjectMutationState::NotModified,
            ),
            next: None,
        })?;
    let (agent, expected_route_revision) = match &run.route {
        Some(WorkflowRoute::Agent {
            agent,
            route_revision,
            ..
        }) => (*agent, route_revision.clone()),
        _ => {
            return Err(AgentLintRepairConfirmationFailure {
                error: repair_error(
                    "LINT_AGENT_ROUTE_REQUIRED",
                    "Agent lint repair requires one exact Agent route.",
                    WorkflowProjectMutationState::NotModified,
                ),
                next: None,
            })
        }
    };
    let authority = RefCell::new(authorize_boundary);
    let execution_task_id = run.task_id.clone();
    confirm_agent_lint_repair_review_with_round_executor(
        context,
        task_id,
        services,
        language,
        || authority.borrow_mut()()?.begin(),
        |_, workspace, prompt| {
            let prepared = services
                .agent_service
                .prepare_lint_repair(agent, is_default, workspace, prompt)?;
            if prepared.route_revision() != expected_route_revision {
                return Err(repair_error(
                    "LINT_AGENT_ROUTE_CHANGED",
                    "The lint repair Agent route changed before launch.",
                    WorkflowProjectMutationState::NotModified,
                ));
            }
            let _publication = authority.borrow_mut()()?.begin()?;
            services.agent_service.run_prepared_lint_streaming(
                &prepared,
                services.task_service,
                &execution_task_id,
            )
        },
    )
}

enum AgentLintRepairRunOutcome {
    Finished(Option<WorkflowRun>),
    Waiting,
}

fn execute_agent_lint_repair<A, F, G>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    language: &str,
    authorize_boundary: &mut A,
    execute_round: &mut F,
) -> Result<AgentLintRepairRunOutcome, BackendError>
where
    A: FnMut() -> Result<G, BackendError>,
    F: FnMut(&AgentLintRepairRequest, &Path, &str) -> Result<String, BackendError>,
{
    let WorkflowOperation::AgentLintRepair {
        report_id,
        selection_revision,
        selected_finding_ids,
        selected_findings,
        skill,
        authorized_path_hashes,
        expected_git_head,
        ..
    } = &run.operation
    else {
        return Err(repair_error(
            "LINT_REPAIR_OPERATION_INVALID",
            "The dispatched workflow is not an Agent lint repair operation.",
            WorkflowProjectMutationState::NotModified,
        ));
    };
    let route = run.route.clone().ok_or_else(|| {
        repair_error(
            "LINT_AGENT_ROUTE_REQUIRED",
            "Agent lint repair requires one exact Agent route.",
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    if !matches!(route, WorkflowRoute::Agent { .. }) || !skill.is_builtin() {
        return Err(repair_error(
            "LINT_AGENT_ROUTE_REQUIRED",
            "Agent lint repair requires the pinned built-in Skill and an Agent route.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);
    sink.start(CREATE_CHECKPOINT).map_err(task_error)?;
    let _authority = authorize_boundary()?;
    let allowed_task_state = task_state_noise_paths(&run.task_id);
    let checkpoint = with_agent_lint_git_cancellation(services, &run.task_id, || {
        services.git_service.clean_head_checkpoint_allowing_paths(
            context,
            CheckpointPurpose::HighRiskOperation,
            &format!("Before Agent lint repair {}", run.task_id),
            &allowed_task_state,
        )
    })
    .map_err(|error| {
        repair_error(
            &error.code,
            error.message,
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let checkpoint_hash = checkpoint.commit_hash.ok_or_else(|| {
        repair_error(
            "LINT_REPAIR_CHECKPOINT_REQUIRED",
            "Agent lint repair could not bind a Git checkpoint.",
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    if &checkpoint_hash != expected_git_head {
        return Err(repair_error(
            "LINT_REPAIR_CHECKPOINT_STALE",
            "Git HEAD changed before the repair checkpoint was captured.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let baseline_hashes = CompileService::snapshot_wiki(context)?;
    let mut descriptor = PersistedAgentLintRepairCandidate {
        schema_version: REPAIR_DESCRIPTOR_SCHEMA_VERSION,
        task_id: run.task_id.clone(),
        operation: run.operation.clone(),
        canonical_identity_key: run.canonical_identity_key.clone(),
        identity_revision: run.identity_revision.clone(),
        route,
        skill: skill.clone(),
        report_id: report_id.clone(),
        selection_revision: selection_revision.clone(),
        checkpoint_hash,
        baseline_fingerprint: run.baseline_fingerprint.clone(),
        completed_round: 0,
        selected_finding_ids: selected_finding_ids.clone(),
        selected_findings: selected_findings.clone(),
        authorized_path_hashes: authorized_path_hashes.clone(),
        unresolved_finding_ids: selected_finding_ids.clone(),
        resolved_finding_ids: Vec::new(),
        introduced_finding_ids: Vec::new(),
        skipped_finding_ids: Vec::new(),
        rounds: Vec::new(),
        affected_paths: Vec::new(),
        baseline_hashes,
        current_hashes: HashMap::new(),
        accumulated_manifest: CompileManifest {
            files: Vec::new(),
            deletions: Vec::new(),
            summary: "Agent lint repair".into(),
        },
        pending_round: None,
        index_refresh_warnings: Vec::new(),
        terminal_affected_path_hashes: BTreeMap::new(),
        final_commit: None,
    };
    bind_persisted_descriptor(services, run, &descriptor, None)?;
    sink.complete(CREATE_CHECKPOINT).map_err(task_error)?;

    continue_agent_lint_repair_rounds(
        context,
        run,
        services,
        language,
        selected_findings,
        authorized_path_hashes,
        &mut descriptor,
        authorize_boundary,
        execute_round,
    )
}

#[allow(clippy::too_many_arguments)]
fn continue_agent_lint_repair_rounds<A, F, G>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    language: &str,
    selected_findings: &[AgentLintRepairFinding],
    authorized_path_hashes: &BTreeMap<String, Option<String>>,
    descriptor: &mut PersistedAgentLintRepairCandidate,
    authorize_boundary: &mut A,
    execute_round: &mut F,
) -> Result<AgentLintRepairRunOutcome, BackendError>
where
    A: FnMut() -> Result<G, BackendError>,
    F: FnMut(&AgentLintRepairRequest, &Path, &str) -> Result<String, BackendError>,
{
    for round in descriptor.completed_round.saturating_add(1)..=MAX_REPAIR_ROUNDS {
        if descriptor.unresolved_finding_ids.is_empty() {
            skip_rounds(
                services.task_service,
                &run.task_id,
                round,
                MAX_REPAIR_ROUNDS,
            )?;
            break;
        }
        // A previously applied round is part of the repair-owned baseline for
        // every later launch. Revalidate it before invoking the Agent so an
        // external edit can never be absorbed into a later candidate/commit.
        ensure_repair_head_and_paths(context, run, descriptor, services)?;
        let prepare_stage = round_stage("prepare_round", round);
        let agent_stage = round_stage("run_agent", round);
        let validate_stage = round_stage("validate_candidate", round);
        let review_stage = round_stage("review_risk", round);
        let apply_stage = round_stage("apply_changes", round);
        let recheck_stage = round_stage("recheck_lint", round);
        let sink =
            WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);

        sink.start(&prepare_stage).map_err(task_error)?;
        let findings = selected_findings
            .iter()
            .filter(|finding| descriptor.unresolved_finding_ids.contains(&finding.id))
            .cloned()
            .collect::<Vec<_>>();
        let request = build_round_request(
            context,
            descriptor,
            findings,
            authorized_path_hashes,
            round,
            language,
            services.file_store,
        )?;
        sink.complete(&prepare_stage).map_err(task_error)?;

        sink.start(&agent_stage).map_err(task_error)?;
        let round_task_id = format!("{}-round-{round}", run.task_id);
        let request_for_executor = request.clone();
        let execution = execute_agent_lint_repair_round_with(
            context,
            &round_task_id,
            request,
            |workspace, prompt| execute_round(&request_for_executor, workspace, prompt),
        );
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => return Err(error),
        };
        sink.complete(&agent_stage).map_err(task_error)?;

        sink.start(&validate_stage).map_err(task_error)?;
        let current_hashes =
            current_manifest_hashes(context, &execution.manifest, services.file_store)?;
        let manifest_hash = manifest_revision(&execution.manifest)?;
        let candidate_id = format!("{}:{round}:{manifest_hash}", run.task_id);
        let candidate_revision = canonical_json(&(
            &run.task_id,
            round,
            &execution.output,
            &execution.manifest,
            &execution.baseline,
            &current_hashes,
            &descriptor.checkpoint_hash,
        ))
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|error| {
            repair_error(
                "LINT_REPAIR_CANDIDATE_BINDING_FAILED",
                error,
                WorkflowProjectMutationState::NotModified,
            )
        })?;
        let action_revision = canonical_json(&(
            &candidate_id,
            &candidate_revision,
            &manifest_hash,
            &run.fingerprint,
            &descriptor.checkpoint_hash,
        ))
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|error| {
            repair_error(
                "LINT_REPAIR_CANDIDATE_BINDING_FAILED",
                error,
                WorkflowProjectMutationState::NotModified,
            )
        })?;
        descriptor.pending_round = Some(PersistedAgentLintRepairRound {
            round,
            output: execution.output,
            manifest: execution.manifest,
            changes: execution.changes,
            baseline_hashes: execution.baseline,
            current_hashes,
            candidate_id,
            candidate_revision,
            manifest_revision: manifest_hash,
            action_revision,
        });
        advance_persisted_descriptor(services, run, descriptor)?;
        sink.complete(&validate_stage).map_err(task_error)?;

        if descriptor
            .pending_round
            .as_ref()
            .is_some_and(|candidate| candidate.changes.requires_confirmation())
        {
            persist_repair_review(context, run, descriptor, services, &review_stage)?;
            return Ok(AgentLintRepairRunOutcome::Waiting);
        }
        sink.skip(&review_stage).map_err(task_error)?;
        apply_pending_round(
            context,
            run,
            services,
            descriptor,
            &apply_stage,
            &recheck_stage,
            authorize_boundary,
        )?;
    }
    finalize_repair(context, run, services, descriptor, authorize_boundary)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedAgentLintRepairCandidate {
    schema_version: u32,
    task_id: String,
    operation: WorkflowOperation,
    canonical_identity_key: String,
    identity_revision: String,
    route: WorkflowRoute,
    skill: WikiLintSkillRef,
    report_id: String,
    selection_revision: String,
    checkpoint_hash: String,
    baseline_fingerprint: String,
    completed_round: u8,
    selected_finding_ids: Vec<String>,
    selected_findings: Vec<AgentLintRepairFinding>,
    authorized_path_hashes: BTreeMap<String, Option<String>>,
    unresolved_finding_ids: Vec<String>,
    resolved_finding_ids: Vec<String>,
    introduced_finding_ids: Vec<String>,
    skipped_finding_ids: Vec<String>,
    rounds: Vec<AgentLintRepairRoundSummary>,
    affected_paths: Vec<String>,
    baseline_hashes: HashMap<String, String>,
    current_hashes: HashMap<String, String>,
    accumulated_manifest: CompileManifest,
    pending_round: Option<PersistedAgentLintRepairRound>,
    index_refresh_warnings: Vec<String>,
    terminal_affected_path_hashes: BTreeMap<String, Option<String>>,
    final_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedAgentLintRepairRound {
    round: u8,
    output: AgentLintRepairRoundOutput,
    manifest: CompileManifest,
    changes: CompileChangeSummary,
    baseline_hashes: HashMap<String, String>,
    current_hashes: HashMap<String, String>,
    candidate_id: String,
    candidate_revision: String,
    manifest_revision: String,
    action_revision: String,
}

fn repair_workspace(task_id: &str) -> PathBuf {
    std::env::temp_dir().join("llm-wiki-desktop").join(task_id)
}

fn descriptor_path(task_id: &str) -> PathBuf {
    repair_workspace(task_id).join(REPAIR_DESCRIPTOR)
}

fn validate_candidate_root(task_id: &str, create: bool) -> Result<PathBuf, BackendError> {
    if uuid::Uuid::parse_str(task_id).is_err() {
        return Err(stale_candidate_error());
    }
    let root = std::env::temp_dir().join("llm-wiki-desktop");
    if create && !root.exists() {
        std::fs::create_dir(&root).map_err(|error| {
            repair_error(
                "LINT_REPAIR_CANDIDATE_PERSIST_FAILED",
                error.to_string(),
                WorkflowProjectMutationState::NotModified,
            )
        })?;
    }
    let root_metadata = std::fs::symlink_metadata(&root).map_err(|_| stale_candidate_error())?;
    if !root_metadata.is_dir() || crate::models::layout::is_link_or_reparse(&root_metadata) {
        return Err(stale_candidate_error());
    }
    let workspace = root.join(task_id);
    if create && !workspace.exists() {
        std::fs::create_dir(&workspace).map_err(|error| {
            repair_error(
                "LINT_REPAIR_CANDIDATE_PERSIST_FAILED",
                error.to_string(),
                WorkflowProjectMutationState::NotModified,
            )
        })?;
    }
    let metadata = std::fs::symlink_metadata(&workspace).map_err(|_| stale_candidate_error())?;
    if !metadata.is_dir() || crate::models::layout::is_link_or_reparse(&metadata) {
        return Err(stale_candidate_error());
    }
    let canonical_root = root.canonicalize().map_err(|_| stale_candidate_error())?;
    let canonical_workspace = workspace
        .canonicalize()
        .map_err(|_| stale_candidate_error())?;
    if canonical_workspace.parent() != Some(canonical_root.as_path()) {
        return Err(stale_candidate_error());
    }
    Ok(workspace)
}

fn persist_descriptor(descriptor: &PersistedAgentLintRepairCandidate) -> Result<(), BackendError> {
    let workspace = validate_candidate_root(&descriptor.task_id, true)?;
    let metadata = std::fs::symlink_metadata(&workspace).map_err(|error| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_PERSIST_FAILED",
            error.to_string(),
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    if !metadata.is_dir() || crate::models::layout::is_link_or_reparse(&metadata) {
        return Err(repair_error(
            "LINT_REPAIR_CANDIDATE_PERSIST_FAILED",
            "The repair candidate root is not a regular task-owned directory.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    FileStore.write_json_atomic_absolute(
        workspace.parent().ok_or_else(stale_candidate_error)?,
        &descriptor_path(&descriptor.task_id),
        descriptor,
    )
}

fn load_descriptor(
    task_id: &str,
    run: &WorkflowRun,
) -> Result<PersistedAgentLintRepairCandidate, BackendError> {
    let path = validate_candidate_root(task_id, false)?.join(REPAIR_DESCRIPTOR);
    let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_STALE",
            error.to_string(),
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    if !metadata.is_file() || crate::models::layout::is_link_or_reparse(&metadata) {
        return Err(repair_error(
            "LINT_REPAIR_CANDIDATE_STALE",
            "The persisted repair descriptor is not a regular file.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let descriptor = FileStore.read_json_file::<PersistedAgentLintRepairCandidate>(&path)?;
    let post_metadata = std::fs::symlink_metadata(&path).map_err(|_| stale_candidate_error())?;
    if !post_metadata.is_file() || crate::models::layout::is_link_or_reparse(&post_metadata) {
        return Err(stale_candidate_error());
    }
    if descriptor.schema_version != REPAIR_DESCRIPTOR_SCHEMA_VERSION
        || descriptor.task_id != task_id
        || descriptor.operation != run.operation
        || descriptor.canonical_identity_key != run.canonical_identity_key
        || descriptor.identity_revision != run.identity_revision
        || run.route.as_ref() != Some(&descriptor.route)
        || descriptor.baseline_fingerprint != run.baseline_fingerprint
    {
        return Err(repair_error(
            "LINT_REPAIR_CANDIDATE_STALE",
            "The persisted repair descriptor no longer belongs to this exact workflow.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let WorkflowOperation::AgentLintRepair {
        report_id,
        selection_revision,
        selected_finding_ids,
        selected_findings,
        skill,
        authorized_path_hashes,
        expected_git_head,
        ..
    } = &descriptor.operation
    else {
        return Err(stale_candidate_error());
    };
    let finding_ids = descriptor
        .selected_findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect::<Vec<_>>();
    if &descriptor.report_id != report_id
        || &descriptor.selection_revision != selection_revision
        || &descriptor.selected_finding_ids != selected_finding_ids
        || &descriptor.selected_findings != selected_findings
        || finding_ids != descriptor.selected_finding_ids
        || descriptor
            .selected_finding_ids
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != descriptor.selected_finding_ids.len()
        || &descriptor.skill != skill
        || &descriptor.authorized_path_hashes != authorized_path_hashes
        || &descriptor.checkpoint_hash != expected_git_head
        || descriptor.completed_round > MAX_REPAIR_ROUNDS
        || descriptor.rounds.len() != descriptor.completed_round as usize
        || descriptor
            .rounds
            .iter()
            .enumerate()
            .any(|(index, round)| round.round != (index + 1) as u8)
        || descriptor.pending_round.as_ref().is_some_and(|candidate| {
            candidate.round != descriptor.completed_round.saturating_add(1)
                || candidate.round > MAX_REPAIR_ROUNDS
        })
        || descriptor.final_commit.is_some() && descriptor.pending_round.is_some()
    {
        return Err(stale_candidate_error());
    }
    if let Some(candidate) = descriptor.pending_round.as_ref() {
        let manifest_hash = manifest_revision(&candidate.manifest)?;
        let expected_candidate_id = format!("{task_id}:{}:{manifest_hash}", candidate.round);
        let expected_candidate_revision = canonical_json(&(
            task_id,
            candidate.round,
            &candidate.output,
            &candidate.manifest,
            &candidate.baseline_hashes,
            &candidate.current_hashes,
            &descriptor.checkpoint_hash,
        ))
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|_| stale_candidate_error())?;
        let expected_action_revision = canonical_json(&(
            &expected_candidate_id,
            &expected_candidate_revision,
            &manifest_hash,
            &run.fingerprint,
            &descriptor.checkpoint_hash,
        ))
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|_| stale_candidate_error())?;
        if candidate.manifest_revision != manifest_hash
            || candidate.candidate_id != expected_candidate_id
            || candidate.candidate_revision != expected_candidate_revision
            || candidate.action_revision != expected_action_revision
        {
            return Err(stale_candidate_error());
        }
    }
    Ok(descriptor)
}

fn operation_digest(
    services: &AgentLintRepairExecutionServices<'_>,
    run: &WorkflowRun,
) -> Result<String, BackendError> {
    let options = services
        .task_service
        .workflow_execution_options(&run.task_id)
        .ok_or_else(|| stale_candidate_error())?;
    agent_lint_repair_attestation_digest(run, &options).map_err(|error| {
        repair_error(
            "LINT_REPAIR_BINDING_FAILED",
            error,
            WorkflowProjectMutationState::NotModified,
        )
    })
}

fn descriptor_digest(
    descriptor: &PersistedAgentLintRepairCandidate,
) -> Result<String, BackendError> {
    canonical_json(descriptor)
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|error| {
            repair_error(
                "LINT_REPAIR_CANDIDATE_BINDING_FAILED",
                error,
                WorkflowProjectMutationState::NotModified,
            )
        })
}

fn bind_persisted_descriptor(
    services: &AgentLintRepairExecutionServices<'_>,
    run: &WorkflowRun,
    descriptor: &PersistedAgentLintRepairCandidate,
    expected_digest: Option<&str>,
) -> Result<String, BackendError> {
    persist_descriptor(descriptor)?;
    let digest = descriptor_digest(descriptor)?;
    let operation_digest = operation_digest(services, run)?;
    services
        .settings_service
        .bind_agent_lint_repair_descriptor_digest(
            &run.task_id,
            &operation_digest,
            expected_digest,
            &digest,
        )?;
    Ok(digest)
}

fn load_attested_descriptor(
    task_id: &str,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
) -> Result<(PersistedAgentLintRepairCandidate, String, String), BackendError> {
    load_attested_descriptor_with(
        task_id,
        run,
        services.settings_service,
        services.task_service,
    )
}

fn load_attested_descriptor_with(
    task_id: &str,
    run: &WorkflowRun,
    settings_service: &SettingsService,
    task_service: &TaskService,
) -> Result<(PersistedAgentLintRepairCandidate, String, String), BackendError> {
    let descriptor = load_descriptor(task_id, run)?;
    let digest = descriptor_digest(&descriptor)?;
    let options = task_service
        .workflow_execution_options(&run.task_id)
        .ok_or_else(stale_candidate_error)?;
    let operation_digest =
        agent_lint_repair_attestation_digest(run, &options).map_err(|_| stale_candidate_error())?;
    let receipt = settings_service.get_agent_lint_repair_attestation(
        &run.canonical_identity_key,
        &run.identity_revision,
        task_id,
        &operation_digest,
    )?;
    if receipt.lifecycle != crate::models::settings::AgentLintRepairAttestationLifecycle::Dispatched
        || receipt.descriptor_digest.as_deref() != Some(digest.as_str())
    {
        return Err(stale_candidate_error());
    }
    Ok((descriptor, digest, operation_digest))
}

fn load_completed_attested_descriptor(
    task_id: &str,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
) -> Result<PersistedAgentLintRepairCandidate, BackendError> {
    let descriptor = load_descriptor(task_id, run)?;
    let descriptor_digest = descriptor_digest(&descriptor)?;
    let operation_digest = operation_digest(services, run)?;
    let receipt = services
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            task_id,
            &operation_digest,
        )?;
    let result = run.result.as_ref().ok_or_else(stale_candidate_error)?;
    if receipt.lifecycle != crate::models::settings::AgentLintRepairAttestationLifecycle::Completed
        || receipt.descriptor_digest.as_deref() != Some(descriptor_digest.as_str())
        || receipt.terminal_result_digest.as_deref()
            != Some(agent_lint_repair_result_digest(result)?.as_str())
    {
        return Err(stale_candidate_error());
    }
    Ok(descriptor)
}

fn persisted_descriptor_digest(
    services: &AgentLintRepairExecutionServices<'_>,
    run: &WorkflowRun,
) -> Result<(String, String), BackendError> {
    let operation_digest = operation_digest(services, run)?;
    let receipt = services
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &operation_digest,
        )?;
    let descriptor_digest = receipt
        .descriptor_digest
        .ok_or_else(stale_candidate_error)?;
    Ok((descriptor_digest, operation_digest))
}

fn advance_persisted_descriptor(
    services: &AgentLintRepairExecutionServices<'_>,
    run: &WorkflowRun,
    descriptor: &PersistedAgentLintRepairCandidate,
) -> Result<String, BackendError> {
    let (prior_digest, _) = persisted_descriptor_digest(services, run)?;
    bind_persisted_descriptor(services, run, descriptor, Some(&prior_digest))
}

/// Validate a restart-time waiting confirmation without recreating any
/// authority from the project-owned task JSON. The task-owned descriptor must
/// still bind every operation, route, baseline, candidate, and action revision
/// captured by the running workflow; this function only reads that descriptor
/// and the current project state.
pub(crate) fn agent_lint_repair_candidate_is_valid_for_workflow(
    task_id: &str,
    candidate_id: &str,
    project_root: &Path,
    workflow: &crate::models::workflow::WorkflowExecutionState,
) -> bool {
    if uuid::Uuid::parse_str(task_id).is_err() {
        return false;
    }
    let WorkflowOperation::AgentLintRepair {
        authorized_path_hashes,
        ..
    } = &workflow.execution_options.operation
    else {
        return false;
    };
    let Some(pending) = workflow.pending_action.as_ref() else {
        return false;
    };
    let Some(WorkflowCandidateReference::TaskOwned {
        candidate_id: pending_candidate_id,
    }) = pending.candidate.as_ref()
    else {
        return false;
    };
    if pending_candidate_id != candidate_id {
        return false;
    }

    let workspace = repair_workspace(task_id);
    let path = descriptor_path(task_id);
    let valid_workspace = std::fs::symlink_metadata(&workspace).is_ok_and(|metadata| {
        metadata.is_dir() && !crate::models::layout::is_link_or_reparse(&metadata)
    });
    let valid_file = std::fs::symlink_metadata(&path).is_ok_and(|metadata| {
        metadata.is_file() && !crate::models::layout::is_link_or_reparse(&metadata)
    });
    if !valid_workspace || !valid_file {
        return false;
    }
    let temp_owner = std::env::temp_dir().join("llm-wiki-desktop");
    let owned_workspace = temp_owner
        .canonicalize()
        .ok()
        .zip(workspace.canonicalize().ok())
        .is_some_and(|(owner, candidate)| candidate.parent() == Some(owner.as_path()));
    if !owned_workspace {
        return false;
    }

    let Ok(descriptor) = FileStore.read_json_file::<PersistedAgentLintRepairCandidate>(&path)
    else {
        return false;
    };
    let Some(candidate) = descriptor.pending_round.as_ref() else {
        return false;
    };
    if descriptor.schema_version != REPAIR_DESCRIPTOR_SCHEMA_VERSION
        || descriptor.task_id != task_id
        || descriptor.operation != workflow.execution_options.operation
        || descriptor.canonical_identity_key != workflow.canonical_identity_key
        || descriptor.identity_revision != workflow.identity_revision
        || Some(&descriptor.route) != workflow.route.as_ref()
        || descriptor.baseline_fingerprint != workflow.baseline_fingerprint
        || descriptor.checkpoint_hash != pending.checkpoint_hash.as_deref().unwrap_or_default()
        || candidate.candidate_id != candidate_id
        || candidate.round == 0
        || candidate.round > MAX_REPAIR_ROUNDS
        || !descriptor.skill.is_builtin()
        || CompileService::validate_lint_repair_manifest(&candidate.manifest).is_err()
    {
        return false;
    }

    let Ok(manifest_hash) = manifest_revision(&candidate.manifest) else {
        return false;
    };
    let expected_candidate_id = format!("{task_id}:{}:{manifest_hash}", candidate.round);
    let Ok(expected_candidate_revision) = canonical_json(&(
        task_id,
        candidate.round,
        &candidate.output,
        &candidate.manifest,
        &candidate.baseline_hashes,
        &candidate.current_hashes,
        &descriptor.checkpoint_hash,
    ))
    .map(|value| hex_sha256(value.as_bytes())) else {
        return false;
    };
    let Ok(expected_action_revision) = canonical_json(&(
        &expected_candidate_id,
        &expected_candidate_revision,
        &manifest_hash,
        &workflow.fingerprint,
        &descriptor.checkpoint_hash,
    ))
    .map(|value| hex_sha256(value.as_bytes())) else {
        return false;
    };
    let expected_action_id = format!("agent-lint-review-{expected_action_revision}");
    let expected_paths = candidate.changes.affected_paths();
    if candidate.manifest_revision != manifest_hash
        || candidate.candidate_id != expected_candidate_id
        || candidate.candidate_revision != expected_candidate_revision
        || candidate.action_revision != expected_action_revision
        || pending.id != expected_action_id
        || pending.action_type != PendingActionType::MergeConflict
        || pending.risk_level != RiskLevel::Destructive
        || pending.affected_paths != expected_paths
        || !candidate.changes.requires_confirmation()
    {
        return false;
    }

    let context = ProjectContext::new("repair-recovery-validation", project_root.to_path_buf());
    let Ok(current_hashes) = current_manifest_hashes(&context, &candidate.manifest, &FileStore)
    else {
        return false;
    };
    let preauthorized = authorized_path_hashes
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let Ok(changes) = CompileService::classify_lint_repair_changes(
        &context,
        &candidate.manifest,
        &candidate.baseline_hashes,
        &preauthorized,
    ) else {
        return false;
    };
    candidate.current_hashes == current_hashes && candidate.changes == changes
}

fn round_stage(prefix: &str, round: u8) -> String {
    format!("{prefix}_{round}")
}

fn skip_rounds(
    tasks: &TaskService,
    task_id: &str,
    first_round: u8,
    last_round: u8,
) -> Result<(), BackendError> {
    for round in first_round..=last_round {
        for prefix in [
            "prepare_round",
            "run_agent",
            "validate_candidate",
            "review_risk",
            "apply_changes",
            "recheck_lint",
        ] {
            tasks
                .skip_workflow_stage(task_id, &round_stage(prefix, round))
                .map_err(task_error)?;
        }
    }
    Ok(())
}

fn task_state_noise_paths(task_id: &str) -> Vec<String> {
    vec![
        format!(".app/tasks/{task_id}.json"),
        format!(".app/tasks/{task_id}.log"),
    ]
}

fn build_round_request(
    context: &ProjectContext,
    descriptor: &PersistedAgentLintRepairCandidate,
    findings: Vec<AgentLintRepairFinding>,
    authorized_path_hashes: &BTreeMap<String, Option<String>>,
    round: u8,
    language: &str,
    file_store: &FileStore,
) -> Result<AgentLintRepairRequest, BackendError> {
    let wiki_root = context.layout.wiki_write_root.clone().ok_or_else(|| {
        repair_error(
            "LINT_REPAIR_WIKI_ROOT_REQUIRED",
            "Agent lint repair requires a writable Wiki root.",
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let mut read_only_roots = vec!["raw".to_string(), "skills".to_string()];
    read_only_roots.extend(
        context
            .layout
            .markdown_roots
            .iter()
            .filter(|root| root.role == crate::models::layout::ProjectMarkdownRootRole::Source)
            .map(|root| root.path.clone()),
    );
    read_only_roots.sort();
    read_only_roots.dedup();
    let optional = |path: &str| -> Result<Option<String>, BackendError> {
        if file_store.exists(context, path) {
            file_store.read_markdown(context, path).map(Some)
        } else {
            Ok(None)
        }
    };
    let request = AgentLintRepairRequest {
        schema_version: WIKI_LINT_SCHEMA_VERSION,
        operation: crate::models::lint::AgentLintRepairOperation::Repair,
        skill: descriptor.skill.clone(),
        report_id: descriptor.report_id.clone(),
        selection_revision: descriptor.selection_revision.clone(),
        round,
        max_rounds: MAX_REPAIR_ROUNDS,
        findings,
        prior_rounds: descriptor.rounds.clone(),
        writable_paths: authorized_path_hashes.keys().cloned().collect(),
        creatable_roots: vec![wiki_root],
        read_only_roots,
        purpose: optional("purpose.md")?,
        schema: optional("schema.md")?,
        language: language.to_string(),
    };
    LintService::validate_agent_lint_repair_round_lineage(
        &request,
        &descriptor.selected_finding_ids,
    )?;
    Ok(request)
}

fn current_manifest_hashes(
    context: &ProjectContext,
    manifest: &CompileManifest,
    file_store: &FileStore,
) -> Result<HashMap<String, String>, BackendError> {
    let mut hashes = HashMap::new();
    for path in manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .chain(manifest.deletions.iter().map(String::as_str))
    {
        if let Some(hash) = file_store.file_hash_if_exists(context, path)? {
            hashes.insert(path.to_string(), hash);
        }
    }
    Ok(hashes)
}

fn persist_repair_review(
    context: &ProjectContext,
    run: &WorkflowRun,
    descriptor: &PersistedAgentLintRepairCandidate,
    services: &AgentLintRepairExecutionServices<'_>,
    review_stage: &str,
) -> Result<(), BackendError> {
    let candidate = descriptor
        .pending_round
        .as_ref()
        .ok_or_else(stale_candidate_error)?;
    let (action, execution) = repair_review_action_and_execution(context, run, descriptor)?;
    let action_id = action.id.clone();
    services
        .confirmation_registry
        .register_idempotent_with_execution(action, execution.clone())?;
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);
    sink.start(review_stage).map_err(task_error)?;
    let waiting = WorkflowPendingAction {
        id: action_id,
        action_type: PendingActionType::MergeConflict,
        risk_level: RiskLevel::Destructive,
        affected_paths: candidate.changes.affected_paths(),
        candidate: Some(WorkflowCandidateReference::TaskOwned {
            candidate_id: candidate.candidate_id.clone(),
        }),
        expires_at: None,
        checkpoint_hash: Some(descriptor.checkpoint_hash.clone()),
    };
    if let Err(message) = sink.wait(review_stage, waiting) {
        let _ = services.confirmation_registry.remove_exact_execution(
            match &execution {
                ConfirmationExecution::AgentLintRepairReview(binding) => &binding.action_id,
                _ => unreachable!(),
            },
            &execution,
        );
        return Err(task_error(message));
    }
    Ok(())
}

fn stale_candidate_error() -> BackendError {
    repair_error(
        "LINT_REPAIR_CANDIDATE_STALE",
        "The dangerous repair candidate disappeared before review.",
        WorkflowProjectMutationState::NotModified,
    )
}

fn repair_review_action_and_execution(
    context: &ProjectContext,
    run: &WorkflowRun,
    descriptor: &PersistedAgentLintRepairCandidate,
) -> Result<(PendingAction, ConfirmationExecution), BackendError> {
    let candidate = descriptor
        .pending_round
        .as_ref()
        .ok_or_else(stale_candidate_error)?;
    let action_id = format!("agent-lint-review-{}", candidate.action_revision);
    let execution = ConfirmationExecution::AgentLintRepairReview(AgentLintRepairReviewBinding {
        project_id: context.project_id.clone(),
        root_path: context.root.to_string_lossy().into_owned(),
        canonical_identity_key: run.canonical_identity_key.clone(),
        identity_revision: run.identity_revision.clone(),
        task_id: run.task_id.clone(),
        round: candidate.round,
        candidate_id: candidate.candidate_id.clone(),
        candidate_revision: candidate.candidate_revision.clone(),
        manifest_revision: candidate.manifest_revision.clone(),
        baseline_fingerprint: descriptor.baseline_fingerprint.clone(),
        checkpoint_hash: descriptor.checkpoint_hash.clone(),
        action_id: action_id.clone(),
        action_revision: candidate.action_revision.clone(),
    });
    let affected_paths = candidate.changes.affected_paths();
    let action = PendingAction {
        id: action_id,
        action_type: PendingActionType::MergeConflict,
        title: "Review Agent lint repair changes".into(),
        message: "This repair candidate deletes, overwrites, or conflicts with existing Wiki content and requires a second confirmation.".into(),
        risk_level: RiskLevel::Destructive,
        affected_paths: affected_paths.clone(),
        preview: Some(ActionPreview {
            summary: format!("{} path(s) require review", affected_paths.len()),
            before: None,
            after: None,
            diff: None,
        }),
        expires_at: None,
        checkpoint_hash: Some(descriptor.checkpoint_hash.clone()),
    };
    Ok((action, execution))
}

pub fn restore_agent_lint_repair_confirmation(
    context: &ProjectContext,
    run: &WorkflowRun,
    registry: &ConfirmationRegistry,
    settings_service: &SettingsService,
    task_service: &TaskService,
) -> Result<(), BackendError> {
    let pending = run.pending_action.as_ref().ok_or_else(|| {
        repair_error(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Agent lint repair has no pending confirmation to restore.",
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let (descriptor, _, _) =
        load_attested_descriptor_with(&run.task_id, run, settings_service, task_service)?;
    let (mut action, execution) = repair_review_action_and_execution(context, run, &descriptor)?;
    if action.id != pending.id
        || action.action_type != pending.action_type
        || action.risk_level != pending.risk_level
        || action.affected_paths != pending.affected_paths
        || action.checkpoint_hash != pending.checkpoint_hash
    {
        return Err(repair_error(
            "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
            "The persisted repair review no longer matches its exact candidate binding.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    action.expires_at = pending.expires_at.clone();
    registry.restore_with_execution(action, execution)
}

pub fn cancel_agent_lint_repair_review(
    context: &ProjectContext,
    run: &WorkflowRun,
    registry: &ConfirmationRegistry,
) -> Result<bool, BackendError> {
    let descriptor = load_descriptor(&run.task_id, run)?;
    let (action, execution) = repair_review_action_and_execution(context, run, &descriptor)?;
    match registry.cancel_exact_execution(&action.id, &execution) {
        Ok(_) => Ok(true),
        Err(error) if error.code == "CONFIRMATION_IN_USE" => Err(error),
        Err(error) if error.code == "CONFIRMATION_NOT_FOUND" => Ok(false),
        Err(error) => Err(error),
    }
}

fn apply_pending_round<A, G>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    descriptor: &mut PersistedAgentLintRepairCandidate,
    apply_stage: &str,
    recheck_stage: &str,
    authorize_boundary: &mut A,
) -> Result<(), BackendError>
where
    A: FnMut() -> Result<G, BackendError>,
{
    let candidate = descriptor.pending_round.clone().ok_or_else(|| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_STALE",
            "The repair candidate disappeared before apply.",
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);
    sink.start(apply_stage).map_err(task_error)?;
    let _authority = authorize_boundary()?;
    ensure_repair_head_and_paths(context, run, descriptor, services)?;
    if current_manifest_hashes(context, &candidate.manifest, services.file_store)?
        != candidate.current_hashes
    {
        return Err(repair_error(
            "LINT_REPAIR_CANDIDATE_STALE",
            "Wiki files changed after the repair candidate was validated.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let before_lint = services
        .lint_service
        .run_local_lint(context, services.search_service)?;
    let (descriptor_digest, operation_digest) = persisted_descriptor_digest(services, run)?;
    let mut expected_post_hashes = descriptor
        .accumulated_manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), Some(hex_sha256(file.content.as_bytes()))))
        .collect::<BTreeMap<_, _>>();
    for deletion in &descriptor.accumulated_manifest.deletions {
        expected_post_hashes.insert(deletion.clone(), None);
    }
    for file in &candidate.manifest.files {
        expected_post_hashes.insert(file.path.clone(), Some(hex_sha256(file.content.as_bytes())));
    }
    for deletion in &candidate.manifest.deletions {
        expected_post_hashes.insert(deletion.clone(), None);
    }
    let pre_round_hashes = expected_post_hashes
        .keys()
        .map(|path| {
            services
                .file_store
                .file_hash_if_exists(context, path)
                .map(|hash| (path.clone(), hash))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    services
        .settings_service
        .begin_agent_lint_repair_mutation_journal_with_pre_hashes(
            &run.task_id,
            &operation_digest,
            &descriptor_digest,
            &descriptor.checkpoint_hash,
            pre_round_hashes.clone(),
            expected_post_hashes.clone(),
        )?;
    services
        .task_service
        .set_task_cancellable(&run.task_id, false)
        .map_err(task_error)?;
    let applied = match CompileService::apply_confirmed_lint_repair_manifest(
        context,
        &candidate.manifest,
        &candidate.current_hashes,
    ) {
        Ok(applied) => applied,
        Err(error) => {
            let _ = services
                .task_service
                .set_task_cancellable(&run.task_id, true);
            let current_hashes = expected_post_hashes
                .keys()
                .map(|path| {
                    services
                        .file_store
                        .file_hash_if_exists(context, path)
                        .map(|hash| (path.clone(), hash))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            verified_partial_apply_hashes(
                &pre_round_hashes,
                &expected_post_hashes,
                &current_hashes,
            )?;
            return Err(error);
        }
    };
    services
        .task_service
        .set_task_cancellable(&run.task_id, true)
        .map_err(task_error)?;
    merge_accumulated_manifest(&mut descriptor.accumulated_manifest, &candidate.manifest);
    descriptor.affected_paths.extend(applied.iter().cloned());
    descriptor.affected_paths.sort();
    descriptor.affected_paths.dedup();
    descriptor.current_hashes = current_manifest_hashes(
        context,
        &descriptor.accumulated_manifest,
        services.file_store,
    )?;
    // Persist the applied-path journal before any recheck/task progress work.
    // Runtime failures after this barrier can reload the descriptor and roll
    // the entire batch back. The app-owned pre-apply journal covers a process
    // crash before this descriptor barrier.
    advance_persisted_descriptor(services, run, descriptor)?;
    sink.progress(
        apply_stage,
        applied.first().cloned(),
        applied.len() as u64,
        Some(applied.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(apply_stage).map_err(task_error)?;

    sink.start(recheck_stage).map_err(task_error)?;
    let after_lint = services
        .lint_service
        .run_local_lint(context, services.search_service)?;
    let mut before_ids = before_lint
        .issues
        .iter()
        .map(|issue| issue.id.clone())
        .collect::<HashSet<_>>();
    before_ids.extend(
        candidate
            .output
            .finding_results
            .iter()
            .map(|result| result.finding_id.clone()),
    );
    let after_ids = after_lint
        .issues
        .iter()
        .map(|issue| issue.id.clone())
        .collect::<HashSet<_>>();
    let recheck_request =
        build_recheck_request(context, descriptor, &candidate, services.file_store)?;
    before_ids.extend(
        recheck_request
            .findings
            .iter()
            .map(|finding| finding.id.clone()),
    );
    // Only backend deterministic rules may make a selected identity disappear.
    // Semantic Agent-only findings have no deterministic oracle in rules.rs, so
    // conservatively keep them unresolved. Neither a model's `attempted` claim
    // nor a changed page hash is evidence that the original issue is gone.
    let deterministic_before = before_lint
        .issues
        .iter()
        .map(|issue| issue.id.as_str())
        .collect::<HashSet<_>>();
    let mut after_ids = after_ids;
    for finding in &recheck_request.findings {
        if !deterministic_before.contains(finding.id.as_str()) {
            after_ids.insert(finding.id.clone());
        }
    }
    let assessment = LintService::correlate_agent_lint_repair_recheck(
        &recheck_request,
        &candidate.output,
        &descriptor.selected_finding_ids,
        &before_ids,
        &after_ids,
    )?;
    merge_correlation(descriptor, &assessment.correlation);
    descriptor.unresolved_finding_ids = assessment.correlation.unresolved_finding_ids.clone();
    descriptor.completed_round = candidate.round;
    descriptor.rounds.push(AgentLintRepairRoundSummary {
        round: candidate.round,
        affected_paths: applied,
        unresolved_finding_ids: assessment.correlation.unresolved_finding_ids,
        summary: candidate.output.summary.clone(),
    });
    descriptor.pending_round = None;
    advance_persisted_descriptor(services, run, descriptor)?;
    sink.complete(recheck_stage).map_err(task_error)?;
    Ok(())
}

fn build_recheck_request(
    context: &ProjectContext,
    descriptor: &PersistedAgentLintRepairCandidate,
    candidate: &PersistedAgentLintRepairRound,
    file_store: &FileStore,
) -> Result<AgentLintRepairRequest, BackendError> {
    let previous = descriptor.rounds.clone();
    let prior_unresolved = if candidate.round == 1 {
        descriptor.selected_finding_ids.clone()
    } else {
        previous
            .last()
            .map(|round| round.unresolved_finding_ids.clone())
            .unwrap_or_default()
    };
    let findings = descriptor
        .selected_findings
        .iter()
        .filter(|finding| prior_unresolved.contains(&finding.id))
        .cloned()
        .collect();
    build_round_request(
        context,
        descriptor,
        findings,
        &descriptor.authorized_path_hashes,
        candidate.round,
        "en",
        file_store,
    )
}

fn merge_correlation(
    descriptor: &mut PersistedAgentLintRepairCandidate,
    correlation: &AgentLintRepairCorrelation,
) {
    descriptor
        .resolved_finding_ids
        .extend(correlation.resolved_finding_ids.iter().cloned());
    descriptor
        .introduced_finding_ids
        .extend(correlation.introduced_finding_ids.iter().cloned());
    descriptor
        .skipped_finding_ids
        .extend(correlation.skipped_finding_ids.iter().cloned());
    for values in [
        &mut descriptor.resolved_finding_ids,
        &mut descriptor.introduced_finding_ids,
        &mut descriptor.skipped_finding_ids,
    ] {
        values.sort();
        values.dedup();
    }
}

fn merge_accumulated_manifest(target: &mut CompileManifest, round: &CompileManifest) {
    let mut files = target
        .files
        .drain(..)
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();
    let mut deletions = target.deletions.drain(..).collect::<HashSet<_>>();
    for file in &round.files {
        deletions.remove(&file.path);
        files.insert(file.path.clone(), file.clone());
    }
    for deletion in &round.deletions {
        files.remove(deletion);
        deletions.insert(deletion.clone());
    }
    target.files = files.into_values().collect();
    target.deletions = deletions.into_iter().collect();
    target.deletions.sort();
    target.summary = round.summary.clone();
}

fn ensure_repair_head_and_paths(
    context: &ProjectContext,
    run: &WorkflowRun,
    descriptor: &PersistedAgentLintRepairCandidate,
    services: &AgentLintRepairExecutionServices<'_>,
) -> Result<(), BackendError> {
    let status = with_agent_lint_git_cancellation(services, &run.task_id, || {
        services.git_service.repository_status(context)
    })?;
    if status.head.as_deref() != Some(descriptor.checkpoint_hash.as_str()) {
        return Err(repair_error(
            "LINT_REPAIR_CHECKPOINT_STALE",
            "Git HEAD changed during Agent lint repair.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let deleted = descriptor
        .accumulated_manifest
        .deletions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    for path in &descriptor.affected_paths {
        let expected = if deleted.contains(path.as_str()) {
            None
        } else {
            descriptor.current_hashes.get(path).cloned()
        };
        if services.file_store.file_hash_if_exists(context, path)? != expected {
            return Err(repair_error(
                "LINT_REPAIR_BATCH_STALE",
                "A previously applied repair path changed outside this batch.",
                WorkflowProjectMutationState::Modified,
            ));
        }
    }
    let mut allowed = task_state_noise_paths(&run.task_id);
    allowed.extend(descriptor.affected_paths.iter().cloned());
    let changed = with_agent_lint_git_cancellation(services, &run.task_id, || {
        services.git_service.changed_paths(context)
    })?;
    if changed.iter().any(|path| !allowed.contains(path)) {
        return Err(repair_error(
            "LINT_REPAIR_GIT_STATE_CHANGED",
            "Project content outside the approved repair batch changed.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    Ok(())
}

fn finalize_repair<A, G>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    descriptor: &mut PersistedAgentLintRepairCandidate,
    authorize_boundary: &mut A,
) -> Result<AgentLintRepairRunOutcome, BackendError>
where
    A: FnMut() -> Result<G, BackendError>,
{
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);
    sink.start(FINALIZE_REPAIR).map_err(task_error)?;
    let _authority = authorize_boundary()?;
    ensure_repair_head_and_paths(context, run, descriptor, services)?;
    let graph_cache_path = super::update_wiki::workflow_graph_cache_relative_path(context);
    let mut commit_paths = descriptor.affected_paths.clone();
    commit_paths.push(graph_cache_path.clone());
    commit_paths.sort();
    commit_paths.dedup();
    let (descriptor_digest, operation_digest) = persisted_descriptor_digest(services, run)?;
    let deleted = descriptor
        .accumulated_manifest
        .deletions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let graph_value =
        super::update_wiki::workflow_stale_graph_cache_value(context, services.file_store);
    let graph_bytes = serde_json::to_string_pretty(&graph_value).map_err(|error| {
        repair_error(
            "LINT_REPAIR_INDEX_BINDING_FAILED",
            error.to_string(),
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let affected_path_hashes = commit_paths
        .iter()
        .map(|path| {
            let hash = if path == &graph_cache_path {
                Some(hex_sha256(graph_bytes.as_bytes()))
            } else if deleted.contains(path.as_str()) {
                None
            } else {
                descriptor.current_hashes.get(path).cloned()
            };
            (path.clone(), hash)
        })
        .collect::<BTreeMap<_, _>>();
    let pre_finalization_hashes = commit_paths
        .iter()
        .map(|path| {
            services
                .file_store
                .file_hash_if_exists(context, path)
                .map(|hash| (path.clone(), hash))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    services
        .settings_service
        .begin_agent_lint_repair_mutation_journal_with_pre_hashes(
            &run.task_id,
            &operation_digest,
            &descriptor_digest,
            &descriptor.checkpoint_hash,
            pre_finalization_hashes,
            affected_path_hashes.clone(),
        )?;
    services
        .settings_service
        .mark_agent_lint_repair_mutation_finalizing(
            &run.task_id,
            &operation_digest,
            &descriptor_digest,
            &descriptor.checkpoint_hash,
        )?;
    descriptor.index_refresh_warnings = super::update_wiki::refresh_workflow_wiki_indexes(
        context,
        &run.task_id,
        services.file_store,
        services.bookmark_service,
        services.search_service,
        services.task_service,
    )?;
    for (path, expected) in &affected_path_hashes {
        if services.file_store.file_hash_if_exists(context, path)? != *expected {
            return Err(repair_error(
                "LINT_REPAIR_INDEX_REFRESH_UNVERIFIED",
                "The repair index refresh did not match its durable finalization journal.",
                WorkflowProjectMutationState::Modified,
            ));
        }
    }
    let final_checkpoint = match with_agent_lint_git_cancellation(services, &run.task_id, || {
        services.git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::FinalResult,
            &format!("Agent lint repair {}", run.task_id),
            &commit_paths,
        )
    }) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            let graph_rollback = services
                .git_service
                .rollback_paths_to_head_preserving_ignored(context, &[graph_cache_path], &[]);
            if let Err(rollback) = graph_rollback {
                return Err(repair_error(
                    "LINT_REPAIR_ROLLBACK_FAILED",
                    format!(
                        "Final commit failed and generated-index rollback was incomplete: commit={}; rollback={}",
                        error.message, rollback.message
                    ),
                    WorkflowProjectMutationState::Modified,
                ));
            }
            return Err(error);
        }
    };
    let final_commit =
        final_checkpoint
            .created
            .then_some(final_checkpoint.commit_hash.clone().ok_or_else(|| {
                repair_error(
                    "LINT_REPAIR_FINAL_COMMIT_FAILED",
                    "The repair final commit has no commit hash.",
                    WorkflowProjectMutationState::Modified,
                )
            })?);
    descriptor.final_commit = final_commit;
    descriptor.current_hashes = current_manifest_hashes(
        context,
        &descriptor.accumulated_manifest,
        services.file_store,
    )?;
    descriptor.terminal_affected_path_hashes = affected_path_hashes.clone();
    let next_descriptor_digest = advance_persisted_descriptor(services, run, descriptor)?;
    if next_descriptor_digest != descriptor_digest {
        // The descriptor now binds the final commit. Advance the receipt to the
        // new canonical descriptor before terminal publication.
        services
            .settings_service
            .mark_agent_lint_repair_mutation_finalizing(
                &run.task_id,
                &operation_digest,
                &next_descriptor_digest,
                &descriptor.checkpoint_hash,
            )?;
    }
    if let Some(final_commit) = descriptor.final_commit.as_deref() {
        services
            .settings_service
            .mark_agent_lint_repair_final_commit(
                &run.task_id,
                &operation_digest,
                &next_descriptor_digest,
                &descriptor.checkpoint_hash,
                final_commit,
            )?;
    } else {
        // A scoped checkpoint can truthfully report a no-op. The runner has
        // already verified every journal path against the predicted hashes,
        // so retire the finalizing WAL before publishing a terminal no-op.
        services
            .settings_service
            .clear_agent_lint_repair_mutation_journal(
                &run.task_id,
                &operation_digest,
                &next_descriptor_digest,
                &descriptor.checkpoint_hash,
            )?;
    }
    let result = completed_repair_result(descriptor);
    let terminal_result_digest = agent_lint_repair_result_digest(&result)?;
    let terminal_result_json = agent_lint_repair_result_json(&result)?;
    services
        .settings_service
        .complete_agent_lint_repair_success_attestation(
            &run.task_id,
            &operation_digest,
            Some(&next_descriptor_digest),
            &terminal_result_digest,
            &terminal_result_json,
        )?;
    sink.complete(FINALIZE_REPAIR).map_err(task_error)?;
    let (_, next) = sink.finish(result).map_err(task_error)?;
    Ok(AgentLintRepairRunOutcome::Finished(next))
}

fn verified_partial_apply_hashes(
    pre_round: &BTreeMap<String, Option<String>>,
    intended_post: &BTreeMap<String, Option<String>>,
    current: &BTreeMap<String, Option<String>>,
) -> Result<BTreeMap<String, Option<String>>, BackendError> {
    if pre_round.keys().ne(intended_post.keys()) || current.keys().ne(intended_post.keys()) {
        return Err(repair_error(
            "LINT_REPAIR_ROLLBACK_CONFLICT",
            "The partial apply journal path set changed unexpectedly.",
            WorkflowProjectMutationState::Modified,
        ));
    }
    for (path, current_hash) in current {
        let before = &pre_round[path];
        let after = &intended_post[path];
        if current_hash != before && current_hash != after {
            return Err(repair_error(
                "LINT_REPAIR_ROLLBACK_CONFLICT",
                format!("A repair path changed outside the checked apply: {path}"),
                WorkflowProjectMutationState::Modified,
            ));
        }
    }
    Ok(current.clone())
}

enum RepairFailureSettlement {
    Committed(WorkflowResult),
    Terminal(WorkflowResult),
}

fn rollback_attested_repair_journal(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    journal: &crate::models::settings::AgentLintRepairMutationJournal,
) -> Result<Vec<String>, BackendError> {
    let mut rollback_paths = journal
        .affected_path_hashes
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    rollback_paths.sort();
    rollback_paths.dedup();
    let allowed_noise = task_state_noise_paths(&run.task_id);
    let changed = services.git_service.changed_paths(context)?;
    if changed
        .iter()
        .any(|path| !rollback_paths.contains(path) && !allowed_noise.contains(path))
    {
        return Err(repair_error(
            "LINT_REPAIR_ROLLBACK_CONFLICT",
            "Project files outside the durable repair journal changed before rollback.",
            WorkflowProjectMutationState::Modified,
        ));
    }
    let current_head = services
        .git_service
        .repository_status(context)?
        .head
        .ok_or_else(|| {
            repair_error(
                "LINT_REPAIR_CHECKPOINT_STALE",
                "The repair checkpoint is unavailable during rollback.",
                WorkflowProjectMutationState::Modified,
            )
        })?;
    if let Some(final_commit) = journal.final_commit.as_deref() {
        if services.git_service.is_exact_compensating_rollback(
            context,
            final_commit,
            &journal.checkpoint_hash,
            &rollback_paths,
        )? {
            return Ok(rollback_paths);
        }
    }
    for path in changed.iter().filter(|path| rollback_paths.contains(path)) {
        let current = services.file_store.file_hash_if_exists(context, path)?;
        if !journal_allows_uncommitted_hash(journal, path, &current) {
            return Err(repair_error(
                "LINT_REPAIR_ROLLBACK_CONFLICT",
                "A repair journal path changed outside the repair before rollback.",
                WorkflowProjectMutationState::Modified,
            ));
        }
    }
    if current_head == journal.checkpoint_hash {
        services
            .git_service
            .rollback_paths_to_head_preserving_ignored(context, &rollback_paths, &allowed_noise)?;
    } else {
        for (path, expected) in &journal.affected_path_hashes {
            if services.file_store.file_hash_if_exists(context, path)? != *expected {
                return Err(repair_error(
                    "LINT_REPAIR_ROLLBACK_CONFLICT",
                    "A committed repair path changed before compensating rollback.",
                    WorkflowProjectMutationState::Modified,
                ));
            }
        }
        services.git_service.rollback_paths_to_checkpoint(
            context,
            &current_head,
            &journal.checkpoint_hash,
            &format!("Roll back Agent lint repair {}", run.task_id),
            &rollback_paths,
        )?;
    }
    Ok(rollback_paths)
}

fn journal_allows_uncommitted_hash(
    journal: &crate::models::settings::AgentLintRepairMutationJournal,
    path: &str,
    current: &Option<String>,
) -> bool {
    journal.affected_path_hashes.get(path) == Some(current)
        || journal.pre_mutation_path_hashes.get(path) == Some(current)
}

fn clear_settled_repair_journal(
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    operation_digest: &str,
    descriptor_digest: &str,
    journal: &crate::models::settings::AgentLintRepairMutationJournal,
) -> Result<(), BackendError> {
    if let Some(final_commit) = journal.final_commit.as_deref() {
        services
            .settings_service
            .clear_agent_lint_repair_journal_after_compensating_rollback(
                &run.task_id,
                operation_digest,
                descriptor_digest,
                &journal.checkpoint_hash,
                final_commit,
            )?;
    } else {
        services
            .settings_service
            .clear_agent_lint_repair_mutation_journal(
                &run.task_id,
                operation_digest,
                descriptor_digest,
                &journal.checkpoint_hash,
            )?;
    }
    Ok(())
}

fn settle_repair_failure(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    failure_outcome: AgentLintRepairOutcome,
) -> Result<RepairFailureSettlement, BackendError> {
    let terminal_task_status = if matches!(&failure_outcome, AgentLintRepairOutcome::Cancelled) {
        "cancelled"
    } else {
        "failed"
    };
    let operation_digest = operation_digest(services, run)?;
    let receipt = services
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &operation_digest,
        )?;
    let descriptor = receipt.descriptor_digest.as_deref().and_then(|expected| {
        load_descriptor(&run.task_id, run)
            .ok()
            .filter(|descriptor| descriptor_digest(descriptor).ok().as_deref() == Some(expected))
    });
    let descriptor_mismatch = receipt.descriptor_digest.is_some() && descriptor.is_none();
    if receipt.lifecycle == crate::models::settings::AgentLintRepairAttestationLifecycle::Completed
    {
        return terminal_result_from_receipt(&receipt).map(RepairFailureSettlement::Committed);
    }
    let mut result = descriptor
        .as_ref()
        .map(|descriptor| repair_result(descriptor, failure_outcome, false))
        .unwrap_or_else(|| empty_repair_result(run, failure_outcome));
    if let Some(journal) = receipt.mutation_journal.as_ref() {
        let rollback_paths = rollback_attested_repair_journal(context, run, services, journal)?;
        if let Some(descriptor) = descriptor.as_ref() {
            let mut rolled_back = descriptor.clone();
            rolled_back.affected_paths.extend(
                rollback_paths
                    .iter()
                    .filter(|path| context.resolve_wiki_write_path(path).is_ok())
                    .cloned(),
            );
            rolled_back.affected_paths.sort();
            rolled_back.affected_paths.dedup();
            rolled_back.terminal_affected_path_hashes = rollback_paths
                .iter()
                .map(|path| {
                    services
                        .file_store
                        .file_hash_if_exists(context, path)
                        .map(|hash| (path.clone(), hash))
                })
                .collect::<Result<_, _>>()?;
            result = repair_result(&rolled_back, AgentLintRepairOutcome::RolledBack, false);
        } else {
            result = empty_repair_result(run, AgentLintRepairOutcome::RolledBack);
        }
        clear_settled_repair_journal(
            run,
            services,
            &operation_digest,
            receipt
                .descriptor_digest
                .as_deref()
                .ok_or_else(stale_candidate_error)?,
            journal,
        )?;
    } else if descriptor_mismatch {
        // A project-owned descriptor can be replaced after its file write but
        // before the app-owned digest CAS. Without a mutation journal there is
        // no owned state to settle, so keep this mismatch fail-closed. With a
        // journal, rollback above intentionally relies only on the receipt's
        // checkpoint/path facts and discards the untrusted descriptor.
        return Err(stale_candidate_error());
    }
    services
        .settings_service
        .complete_agent_lint_repair_terminal_attestation(
            &run.task_id,
            &operation_digest,
            receipt.descriptor_digest.as_deref(),
            &agent_lint_repair_result_digest(&result)?,
            &agent_lint_repair_result_json(&result)?,
            terminal_task_status,
        )?;
    Ok(RepairFailureSettlement::Terminal(result))
}

fn finish_repair_error(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    mut error: BackendError,
) -> Option<WorkflowRun> {
    let _ = services
        .task_service
        .append_log(&run.task_id, LogLevel::Error, error.message.clone());
    let cancelled = services.task_service.is_cancelled(&run.task_id);
    let settlement = settle_repair_failure(
        context,
        run,
        services,
        if cancelled {
            AgentLintRepairOutcome::Cancelled
        } else {
            AgentLintRepairOutcome::Failed
        },
    );
    match settlement {
        Ok(RepairFailureSettlement::Committed(result)) => {
            if services
                .task_service
                .reconcile_committed_agent_lint_repair(&run.task_id, result)
                .is_ok()
            {
                return services
                    .coordinator
                    .claim_next(
                        services.task_service,
                        &run.canonical_identity_key,
                        &run.identity_revision,
                    )
                    .ok()
                    .flatten();
            }
        }
        Ok(RepairFailureSettlement::Terminal(result)) if cancelled => {
            return services
                .coordinator
                .finish_cancelled_and_claim_next_with_result(
                    services.task_service,
                    &run.task_id,
                    Some(result),
                )
                .ok()
                .and_then(|(_, next)| next);
        }
        Ok(RepairFailureSettlement::Terminal(result)) => {
            let project_mutation_state = match &result {
                WorkflowResult::AgentLintRepair {
                    outcome: AgentLintRepairOutcome::RolledBack,
                    ..
                } => WorkflowProjectMutationState::RolledBack,
                _ => WorkflowProjectMutationState::NotModified,
            };
            error = repair_error(&error.code, error.message, project_mutation_state);
            let _ = services.task_service.set_error(&run.task_id, error.clone());
            let current = services.task_service.get_workflow_run(&run.task_id)?;
            let stage_id = current
                .stages
                .iter()
                .find(|stage| {
                    stage.status == crate::models::workflow::WorkflowStageStatus::Running
                        || stage.status == crate::models::workflow::WorkflowStageStatus::Waiting
                })
                .map(|stage| stage.id.clone())?;
            let summary = workflow_error_summary(&error);
            return WorkflowStageSink::new(
                services.task_service,
                services.coordinator,
                &run.task_id,
            )
            .fail_with_result(&stage_id, summary, result)
            .ok()
            .and_then(|(_, next)| next);
        }
        Err(settlement_error) => {
            error = repair_error(
                "LINT_REPAIR_ROLLBACK_FAILED",
                format!(
                    "Repair failed and durable settlement was incomplete: original={}; settlement={}",
                    error.message, settlement_error.message
                ),
                mutation_state_from_error(&settlement_error),
            );
        }
    }
    let _ = services.task_service.set_error(&run.task_id, error.clone());
    let current = services.task_service.get_workflow_run(&run.task_id)?;
    let stage_id = current
        .stages
        .iter()
        .find(|stage| {
            stage.status == crate::models::workflow::WorkflowStageStatus::Running
                || stage.status == crate::models::workflow::WorkflowStageStatus::Waiting
        })
        .map(|stage| stage.id.clone())?;
    if current.display_status
        == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
    {
        let _ = services
            .task_service
            .clear_workflow_pending_action(&run.task_id);
    }
    let summary = workflow_error_summary(&error);
    WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id)
        .fail_with_result(
            &stage_id,
            summary,
            empty_repair_result(run, AgentLintRepairOutcome::Interrupted),
        )
        .ok()
        .and_then(|(_, next)| next)
}

fn workflow_error_summary(error: &BackendError) -> WorkflowErrorSummary {
    WorkflowErrorSummary {
        code: error.code.clone(),
        message_key: "workflows.error.agentLintRepair".into(),
        recoverable: error.recoverable,
        user_action_required: error.user_action_required,
        suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
        project_mutation_state: mutation_state_from_error(error),
    }
}

fn mutation_state_from_error(error: &BackendError) -> WorkflowProjectMutationState {
    match error
        .details
        .as_ref()
        .and_then(|details| details.get("workflowProjectMutationState"))
        .and_then(serde_json::Value::as_str)
    {
        Some("not_modified") => WorkflowProjectMutationState::NotModified,
        Some("modified") => WorkflowProjectMutationState::Modified,
        Some("rolled_back") => WorkflowProjectMutationState::RolledBack,
        _ => WorkflowProjectMutationState::Unknown,
    }
}

fn manifest_revision(manifest: &CompileManifest) -> Result<String, BackendError> {
    canonical_json(manifest)
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|error| {
            repair_error(
                "LINT_REPAIR_CANDIDATE_BINDING_FAILED",
                error,
                WorkflowProjectMutationState::NotModified,
            )
        })
}

pub fn agent_lint_repair_result_digest(result: &WorkflowResult) -> Result<String, BackendError> {
    if !matches!(result, WorkflowResult::AgentLintRepair { .. }) {
        return Err(repair_error(
            "LINT_REPAIR_TERMINAL_BINDING_FAILED",
            "Only an Agent lint repair result can be bound to its terminal receipt.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    canonical_json(&("agent-lint-repair-result-v1", result))
        .map(|value| hex_sha256(value.as_bytes()))
        .map_err(|error| {
            repair_error(
                "LINT_REPAIR_TERMINAL_BINDING_FAILED",
                error,
                WorkflowProjectMutationState::NotModified,
            )
        })
}

fn agent_lint_repair_result_json(result: &WorkflowResult) -> Result<String, BackendError> {
    if !matches!(result, WorkflowResult::AgentLintRepair { .. }) {
        return Err(repair_error(
            "LINT_REPAIR_TERMINAL_BINDING_FAILED",
            "Only an Agent lint repair result can be stored in its terminal receipt.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    canonical_json(result).map_err(|error| {
        repair_error(
            "LINT_REPAIR_TERMINAL_BINDING_FAILED",
            error,
            WorkflowProjectMutationState::NotModified,
        )
    })
}

fn terminal_result_from_receipt(
    receipt: &crate::models::settings::AgentLintRepairAttestation,
) -> Result<WorkflowResult, BackendError> {
    let json = receipt
        .terminal_result_json
        .as_deref()
        .ok_or_else(stale_candidate_error)?;
    let result: WorkflowResult = serde_json::from_str(json).map_err(|_| stale_candidate_error())?;
    if receipt.terminal_result_digest.as_deref()
        != Some(agent_lint_repair_result_digest(&result)?.as_str())
    {
        return Err(stale_candidate_error());
    }
    Ok(result)
}

fn completed_repair_result(descriptor: &PersistedAgentLintRepairCandidate) -> WorkflowResult {
    let outcome = if descriptor.unresolved_finding_ids.is_empty() {
        AgentLintRepairOutcome::Succeeded
    } else if descriptor.resolved_finding_ids.is_empty() {
        AgentLintRepairOutcome::ManualReviewRequired
    } else {
        AgentLintRepairOutcome::PartiallyCompleted
    };
    repair_result(descriptor, outcome, descriptor.final_commit.is_some())
}

fn repair_result(
    descriptor: &PersistedAgentLintRepairCandidate,
    outcome: AgentLintRepairOutcome,
    rollback_available: bool,
) -> WorkflowResult {
    WorkflowResult::AgentLintRepair {
        outcome,
        resolved_finding_ids: descriptor.resolved_finding_ids.clone(),
        unresolved_finding_ids: descriptor.unresolved_finding_ids.clone(),
        introduced_finding_ids: descriptor.introduced_finding_ids.clone(),
        skipped_finding_ids: descriptor.skipped_finding_ids.clone(),
        rounds: descriptor.rounds.clone(),
        affected_paths: descriptor.affected_paths.clone(),
        affected_path_hashes: descriptor.terminal_affected_path_hashes.clone(),
        checkpoint_hash: Some(descriptor.checkpoint_hash.clone()),
        final_commit: descriptor.final_commit.clone(),
        diff_available: !descriptor.affected_paths.is_empty(),
        rollback_available,
        index_refresh_warnings: descriptor.index_refresh_warnings.clone(),
    }
}

fn empty_repair_result(run: &WorkflowRun, outcome: AgentLintRepairOutcome) -> WorkflowResult {
    let unresolved_finding_ids = match &run.operation {
        WorkflowOperation::AgentLintRepair {
            selected_finding_ids,
            ..
        } => selected_finding_ids.clone(),
        WorkflowOperation::BuiltIn => Vec::new(),
    };
    WorkflowResult::AgentLintRepair {
        outcome,
        resolved_finding_ids: Vec::new(),
        unresolved_finding_ids,
        introduced_finding_ids: Vec::new(),
        skipped_finding_ids: Vec::new(),
        rounds: Vec::new(),
        affected_paths: Vec::new(),
        affected_path_hashes: BTreeMap::new(),
        checkpoint_hash: None,
        final_commit: None,
        diff_available: false,
        rollback_available: false,
        index_refresh_warnings: Vec::new(),
    }
}

pub fn agent_lint_repair_interrupted_result(run: &WorkflowRun) -> WorkflowResult {
    empty_repair_result(run, AgentLintRepairOutcome::Interrupted)
}

pub fn reconcile_agent_lint_repair_after_recovery(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
) -> Result<Option<WorkflowRun>, BackendError> {
    if !matches!(run.operation, WorkflowOperation::AgentLintRepair { .. }) {
        return Ok(None);
    }
    let operation_digest = operation_digest(services, run)?;
    let receipt = match services.settings_service.get_agent_lint_repair_attestation(
        &run.canonical_identity_key,
        &run.identity_revision,
        &run.task_id,
        &operation_digest,
    ) {
        Ok(receipt) => receipt,
        Err(error) if error.code == "LINT_REPAIR_ATTESTATION_REQUIRED" => {
            let result = empty_repair_result(run, AgentLintRepairOutcome::Interrupted);
            return services
                .task_service
                .attach_interrupted_agent_lint_repair_result(
                    &run.task_id,
                    result,
                    WorkflowProjectMutationState::NotModified,
                )
                .map(Some)
                .map_err(task_error);
        }
        Err(error) => return Err(error),
    };
    let descriptor = load_descriptor(&run.task_id, run)
        .ok()
        .filter(|descriptor| {
            descriptor_digest(descriptor).ok().as_deref() == receipt.descriptor_digest.as_deref()
        });
    if receipt.lifecycle == crate::models::settings::AgentLintRepairAttestationLifecycle::Completed
    {
        let result = terminal_result_from_receipt(&receipt)?;
        let (checkpoint_hash, final_commit, affected_path_hashes) = match &result {
            WorkflowResult::AgentLintRepair {
                checkpoint_hash,
                final_commit,
                affected_path_hashes,
                ..
            } => (checkpoint_hash, final_commit, affected_path_hashes),
            _ => return Err(stale_candidate_error()),
        };
        let terminal_status = receipt
            .terminal_task_status
            .as_deref()
            .ok_or_else(stale_candidate_error)?;
        if let Some(expected_head) = final_commit.as_deref().or(checkpoint_hash.as_deref()) {
            if services
                .git_service
                .repository_status(context)?
                .head
                .as_deref()
                != Some(expected_head)
            {
                return Err(repair_error(
                    "LINT_REPAIR_FINAL_COMMIT_STALE",
                    "The durable repair result is no longer the current Git HEAD.",
                    WorkflowProjectMutationState::Modified,
                ));
            }
        } else if !affected_path_hashes.is_empty() || terminal_status == "succeeded" {
            // A failure can be terminal before the initial checkpoint exists.
            // Only the app-owned empty/no-mutation receipt may omit that Git
            // fact; a successful or path-bearing result remains fail-closed.
            return Err(stale_candidate_error());
        }
        for (path, expected) in affected_path_hashes {
            if services.file_store.file_hash_if_exists(context, path)? != *expected {
                return Err(repair_error(
                    "LINT_REPAIR_TERMINAL_BINDING_STALE",
                    "A terminal repair path no longer matches its completed receipt.",
                    WorkflowProjectMutationState::Modified,
                ));
            }
        }
        return services
            .task_service
            .reconcile_terminal_agent_lint_repair(&run.task_id, result, terminal_status)
            .map(Some)
            .map_err(task_error);
    }
    let Some(journal) = receipt.mutation_journal.as_ref() else {
        let result = run
            .result
            .as_ref()
            .filter(|result| matches!(result, WorkflowResult::AgentLintRepair { .. }))
            .cloned()
            .or_else(|| {
                descriptor.as_ref().map(|descriptor| {
                    repair_result(descriptor, AgentLintRepairOutcome::Interrupted, false)
                })
            })
            .unwrap_or_else(|| empty_repair_result(run, AgentLintRepairOutcome::Interrupted));
        // No mutation crossed the durable WAL boundary. Persist the truthful
        // task result first, then terminalize the old receipt. If the process
        // stops between those two writes, the next recovery can repeat the
        // exact idempotent completion from the already-persisted task result.
        let updated = services
            .task_service
            .attach_interrupted_agent_lint_repair_result(
                &run.task_id,
                result.clone(),
                WorkflowProjectMutationState::NotModified,
            )
            .map_err(task_error);
        let updated = updated?;
        services
            .settings_service
            .complete_agent_lint_repair_terminal_attestation(
                &run.task_id,
                &operation_digest,
                receipt.descriptor_digest.as_deref(),
                &agent_lint_repair_result_digest(&result)?,
                &agent_lint_repair_result_json(&result)?,
                "interrupted",
            )?;
        return Ok(Some(updated));
    };
    let rollback_paths = rollback_attested_repair_journal(context, run, services, journal)?;
    let result = if let Some(descriptor) = descriptor.as_ref() {
        let mut recovered = descriptor.clone();
        recovered.affected_paths.extend(
            rollback_paths
                .iter()
                .filter(|path| context.resolve_wiki_write_path(path).is_ok())
                .cloned(),
        );
        recovered.affected_paths.sort();
        recovered.affected_paths.dedup();
        recovered.terminal_affected_path_hashes = rollback_paths
            .iter()
            .map(|path| {
                services
                    .file_store
                    .file_hash_if_exists(context, path)
                    .map(|hash| (path.clone(), hash))
            })
            .collect::<Result<_, _>>()?;
        repair_result(&recovered, AgentLintRepairOutcome::RolledBack, false)
    } else {
        empty_repair_result(run, AgentLintRepairOutcome::RolledBack)
    };
    let digest = receipt
        .descriptor_digest
        .as_deref()
        .ok_or_else(stale_candidate_error)?;
    clear_settled_repair_journal(run, services, &operation_digest, digest, journal)?;
    services
        .settings_service
        .complete_agent_lint_repair_terminal_attestation(
            &run.task_id,
            &operation_digest,
            Some(digest),
            &agent_lint_repair_result_digest(&result)?,
            &agent_lint_repair_result_json(&result)?,
            "interrupted",
        )?;
    services
        .task_service
        .attach_interrupted_agent_lint_repair_result(
            &run.task_id,
            result,
            WorkflowProjectMutationState::RolledBack,
        )
        .map(Some)
        .map_err(task_error)
}

pub fn record_agent_lint_repair_recovery_failure(
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    error: &BackendError,
) -> Result<WorkflowRun, BackendError> {
    services
        .task_service
        .append_log(&run.task_id, LogLevel::Error, error.message.clone())
        .map_err(task_error)?;
    services
        .task_service
        .attach_interrupted_agent_lint_repair_result(
            &run.task_id,
            empty_repair_result(run, AgentLintRepairOutcome::Interrupted),
            mutation_state_from_error(error),
        )
        .map_err(task_error)
}

pub(crate) fn discard_agent_lint_repair_candidate(task_id: &str) -> Result<(), BackendError> {
    if uuid::Uuid::parse_str(task_id).is_err() {
        return Err(repair_error(
            "LINT_REPAIR_CANDIDATE_ID_INVALID",
            "The repair task id is invalid.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    let workspace = repair_workspace(task_id);
    if !workspace.exists() {
        return Ok(());
    }
    let root = std::env::temp_dir().join("llm-wiki-desktop");
    let canonical_root = root.canonicalize().map_err(|error| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let canonical = workspace.canonicalize().map_err(|error| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    if canonical.parent() != Some(canonical_root.as_path()) {
        return Err(repair_error(
            "LINT_REPAIR_CANDIDATE_DISCARD_FAILED",
            "The repair candidate root escaped its task owner.",
            WorkflowProjectMutationState::NotModified,
        ));
    }
    std::fs::remove_dir_all(canonical).map_err(|error| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            WorkflowProjectMutationState::NotModified,
        )
    })
}

pub(crate) fn agent_lint_repair_decision_review(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    include_diffs: bool,
) -> Option<crate::models::workflow::WorkflowDecisionReview> {
    let (descriptor, _, _) = load_attested_descriptor(&run.task_id, run, services).ok()?;
    let round = descriptor.pending_round.as_ref()?;
    let source = TaskOwnedCandidateReviewSource {
        manifest: &round.manifest,
        baseline_hashes: &round.baseline_hashes,
        current_hashes: &round.current_hashes,
        checkpoint_hash: Some(&descriptor.checkpoint_hash),
    };
    task_owned_candidate_decision_review(
        context,
        &source,
        &round.changes,
        &round.output.summary,
        include_diffs,
    )
}

pub(crate) fn agent_lint_repair_file_diff_page(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    file_id: &str,
    start: usize,
    limit: usize,
) -> Result<Option<crate::models::workflow::WorkflowFileDiffPage>, BackendError> {
    let Some(index) = file_id
        .strip_prefix("file-")
        .and_then(|value| usize::from_str_radix(value, 16).ok())
    else {
        return Ok(None);
    };
    let (descriptor, _, _) = load_attested_descriptor(&run.task_id, run, services)?;
    let round = descriptor.pending_round.as_ref().ok_or_else(|| {
        repair_error(
            "LINT_REPAIR_CANDIDATE_STALE",
            "The repair task has no pending candidate.",
            WorkflowProjectMutationState::NotModified,
        )
    })?;
    let source = TaskOwnedCandidateReviewSource {
        manifest: &round.manifest,
        baseline_hashes: &round.baseline_hashes,
        current_hashes: &round.current_hashes,
        checkpoint_hash: Some(&descriptor.checkpoint_hash),
    };
    task_owned_candidate_file_diff_page(context, &source, file_id, index, start, limit)
}

pub fn agent_lint_repair_terminal_file_diff_page(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &AgentLintRepairExecutionServices<'_>,
    file_id: &str,
    start: usize,
    limit: usize,
) -> Result<Option<crate::models::workflow::WorkflowFileDiffPage>, BackendError> {
    let Some(index) = file_id
        .strip_prefix("file-")
        .and_then(|value| usize::from_str_radix(value, 16).ok())
    else {
        return Ok(None);
    };
    let descriptor = load_completed_attested_descriptor(&run.task_id, run, services)?;
    if descriptor.accumulated_manifest.files.is_empty()
        && descriptor.accumulated_manifest.deletions.is_empty()
    {
        return Ok(None);
    }
    let source = TaskOwnedCandidateReviewSource {
        manifest: &descriptor.accumulated_manifest,
        baseline_hashes: &descriptor.baseline_hashes,
        current_hashes: &descriptor.current_hashes,
        checkpoint_hash: Some(&descriptor.checkpoint_hash),
    };
    task_owned_candidate_file_diff_page(context, &source, file_id, index, start, limit)
}

fn repair_error(
    code: &str,
    message: impl Into<String>,
    mutation_state: WorkflowProjectMutationState,
) -> BackendError {
    BackendError::new(code, message.into(), true, true).with_details(serde_json::json!({
        "workflowProjectMutationState": match mutation_state {
            WorkflowProjectMutationState::NotModified => "not_modified",
            WorkflowProjectMutationState::Modified => "modified",
            WorkflowProjectMutationState::RolledBack => "rolled_back",
            WorkflowProjectMutationState::Unknown => "unknown",
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{journal_allows_uncommitted_hash, verified_partial_apply_hashes};
    use crate::models::settings::{AgentLintRepairMutationJournal, AgentLintRepairMutationPhase};
    use std::collections::BTreeMap;

    #[test]
    fn partial_apply_journal_accepts_only_an_exact_pre_or_post_hash_per_path() {
        let pre = BTreeMap::from([
            ("wiki/a.md".into(), Some("a1".into())),
            ("wiki/b.md".into(), Some("b0".into())),
        ]);
        let post = BTreeMap::from([
            ("wiki/a.md".into(), Some("a2".into())),
            ("wiki/b.md".into(), Some("b1".into())),
        ]);
        let partial = BTreeMap::from([
            ("wiki/a.md".into(), Some("a2".into())),
            ("wiki/b.md".into(), Some("b0".into())),
        ]);
        assert_eq!(
            verified_partial_apply_hashes(&pre, &post, &partial).unwrap(),
            partial
        );

        let external = BTreeMap::from([
            ("wiki/a.md".into(), Some("external".into())),
            ("wiki/b.md".into(), Some("b0".into())),
        ]);
        assert!(verified_partial_apply_hashes(&pre, &post, &external).is_err());

        let durable = AgentLintRepairMutationJournal {
            phase: AgentLintRepairMutationPhase::Applying,
            checkpoint_hash: "checkpoint".into(),
            pre_mutation_path_hashes: pre,
            affected_path_hashes: post,
            final_commit: None,
        };
        assert!(journal_allows_uncommitted_hash(
            &durable,
            "wiki/a.md",
            &Some("a1".into())
        ));
        assert!(journal_allows_uncommitted_hash(
            &durable,
            "wiki/a.md",
            &Some("a2".into())
        ));
        assert!(!journal_allows_uncommitted_hash(
            &durable,
            "wiki/a.md",
            &Some("external".into())
        ));
    }
}

fn task_error(message: String) -> BackendError {
    repair_error(
        "TASK_OPERATION_FAILED",
        message,
        WorkflowProjectMutationState::Unknown,
    )
}

fn with_agent_lint_git_cancellation<T>(
    services: &AgentLintRepairExecutionServices<'_>,
    task_id: &str,
    operation: impl FnOnce() -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let token = services
        .task_service
        .get_cancellation_token(task_id)
        .ok_or_else(|| task_error(format!("Task cancellation token is unavailable: {task_id}")))?;
    services
        .git_service
        .with_task_cancellation(token, operation)
}

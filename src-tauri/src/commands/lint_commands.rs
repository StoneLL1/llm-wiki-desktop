use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;

use sha2::{Digest, Sha256};
use tauri::State;

use crate::app_state::{AppState, ProjectWriteRootKind};
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::confirmation::{
    ConfirmationClaimDisposition, ConfirmationExecution, ConfirmationStatus, PendingAction,
    PendingActionType, RiskLevel,
};
use crate::models::lint::{
    AddLintIgnoreRequest, AgentLintRepairFinding, AgentLintRepairPreparation,
    AgentLintRepairRollbackResult, ApplyLintFixRequest, ApplyLintFixesBatchRequest,
    CancelAgentLintRepairPreparationRequest, ConfirmAgentLintRepairStartRequest, DeepLintIssueType,
    DeepLintReport, GetDeepLintReportRequest, LintBatchOutcome, LintBatchSkip, LintFixOutcome,
    LintHistoryFile, LintIgnoreFile, LintIssueSource, LintIssueType, LintReport,
    ListLintHistoryRequest, ListLintIgnoresRequest, PersistedLintReport,
    PrepareAgentLintRepairRequest, ReadLintHistoryReportRequest, RemoveLintIgnoreRequest,
    RollbackAgentLintRepairRequest, RunLocalLintRequest, StartDeepLintRequest, WikiLintSkillRef,
};
use crate::models::settings::AgentLintRepairAttestationLifecycle;
use crate::models::task::BackendTask;
use crate::models::workflow::{
    HealthCheckMode, WorkflowDisplayStatus, WorkflowExecutionOptions, WorkflowKind,
    WorkflowOperation, WorkflowPersistenceMode, WorkflowResult, WorkflowRoute, WorkflowScope,
    WorkflowStartOutcome,
};
use crate::services::{
    agent_lint_repair_attestation_digest, canonical_json, project_identity,
    resolve_workflow_persistence_binding, workflow_baseline_for_scope, AgentService,
    EnqueueWorkflow,
};

const AGENT_LINT_REPAIR_CONFIRMATION_TTL_MINUTES: i64 = 15;

fn agent_lint_repair_expires_at(now: chrono::DateTime<chrono::Utc>) -> String {
    (now + chrono::Duration::minutes(AGENT_LINT_REPAIR_CONFIRMATION_TTL_MINUTES)).to_rfc3339()
}

/// Run the deterministic local lint pass. Synchronous — it never calls a
/// model and completes in a single wiki scan.
#[tauri::command]
pub fn run_local_lint(
    state: State<'_, AppState>,
    request: RunLocalLintRequest,
) -> Result<LintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let report = state
        .lint_service
        .run_local_lint(&context, &state.search_service)?;
    state.lint_service.persist_local_report(&context, &report)?;
    Ok(report)
}

/// The legacy task-owned Deep Lint launch path is intentionally migrated to
/// Complete Health. Keeping it disabled avoids a second launch-authority,
/// snapshot, cancellation, and report-persistence contract. Complete Health
/// retains both explicit Agent and explicit BYOK analysis routes.
#[tauri::command]
pub fn start_deep_lint(
    _state: State<'_, AppState>,
    _request: StartDeepLintRequest,
) -> Result<BackendTask, BackendError> {
    Err(legacy_deep_lint_migrated_error())
}

fn legacy_deep_lint_migrated_error() -> BackendError {
    BackendError::new(
        "LINT_DEEP_HEALTH_REQUIRED",
        "Deep lint now runs through Complete Health so explicit Agent or BYOK launches share one current-authority, snapshot, cancellation, and report contract.",
        true,
        true,
    )
}

/// Prepare one explicit, backend-bound approval for repairing selected Agent
/// findings. This is read-only: no task, checkpoint, workspace, or Agent
/// process exists before the user confirms.
#[tauri::command]
pub fn prepare_agent_lint_repair(
    state: State<'_, AppState>,
    request: PrepareAgentLintRepairRequest,
) -> Result<AgentLintRepairPreparation, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |context| prepare_agent_lint_repair_current(&state, context, &request),
    )
}

/// Claim the exact backend approval, revalidate every mutable authority fact,
/// then enqueue the single bounded repair operation.
#[tauri::command]
pub fn confirm_agent_lint_repair_start(
    state: State<'_, AppState>,
    request: ConfirmAgentLintRepairStartRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    let stored = state.confirmation_registry.claim(&request.action_id)?;
    let result = (|| {
        let execution = stored.execution.ok_or_else(|| {
            lint_repair_error(
                "CONFIRMATION_EXECUTION_MISSING",
                "The Agent lint approval has no backend execution binding.",
            )
        })?;
        let binding = match execution {
            ConfirmationExecution::AgentLintRepairStart {
                project_id,
                root_path,
                canonical_identity_key,
                identity_revision,
                preparation_id,
                preparation_revision,
                report_id,
                selection_revision,
                selected_finding_ids,
                route,
                skill,
                authorized_path_hashes,
                baseline_fingerprint,
                expected_git_head,
            } => AgentLintStartBinding {
                project_id,
                root_path,
                canonical_identity_key,
                identity_revision,
                preparation_id,
                preparation_revision,
                report_id,
                selection_revision,
                selected_finding_ids,
                route,
                skill,
                authorized_path_hashes,
                baseline_fingerprint,
                expected_git_head,
            },
            _ => {
                return Err(lint_repair_error(
                    "CONFIRMATION_TYPE_MISMATCH",
                    "The pending action is not an Agent lint repair approval.",
                ))
            }
        };
        if binding.project_id != request.project_id
            || binding.preparation_id != request.preparation_id
            || binding.preparation_revision != request.preparation_revision
        {
            return Err(lint_repair_error(
                "LINT_REPAIR_CONFIRMATION_MISMATCH",
                "The Agent lint approval does not match this preparation.",
            ));
        }
        state.with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |context| revalidate_and_enqueue_agent_lint_repair(&state, context, &binding),
        )
    })();
    // A failed post-claim validation makes this preparation stale; consuming
    // it prevents a later retry from approving different project facts.
    let finish = state
        .confirmation_registry
        .finish_claim_with_disposition(&request.action_id, true);
    match (result, finish) {
        (Ok(outcome), Ok(ConfirmationClaimDisposition::CancelRequested)) => {
            cancel_confirmation_created_repair_run(&state, &outcome)?;
            Err(lint_repair_error(
                "LINT_REPAIR_CONFIRMATION_CANCELLED",
                "The Agent lint repair approval was cancelled before dispatch.",
            ))
        }
        (Err(_), Ok(ConfirmationClaimDisposition::CancelRequested)) => Err(lint_repair_error(
            "LINT_REPAIR_CONFIRMATION_CANCELLED",
            "The Agent lint repair approval was cancelled before dispatch.",
        )),
        (Ok(outcome), Ok(ConfirmationClaimDisposition::Completed)) => {
            if let WorkflowStartOutcome::Created { run } = outcome {
                if let Err(error) = attest_agent_lint_repair_run(&state, &run) {
                    cancel_confirmation_created_repair_run(
                        &state,
                        &WorkflowStartOutcome::Created { run },
                    )?;
                    return Err(error);
                }
                let (released, claimed) = match state
                    .workflow_service
                    .coordinator
                    .release_initial_approval_hold_and_claim_next(&state.task_service, &run.task_id)
                {
                    Ok(result) => result,
                    Err(message) => {
                        let _ = cancel_agent_lint_repair_attestation_for_run(&state, &run);
                        cancel_confirmation_created_repair_run(
                            &state,
                            &WorkflowStartOutcome::Created { run },
                        )?;
                        return Err(BackendError::new(
                            "LINT_REPAIR_CONFIRMATION_RELEASE_FAILED",
                            message,
                            true,
                            false,
                        ));
                    }
                };
                if let Some(claimed) = claimed {
                    state.workflow_service.dispatch_claimed_run_with_settings(
                        &state.task_service,
                        &state.settings_service,
                        &claimed,
                    )?;
                }
                Ok(WorkflowStartOutcome::Created { run: released })
            } else {
                Ok(outcome)
            }
        }
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn cancel_confirmation_created_repair_run(
    state: &AppState,
    outcome: &WorkflowStartOutcome,
) -> Result<(), BackendError> {
    let WorkflowStartOutcome::Created { run } = outcome else {
        return Ok(());
    };
    let (_, next) = state
        .workflow_service
        .coordinator
        .cancel_created_without_undo_and_claim_next(&state.task_service, &run.task_id)
        .map_err(|message| {
            BackendError::new(
                "LINT_REPAIR_CONFIRMATION_CANCEL_FAILED",
                message,
                true,
                false,
            )
        })?;
    state
        .settings_service
        .revoke_agent_lint_repair_attestation(&run.task_id)?;
    if let Some(next) = next {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            &next,
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_agent_lint_repair_preparation(
    state: State<'_, AppState>,
    request: CancelAgentLintRepairPreparationRequest,
) -> Result<PendingAction, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let stored = state.confirmation_registry.peek(&request.action_id)?;
    let matches = matches!(
        stored.execution.as_ref(),
        Some(ConfirmationExecution::AgentLintRepairStart {
            project_id,
            root_path,
            preparation_id,
            preparation_revision,
            ..
        }) if project_id == &request.project_id
            && preparation_id == &request.preparation_id
            && preparation_revision == &request.preparation_revision
            && project_identity(std::path::Path::new(root_path)).ok().map(|value| value.canonical_root)
                == project_identity(&context.root).ok().map(|value| value.canonical_root)
    );
    if !matches {
        return Err(lint_repair_error(
            "LINT_REPAIR_CONFIRMATION_MISMATCH",
            "The Agent lint approval does not match this preparation.",
        ));
    }
    Ok(state
        .confirmation_registry
        .confirm(&request.action_id, ConfirmationStatus::Cancelled)?
        .action)
}

#[derive(Clone)]
struct AgentLintStartBinding {
    project_id: String,
    root_path: String,
    canonical_identity_key: String,
    identity_revision: String,
    preparation_id: String,
    preparation_revision: String,
    report_id: String,
    selection_revision: String,
    selected_finding_ids: Vec<String>,
    route: WorkflowRoute,
    skill: WikiLintSkillRef,
    authorized_path_hashes: BTreeMap<String, Option<String>>,
    baseline_fingerprint: String,
    expected_git_head: String,
}

fn prepare_agent_lint_repair_current(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    request: &PrepareAgentLintRepairRequest,
) -> Result<AgentLintRepairPreparation, BackendError> {
    state.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)?;
    let identity = project_identity(&context.root)
        .map_err(|message| BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true))?;
    let routes = current_agent_lint_routes(state, context, request.agent)?;
    let route = routes.repair;
    let route_revision = route_revision(&route)?;
    let scope = WorkflowScope::HealthCheck {
        mode: HealthCheckMode::Complete,
    };
    let baseline_fingerprint = workflow_baseline_for_scope(context, &scope)?.fingerprint;
    let (selected_finding_ids, selected_findings, authorized_path_hashes) =
        selected_agent_findings(
            state,
            context,
            &request.report_id,
            &request.selected_finding_ids,
            request.agent,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &routes.analysis,
            &baseline_fingerprint,
        )?;
    let selection_hashes = authorized_path_hashes
        .clone()
        .into_iter()
        .collect::<HashMap<_, _>>();
    let selection_revision = crate::services::LintService::compute_agent_lint_selection_revision(
        &identity.identity_revision,
        &request.report_id,
        route_revision,
        &selected_finding_ids,
        &selection_hashes,
    )?;
    let git = clean_git_status(state, context)?;
    let expected_git_head = git.head.ok_or_else(|| {
        lint_repair_error(
            "LINT_REPAIR_GIT_HEAD_REQUIRED",
            "Agent lint repair requires an existing Git commit.",
        )
    })?;
    let skill = WikiLintSkillRef::builtin();
    let preparation_revision = sha256_canonical(&(
        &identity.canonical_identity_key,
        &identity.identity_revision,
        &request.report_id,
        &selection_revision,
        &selected_finding_ids,
        &selected_findings,
        &route,
        &skill,
        &authorized_path_hashes,
        &baseline_fingerprint,
        &expected_git_head,
    ))?;
    let preparation_id = format!("lint-repair-{}", &preparation_revision[..24]);
    let action = PendingAction {
        id: format!("agent-lint-repair-{preparation_revision}"),
        action_type: PendingActionType::AgentAutoFix,
        title: "Repair selected lint findings".into(),
        message: format!(
            "Start Agent repair for {} selected lint finding(s). No checkpoint is created until execution begins.",
            selected_finding_ids.len()
        ),
        risk_level: RiskLevel::High,
        affected_paths: authorized_path_hashes.keys().cloned().collect(),
        preview: None,
        expires_at: None,
        checkpoint_hash: None,
    };
    let binding = ConfirmationExecution::AgentLintRepairStart {
        project_id: context.project_id.clone(),
        root_path: context.root.to_string_lossy().into_owned(),
        canonical_identity_key: identity.canonical_identity_key,
        identity_revision: identity.identity_revision,
        preparation_id: preparation_id.clone(),
        preparation_revision: preparation_revision.clone(),
        report_id: request.report_id.clone(),
        selection_revision: selection_revision.clone(),
        selected_finding_ids: selected_finding_ids.clone(),
        route: route.clone(),
        skill: skill.clone(),
        authorized_path_hashes: authorized_path_hashes.clone(),
        baseline_fingerprint: baseline_fingerprint.clone(),
        expected_git_head: expected_git_head.clone(),
    };
    let registration = state
        .confirmation_registry
        .register_idempotent_expiring_with_execution(
            action,
            binding,
            agent_lint_repair_expires_at(chrono::Utc::now()),
        )?;
    let action = registration.stored.action;
    Ok(AgentLintRepairPreparation {
        preparation_id,
        preparation_revision,
        report_id: request.report_id.clone(),
        selection_revision: selection_revision.clone(),
        selected_finding_ids,
        route,
        skill,
        authorized_paths: authorized_path_hashes.keys().cloned().collect(),
        authorized_path_hashes,
        baseline_fingerprint,
        expected_git_head,
        pending_action: action,
    })
}

fn revalidate_and_enqueue_agent_lint_repair(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    binding: &AgentLintStartBinding,
) -> Result<WorkflowStartOutcome, BackendError> {
    state.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)?;
    let identity = project_identity(&context.root)
        .map_err(|message| BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true))?;
    let bound_identity = project_identity(std::path::Path::new(&binding.root_path))
        .map_err(|message| BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true))?;
    if context.project_id != binding.project_id
        || identity.canonical_root != bound_identity.canonical_root
        || identity.canonical_identity_key != binding.canonical_identity_key
        || identity.identity_revision != binding.identity_revision
    {
        return Err(lint_repair_stale(
            "Project identity changed after preparation.",
        ));
    }
    let agent = match &binding.route {
        WorkflowRoute::Agent { agent, .. } => *agent,
        _ => return Err(lint_repair_stale("The repair route is not an Agent route.")),
    };
    state.agent_service.invalidate_workflow_route_cache();
    let routes = current_agent_lint_routes(state, context, agent)?;
    let current_route = routes.repair;
    if current_route != binding.route {
        return Err(lint_repair_stale(
            "The Agent route changed after preparation.",
        ));
    }
    let scope = WorkflowScope::HealthCheck {
        mode: HealthCheckMode::Complete,
    };
    let baseline = workflow_baseline_for_scope(context, &scope)?;
    if baseline.fingerprint != binding.baseline_fingerprint {
        return Err(lint_repair_stale(
            "Project Markdown changed after preparation.",
        ));
    }
    let (_, selected_findings, current_hashes) = selected_agent_findings(
        state,
        context,
        &binding.report_id,
        &binding.selected_finding_ids,
        agent,
        &identity.canonical_identity_key,
        &identity.identity_revision,
        &routes.analysis,
        &baseline.fingerprint,
    )?;
    if current_hashes != binding.authorized_path_hashes {
        return Err(lint_repair_stale(
            "Selected lint paths changed after preparation.",
        ));
    }
    let selection_hashes = current_hashes.into_iter().collect::<HashMap<_, _>>();
    let current_selection_revision =
        crate::services::LintService::compute_agent_lint_selection_revision(
            &identity.identity_revision,
            &binding.report_id,
            route_revision(&current_route)?,
            &binding.selected_finding_ids,
            &selection_hashes,
        )?;
    if current_selection_revision != binding.selection_revision {
        return Err(lint_repair_stale("The selected lint findings are stale."));
    }
    let git = clean_git_status(state, context)?;
    if git.head.as_deref() != Some(binding.expected_git_head.as_str()) {
        return Err(lint_repair_stale("Git HEAD changed after preparation."));
    }
    let persistence =
        resolve_workflow_persistence_binding(context, WorkflowPersistenceMode::Persistent)?;
    let task_state_root = persistence.task_state_root.ok_or_else(|| {
        lint_repair_error(
            "LINT_REPAIR_PERSISTENCE_REQUIRED",
            "Agent lint repair requires persistent project task state.",
        )
    })?;
    let operation = WorkflowOperation::AgentLintRepair {
        preparation_id: binding.preparation_id.clone(),
        preparation_revision: binding.preparation_revision.clone(),
        report_id: binding.report_id.clone(),
        selection_revision: binding.selection_revision.clone(),
        selected_finding_ids: binding.selected_finding_ids.clone(),
        selected_findings,
        skill: binding.skill.clone(),
        authorized_path_hashes: binding.authorized_path_hashes.clone(),
        expected_git_head: binding.expected_git_head.clone(),
    };
    let outcome = state
        .workflow_service
        .coordinator
        .enqueue_for_owner_pending_approval(
            &state.task_service,
            EnqueueWorkflow {
                project_id: context.project_id.clone(),
                project_root: context.root.clone(),
                task_state_root: Some(task_state_root),
                title: "Agent lint repair".into(),
                kind: WorkflowKind::HealthCheck,
                scope,
                route: Some(current_route),
                baseline_fingerprint: binding.baseline_fingerprint.clone(),
                execution_options: WorkflowExecutionOptions {
                    preparation_revision: binding.preparation_revision.clone(),
                    operation,
                    preparation_fingerprint: Some(binding.preparation_revision.clone()),
                    existing_target_hash: None,
                    restricted_content_acknowledgement_revision: None,
                    remote_provider_acknowledgement_revision: None,
                },
                stages: crate::services::agent_lint_repair_stages(),
                retry: None,
            },
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )
        .map_err(|message| BackendError::new("WORKFLOW_START_FAILED", message, true, false))?;
    if let WorkflowStartOutcome::Existing { run } = &outcome {
        verify_agent_lint_repair_attestation(state, run, AgentLintRepairReplayIntent::Continue)?;
    }
    Ok(outcome)
}

pub(crate) fn attest_agent_lint_repair_run(
    state: &AppState,
    run: &crate::models::workflow::WorkflowRun,
) -> Result<(), BackendError> {
    let execution_options = state
        .task_service
        .workflow_execution_options(&run.task_id)
        .ok_or_else(|| {
            lint_repair_error(
                "LINT_REPAIR_ATTESTATION_REQUIRED",
                "The Agent lint repair has no exact execution options to attest.",
            )
        })?;
    let digest = agent_lint_repair_attestation_digest(run, &execution_options)
        .map_err(|error| BackendError::new("LINT_REPAIR_BINDING_FAILED", error, false, true))?;
    state
        .settings_service
        .record_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &digest,
        )?;
    Ok(())
}

fn agent_lint_repair_attestation_digest_for_run(
    state: &AppState,
    run: &crate::models::workflow::WorkflowRun,
) -> Result<String, BackendError> {
    let execution_options = state
        .task_service
        .workflow_execution_options(&run.task_id)
        .ok_or_else(|| {
            lint_repair_error(
                "LINT_REPAIR_ATTESTATION_REQUIRED",
                "The Agent lint repair has no exact execution options to attest.",
            )
        })?;
    agent_lint_repair_attestation_digest(run, &execution_options)
        .map_err(|error| BackendError::new("LINT_REPAIR_BINDING_FAILED", error, false, true))
}

pub(crate) fn cancel_agent_lint_repair_attestation_for_run(
    state: &AppState,
    run: &crate::models::workflow::WorkflowRun,
) -> Result<bool, BackendError> {
    if !matches!(run.operation, WorkflowOperation::AgentLintRepair { .. }) {
        return Ok(false);
    }
    let digest = agent_lint_repair_attestation_digest_for_run(state, run)?;
    state
        .settings_service
        .cancel_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &digest,
        )?;
    Ok(true)
}

pub(crate) fn restore_agent_lint_repair_attestation_for_run(
    state: &AppState,
    run: &crate::models::workflow::WorkflowRun,
) -> Result<(), BackendError> {
    let digest = agent_lint_repair_attestation_digest_for_run(state, run)?;
    state
        .settings_service
        .transition_agent_lint_repair_attestation(
            &run.task_id,
            &digest,
            &[AgentLintRepairAttestationLifecycle::Cancelled],
            AgentLintRepairAttestationLifecycle::QueuedAuthorized,
        )
}

#[derive(Clone, Copy)]
pub(crate) enum AgentLintRepairReplayIntent {
    Continue,
    Retry,
    Undo,
}

fn verify_agent_lint_repair_attestation(
    state: &AppState,
    run: &crate::models::workflow::WorkflowRun,
    intent: AgentLintRepairReplayIntent,
) -> Result<(), BackendError> {
    let execution_options = state
        .task_service
        .workflow_execution_options(&run.task_id)
        .ok_or_else(|| {
            lint_repair_error(
                "LINT_REPAIR_ATTESTATION_REQUIRED",
                "The persisted Agent lint repair has no exact execution options.",
            )
        })?;
    let digest = agent_lint_repair_attestation_digest(run, &execution_options)
        .map_err(|error| BackendError::new("LINT_REPAIR_BINDING_FAILED", error, false, true))?;
    let allowed = match intent {
        AgentLintRepairReplayIntent::Continue => {
            vec![AgentLintRepairAttestationLifecycle::QueuedAuthorized]
        }
        AgentLintRepairReplayIntent::Retry => vec![
            AgentLintRepairAttestationLifecycle::QueuedAuthorized,
            AgentLintRepairAttestationLifecycle::Dispatched,
            // The old attempt is immutable and terminal. Its app-owned receipt
            // proves the original batch approval; current route/trust/Git/path
            // facts are still fully revalidated before a linked new attempt is
            // created with its own fresh receipt.
            AgentLintRepairAttestationLifecycle::Completed,
        ],
        AgentLintRepairReplayIntent::Undo => {
            vec![AgentLintRepairAttestationLifecycle::Cancelled]
        }
    };
    if !state.settings_service.has_agent_lint_repair_attestation(
        &run.canonical_identity_key,
        &run.identity_revision,
        &run.task_id,
        &digest,
        &allowed,
    )? {
        return Err(lint_repair_error(
            "LINT_REPAIR_ATTESTATION_REQUIRED",
            "The persisted Agent lint repair has no exact app-owned approval attestation.",
        ));
    }
    Ok(())
}

/// Replay authority for a persisted repair operation. Unlike initial prepare,
/// retry/continue does not depend on the process-local Health report; it uses
/// only the bounded operation facts that were already approved and persisted.
/// Every mutable authority fact is nevertheless recomputed while the caller
/// holds the project trust-transition lock.
pub(crate) fn validate_agent_lint_repair_replay_facts(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &crate::models::workflow::WorkflowRun,
    access: &crate::services::WorkflowAccessSnapshot,
    intent: AgentLintRepairReplayIntent,
) -> Result<(), BackendError> {
    let WorkflowOperation::AgentLintRepair {
        preparation_id,
        preparation_revision,
        report_id,
        selection_revision,
        selected_finding_ids,
        selected_findings,
        skill,
        authorized_path_hashes,
        expected_git_head,
    } = &run.operation
    else {
        return Err(lint_repair_stale("The workflow is not a repair operation."));
    };
    if access.trust != crate::models::workflow::WorkflowProjectTrust::Trusted
        || access.filesystem_access != crate::models::workflow::WorkflowFilesystemAccess::Writable
        || access.persistence != WorkflowPersistenceMode::Persistent
    {
        return Err(lint_repair_stale(
            "Agent lint repair requires trusted, writable, persistent project access.",
        ));
    }
    state.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)?;
    let persistence =
        resolve_workflow_persistence_binding(context, WorkflowPersistenceMode::Persistent)?;
    if persistence.mode != WorkflowPersistenceMode::Persistent
        || persistence.task_state_root.is_none()
    {
        return Err(lint_repair_stale(
            "Agent lint repair requires persistent project task state.",
        ));
    }
    let identity = project_identity(&context.root)
        .map_err(|message| BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true))?;
    if identity.canonical_identity_key != run.canonical_identity_key
        || identity.identity_revision != run.identity_revision
    {
        return Err(lint_repair_stale(
            "Project identity changed after repair approval.",
        ));
    }
    verify_agent_lint_repair_attestation(state, run, intent)?;
    let agent = match run.route.as_ref() {
        Some(WorkflowRoute::Agent { agent, .. }) => *agent,
        _ => return Err(lint_repair_stale("The repair route is not an Agent route.")),
    };
    state.agent_service.invalidate_workflow_route_cache();
    let current_route = current_agent_lint_routes(state, context, agent)?.repair;
    if run.route.as_ref() != Some(&current_route) {
        return Err(lint_repair_stale(
            "The Agent repair route changed after approval.",
        ));
    }
    let scope = WorkflowScope::HealthCheck {
        mode: HealthCheckMode::Complete,
    };
    let baseline = workflow_baseline_for_scope(context, &scope)?;
    if run.scope != scope || baseline.fingerprint != run.baseline_fingerprint {
        return Err(lint_repair_stale(
            "Project Markdown changed after repair approval.",
        ));
    }
    let git = repair_replay_git_status(state, context, &run.task_id)?;
    if git.head.as_deref() != Some(expected_git_head.as_str()) {
        return Err(lint_repair_stale("Git HEAD changed after repair approval."));
    }
    let mut current_hashes = BTreeMap::new();
    for (path, expected_hash) in authorized_path_hashes {
        let current_hash = hash_optional_project_file(context, path)?;
        if &current_hash != expected_hash {
            return Err(lint_repair_stale(
                "An authorized Wiki path changed after repair approval.",
            ));
        }
        current_hashes.insert(path.clone(), current_hash);
    }
    let selection_hashes = current_hashes
        .clone()
        .into_iter()
        .collect::<HashMap<_, _>>();
    let current_selection_revision =
        crate::services::LintService::compute_agent_lint_selection_revision(
            &identity.identity_revision,
            report_id,
            route_revision(&current_route)?,
            selected_finding_ids,
            &selection_hashes,
        )?;
    if &current_selection_revision != selection_revision {
        return Err(lint_repair_stale(
            "The approved repair selection is no longer exact.",
        ));
    }
    let current_preparation_revision = sha256_canonical(&(
        &identity.canonical_identity_key,
        &identity.identity_revision,
        report_id,
        selection_revision,
        selected_finding_ids,
        selected_findings,
        &current_route,
        skill,
        &current_hashes,
        &baseline.fingerprint,
        expected_git_head,
    ))?;
    if &current_preparation_revision != preparation_revision
        || preparation_id != &format!("lint-repair-{}", &current_preparation_revision[..24])
    {
        return Err(lint_repair_stale(
            "The persisted repair approval binding is invalid.",
        ));
    }
    Ok(())
}

fn repair_replay_git_status(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    task_id: &str,
) -> Result<crate::models::git::GitRepositoryStatus, BackendError> {
    let status = state.git_service.repository_status(context)?;
    if !status.is_repository {
        return Err(lint_repair_error(
            "LINT_REPAIR_GIT_CLEAN_REQUIRED",
            "Agent lint repair requires a clean project-local Git repository.",
        ));
    }
    if status.has_changes {
        let allowed = [
            format!(".app/tasks/{task_id}.json"),
            format!(".app/tasks/{task_id}.log"),
        ];
        let changed = state.git_service.changed_paths(context)?;
        if changed.iter().any(|path| !allowed.contains(path)) {
            return Err(lint_repair_error(
                "LINT_REPAIR_GIT_CLEAN_REQUIRED",
                "Project content or unrelated app state changed after repair approval.",
            ));
        }
    }
    Ok(status)
}

fn selected_agent_findings(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    report_id: &str,
    requested_ids: &[String],
    expected_agent: AgentKind,
    canonical_identity_key: &str,
    identity_revision: &str,
    current_health_route: &WorkflowRoute,
    current_baseline_fingerprint: &str,
) -> Result<
    (
        Vec<String>,
        Vec<AgentLintRepairFinding>,
        BTreeMap<String, Option<String>>,
    ),
    BackendError,
> {
    if requested_ids.is_empty() || requested_ids.len() > 100 {
        return Err(lint_repair_error(
            "LINT_REPAIR_SELECTION_INVALID",
            "Select between 1 and 100 Agent lint findings.",
        ));
    }
    let mut selected = requested_ids.to_vec();
    selected.sort();
    if selected.iter().any(|id| id.trim().is_empty())
        || selected.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(lint_repair_error(
            "LINT_REPAIR_SELECTION_INVALID",
            "Selected Agent lint finding ids must be unique and non-empty.",
        ));
    }
    let report = state
        .lint_service
        .read_current_memory_health_report(context, report_id)?;
    let owner = state
        .task_service
        .get_workflow_run(&report.task_id)
        .ok_or_else(|| {
            lint_repair_error(
                "LINT_REPAIR_HEALTH_REPORT_REQUIRED",
                "The Health report has no current Workflow owner.",
            )
        })?;
    crate::services::LintService::validate_current_memory_health_report_owner(
        &report,
        &owner,
        &context.project_id,
        canonical_identity_key,
        identity_revision,
        current_health_route,
        current_baseline_fingerprint,
    )?;
    if report.report_id != report_id
        || report.mode != HealthCheckMode::Complete
        || !report_route_matches_repair_agent(&report.route, expected_agent)
    {
        return Err(lint_repair_error(
            "LINT_REPAIR_HEALTH_REPORT_REQUIRED",
            "The selected report is not a Complete Health Agent report.",
        ));
    }
    let issue_by_id = report
        .issues
        .iter()
        .map(|issue| (issue.id.as_str(), issue))
        .collect::<HashMap<_, _>>();
    let wiki_root = context.layout.wiki_write_root.as_deref().ok_or_else(|| {
        lint_repair_error(
            "LINT_REPAIR_WIKI_ROOT_REQUIRED",
            "Agent lint repair requires a writable wiki root.",
        )
    })?;
    let wiki_prefix = format!("{}/", wiki_root.trim_matches('/').replace('\\', "/"));
    let mut hashes = BTreeMap::new();
    let mut findings = Vec::with_capacity(selected.len());
    for id in &selected {
        let issue = issue_by_id.get(id.as_str()).ok_or_else(|| {
            lint_repair_error(
                "LINT_REPAIR_FINDING_NOT_FOUND",
                "A selected finding is not present in the Health report.",
            )
        })?;
        let origins = report
            .finding_origins
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let path = issue.path.replace('\\', "/");
        if !is_supported_agent_repair_finding(
            &report.route,
            issue.source,
            issue.issue_type,
            origins,
        ) || !(path == wiki_root || path.starts_with(&wiki_prefix))
            || !path.to_ascii_lowercase().ends_with(".md")
        {
            return Err(lint_repair_error(
                "LINT_REPAIR_FINDING_NOT_AUTHORIZED",
                "Only Agent findings inside the writable wiki root may be repaired.",
            ));
        }
        let current = hash_optional_project_file(context, &path)?;
        if current != issue.scan_hash {
            return Err(lint_repair_stale(
                "A selected lint path changed after the report.",
            ));
        }
        if let Some(existing) = hashes.insert(path, current.clone()) {
            if existing != current {
                return Err(lint_repair_stale(
                    "Selected findings disagree about a path snapshot.",
                ));
            }
        }
        findings.push(AgentLintRepairFinding {
            id: issue.id.clone(),
            issue_type: deep_lint_issue_type(issue.issue_type)?,
            severity: issue.severity,
            path: issue.path.replace('\\', "/"),
            message: issue.message.clone(),
            evidence: issue.evidence.clone(),
            suggested_action: issue.suggested_action.clone(),
        });
    }
    Ok((selected, findings, hashes))
}

fn deep_lint_issue_type(issue_type: LintIssueType) -> Result<DeepLintIssueType, BackendError> {
    match issue_type {
        LintIssueType::DuplicateTopic => Ok(DeepLintIssueType::DuplicateTopic),
        LintIssueType::WeakCrossReference => Ok(DeepLintIssueType::WeakCrossReference),
        LintIssueType::MissingSource => Ok(DeepLintIssueType::MissingSource),
        LintIssueType::SchemaMismatch => Ok(DeepLintIssueType::SchemaMismatch),
        LintIssueType::OutdatedContent => Ok(DeepLintIssueType::OutdatedContent),
        LintIssueType::Contradiction => Ok(DeepLintIssueType::Contradiction),
        _ => Err(lint_repair_error(
            "LINT_REPAIR_FINDING_NOT_AUTHORIZED",
            "Only pinned Agent lint finding types may be repaired.",
        )),
    }
}

fn report_route_matches_repair_agent(route: &WorkflowRoute, expected_agent: AgentKind) -> bool {
    matches!(route, WorkflowRoute::Agent { agent, .. } if *agent == expected_agent)
}

fn is_supported_agent_repair_finding(
    report_route: &WorkflowRoute,
    source: LintIssueSource,
    issue_type: LintIssueType,
    origins: &[LintIssueSource],
) -> bool {
    matches!(report_route, WorkflowRoute::Agent { .. })
        && source == LintIssueSource::Agent
        && origins.contains(&LintIssueSource::Agent)
        && matches!(
            issue_type,
            LintIssueType::DuplicateTopic
                | LintIssueType::WeakCrossReference
                | LintIssueType::MissingSource
                | LintIssueType::SchemaMismatch
                | LintIssueType::OutdatedContent
                | LintIssueType::Contradiction
        )
}

struct CurrentAgentLintRoutes {
    analysis: WorkflowRoute,
    repair: WorkflowRoute,
}

fn current_agent_lint_routes(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    agent: AgentKind,
) -> Result<CurrentAgentLintRoutes, BackendError> {
    if !AgentService::supports_lint_agent(agent) {
        return Err(lint_repair_error(
            "LINT_AGENT_PROFILE_UNSUPPORTED",
            "This Agent has no pinned lint repair profile.",
        ));
    }
    let settings = state.settings_service.read_settings(context)?;
    let (info, target_revision) = state
        .agent_service
        .lint_analysis_route_facts(agent, settings.agent_default == Some(agent))?;
    let (repair_info, repair_route_revision) = state
        .agent_service
        .lint_repair_route_facts(agent, settings.agent_default == Some(agent))?;
    if info.state != AgentDetectionState::Installed {
        return Err(lint_repair_error(
            "LINT_AGENT_ROUTE_UNAVAILABLE",
            "The selected Agent route is unavailable.",
        ));
    }
    if repair_info.state != AgentDetectionState::Installed {
        return Err(lint_repair_error(
            "LINT_AGENT_ROUTE_UNAVAILABLE",
            "The selected Agent repair route is unavailable.",
        ));
    }
    let analysis_profile = AgentService::lint_route_profile_revision(agent).ok_or_else(|| {
        lint_repair_error(
            "LINT_AGENT_PROFILE_UNSUPPORTED",
            "This Agent has no pinned lint analysis profile.",
        )
    })?;
    let revision = |profile| {
        sha256_canonical(&(
            agent,
            &info.state,
            &info.version,
            &info.executable_path,
            profile,
            &target_revision,
        ))
    };
    Ok(CurrentAgentLintRoutes {
        analysis: WorkflowRoute::Agent {
            agent,
            model: None,
            route_revision: revision(analysis_profile)?,
        },
        repair: WorkflowRoute::Agent {
            agent,
            model: None,
            route_revision: repair_route_revision,
        },
    })
}

fn clean_git_status(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
) -> Result<crate::models::git::GitRepositoryStatus, BackendError> {
    let status = state.git_service.repository_status(context)?;
    if !status.is_repository || status.has_changes {
        return Err(lint_repair_error(
            "LINT_REPAIR_GIT_CLEAN_REQUIRED",
            "Commit or discard project changes before starting Agent lint repair.",
        ));
    }
    Ok(status)
}

fn hash_optional_project_file(
    context: &crate::models::paths::ProjectContext,
    path: &str,
) -> Result<Option<String>, BackendError> {
    // Preparation is read-only, but its authorization must use the same path
    // safety boundary as the later Wiki mutation owner. This rejects linked
    // ancestors and broken leaf links instead of following them while hashing.
    let absolute = context.resolve_wiki_write_path(path)?;
    let metadata = match fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(BackendError::new(
                "LINT_REPAIR_PATH_READ_FAILED",
                error.to_string(),
                true,
                true,
            ))
        }
    };
    if crate::models::layout::is_link_or_reparse(&metadata) {
        return Err(lint_repair_error(
            "LINT_REPAIR_PATH_UNSAFE",
            "A selected lint path is a linked or reparse-point entry.",
        ));
    }
    if !metadata.is_file() {
        return Err(lint_repair_error(
            "LINT_REPAIR_PATH_INVALID",
            "A selected lint path is not a regular file.",
        ));
    }
    let mut file = fs::File::open(&absolute).map_err(|error| {
        BackendError::new(
            "LINT_REPAIR_PATH_READ_FAILED",
            error.to_string(),
            true,
            true,
        )
    })?;
    let opened_metadata = file.metadata().map_err(|error| {
        BackendError::new(
            "LINT_REPAIR_PATH_READ_FAILED",
            error.to_string(),
            true,
            true,
        )
    })?;
    if !same_lint_path_identity(&metadata, &opened_metadata) {
        return Err(lint_repair_error(
            "LINT_REPAIR_PATH_CHANGED",
            "A selected lint path changed while its repair baseline was being read.",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|error| {
        BackendError::new(
            "LINT_REPAIR_PATH_READ_FAILED",
            error.to_string(),
            true,
            true,
        )
    })?;
    let verified_absolute = context.resolve_wiki_write_path(path)?;
    let verified_metadata = fs::symlink_metadata(&verified_absolute).map_err(|error| {
        BackendError::new(
            "LINT_REPAIR_PATH_READ_FAILED",
            error.to_string(),
            true,
            true,
        )
    })?;
    if verified_absolute != absolute
        || crate::models::layout::is_link_or_reparse(&verified_metadata)
        || !same_lint_path_identity(&opened_metadata, &verified_metadata)
    {
        return Err(lint_repair_error(
            "LINT_REPAIR_PATH_CHANGED",
            "A selected lint path changed while its repair baseline was being read.",
        ));
    }
    Ok(Some(format!("{:x}", Sha256::digest(bytes))))
}

fn same_lint_path_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    if left.len() != right.len()
        || left.modified().ok() != right.modified().ok()
        || left.created().ok() != right.created().ok()
        || left.file_type() != right.file_type()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn route_revision(route: &WorkflowRoute) -> Result<&str, BackendError> {
    match route {
        WorkflowRoute::Agent { route_revision, .. } => Ok(route_revision),
        _ => Err(lint_repair_error(
            "LINT_AGENT_ROUTE_REQUIRED",
            "Agent lint repair requires an Agent route.",
        )),
    }
}

fn sha256_canonical(value: &impl serde::Serialize) -> Result<String, BackendError> {
    let canonical = canonical_json(value)
        .map_err(|error| BackendError::new("LINT_REPAIR_BINDING_FAILED", error, false, true))?;
    Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
}

fn lint_repair_stale(message: &str) -> BackendError {
    BackendError::new("LINT_REPAIR_PREPARATION_STALE", message, true, true)
}

fn lint_repair_error(code: &'static str, message: &'static str) -> BackendError {
    BackendError::new(code, message, true, true)
}

/// Roll back only the final commit produced by one completed Agent lint repair.
/// The backend task result, project identity, current HEAD, and every affected
/// path hash are revalidated before the scoped rollback commit is created.
#[tauri::command]
pub fn rollback_agent_lint_repair(
    state: State<'_, AppState>,
    request: RollbackAgentLintRepairRequest,
) -> Result<AgentLintRepairRollbackResult, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |context| {
            state.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)?;
            let identity = project_identity(&context.root).map_err(|message| {
                BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true)
            })?;
            let run = state
                .task_service
                .get_workflow_run(&request.task_id)
                .ok_or_else(|| {
                    lint_repair_error(
                        "LINT_REPAIR_TASK_MISSING",
                        "The Agent lint repair task is unavailable.",
                    )
                })?;
            if run.kind != WorkflowKind::HealthCheck
                || !matches!(run.operation, WorkflowOperation::AgentLintRepair { .. })
                || run.display_status != WorkflowDisplayStatus::Completed
                || run.canonical_identity_key != identity.canonical_identity_key
                || run.identity_revision != identity.identity_revision
            {
                return Err(lint_repair_error(
                    "LINT_REPAIR_ROLLBACK_NOT_AVAILABLE",
                    "This task is not a completed Agent lint repair for the current project identity.",
                ));
            }
            let execution_options = state
                .task_service
                .workflow_execution_options(&run.task_id)
                .ok_or_else(|| {
                    lint_repair_error(
                        "LINT_REPAIR_ATTESTATION_REQUIRED",
                        "The completed repair has no exact execution binding.",
                    )
                })?;
            let operation_digest = agent_lint_repair_attestation_digest(
                &run,
                &execution_options,
            )
            .map_err(|error| {
                BackendError::new("LINT_REPAIR_BINDING_FAILED", error, false, true)
            })?;
            let receipt = state.settings_service.get_agent_lint_repair_attestation(
                &run.canonical_identity_key,
                &run.identity_revision,
                &run.task_id,
                &operation_digest,
            )?;
            let result = run.result.as_ref().ok_or_else(|| {
                lint_repair_error(
                    "LINT_REPAIR_ROLLBACK_NOT_AVAILABLE",
                    "This repair task has no terminal result.",
                )
            })?;
            let terminal_digest = crate::services::agent_lint_repair_result_digest(result)?;
            if receipt.lifecycle
                != crate::models::settings::AgentLintRepairAttestationLifecycle::Completed
                || receipt.terminal_result_digest.as_deref() != Some(terminal_digest.as_str())
            {
                return Err(lint_repair_error(
                    "LINT_REPAIR_ATTESTATION_REQUIRED",
                    "The repair result is not bound to its app-owned completed receipt.",
                ));
            }
            let Some(WorkflowResult::AgentLintRepair {
                affected_paths,
                affected_path_hashes,
                checkpoint_hash: Some(checkpoint_hash),
                final_commit: Some(final_commit),
                rollback_available: true,
                ..
            }) = run.result
            else {
                return Err(lint_repair_error(
                    "LINT_REPAIR_ROLLBACK_NOT_AVAILABLE",
                    "This repair result has no complete rollback binding.",
                ));
            };
            if request.expected_final_commit != final_commit {
                return Err(lint_repair_stale(
                    "The requested repair final commit no longer matches the task result.",
                ));
            }
            let mut canonical_paths = affected_paths.clone();
            canonical_paths.sort();
            canonical_paths.dedup();
            if canonical_paths.is_empty() || canonical_paths != affected_paths {
                return Err(lint_repair_error(
                    "LINT_REPAIR_ROLLBACK_BINDING_INVALID",
                    "The repair result has an invalid affected-path binding.",
                ));
            }
            for path in &affected_paths {
                context.resolve_wiki_write_path(path)?;
            }
            let app_root = context.layout.app_state_root.as_deref().unwrap_or(".app");
            let graph_cache_path =
                format!("{}/graph-cache.json", app_root.trim_end_matches('/'));
            let mut rollback_paths = affected_paths.clone();
            rollback_paths.push(graph_cache_path);
            rollback_paths.sort();
            rollback_paths.dedup();
            if affected_path_hashes.len() != rollback_paths.len() {
                return Err(lint_repair_error(
                    "LINT_REPAIR_ROLLBACK_BINDING_INVALID",
                    "The repair result has an incomplete affected-path hash binding.",
                ));
            }
            for path in &rollback_paths {
                let expected = affected_path_hashes.get(path).ok_or_else(|| {
                    lint_repair_error(
                        "LINT_REPAIR_ROLLBACK_BINDING_INVALID",
                        "The repair result is missing an affected-path hash.",
                    )
                })?;
                let current = state.file_store.file_hash_if_exists(context, path)?;
                if &current != expected {
                    return Err(BackendError::new(
                        "LINT_REPAIR_ROLLBACK_NOT_CURRENT",
                        "A repair-owned file changed after the Agent finished; rollback was not applied.",
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({
                        "path": path,
                        "expectedHash": expected,
                        "currentHash": current,
                    })));
                }
            }
            let rollback = state.git_service.rollback_paths_to_checkpoint(
                context,
                &final_commit,
                &checkpoint_hash,
                &format!("Rollback Agent lint repair {}", run.task_id),
                &rollback_paths,
            )?;
            if let Ok(bookmarks) = state.bookmark_service.wiki_page_paths(context) {
                let _ = state.search_service.scan_wiki(context, &bookmarks);
            }
            let rollback_commit = rollback.commit_hash.ok_or_else(|| {
                lint_repair_error(
                    "LINT_REPAIR_ROLLBACK_COMMIT_MISSING",
                    "The repair rollback commit has no commit hash.",
                )
            })?;
            Ok(AgentLintRepairRollbackResult {
                task_id: run.task_id,
                rolled_back_commit: final_commit,
                rollback_commit,
                affected_paths,
            })
        },
    )
}

/// Load the persisted deep-lint report for a completed (or in-flight) task.
#[tauri::command]
pub fn get_deep_lint_report(
    state: State<'_, AppState>,
    request: GetDeepLintReportRequest,
) -> Result<DeepLintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let persisted = state
        .lint_service
        .read_lint_history_report(&context, &request.task_id)?;
    persisted.deep_report.ok_or_else(|| {
        BackendError::new(
            "LINT_DEEP_REPORT_MISSING",
            "The selected lint history report is not a deep lint report.",
            true,
            true,
        )
    })
}

#[tauri::command]
pub fn list_lint_history(
    state: State<'_, AppState>,
    request: ListLintHistoryRequest,
) -> Result<LintHistoryFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.lint_service.list_lint_history(&context)
}

#[tauri::command]
pub fn read_lint_history_report(
    state: State<'_, AppState>,
    request: ReadLintHistoryReportRequest,
) -> Result<PersistedLintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .lint_service
        .read_lint_history_report(&context, &request.id)
}

/// Apply (or plan) a single lint fix. Safe fixes apply under a Git checkpoint;
/// high-risk fixes return a `PendingAction` until confirmed.
#[tauri::command]
pub fn apply_lint_fix(
    state: State<'_, AppState>,
    request: ApplyLintFixRequest,
) -> Result<LintFixOutcome, BackendError> {
    if request.confirm_high_risk {
        let action_id = request.action_id.as_deref().ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_REQUIRED",
                "A backend-issued lint confirmation action is required.",
                true,
                true,
            )
        })?;
        // Claim the action before the write. Cancellation is rejected while a
        // claim is active, so an approval cannot be removed halfway through a
        // destructive mutation and then report a misleading NOT_FOUND error.
        let stored = state.confirmation_registry.claim(action_id)?;
        // Keep every post-claim validation inside the cleanup boundary. If
        // execution data or project resolution is invalid, the claim must be
        // released so a later retry/cancel is not permanently blocked.
        let result = (|| -> Result<LintFixOutcome, BackendError> {
            let execution = stored.execution.ok_or_else(|| {
                BackendError::new(
                    "CONFIRMATION_EXECUTION_MISSING",
                    "Lint confirmation has no execution plan.",
                    false,
                    true,
                )
            })?;
            let ConfirmationExecution::LintFix {
                project_id,
                root_path,
                issue,
            } = execution
            else {
                return Err(BackendError::new(
                    "CONFIRMATION_TYPE_MISMATCH",
                    "The pending action is not a lint fix.",
                    false,
                    true,
                ));
            };
            let context = state.resolve_project_context(&project_id, &root_path)?;
            state.lint_service.apply_fix(
                &context,
                &state.git_service,
                &issue,
                true,
                request.expected_hash.as_deref(),
            )
        })();
        match result {
            Ok(outcome) => {
                state.confirmation_registry.finish_claim(
                    action_id,
                    outcome.kind == crate::models::lint::LintFixOutcomeKind::Applied,
                )?;
                return Ok(outcome);
            }
            Err(error) => {
                let _ = state.confirmation_registry.finish_claim(action_id, false);
                return Err(error);
            }
        }
    }

    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let outcome = state.lint_service.apply_fix(
        &context,
        &state.git_service,
        &request.issue,
        false,
        request.expected_hash.as_deref(),
    )?;
    if let Some(action) = outcome.pending_action.clone() {
        state.confirmation_registry.register_with_execution(
            action,
            Some(ConfirmationExecution::LintFix {
                project_id: request.project_id,
                root_path: request.project_root_path,
                issue: request.issue,
            }),
        )?;
    }
    Ok(outcome)
}

/// Apply many lint fixes in one shot (PRD-LINT-003). One Git checkpoint
/// protects every safe write; high-risk fixes come back as confirmations for
/// unified review. Each confirmation is registered here so the existing
/// `apply_lint_fix(confirm_high_risk=true, action_id)` path can execute it.
#[tauri::command]
pub fn apply_lint_fixes(
    state: State<'_, AppState>,
    request: ApplyLintFixesBatchRequest,
) -> Result<LintBatchOutcome, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let outcome = state.lint_service.apply_fixes_batch(
        &context,
        &state.git_service,
        &request.issues,
        &request.expected_hashes,
    )?;
    let mut registered_confirmations = Vec::with_capacity(outcome.needs_confirmation.len());
    let mut skipped = outcome.skipped.clone();
    for confirmation in outcome.needs_confirmation {
        match state.confirmation_registry.register_with_execution(
            confirmation.pending_action.clone(),
            Some(ConfirmationExecution::LintFix {
                project_id: request.project_id.clone(),
                root_path: request.project_root_path.clone(),
                issue: confirmation.issue.clone(),
            }),
        ) {
            Ok(()) => registered_confirmations.push(confirmation),
            Err(error) => skipped.push(LintBatchSkip {
                issue_id: confirmation.issue.id,
                path: confirmation.issue.path,
                reason_code: "LINT_CONFIRMATION_REGISTER_FAILED".into(),
                reason: format!(
                    "The safe batch result was kept, but this high-risk confirmation could not be registered: {}",
                    error.message
                ),
            }),
        }
    }
    Ok(LintBatchOutcome {
        needs_confirmation: registered_confirmations,
        skipped,
        ..outcome
    })
}

/// Record an ignored (path, rule) so `run_local_lint` skips it on future scans.
#[tauri::command]
pub fn add_lint_ignore(
    state: State<'_, AppState>,
    request: AddLintIgnoreRequest,
) -> Result<LintIgnoreFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .lint_service
        .add_ignore(&context, &request.path, request.rule)
}

/// Remove an ignored (path, rule) so it is reported again.
#[tauri::command]
pub fn remove_lint_ignore(
    state: State<'_, AppState>,
    request: RemoveLintIgnoreRequest,
) -> Result<LintIgnoreFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .lint_service
        .remove_ignore(&context, &request.path, request.rule)
}

/// Return the current ignore list.
#[tauri::command]
pub fn list_lint_ignores(
    state: State<'_, AppState>,
    request: ListLintIgnoresRequest,
) -> Result<LintIgnoreFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.lint_service.list_ignores(&context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentKind;
    use crate::models::lint::{LintIssueSource, LintIssueType};
    use crate::models::llm::LlmProviderKind;
    use crate::models::project::ProjectTrustKind;
    use crate::models::settings::Settings;
    use crate::models::workflow::{
        WorkflowFilesystemAccess, WorkflowGitState, WorkflowProjectTrust, WorkflowRoute,
    };
    use crate::services::{
        workflow_stages, AgentInvocation, AgentProbeTarget, ProcessRunner, SettingsService,
    };
    use crate::tasks::TaskService;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    struct InstalledCodex;

    impl ProcessRunner for InstalledCodex {
        fn find_executable(&self, command: &str) -> Option<PathBuf> {
            (command == "codex").then(|| PathBuf::from("codex"))
        }

        fn resolve_probe_target(&self, command: &str) -> AgentProbeTarget {
            AgentProbeTarget {
                logical_command: command.into(),
                executable_path: self.find_executable(command),
                program: command.into(),
                leading_args: Vec::new(),
            }
        }

        fn run_with_timeout(
            &self,
            _: &str,
            args: &[&str],
            _: Duration,
        ) -> Result<String, BackendError> {
            if args == ["--version"] {
                return Ok("codex 1.0.0".into());
            }
            Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
        }

        fn run_capture(&self, _: &AgentInvocation) -> Result<(String, String), BackendError> {
            unreachable!()
        }

        fn run_task_streaming(
            &self,
            _: &AgentInvocation,
            _: &TaskService,
            _: &str,
        ) -> Result<String, BackendError> {
            unreachable!()
        }
    }

    #[test]
    fn report_path_is_task_scoped() {
        assert_eq!(
            format!(".app/lint-reports/{}.json", "task-1"),
            ".app/lint-reports/task-1.json"
        );
    }

    #[test]
    fn legacy_deep_lint_is_migrated_before_any_task_or_external_launch() {
        let error = legacy_deep_lint_migrated_error();
        assert_eq!(error.code, "LINT_DEEP_HEALTH_REQUIRED");
        assert!(error.message.contains("Complete Health"));
    }

    #[test]
    fn repair_selection_requires_agent_report_provenance_and_deep_type() {
        let agent = WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: None,
            route_revision: "route".into(),
        };
        assert!(is_supported_agent_repair_finding(
            &agent,
            LintIssueSource::Agent,
            LintIssueType::Contradiction,
            &[LintIssueSource::Agent],
        ));
        assert!(!is_supported_agent_repair_finding(
            &agent,
            LintIssueSource::Agent,
            LintIssueType::DeadLink,
            &[LintIssueSource::Agent],
        ));
        assert!(report_route_matches_repair_agent(&agent, AgentKind::Codex));
        assert!(!report_route_matches_repair_agent(
            &agent,
            AgentKind::Claude
        ));
        assert!(!is_supported_agent_repair_finding(
            &WorkflowRoute::Byok {
                provider: LlmProviderKind::OpenAi,
                model: "model".into(),
                route_revision: "route".into(),
            },
            LintIssueSource::Agent,
            LintIssueType::Contradiction,
            &[LintIssueSource::Agent],
        ));
        assert!(!is_supported_agent_repair_finding(
            &agent,
            LintIssueSource::Local,
            LintIssueType::Contradiction,
            &[LintIssueSource::Agent],
        ));
    }

    #[test]
    fn agent_lint_start_expiry_uses_the_fixed_bounded_ttl() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-12T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let expires_at = agent_lint_repair_expires_at(now);
        let expires_at = chrono::DateTime::parse_from_rfc3339(&expires_at).unwrap();

        assert_eq!(
            expires_at.signed_duration_since(now),
            chrono::Duration::minutes(AGENT_LINT_REPAIR_CONFIRMATION_TTL_MINUTES)
        );
    }

    #[test]
    fn persisted_repair_replay_accepts_only_exact_route_git_and_authorized_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("wiki")).unwrap();
        fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        fs::write(root.path().join("wiki/a.md"), "# A\n").unwrap();
        let context =
            crate::models::paths::ProjectContext::new("repair-replay", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let state = AppState {
            agent_service: AgentService::with_runner(Arc::new(InstalledCodex)),
            settings_service: SettingsService::with_config_dir(config.path().to_path_buf()),
            ..AppState::default()
        };
        state
            .settings_service
            .save_settings(
                &context,
                &Settings {
                    agent_default: Some(AgentKind::Codex),
                    ..Settings::default()
                },
            )
            .unwrap();
        let git = state
            .git_service
            .initialize_repository(&context, "Initial repair fixture")
            .unwrap();
        let expected_git_head = git.head.unwrap();
        let identity = project_identity(&context.root).unwrap();
        let route = current_agent_lint_routes(&state, &context, AgentKind::Codex)
            .unwrap()
            .repair;
        let scope = WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete,
        };
        let baseline = workflow_baseline_for_scope(&context, &scope).unwrap();
        let selected_finding_ids = vec!["contradiction:wiki/a.md".to_string()];
        let selected_findings = vec![AgentLintRepairFinding {
            id: selected_finding_ids[0].clone(),
            issue_type: DeepLintIssueType::Contradiction,
            severity: crate::models::lint::LintSeverity::Warning,
            path: "wiki/a.md".into(),
            message: "Contradiction".into(),
            evidence: None,
            suggested_action: None,
        }];
        let authorized_path_hashes = [(
            "wiki/a.md".to_string(),
            hash_optional_project_file(&context, "wiki/a.md").unwrap(),
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let selection_revision =
            crate::services::LintService::compute_agent_lint_selection_revision(
                &identity.identity_revision,
                "health-report-1",
                route_revision(&route).unwrap(),
                &selected_finding_ids,
                &authorized_path_hashes.clone().into_iter().collect(),
            )
            .unwrap();
        let skill = WikiLintSkillRef::builtin();
        let preparation_revision = sha256_canonical(&(
            &identity.canonical_identity_key,
            &identity.identity_revision,
            "health-report-1",
            &selection_revision,
            &selected_finding_ids,
            &selected_findings,
            &route,
            &skill,
            &authorized_path_hashes,
            &baseline.fingerprint,
            &expected_git_head,
        ))
        .unwrap();
        let operation = WorkflowOperation::AgentLintRepair {
            preparation_id: format!("lint-repair-{}", &preparation_revision[..24]),
            preparation_revision: preparation_revision.clone(),
            report_id: "health-report-1".into(),
            selection_revision,
            selected_finding_ids,
            selected_findings,
            skill,
            authorized_path_hashes,
            expected_git_head,
        };
        let run = match state
            .workflow_service
            .coordinator
            .enqueue(
                &state.task_service,
                EnqueueWorkflow {
                    project_id: context.project_id.clone(),
                    project_root: context.root.clone(),
                    task_state_root: Some(context.root.join(".app/tasks")),
                    title: "Agent lint repair".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope,
                    route: Some(route),
                    baseline_fingerprint: baseline.fingerprint,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision,
                        operation,
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: workflow_stages(&WorkflowKind::HealthCheck),
                    retry: None,
                },
            )
            .unwrap()
        {
            WorkflowStartOutcome::Created { run } => run,
            _ => panic!("fixture must create a repair run"),
        };
        let access = crate::services::WorkflowAccessSnapshot {
            trust: WorkflowProjectTrust::Trusted,
            trust_kind: Some(ProjectTrustKind::Native),
            filesystem_access: WorkflowFilesystemAccess::Writable,
            persistence: WorkflowPersistenceMode::Persistent,
            git_state: WorkflowGitState::Clean,
            authority_revision: "authority-1".into(),
        };

        assert_eq!(
            validate_agent_lint_repair_replay_facts(
                &state,
                &context,
                &run,
                &access,
                AgentLintRepairReplayIntent::Continue,
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_ATTESTATION_REQUIRED"
        );
        attest_agent_lint_repair_run(&state, &run).unwrap();
        validate_agent_lint_repair_replay_facts(
            &state,
            &context,
            &run,
            &access,
            AgentLintRepairReplayIntent::Continue,
        )
        .unwrap();
        let mut rebound = run.clone();
        rebound.project_id = "same-root-new-runtime-project-id".into();
        verify_agent_lint_repair_attestation(
            &state,
            &rebound,
            AgentLintRepairReplayIntent::Continue,
        )
        .unwrap();
        let execution_options = state
            .task_service
            .workflow_execution_options(&run.task_id)
            .unwrap();
        let digest = agent_lint_repair_attestation_digest(&run, &execution_options).unwrap();
        state
            .settings_service
            .transition_agent_lint_repair_attestation(
                &run.task_id,
                &digest,
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();
        let mut forged_queued = run.clone();
        forged_queued.display_status = crate::models::workflow::WorkflowDisplayStatus::Queued;
        assert_eq!(
            validate_agent_lint_repair_replay_facts(
                &state,
                &context,
                &forged_queued,
                &access,
                AgentLintRepairReplayIntent::Continue,
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_ATTESTATION_REQUIRED"
        );
        validate_agent_lint_repair_replay_facts(
            &state,
            &context,
            &run,
            &access,
            AgentLintRepairReplayIntent::Retry,
        )
        .unwrap();
        state
            .settings_service
            .complete_agent_lint_repair_attestation(
                &run.task_id,
                &digest,
                None,
                "terminal-old-attempt",
            )
            .unwrap();
        validate_agent_lint_repair_replay_facts(
            &state,
            &context,
            &run,
            &access,
            AgentLintRepairReplayIntent::Retry,
        )
        .unwrap();
        assert_eq!(
            verify_agent_lint_repair_attestation(
                &state,
                &run,
                AgentLintRepairReplayIntent::Continue,
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_ATTESTATION_REQUIRED"
        );
        fs::write(root.path().join("wiki/a.md"), "# externally changed\n").unwrap();
        assert_eq!(
            validate_agent_lint_repair_replay_facts(
                &state,
                &context,
                &run,
                &access,
                AgentLintRepairReplayIntent::Continue,
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_GIT_CLEAN_REQUIRED"
        );
    }
}

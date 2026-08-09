use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{ConfirmationExecution, PendingActionType};
use crate::models::workflow::{
    WorkflowDecisionCounts, WorkflowDecisionReview, WorkflowDisplayStatus, WorkflowErrorSummary,
    WorkflowFileDiff, WorkflowKind, WorkflowPreparation, WorkflowPrerequisiteAction,
    WorkflowRouteSelection, WorkflowRun, WorkflowRunPage, WorkflowScope, WorkflowStartOutcome,
    WorkflowsOverview,
};
use crate::services::{
    resolve_workflow_persistence_binding, restore_generate_content_confirmation,
    restore_update_wiki_confirmation, CompileExecutionServices, GenerateContentExecutionServices,
    PrepareWorkflowInput, UpdateWikiExecutionServices, WorkflowPersistenceBinding,
    WorkflowPreparationEnvironment,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub kind: WorkflowKind,
    pub scope: Option<WorkflowScope>,
    pub route_selection: Option<WorkflowRouteSelection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub preparation_id: String,
    pub preparation_revision: String,
    #[serde(default)]
    pub acknowledge_restricted_content: bool,
    #[serde(default)]
    pub acknowledge_remote_provider: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkflowRunsRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub workflow_kind: Option<WorkflowKind>,
    pub display_status: Option<WorkflowDisplayStatus>,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderQueuedWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
    pub before_task_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkflowActionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
    pub action_id: String,
}

#[tauri::command]
pub fn get_workflows_overview(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<WorkflowsOverview, BackendError> {
    if request.project_id.trim().is_empty() && request.project_root_path.trim().is_empty() {
        return Ok(state.workflow_service.no_project_overview());
    }
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = state.resolve_workflow_access(&context)?;
    state.workflow_service.project_overview(
        &context,
        access,
        &state.settings_service,
        &state.secret_service,
        &state.agent_service,
        &state.task_service,
    )
}

#[tauri::command]
pub fn prepare_workflow(
    state: State<'_, AppState>,
    request: PrepareWorkflowRequest,
) -> Result<WorkflowPreparation, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = state.resolve_workflow_access(&context)?;
    state.workflow_service.prepare(
        &WorkflowPreparationEnvironment {
            context: &context,
            access,
            settings_service: &state.settings_service,
            secret_service: &state.secret_service,
            agent_service: &state.agent_service,
        },
        PrepareWorkflowInput {
            kind: request.kind,
            scope: request.scope,
            route_selection: request.route_selection,
        },
    )
}

#[tauri::command]
pub fn start_workflow(
    state: State<'_, AppState>,
    request: StartWorkflowRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let outcome = state.with_workflow_access(&context, |access| {
        state.workflow_service.enqueue_with_acknowledgements(
            &context,
            access,
            &state.settings_service,
            &state.secret_service,
            &state.agent_service,
            &state.task_service,
            &request.preparation_id,
            &request.preparation_revision,
            request.acknowledge_restricted_content,
            request.acknowledge_remote_provider,
        )
    })?;
    if let WorkflowStartOutcome::Created { run } = &outcome {
        if run.display_status == WorkflowDisplayStatus::Running {
            state
                .workflow_service
                .dispatch_claimed_run(&state.task_service, run)?;
        }
    }
    Ok(outcome)
}

#[tauri::command]
pub fn list_workflow_runs(
    state: State<'_, AppState>,
    request: ListWorkflowRunsRequest,
) -> Result<WorkflowRunPage, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let identity = crate::services::project_identity(&context.root)
        .map_err(|message| workflow_error("WORKFLOW_IDENTITY_FAILED", message))?;
    let mut runs = state
        .task_service
        .list_workflow_runs()
        .into_iter()
        .filter(|run| {
            run.canonical_identity_key == identity.canonical_identity_key
                && run.identity_revision == identity.identity_revision
                && request
                    .workflow_kind
                    .as_ref()
                    .is_none_or(|kind| &run.kind == kind)
                && request
                    .display_status
                    .as_ref()
                    .is_none_or(|status| &run.display_status == status)
        })
        .collect::<Vec<_>>();
    runs.sort_by(|left, right| {
        right
            .started_at
            .cmp(&left.started_at)
            .then_with(|| right.task_id.cmp(&left.task_id))
    });
    if let Some(cursor) = request.cursor.as_deref() {
        let (cursor_started_at, cursor_task_id) = cursor.rsplit_once('|').ok_or_else(|| {
            workflow_error(
                "WORKFLOW_CURSOR_INVALID",
                "The workflow history cursor is invalid.",
            )
        })?;
        if cursor_started_at.is_empty() || cursor_task_id.is_empty() {
            return Err(workflow_error(
                "WORKFLOW_CURSOR_INVALID",
                "The workflow history cursor is invalid.",
            ));
        }
        runs.retain(|run| {
            (run.started_at.as_str(), run.task_id.as_str()) < (cursor_started_at, cursor_task_id)
        });
    }
    let limit = request.limit.clamp(1, 100);
    let has_more = runs.len() > limit;
    runs.truncate(limit);
    let next_runs = state.with_workflow_access(&context, |_| {
        let mut next_runs = Vec::new();
        for run in &mut runs {
            let Some(pending) = run.pending_action.clone() else {
                continue;
            };
            match hydrate_workflow_confirmation(&state, &context, run, &pending) {
                Ok(review) => run.decision_review = Some(review),
                Err(error) => {
                    let (interrupted, next) =
                        interrupt_unconfirmable_workflow(&state, &context, run, &pending, error)?;
                    *run = interrupted;
                    if let Some(next) = next {
                        next_runs.push(next);
                    }
                }
            }
        }
        Ok(next_runs)
    })?;
    for next in next_runs {
        state
            .workflow_service
            .dispatch_claimed_run(&state.task_service, &next)?;
    }
    let next_cursor = has_more
        .then(|| {
            runs.last()
                .map(|run| format!("{}|{}", run.started_at, run.task_id))
        })
        .flatten();
    Ok(WorkflowRunPage { runs, next_cursor })
}

#[tauri::command]
pub fn get_workflow_run(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    let context = require_workflow_project(&state, &request)?;
    let mut run = workflow_run(&state, &request.task_id)?;
    let next = if let Some(pending) = run.pending_action.clone() {
        state.with_workflow_access(&context, |_| {
            match hydrate_workflow_confirmation(&state, &context, &run, &pending) {
                Ok(review) => {
                    run.decision_review = Some(review);
                    Ok(None)
                }
                Err(error) => {
                    let (interrupted, next) =
                        interrupt_unconfirmable_workflow(&state, &context, &run, &pending, error)?;
                    run = interrupted;
                    Ok(next)
                }
            }
        })?
    } else {
        None
    };
    if let Some(next) = next {
        state
            .workflow_service
            .dispatch_claimed_run(&state.task_service, &next)?;
    }
    Ok(run)
}

pub(crate) fn interrupt_unconfirmable_workflow(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    pending: &crate::models::workflow::WorkflowPendingAction,
    error: BackendError,
) -> Result<(WorkflowRun, Option<WorkflowRun>), BackendError> {
    let _ = state
        .confirmation_registry
        .cancel_workflow_binding(context, run, pending);
    match &run.kind {
        WorkflowKind::UpdateWiki => {
            let _ = crate::services::discard_update_wiki_candidate(&run.task_id);
        }
        WorkflowKind::GenerateContent => {
            let _ = crate::services::discard_generate_content_candidate(&run.task_id);
        }
        WorkflowKind::HealthCheck => {}
    }
    state
        .workflow_service
        .coordinator
        .interrupt_invalid_confirmation(
            &state.task_service,
            &run.task_id,
            WorkflowErrorSummary {
                code: error.code,
                message_key: error.message,
                recoverable: false,
                user_action_required: true,
                suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
            },
        )
        .map_err(|message| workflow_error("WORKFLOW_CONFIRMATION_RECOVERY_FAILED", message))
}

fn hydrate_workflow_confirmation(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    pending: &crate::models::workflow::WorkflowPendingAction,
) -> Result<WorkflowDecisionReview, BackendError> {
    match &run.kind {
        WorkflowKind::UpdateWiki => restore_update_wiki_confirmation(
            context,
            run,
            &state.task_service,
            &state.confirmation_registry,
        )?,
        WorkflowKind::GenerateContent => restore_generate_content_confirmation(
            context,
            run,
            &state.task_service,
            &state.confirmation_registry,
        )?,
        WorkflowKind::HealthCheck => {}
    }
    let stored = state.confirmation_registry.peek(&pending.id)?;
    if !crate::models::confirmation::workflow_execution_matches(
        &run.kind,
        stored.execution.as_ref(),
        context,
        run,
        pending,
    ) {
        return Err(workflow_error(
            "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
            "The confirmation execution plan does not match this workflow run.",
        ));
    }
    let affected = stored.action.affected_paths.len() as u32;
    let counts = match stored.action.action_type {
        PendingActionType::OverwriteFile => WorkflowDecisionCounts {
            overwritten: affected,
            ..WorkflowDecisionCounts::default()
        },
        PendingActionType::DeleteFile => WorkflowDecisionCounts {
            deleted: affected,
            ..WorkflowDecisionCounts::default()
        },
        _ => WorkflowDecisionCounts {
            modified: affected,
            ..WorkflowDecisionCounts::default()
        },
    };
    let file_diffs = stored
        .action
        .preview
        .as_ref()
        .and_then(|preview| preview.diff.as_ref())
        .map(|diff| {
            vec![WorkflowFileDiff {
                path: stored
                    .action
                    .affected_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "candidate".into()),
                diff: diff.clone(),
            }]
        })
        .unwrap_or_default();
    if run.kind == WorkflowKind::UpdateWiki {
        let workflow = state
            .task_service
            .workflow_execution_state(&run.task_id)
            .ok_or_else(|| {
                workflow_error(
                    "WORKFLOW_CANDIDATE_STALE",
                    "The persisted workflow candidate is no longer valid.",
                )
            })?;
        crate::services::update_wiki_decision_review_for_workflow(
            &run.task_id,
            &context.root,
            &workflow,
        )
        .ok_or_else(|| {
            workflow_error(
                "WORKFLOW_CANDIDATE_STALE",
                "The persisted workflow candidate is no longer valid.",
            )
        })
    } else {
        Ok(WorkflowDecisionReview {
            reason: stored.action.message,
            counts,
            user_edits_detected: stored.action.action_type == PendingActionType::MergeConflict,
            file_diffs,
        })
    }
}

#[tauri::command]
pub fn cancel_workflow_run(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    let context = require_workflow_project(&state, &request)?;
    let (run, next) = state.with_workflow_access(&context, |_| {
        cancel_or_discard_workflow(&state, &context, &request.task_id, false)
    })?;
    dispatch_next(&state, next)?;
    Ok(run)
}

#[tauri::command]
pub fn undo_cancel_queued_workflow(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    require_workflow_project(&state, &request)?;
    let (run, claimed) = state
        .workflow_service
        .coordinator
        .undo_cancel(&state.task_service, &request.task_id)
        .map_err(|message| workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message))?;
    dispatch_next(&state, claimed)?;
    Ok(run)
}

#[tauri::command]
pub fn reorder_queued_workflow(
    state: State<'_, AppState>,
    request: ReorderQueuedWorkflowRequest,
) -> Result<WorkflowRunPage, BackendError> {
    let task_request = WorkflowRunRequest {
        project_id: request.project_id,
        project_root_path: request.project_root_path,
        task_id: request.task_id,
    };
    require_workflow_project(&state, &task_request)?;
    if let Some(before_task_id) = request.before_task_id.as_deref() {
        require_workflow_project(
            &state,
            &WorkflowRunRequest {
                project_id: task_request.project_id.clone(),
                project_root_path: task_request.project_root_path.clone(),
                task_id: before_task_id.to_string(),
            },
        )?;
    }
    let runs = state
        .workflow_service
        .coordinator
        .reorder_queued(
            &state.task_service,
            &task_request.task_id,
            request.before_task_id.as_deref(),
        )
        .map_err(|message| workflow_error("WORKFLOW_REORDER_FAILED", message))?;
    Ok(WorkflowRunPage {
        runs,
        next_cursor: None,
    })
}

#[tauri::command]
pub fn retry_workflow(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    retry_workflow_for_state(&state, request)
}

pub(crate) fn retry_workflow_for_state(
    state: &AppState,
    request: WorkflowRunRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    let context = require_workflow_project(state, &request)?;
    let original = workflow_run(state, &request.task_id)?;
    let outcome = state.with_workflow_access(&context, |access| {
        let replay = revalidate_workflow_replay_with_access(state, &context, &original, access)?;
        if let Err(error) = replay.eligibility {
            state
                .workflow_service
                .coordinator
                .apply_persistence_and_continue_queued(
                    &state.task_service,
                    &original.canonical_identity_key,
                    &original.identity_revision,
                    &[(original.task_id.clone(), replay.persistence.task_state_root)],
                    false,
                )
                .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
            return Err(error);
        }
        state
            .workflow_service
            .coordinator
            .retry(
                &state.task_service,
                &request.task_id,
                context.project_id.clone(),
                context.root.clone(),
                replay.persistence.task_state_root,
            )
            .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))
    })?;
    let run = match &outcome {
        WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => run,
    };
    if matches!(outcome, WorkflowStartOutcome::Created { .. }) {
        state
            .workflow_service
            .dispatch_claimed_run(&state.task_service, run)?;
    }
    Ok(outcome)
}

pub(crate) struct WorkflowReplayValidation {
    pub persistence: WorkflowPersistenceBinding,
    pub eligibility: Result<(), BackendError>,
}

pub(crate) fn revalidate_workflow_replay_with_access(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    access: crate::services::WorkflowAccessSnapshot,
) -> Result<WorkflowReplayValidation, BackendError> {
    ensure_workflow_identity(context, run)?;
    state.require_workflow_content_write_root(context, &run.kind)?;
    let persistence = resolve_workflow_persistence_binding(context, access.persistence.clone())?;
    let route_selection = run.route.as_ref().and_then(|route| match route {
        crate::models::workflow::WorkflowRoute::Agent { agent, .. } => {
            Some(WorkflowRouteSelection::Agent {
                agent: agent.clone(),
            })
        }
        crate::models::workflow::WorkflowRoute::Byok { provider, .. } => {
            Some(WorkflowRouteSelection::Byok {
                provider: provider.clone(),
            })
        }
        crate::models::workflow::WorkflowRoute::Local { .. } => None,
    });
    let eligibility = (|| {
        let preparation = state.workflow_service.prepare(
            &WorkflowPreparationEnvironment {
                context,
                access,
                settings_service: &state.settings_service,
                secret_service: &state.secret_service,
                agent_service: &state.agent_service,
            },
            PrepareWorkflowInput {
                kind: run.kind.clone(),
                scope: Some(run.scope.clone()),
                route_selection,
            },
        )?;
        let blocking = preparation.prerequisites.iter().find(|item| {
            item.blocking
                && !matches!(
                    item.action,
                    crate::models::workflow::WorkflowPrerequisiteAction::AcknowledgeRemoteProvider
                        | crate::models::workflow::WorkflowPrerequisiteAction::AcknowledgeRestrictedContent
                )
        });
        if preparation.baseline.fingerprint != run.baseline_fingerprint
            || preparation.route != run.route
            || blocking.is_some()
        {
            return Err(BackendError::new(
                "WORKFLOW_REPREPARATION_REQUIRED",
                "Project access, inputs, Git state, or execution route changed. Prepare the workflow again.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "action": crate::models::workflow::WorkflowPrerequisiteAction::PrepareAgain,
                "prerequisite": blocking,
            })));
        }
        Ok(())
    })();
    Ok(WorkflowReplayValidation {
        persistence,
        eligibility,
    })
}

#[tauri::command]
pub fn confirm_workflow_action(
    state: State<'_, AppState>,
    request: ConfirmWorkflowActionRequest,
) -> Result<WorkflowRun, BackendError> {
    let run_request = WorkflowRunRequest {
        project_id: request.project_id,
        project_root_path: request.project_root_path,
        task_id: request.task_id,
    };
    let context = require_workflow_project(&state, &run_request)?;
    let run = workflow_run(&state, &run_request.task_id)?;
    let pending = run.pending_action.clone().ok_or_else(|| {
        workflow_error(
            "WORKFLOW_CONFIRMATION_NOT_FOUND",
            "The workflow is not waiting for a confirmation.",
        )
    })?;
    if pending.id != request.action_id {
        return Err(workflow_error(
            "WORKFLOW_CONFIRMATION_MISMATCH",
            "The confirmation does not belong to this workflow run.",
        ));
    }
    let execution_result = state.with_workflow_access(&context, |access| {
        if access.trust != crate::models::workflow::WorkflowProjectTrust::Trusted {
            return Err(workflow_error(
                "WORKFLOW_PROJECT_UNTRUSTED",
                "Workflow confirmation requires a trusted project.",
            ));
        }
        if access.filesystem_access != crate::models::workflow::WorkflowFilesystemAccess::Writable {
            return Err(workflow_error(
                "WORKFLOW_PROJECT_READ_ONLY",
                "Workflow confirmation requires writable project access.",
            ));
        }
        state.require_workflow_content_write_root(&context, &run.kind)?;
        let stored = state.confirmation_registry.claim(&request.action_id)?;
        if !crate::models::confirmation::workflow_execution_matches(
            &run.kind,
            stored.execution.as_ref(),
            &context,
            &run,
            &pending,
        ) {
            let _ = state
                .confirmation_registry
                .finish_claim(&request.action_id, false);
            let _ = state
                .confirmation_registry
                .cancel_workflow_binding(&context, &run, &pending);
            match &run.kind {
                WorkflowKind::UpdateWiki => {
                    let _ = crate::services::discard_update_wiki_candidate(&run.task_id);
                }
                WorkflowKind::GenerateContent => {
                    let _ = crate::services::discard_generate_content_candidate(&run.task_id);
                }
                WorkflowKind::HealthCheck => {}
            }
            let (_, next) = state
                .workflow_service
                .coordinator
                .interrupt_invalid_confirmation(
                    &state.task_service,
                    &run.task_id,
                    WorkflowErrorSummary {
                        code: "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH".into(),
                        message_key: "workflows.error.prepareAgain".into(),
                        recoverable: false,
                        user_action_required: true,
                        suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
                    },
                )
                .map_err(|message| workflow_error("WORKFLOW_CONFIRMATION_FAILED", message))?;
            return Ok(Err((
                workflow_error(
                    "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
                    "The confirmation execution plan does not match this workflow run.",
                ),
                next,
            )));
        }
        let execution_result = match (run.kind.clone(), stored.execution) {
            (
                WorkflowKind::GenerateContent,
                Some(ConfirmationExecution::GenerateContentOverwrite {
                    project_id,
                    root_path,
                    task_id,
                    ..
                }),
            ) if project_id == context.project_id
                && root_path == context.root.to_string_lossy()
                && task_id == run_request.task_id =>
            {
                match crate::services::confirm_generate_content_overwrite(
                    &context,
                    &run_request.task_id,
                    &generate_content_services(&state),
                ) {
                    Ok(value) => Ok(value),
                    Err(failure) => Err((failure.error, failure.next)),
                }
            }
            (
                WorkflowKind::UpdateWiki,
                Some(ConfirmationExecution::UpdateWikiReview {
                    project_id,
                    root_path,
                    task_id,
                    ..
                }),
            ) if project_id == context.project_id
                && root_path == context.root.to_string_lossy()
                && task_id == run_request.task_id =>
            {
                let compile = CompileExecutionServices {
                    agent_service: &state.agent_service,
                    llm_service: &state.llm_service,
                    secret_service: &state.secret_service,
                    settings_service: &state.settings_service,
                    task_service: &state.task_service,
                };
                match crate::services::confirm_update_wiki_review(
                    &context,
                    &run_request.task_id,
                    &UpdateWikiExecutionServices {
                        compile,
                        git_service: &state.git_service,
                        file_store: &state.file_store,
                        bookmark_service: &state.bookmark_service,
                        search_service: &state.search_service,
                        confirmation_registry: &state.confirmation_registry,
                        coordinator: &state.workflow_service.coordinator,
                    },
                ) {
                    Ok(value) => Ok(value),
                    Err(failure) => Err((failure.error, failure.next)),
                }
            }
            _ => Err((
                workflow_error(
                    "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
                    "The confirmation execution plan does not match this workflow run.",
                ),
                None,
            )),
        };
        let consume_confirmation = execution_result.is_ok()
            || state
                .task_service
                .get_workflow_run(&run_request.task_id)
                .is_none_or(|current| {
                    current
                        .pending_action
                        .as_ref()
                        .is_none_or(|pending| pending.id != request.action_id)
                });
        state
            .confirmation_registry
            .finish_claim(&request.action_id, consume_confirmation)?;
        Ok(execution_result)
    })?;
    match execution_result {
        Ok((completed, next)) => {
            dispatch_next(&state, next)?;
            Ok(completed)
        }
        Err((error, next)) => {
            dispatch_next(&state, next)?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn discard_workflow_result(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    let context = require_workflow_project(&state, &request)?;
    let (run, next) = state.with_workflow_access(&context, |_| {
        cancel_or_discard_workflow(&state, &context, &request.task_id, true)
    })?;
    dispatch_next(&state, next)?;
    Ok(run)
}

fn require_workflow_project(
    state: &AppState,
    request: &WorkflowRunRequest,
) -> Result<crate::models::paths::ProjectContext, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if !state
        .task_service
        .task_belongs_to_root(&request.task_id, &context.root)
    {
        return Err(workflow_error(
            "WORKFLOW_PROJECT_MISMATCH",
            "Workflow does not belong to the asserted project.",
        ));
    }
    let run = workflow_run(state, &request.task_id)?;
    ensure_workflow_identity(&context, &run)?;
    Ok(context)
}

fn ensure_workflow_identity(
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
) -> Result<(), BackendError> {
    let identity = crate::services::project_identity(&context.root)
        .map_err(|message| workflow_error("WORKFLOW_IDENTITY_FAILED", message))?;
    if run.canonical_identity_key != identity.canonical_identity_key
        || run.identity_revision != identity.identity_revision
    {
        return Err(workflow_error(
            "WORKFLOW_PROJECT_IDENTITY_CHANGED",
            "The project folder identity changed after this workflow was created.",
        ));
    }
    Ok(())
}

fn workflow_run(state: &AppState, task_id: &str) -> Result<WorkflowRun, BackendError> {
    state
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| workflow_error("WORKFLOW_NOT_FOUND", "The workflow run was not found."))
}

fn cancel_or_discard_workflow(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    task_id: &str,
    require_waiting: bool,
) -> Result<(WorkflowRun, Option<WorkflowRun>), BackendError> {
    let before = workflow_run(state, task_id)?;
    if require_waiting && before.display_status != WorkflowDisplayStatus::WaitingForConfirmation {
        return Err(workflow_error(
            "WORKFLOW_RESULT_NOT_DISCARDABLE",
            "Only a workflow result waiting for confirmation can be discarded.",
        ));
    }
    state
        .workflow_service
        .coordinator
        .cancel(&state.task_service, task_id)
        .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
    let cancelling = workflow_run(state, task_id)?;
    if let Some(action) = cancelling.pending_action.as_ref() {
        if let Err(error) =
            state
                .confirmation_registry
                .cancel_workflow_binding(context, &cancelling, action)
        {
            if error.code == "CONFIRMATION_IN_USE" {
                return Ok((cancelling, None));
            }
            return Err(error);
        }
        let _ = crate::services::discard_update_wiki_candidate(task_id);
        let _ = crate::services::discard_generate_content_candidate(task_id);
        let (cancelled, next) = state
            .workflow_service
            .coordinator
            .finish_cancelled_and_claim_next(&state.task_service, task_id)
            .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
        return Ok((cancelled, next));
    }
    Ok((cancelling, None))
}

fn generate_content_services(state: &AppState) -> GenerateContentExecutionServices<'_> {
    GenerateContentExecutionServices {
        export_service: &state.export_service,
        search_service: &state.search_service,
        settings_service: &state.settings_service,
        secret_service: &state.secret_service,
        agent_service: &state.agent_service,
        llm_service: &state.llm_service,
        git_service: &state.git_service,
        confirmation_registry: &state.confirmation_registry,
        task_service: &state.task_service,
        coordinator: &state.workflow_service.coordinator,
    }
}

fn dispatch_next(state: &AppState, next: Option<WorkflowRun>) -> Result<(), BackendError> {
    if let Some(next) = next {
        state
            .workflow_service
            .dispatch_claimed_run(&state.task_service, &next)?;
    }
    Ok(())
}

fn workflow_error(code: &str, message: impl Into<String>) -> BackendError {
    BackendError::new(code, message, true, true)
}

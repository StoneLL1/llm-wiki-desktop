use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::commands::runtime::run_blocking;
use crate::errors::BackendError;
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowFileDiffPage, WorkflowPreparation, WorkflowRouteSelection,
    WorkflowRun, WorkflowRunHistoryPage, WorkflowRunPage, WorkflowStartOutcome, WorkflowsOverview,
};
pub use crate::models::workflow_requests::{
    ConfirmWorkflowActionRequest, ListWorkflowRunsRequest, PrepareWorkflowRequest,
    ReorderQueuedWorkflowRequest, StartWorkflowRequest, WorkflowFileDiffRequest,
    WorkflowProjectRequest, WorkflowRunRequest,
};
pub(crate) use crate::services::{agent_lint_repair_services, interrupt_unconfirmable_workflow};
use crate::services::{
    confirm_workflow_action_for_state, dispatch_next, ensure_workflow_identity,
    get_workflow_file_diff_for_state, get_workflow_run_for_state, list_workflow_runs_for_state,
    require_workflow_project, resolve_workflow_persistence_binding, workflow_run,
    BlockingWorkClass, PrepareWorkflowInput, WorkflowPersistenceBinding,
    WorkflowPreparationEnvironment,
};

#[tauri::command]
pub async fn get_workflows_overview(
    app: AppHandle,
    request: WorkflowProjectRequest,
) -> Result<WorkflowsOverview, BackendError> {
    run_blocking(app, BlockingWorkClass::MetadataIo, move |app| {
        let state = app.state::<AppState>();
        if request.project_id.trim().is_empty() && request.project_root_path.trim().is_empty() {
            return Ok(state.workflow_service.no_project_overview());
        }
        let access = state.project_registry.workflow_overview_access(
            &request.project_id,
            std::path::Path::new(&request.project_root_path),
        )?;
        state
            .workflow_service
            .overview
            .for_project(access, &state.task_service)
    })
    .await
}

#[tauri::command]
pub async fn prepare_workflow(
    app: AppHandle,
    request: PrepareWorkflowRequest,
) -> Result<WorkflowPreparation, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        let access = if matches!(
            request.kind,
            crate::models::workflow::WorkflowKind::HealthCheck
                | crate::models::workflow::WorkflowKind::UpdateWiki
        ) {
            state.resolve_workflow_read_access(&context)?
        } else {
            state.resolve_workflow_access(&context)?
        };
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
    })
    .await
}

#[tauri::command]
pub async fn start_workflow(
    app: AppHandle,
    request: StartWorkflowRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        start_workflow_for_state(&app.state::<AppState>(), request)
    })
    .await
}

pub(crate) fn start_workflow_for_state(
    state: &AppState,
    request: StartWorkflowRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let kind = state.workflow_service.preparation.kind_for_start(
        &state.task_service,
        &context,
        &request.preparation_id,
        &request.preparation_revision,
    )?;
    let read_access = matches!(
        kind,
        crate::models::workflow::WorkflowKind::HealthCheck
            | crate::models::workflow::WorkflowKind::UpdateWiki
    );
    let access = if read_access {
        state.resolve_workflow_read_access(&context)?
    } else {
        state.resolve_workflow_access(&context)?
    };
    // Hashing, route probes and acknowledgements run before taking the
    // project authority lock. The permit below only admits this result if
    // the exact authority and writable/persistence facts are still current.
    let admission = state.workflow_service.prepare_start_admission(
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
        request.retry_of_task_id.as_deref(),
    )?;
    let enqueue = |permit: &crate::app_state::ProjectTaskMutationPermit<'_>| {
        state
            .workflow_service
            .enqueue_prevalidated(permit, &state.task_service, &admission)
    };
    let outcome = if read_access {
        state.with_current_project_read_task_access(
            &request.project_id,
            &request.project_root_path,
            enqueue,
        )?
    } else {
        state.with_current_project_task_access(
            &request.project_id,
            &request.project_root_path,
            enqueue,
        )?
    };
    if let WorkflowStartOutcome::Created { run } = &outcome {
        if run.display_status == WorkflowDisplayStatus::Running {
            state.workflow_service.dispatch_claimed_run_with_settings(
                &state.task_service,
                &state.settings_service,
                run,
            )?;
        }
    }
    Ok(outcome)
}

#[tauri::command]
pub async fn list_workflow_runs(
    app: AppHandle,
    request: ListWorkflowRunsRequest,
) -> Result<WorkflowRunHistoryPage, BackendError> {
    run_blocking(app, BlockingWorkClass::MetadataIo, move |app| {
        list_workflow_runs_for_state(&app.state::<AppState>(), request)
    })
    .await
}

#[tauri::command]
pub async fn get_workflow_run(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        get_workflow_run_for_state(&app.state::<AppState>(), request)
    })
    .await
}

#[tauri::command]
pub async fn get_workflow_file_diff(
    app: AppHandle,
    request: WorkflowFileDiffRequest,
) -> Result<WorkflowFileDiffPage, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        get_workflow_file_diff_for_state(&app.state::<AppState>(), request)
    })
    .await
}

#[tauri::command]
pub async fn get_workflow_history_state(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<crate::services::UpdateWikiHistoryState, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let context = require_workflow_project(&state, &request)?;
        let run = workflow_run(&state, &request.task_id)?;
        ensure_workflow_identity(&context, &run)?;
        crate::services::get_update_wiki_history_state(&context, &run)
    })
    .await
}

#[tauri::command]
pub async fn undo_workflow_update(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<crate::services::UpdateWikiHistoryState, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        require_workflow_project(&state, &request)?;
        state.with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |_permit, context| {
                let run = workflow_run(&state, &request.task_id)?;
                ensure_workflow_identity(context, &run)?;
                crate::services::undo_update_wiki_history(
                    context,
                    &run,
                    &state.file_store,
                    &state.bookmark_service,
                    &state.search_service,
                    &state.task_service,
                )
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn cancel_workflow_run(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    run_blocking(app, BlockingWorkClass::MetadataIo, move |app| {
        let state = app.state::<AppState>();
        require_workflow_project(&state, &request)?;
        let (run, next) = state.with_current_project_read_task_access(
            &request.project_id,
            &request.project_root_path,
            |permit| {
                cancel_or_discard_workflow(
                    &state,
                    permit.context(),
                    &request.task_id,
                    false,
                    permit.workflow_access().persistence
                        == crate::models::workflow::WorkflowPersistenceMode::Persistent,
                )
            },
        )?;
        dispatch_next(&state, next)?;
        Ok(run)
    })
    .await
}

#[tauri::command]
pub async fn undo_cancel_queued_workflow(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        require_workflow_project(&state, &request)?;
        let before = workflow_run(&state, &request.task_id)?;
        let is_repair = matches!(
            before.operation,
            crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
        );
        if is_builtin_health(&before) {
            let (run, claimed) = state.with_current_project_read_task_access(
                &request.project_id,
                &request.project_root_path,
                |_permit| {
                    state.workflow_service.coordinator
                        .undo_cancel(&state.task_service, &request.task_id)
                        .map_err(|message| workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message))
                },
            )?;
            dispatch_next(&state, claimed)?;
            return Ok(run);
        }
        let (run, claimed) = state.with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |permit, current| {
                let access = permit.workflow_access();
                if !is_repair {
                    return state
                        .workflow_service
                        .coordinator
                        .undo_cancel(&state.task_service, &request.task_id)
                        .map_err(|message| workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message));
                }
                crate::commands::lint_commands::validate_agent_lint_repair_replay_facts(
                    &state,
                    current,
                    &before,
                    &access,
                    crate::commands::lint_commands::AgentLintRepairReplayIntent::Undo,
                )?;
                let (held, _) = state
                    .workflow_service
                    .coordinator
                    .undo_cancel_pending_approval(&state.task_service, &request.task_id)
                    .map_err(|message| workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message))?;
                if let Err(error) =
                    crate::commands::lint_commands::restore_agent_lint_repair_attestation_for_run(
                        &state, &held,
                    )
                {
                    let _ = state
                        .workflow_service
                        .coordinator
                        .cancel_created_without_undo_and_claim_next(&state.task_service, &held.task_id);
                    return Err(error);
                }
                match state
                    .workflow_service
                    .coordinator
                    .release_initial_approval_hold_and_claim_next(&state.task_service, &held.task_id)
                {
                    Ok(result) => Ok(result),
                    Err(message) => {
                        let _ =
                        crate::commands::lint_commands::cancel_agent_lint_repair_attestation_for_run(
                            &state, &held,
                        );
                        let _ = state
                            .workflow_service
                            .coordinator
                            .cancel_created_without_undo_and_claim_next(
                                &state.task_service,
                                &held.task_id,
                            );
                        Err(workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message))
                    }
                }
            },
        )?;
        dispatch_next(&state, claimed)?;
        Ok(run)
    })
    .await
}

#[tauri::command]
pub async fn reorder_queued_workflow(
    app: AppHandle,
    request: ReorderQueuedWorkflowRequest,
) -> Result<WorkflowRunPage, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
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
        if is_builtin_health(&workflow_run(&state, &task_request.task_id)?) {
            let runs = state.with_current_project_read_task_access(
                &task_request.project_id,
                &task_request.project_root_path,
                |_permit| {
                    state
                        .workflow_service
                        .coordinator
                        .reorder_queued(
                            &state.task_service,
                            &task_request.task_id,
                            request.before_task_id.as_deref(),
                        )
                        .map_err(|message| workflow_error("WORKFLOW_REORDER_FAILED", message))
                },
            )?;
            return Ok(WorkflowRunPage {
                runs,
                next_cursor: None,
            });
        }
        let runs = state.with_current_project_write_access(
            &task_request.project_id,
            &task_request.project_root_path,
            |_permit, _context| {
                state
                    .workflow_service
                    .coordinator
                    .reorder_queued(
                        &state.task_service,
                        &task_request.task_id,
                        request.before_task_id.as_deref(),
                    )
                    .map_err(|message| workflow_error("WORKFLOW_REORDER_FAILED", message))
            },
        )?;
        Ok(WorkflowRunPage {
            runs,
            next_cursor: None,
        })
    })
    .await
}

#[tauri::command]
pub async fn retry_workflow(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        retry_workflow_for_state(&state, request)
    })
    .await
}

pub(crate) fn retry_workflow_for_state(
    state: &AppState,
    request: WorkflowRunRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    require_workflow_project(state, &request)?;
    let original = workflow_run(state, &request.task_id)?;
    let repair_retry = matches!(
        original.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    );
    if is_builtin_health(&original) {
        let outcome = state.with_current_project_read_task_access(
            &request.project_id,
            &request.project_root_path,
            |permit| {
                let current = permit.context();
                let replay = revalidate_workflow_replay_with_access(
                    state,
                    current,
                    &original,
                    permit.workflow_access(),
                    super::lint_commands::AgentLintRepairReplayIntent::Retry,
                )?;
                replay.eligibility?;
                state
                    .workflow_service
                    .coordinator
                    .retry(
                        &state.task_service,
                        &request.task_id,
                        current.project_id.clone(),
                        current.root.clone(),
                        replay.persistence.task_state_root,
                    )
                    .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))
            },
        )?;
        if let WorkflowStartOutcome::Created { run } = &outcome {
            state.workflow_service.dispatch_claimed_run_with_settings(
                &state.task_service,
                &state.settings_service,
                run,
            )?;
        }
        return Ok(outcome);
    }
    let (outcome, released_claim) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, current| {
            let access = permit.workflow_access();
            let replay = revalidate_workflow_replay_with_access(
                state,
                current,
                &original,
                access,
                super::lint_commands::AgentLintRepairReplayIntent::Retry,
            )?;
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
            let outcome = if repair_retry {
                state.workflow_service.coordinator.retry_pending_approval(
                    &state.task_service,
                    &request.task_id,
                    current.project_id.clone(),
                    current.root.clone(),
                    replay.persistence.task_state_root,
                )
            } else {
                state.workflow_service.coordinator.retry(
                    &state.task_service,
                    &request.task_id,
                    current.project_id.clone(),
                    current.root.clone(),
                    replay.persistence.task_state_root,
                )
            }
            .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
            if repair_retry {
                let WorkflowStartOutcome::Created { run } = outcome else {
                    return Ok((outcome, None));
                };
                if let Err(error) = super::lint_commands::attest_agent_lint_repair_run(state, &run)
                {
                    let (_, next) = state
                        .workflow_service
                        .coordinator
                        .cancel_created_without_undo_and_claim_next(
                            &state.task_service,
                            &run.task_id,
                        )
                        .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
                    if let Some(next) = next {
                        state.workflow_service.dispatch_claimed_run_with_settings(
                            &state.task_service,
                            &state.settings_service,
                            &next,
                        )?;
                    }
                    return Err(error);
                }
                let (released, claimed) = state
                    .workflow_service
                    .coordinator
                    .release_initial_approval_hold_and_claim_next(&state.task_service, &run.task_id)
                    .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
                return Ok((WorkflowStartOutcome::Created { run: released }, claimed));
            }
            Ok((outcome, None))
        },
    )?;
    let run = match &outcome {
        WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => run,
    };
    if let Some(claimed) = released_claim {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            &claimed,
        )?;
    } else if !repair_retry && matches!(outcome, WorkflowStartOutcome::Created { .. }) {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            run,
        )?;
    }
    Ok(outcome)
}

fn is_builtin_health(run: &WorkflowRun) -> bool {
    run.kind == crate::models::workflow::WorkflowKind::HealthCheck
        && matches!(
            run.operation,
            crate::models::workflow::WorkflowOperation::BuiltIn
        )
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
    repair_intent: super::lint_commands::AgentLintRepairReplayIntent,
) -> Result<WorkflowReplayValidation, BackendError> {
    ensure_workflow_identity(context, run)?;
    if matches!(
        run.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    ) {
        let persistence =
            resolve_workflow_persistence_binding(context, access.persistence.clone())?;
        let eligibility = super::lint_commands::validate_agent_lint_repair_replay_facts(
            state,
            context,
            run,
            &access,
            repair_intent,
        )
        .map_err(|error| {
            BackendError::new(
                "WORKFLOW_REPREPARATION_REQUIRED",
                "Agent lint repair access, route, Git state, or authorized Wiki paths changed. Prepare and approve the repair again.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "action": crate::models::workflow::WorkflowPrerequisiteAction::PrepareAgain,
                "reasonCode": error.code,
            }))
        });
        return Ok(WorkflowReplayValidation {
            persistence,
            eligibility,
        });
    }
    state.require_workflow_content_write_root(context, &run.kind)?;
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
    if matches!(
        run.route,
        Some(crate::models::workflow::WorkflowRoute::Agent { .. })
    ) {
        // Retry and project-open continuation share this replay validator.
        // Neither boundary may accept the short-lived route-presentation cache
        // when deciding whether an Agent attempt can be created or claimed.
        state.agent_service.invalidate_workflow_route_cache();
    }
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
    let persistence = resolve_workflow_persistence_binding(
        context,
        preparation.project_access.persistence.clone(),
    )?;
    let eligibility = (|| {
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
pub async fn confirm_workflow_action(
    app: AppHandle,
    request: ConfirmWorkflowActionRequest,
) -> Result<WorkflowRun, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        confirm_workflow_action_for_state(&app.state::<AppState>(), request)
    })
    .await
}

#[tauri::command]
pub async fn discard_workflow_result(
    app: AppHandle,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        require_workflow_project(&state, &request)?;
        let (run, next) = state.with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |_permit, current| {
                cancel_or_discard_workflow(&state, current, &request.task_id, true, true)
            },
        )?;
        dispatch_next(&state, next)?;
        Ok(run)
    })
    .await
}

pub(crate) fn cancel_or_discard_workflow(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    task_id: &str,
    require_waiting: bool,
    allow_project_cleanup: bool,
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
    if allow_project_cleanup
        && matches!(
            before.display_status,
            WorkflowDisplayStatus::Queued
                | WorkflowDisplayStatus::Running
                | WorkflowDisplayStatus::WaitingForConfirmation
        )
    {
        // The task owner decides whether cancellation can win (notably, a
        // checked apply is temporarily non-cancellable) before the app-owned
        // receipt is tombstoned. A late success publication only accepts a
        // still-Dispatched receipt, so a winning cancellation cannot be
        // overwritten by the final commit path.
        crate::commands::lint_commands::cancel_agent_lint_repair_attestation_for_run(
            state, &before,
        )?;
    }
    let cancelling = workflow_run(state, task_id)?;
    if let Some(action) = before.pending_action.as_ref() {
        if action.action_type == crate::models::confirmation::PendingActionType::ReviewScope {
            return state
                .workflow_service
                .coordinator
                .finish_cancelled_and_claim_next(&state.task_service, task_id)
                .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message));
        }
        if let Err(error) = state
            .confirmation_registry
            .cancel_workflow_binding(context, &before, action)
        {
            if error.code == "CONFIRMATION_IN_USE" {
                return Ok((cancelling, None));
            }
            return Err(error);
        }
        if allow_project_cleanup
            && matches!(
                &before.operation,
                crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
            )
        {
            let result = crate::services::rollback_and_discard_agent_lint_repair_candidate(
                context,
                &before,
                &agent_lint_repair_services(state),
            )?;
            let (cancelled, next) = state
                .workflow_service
                .coordinator
                .finish_cancelled_and_claim_next_with_result(
                    &state.task_service,
                    task_id,
                    Some(result),
                )
                .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
            return Ok((cancelled, next));
        } else {
            let _ = crate::services::discard_update_wiki_candidate(task_id);
            let _ = crate::services::discard_generate_content_candidate(task_id);
        }
        let (cancelled, next) = state
            .workflow_service
            .coordinator
            .finish_cancelled_and_claim_next(&state.task_service, task_id)
            .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
        return Ok((cancelled, next));
    }
    Ok((cancelling, None))
}

fn workflow_error(code: &str, message: impl Into<String>) -> BackendError {
    BackendError::new(code, message, true, true)
}

#[cfg(test)]
mod batch6_tests {
    use super::*;
    use crate::models::workflow::WorkflowKind;

    #[test]
    fn retry_and_continue_shared_replay_gate_reprobe_agent_route_before_claim() {
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        use crate::models::agent::AgentKind;
        use crate::models::project::ProjectTrustKind;
        use crate::models::settings::Settings;
        use crate::models::workflow::{
            HealthCheckMode, WorkflowExecutionOptions, WorkflowFilesystemAccess, WorkflowGitState,
            WorkflowPersistenceMode, WorkflowProjectTrust, WorkflowScope, WorkflowStartOutcome,
        };
        use crate::services::{
            AgentInvocation, AgentProbeTarget, AgentService, EnqueueWorkflow, ProcessRunner,
            SettingsService, WorkflowAccessSnapshot,
        };
        use crate::tasks::TaskService;

        struct MutableCodex {
            version: AtomicUsize,
            invocations: AtomicUsize,
        }

        impl ProcessRunner for MutableCodex {
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
                    return Ok(format!("codex {}.0.0", self.version.load(Ordering::SeqCst)));
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
                self.invocations.fetch_add(1, Ordering::SeqCst);
                Ok("[]".into())
            }
        }

        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki")).unwrap();
        std::fs::write(root.path().join("wiki/index.md"), "# Index\n").unwrap();
        let context =
            crate::models::paths::ProjectContext::new("replay-route", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let runner = Arc::new(MutableCodex {
            version: AtomicUsize::new(1),
            invocations: AtomicUsize::new(0),
        });
        let state = AppState {
            agent_service: AgentService::with_runner(runner.clone()),
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
        let access = WorkflowAccessSnapshot {
            trust: WorkflowProjectTrust::Trusted,
            trust_kind: Some(ProjectTrustKind::Native),
            filesystem_access: WorkflowFilesystemAccess::Writable,
            persistence: WorkflowPersistenceMode::Persistent,
            git_state: WorkflowGitState::Clean,
            authority_revision: "authority-v1".into(),
        };
        let preparation = state
            .workflow_service
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &context,
                    access: access.clone(),
                    settings_service: &state.settings_service,
                    secret_service: &state.secret_service,
                    agent_service: &state.agent_service,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::HealthCheck,
                    scope: Some(WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    }),
                    route_selection: Some(WorkflowRouteSelection::Agent {
                        agent: AgentKind::Codex,
                    }),
                },
            )
            .unwrap();
        let outcome = state
            .workflow_service
            .coordinator
            .enqueue(
                &state.task_service,
                EnqueueWorkflow {
                    project_id: context.project_id.clone(),
                    project_root: context.root.clone(),
                    task_state_root: None,
                    title: "Health Check".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope: WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    },
                    route: preparation.route,
                    baseline_fingerprint: preparation.baseline.fingerprint,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: preparation.preparation_revision,
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: crate::services::workflow_stages(&WorkflowKind::HealthCheck),
                    retry: None,
                },
            )
            .unwrap();
        let run = match outcome {
            WorkflowStartOutcome::Created { run } => run,
            _ => panic!("fixture must create a workflow run"),
        };

        runner.version.store(2, Ordering::SeqCst);
        let replay = revalidate_workflow_replay_with_access(
            &state,
            &context,
            &run,
            access,
            crate::commands::lint_commands::AgentLintRepairReplayIntent::Continue,
        )
        .unwrap();
        assert!(replay.persistence.task_state_root.is_none());
        assert_eq!(
            replay.eligibility.unwrap_err().code,
            "WORKFLOW_REPREPARATION_REQUIRED"
        );
        assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    }
}

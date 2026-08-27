use tauri::{AppHandle, Manager, State};

use super::import_v2_commands::RESTRICTED_CONTENT_ACK_PATH;
use crate::{
    app_state::AppState,
    errors::BackendError,
    models::import_v2_agent::{
        AcceptImportAgentCandidateRequest, AgentCandidateActionResult, AgentCandidateView,
        AgentInvocationRequest, DiscardImportAgentCandidateRequest,
        SelectImportAgentCandidateRequest,
    },
    models::task::BackendTask,
    services::import_v2::agent_assistance::AgentAssistanceService,
    services::import_v2::agent_candidate::AgentCandidateService,
    services::BlockingWorkClass,
};

pub fn accept_import_agent_candidate_v2(
    state: State<'_, AppState>,
    request: AcceptImportAgentCandidateRequest,
) -> Result<AgentCandidateView, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let service = AgentCandidateService::new(
                &state.import_v2_service,
                &state.file_store,
                &state.task_service,
            );
            let candidate = service.accept_staged_output_authorized(
                permit,
                &request.session_id,
                &request.item_id,
                &request.task_id,
            )?;
            let (_, diff) = service.load_candidate(
                &context,
                &request.session_id,
                &request.item_id,
                &candidate.candidate_id,
            )?;
            Ok(AgentCandidateView {
                project_id: request.project_id.clone(),
                session_id: request.session_id.clone(),
                item_id: request.item_id.clone(),
                candidate,
                diff,
            })
        },
    )
}

pub fn select_import_agent_candidate_v2(
    state: State<'_, AppState>,
    request: SelectImportAgentCandidateRequest,
) -> Result<AgentCandidateActionResult, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let canonical_project_identity = crate::services::project_identity(&context.root)
                .map_err(|error| BackendError::new("PROJECT_IDENTITY_FAILED", error, true, false))?
                .canonical_identity_key;
            let (item, batch) = state.blocking_work.run_project_git_blocking(
                canonical_project_identity,
                None,
                || {
                    AgentCandidateService::new(
                        &state.import_v2_service,
                        &state.file_store,
                        &state.task_service,
                    )
                    .select_candidate_and_finalize_exact_duplicate(
                        permit,
                        &state.git_service,
                        &request.session_id,
                        &request.item_id,
                        &request.candidate_id,
                        request.merged_markdown.as_deref(),
                        request.expected_current_wiki_sha256.as_deref(),
                        state
                            .file_store
                            .exists(context, RESTRICTED_CONTENT_ACK_PATH),
                    )
                },
            )?;
            Ok(AgentCandidateActionResult {
                project_id: request.project_id.clone(),
                session_id: request.session_id.clone(),
                item_id: request.item_id.clone(),
                candidate_id: request.candidate_id.clone(),
                item,
                completion: batch.and_then(|batch| batch.completion),
            })
        },
    )
}

pub fn discard_import_agent_candidate_v2(
    state: State<'_, AppState>,
    request: DiscardImportAgentCandidateRequest,
) -> Result<AgentCandidateActionResult, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _context| {
            let item = AgentCandidateService::new(
                &state.import_v2_service,
                &state.file_store,
                &state.task_service,
            )
            .discard_candidate_authorized(
                permit,
                &request.session_id,
                &request.item_id,
                &request.candidate_id,
            )?;
            Ok(AgentCandidateActionResult {
                project_id: request.project_id.clone(),
                session_id: request.session_id.clone(),
                item_id: request.item_id.clone(),
                candidate_id: request.candidate_id.clone(),
                item,
                completion: None,
            })
        },
    )
}

pub fn start_import_agent_assistance_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentInvocationRequest,
) -> Result<BackendTask, BackendError> {
    let task = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _context| {
            AgentAssistanceService::new(
                &state.import_v2_service,
                &state.file_store,
                &state.settings_service,
                &state.agent_service,
                &state.task_service,
            )
            .start_local(
                permit,
                &request.session_id,
                &request.item_id,
                request.trigger,
                request.agent_kind,
            )
        },
    )?;
    let task_id = task.id.clone();
    let coordinator = state.blocking_work.clone();
    let cancellation = state
        .task_service
        .get_cancellation_token(&task_id)
        .unwrap_or_default();
    let failure_app = app.clone();
    let failure_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let worker_result = coordinator
            .run_cancellable(BlockingWorkClass::HeavyIo, cancellation, move || {
                let state = app.state::<AppState>();
                let result = state
                    .resolve_project_context(&request.project_id, &request.project_root_path)
                    .and_then(|context| {
                        let execution_lease =
                            state.begin_project_external_task(&context, &task_id)?;
                        let run = AgentAssistanceService::new(
                            &state.import_v2_service,
                            &state.file_store,
                            &state.settings_service,
                            &state.agent_service,
                            &state.task_service,
                        )
                        .run_local(
                            &state,
                            &execution_lease,
                            &context,
                            &request.session_id,
                            &request.item_id,
                            &task_id,
                            request.trigger,
                            request.agent_kind,
                        );
                        run.and_then(|()| {
                            state.require_current_execution_epoch(&context, &execution_lease)?;
                            state.with_current_project_write_access(
                                &request.project_id,
                                &request.project_root_path,
                                |permit, _current| {
                                    AgentCandidateService::new(
                                        &state.import_v2_service,
                                        &state.file_store,
                                        &state.task_service,
                                    )
                                    .accept_staged_output_authorized(
                                        permit,
                                        &request.session_id,
                                        &request.item_id,
                                        &task_id,
                                    )
                                    .map(|_| ())
                                },
                            )
                        })
                    });
                if let Err(error) = result {
                    let status = state
                        .task_service
                        .get_task(&task_id)
                        .map(|task| task.status);
                    if status == Some(crate::models::task::TaskStatus::Succeeded) {
                        let _ = state.task_service.append_log(
                            &task_id,
                            crate::tasks::task_model::LogLevel::Warn,
                            "Agent output was staged but candidate validation failed; the deterministic result was preserved.".into(),
                        );
                    }
                    if !state.task_service.is_cancelled(&task_id)
                        && !matches!(
                            status,
                            Some(crate::models::task::TaskStatus::Failed)
                                | Some(crate::models::task::TaskStatus::Succeeded)
                                | Some(crate::models::task::TaskStatus::Cancelled)
                        )
                    {
                        let safe = BackendError::new(
                            error.code,
                            "Local Agent assistance failed before candidate validation.",
                            true,
                            true,
                        );
                        let _ = state.task_service.set_error(&task_id, safe);
                        let _ = state.task_service.transition_status(
                            &task_id,
                            crate::models::task::TaskStatus::Failed,
                        );
                    }
                }
                Ok(())
            })
            .await;
        if let Err(error) = worker_result {
            let state = failure_app.state::<AppState>();
            if state.task_service.is_cancelled(&failure_task_id) {
                let _ = state.task_service.finalize_cancellation(&failure_task_id);
            } else {
                let _ = state.task_service.set_error(&failure_task_id, error);
                let _ = state
                    .task_service
                    .transition_status(&failure_task_id, crate::models::task::TaskStatus::Failed);
            }
        }
    });
    Ok(task)
}

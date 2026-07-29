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
};

#[tauri::command]
pub fn accept_import_agent_candidate_v2(
    state: State<'_, AppState>,
    request: AcceptImportAgentCandidateRequest,
) -> Result<AgentCandidateView, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let service = AgentCandidateService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.task_service,
    );
    let candidate = service.accept_staged_output(
        &context,
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
        project_id: request.project_id,
        session_id: request.session_id,
        item_id: request.item_id,
        candidate,
        diff,
    })
}

#[tauri::command]
pub fn select_import_agent_candidate_v2(
    state: State<'_, AppState>,
    request: SelectImportAgentCandidateRequest,
) -> Result<AgentCandidateActionResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let (item, batch) = AgentCandidateService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.task_service,
    )
    .select_candidate_and_finalize_exact_duplicate(
        &context,
        &state.git_service,
        &request.session_id,
        &request.item_id,
        &request.candidate_id,
        request.merged_markdown.as_deref(),
        request.expected_current_wiki_sha256.as_deref(),
        state
            .file_store
            .exists(&context, RESTRICTED_CONTENT_ACK_PATH),
    )?;
    Ok(AgentCandidateActionResult {
        project_id: request.project_id,
        session_id: request.session_id,
        item_id: request.item_id,
        candidate_id: request.candidate_id,
        item,
        completion: batch.and_then(|batch| batch.completion),
    })
}

#[tauri::command]
pub fn discard_import_agent_candidate_v2(
    state: State<'_, AppState>,
    request: DiscardImportAgentCandidateRequest,
) -> Result<AgentCandidateActionResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let item = AgentCandidateService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.task_service,
    )
    .discard_candidate(
        &context,
        &request.session_id,
        &request.item_id,
        &request.candidate_id,
    )?;
    Ok(AgentCandidateActionResult {
        project_id: request.project_id,
        session_id: request.session_id,
        item_id: request.item_id,
        candidate_id: request.candidate_id,
        item,
        completion: None,
    })
}

#[tauri::command]
pub fn start_import_agent_assistance_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentInvocationRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let service = AgentAssistanceService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.settings_service,
        &state.agent_service,
        &state.task_service,
    );
    let task = service.start_local(
        &context,
        &request.session_id,
        &request.item_id,
        request.trigger,
        request.agent_kind,
    )?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = state
            .resolve_project_context(&request.project_id, &request.project_root_path)
            .and_then(|context| {
                let run = AgentAssistanceService::new(
                    &state.import_v2_service,
                    &state.file_store,
                    &state.settings_service,
                    &state.agent_service,
                    &state.task_service,
                )
                .run_local(
                    &context,
                    &request.session_id,
                    &request.item_id,
                    &task_id,
                    request.trigger,
                    request.agent_kind,
                );
                run.and_then(|()| {
                    AgentCandidateService::new(
                        &state.import_v2_service,
                        &state.file_store,
                        &state.task_service,
                    )
                    .accept_staged_output(&context, &request.session_id, &request.item_id, &task_id)
                    .map(|_| ())
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
                let _ = state
                    .task_service
                    .transition_status(&task_id, crate::models::task::TaskStatus::Failed);
            }
        }
    });
    Ok(task)
}

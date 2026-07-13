use tauri::{AppHandle, Manager, State};

use crate::{
    app_state::AppState,
    errors::BackendError,
    models::import_v2_agent::{
        AgentAssistancePolicy, AgentInvocationRequest, GetImportAgentPolicyRequest,
        SetImportAgentPolicyRequest,
    },
    models::task::BackendTask,
    services::import_v2::agent_assistance::AgentAssistanceService,
};

#[tauri::command]
pub fn get_import_agent_policy_v2(
    state: State<'_, AppState>,
    request: GetImportAgentPolicyRequest,
) -> Result<AgentAssistancePolicy, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.settings_service.get_import_agent_policy(&context)
}

#[tauri::command]
pub fn set_import_agent_policy_v2(
    state: State<'_, AppState>,
    request: SetImportAgentPolicyRequest,
) -> Result<AgentAssistancePolicy, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.settings_service.set_import_agent_policy(
        &context,
        request.policy,
        request.local_agent_kind,
    )
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
        AgentAssistanceService::bundled_skill_path(),
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
                AgentAssistanceService::new(
                    &state.import_v2_service,
                    &state.file_store,
                    &state.settings_service,
                    &state.agent_service,
                    &state.task_service,
                    AgentAssistanceService::bundled_skill_path(),
                )
                .run_local(
                    &context,
                    &request.session_id,
                    &request.item_id,
                    &task_id,
                    request.trigger,
                    request.agent_kind,
                )
            });
        if let Err(error) = result {
            let status = state
                .task_service
                .get_task(&task_id)
                .map(|task| task.status);
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

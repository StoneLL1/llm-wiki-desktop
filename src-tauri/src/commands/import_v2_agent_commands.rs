use tauri::State;

use crate::{
    app_state::AppState,
    errors::BackendError,
    models::import_v2_agent::{
        AgentAssistancePolicy, GetImportAgentPolicyRequest, SetImportAgentPolicyRequest,
    },
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

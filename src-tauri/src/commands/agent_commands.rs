use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentConfig, AgentInfo, SetDefaultAgentRequest};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default)]
    pub force_refresh: bool,
}

#[tauri::command]
pub async fn detect_agents(
    app: AppHandle,
    request: AgentProjectRequest,
) -> Result<Vec<AgentInfo>, BackendError> {
    let coordinator = app.state::<AppState>().blocking_work.clone();
    coordinator
        .run_project_facts_agent(move || {
            let state = app.state::<AppState>();
            let context =
                state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            let config = crate::services::AgentService::load_config(&context)?;
            if request.force_refresh {
                state.agent_service.invalidate_workflow_route_cache();
            }
            Ok(state.agent_service.detect_agents(config.default_agent))
        })
        .await
}

#[tauri::command]
pub fn get_agent_config(
    state: State<'_, AppState>,
    request: AgentProjectRequest,
) -> Result<AgentConfig, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    crate::services::AgentService::load_config(&context)
}

#[tauri::command]
pub fn set_default_agent(
    state: State<'_, AppState>,
    request: SetDefaultAgentRequest,
) -> Result<AgentConfig, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let config = state
                .settings_service
                .save_agent_default(context, request.agent)?;
            Ok(config)
        },
    )
}

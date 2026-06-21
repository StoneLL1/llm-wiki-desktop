use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentConfig, AgentInfo, SetDefaultAgentRequest};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn detect_agents(
    state: State<'_, AppState>,
    request: AgentProjectRequest,
) -> Result<Vec<AgentInfo>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let config = crate::services::AgentService::load_config(&context)?;
    Ok(state.agent_service.detect_agents(config.default_agent))
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
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let config = AgentConfig {
        default_agent: request.agent,
    };
    crate::services::AgentService::save_config(&context, &config)?;
    Ok(config)
}

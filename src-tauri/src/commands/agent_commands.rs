use std::path::PathBuf;

use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentConfig, AgentInfo, SetDefaultAgentRequest};
use crate::models::paths::ProjectContext;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

fn context(request: &AgentProjectRequest) -> ProjectContext {
    ProjectContext::new(
        request.project_id.clone(),
        PathBuf::from(&request.project_root_path),
    )
}

#[tauri::command]
pub fn detect_agents(
    state: State<'_, AppState>,
    request: AgentProjectRequest,
) -> Result<Vec<AgentInfo>, BackendError> {
    let config = crate::services::AgentService::load_config(&context(&request))?;
    Ok(state.agent_service.detect_agents(config.default_agent))
}

#[tauri::command]
pub fn get_agent_config(request: AgentProjectRequest) -> Result<AgentConfig, BackendError> {
    crate::services::AgentService::load_config(&context(&request))
}

#[tauri::command]
pub fn set_default_agent(request: SetDefaultAgentRequest) -> Result<AgentConfig, BackendError> {
    let context = ProjectContext::new(request.project_id, PathBuf::from(request.project_root_path));
    let config = AgentConfig {
        default_agent: request.agent,
    };
    crate::services::AgentService::save_config(&context, &config)?;
    Ok(config)
}

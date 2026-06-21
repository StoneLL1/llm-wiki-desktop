use std::path::PathBuf;

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::project::{
    AppSummary, CreateProjectRequest, OpenProjectKind, OpenProjectRequest, OpenProjectResponse,
    ProjectSummary, RecentProject, RememberRecentProjectRequest,
};
use crate::utils::time_utils::now_rfc3339;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn get_app_summary(_state: State<'_, AppState>) -> AppSummary {
    AppSummary {
        name: "LLM Wiki Desktop".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    request: CreateProjectRequest,
) -> Result<ProjectSummary, BackendError> {
    let summary = state.project_service.create_project(
        &request.root_path,
        &request.name,
        request.template,
    )?;
    state.project_registry.register(
        summary.project_id.clone(),
        &PathBuf::from(&summary.root_path),
    )?;

    let recents = state
        .project_service
        .remember_recent_project(RecentProject {
            project_id: summary.project_id.clone(),
            name: summary.name.clone(),
            root_path: summary.root_path.clone(),
            template: summary.template,
            opened_at: now_rfc3339(),
        })?;
    let _ = recents;
    Ok(summary)
}

#[tauri::command]
pub fn open_project(
    state: State<'_, AppState>,
    request: OpenProjectRequest,
) -> Result<OpenProjectResponse, BackendError> {
    let outcome = state.project_service.open_project(&request.path)?;
    if let Some(summary) = outcome.summary.as_ref() {
        if outcome.kind == OpenProjectKind::Opened {
            let context = crate::models::paths::ProjectContext::new(
                summary.project_id.clone(),
                PathBuf::from(&summary.root_path),
            );
            state
                .git_service
                .initialize_repository(&context, "Initialize existing wiki project")?;
            state.project_registry.register(
                summary.project_id.clone(),
                &PathBuf::from(&summary.root_path),
            )?;
            state
                .project_service
                .remember_recent_project(RecentProject {
                    project_id: summary.project_id.clone(),
                    name: summary.name.clone(),
                    root_path: summary.root_path.clone(),
                    template: summary.template,
                    opened_at: now_rfc3339(),
                })?;
        }
    }
    if let Some(pending_action) = outcome.pending_action.as_ref() {
        let execution = state
            .project_service
            .folder_initialization_execution(&PathBuf::from(&request.path), pending_action)?;
        state
            .confirmation_registry
            .register_with_execution(pending_action.clone(), Some(execution))?;
    }
    Ok(outcome)
}

#[tauri::command]
pub fn scan_project(
    state: State<'_, AppState>,
    request: ScanProjectRequest,
) -> Result<ProjectSummary, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(state.project_service.scan_project(&context, None))
}

#[tauri::command]
pub fn list_recent_projects(
    state: State<'_, AppState>,
) -> Result<Vec<RecentProject>, BackendError> {
    state.project_service.list_recent_projects()
}

#[tauri::command]
pub fn remember_recent_project(
    state: State<'_, AppState>,
    request: RememberRecentProjectRequest,
) -> Result<Vec<RecentProject>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.root_path)?;
    state
        .project_service
        .remember_recent_project(RecentProject {
            project_id: request.project_id,
            name: request.name,
            root_path: context.root.to_string_lossy().into_owned(),
            template: request.template,
            opened_at: now_rfc3339(),
        })
}

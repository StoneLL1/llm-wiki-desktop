use std::path::PathBuf;

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::project::{
    AppSummary, CreateProjectRequest, OpenProjectKind, OpenProjectRequest, OpenProjectResponse,
    ProjectSummary, RecentProject, RememberRecentProjectRequest,
};
use crate::utils::time_utils::now_rfc3339;

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
    let summary = state
        .project_service
        .create_project(&request.root_path, &request.name, request.template)?;

    let recents = state.project_service.remember_recent_project(RecentProject {
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
    Ok(outcome)
}

#[tauri::command]
pub fn scan_project(path: String) -> Result<ProjectSummary, BackendError> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(BackendError::new(
            "PROJECT_NOT_FOUND",
            "The selected project folder does not exist.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": path })));
    }
    let context = ProjectContext::new(uuid::Uuid::new_v4().to_string(), root.canonicalize().unwrap_or(root));
    let service = crate::services::ProjectService::default();
    Ok(service.scan_project(&context, None))
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
    state.project_service.remember_recent_project(RecentProject {
        project_id: request.project_id,
        name: request.name,
        root_path: request.root_path,
        template: request.template,
        opened_at: now_rfc3339(),
    })
}

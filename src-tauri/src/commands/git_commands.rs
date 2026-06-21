use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::git::{CheckpointPurpose, GitCheckpoint, GitDiff, GitRepositoryStatus};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRepositoryRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub initial_message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCheckpointRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub purpose: CheckpointPurpose,
    pub message: String,
}

#[tauri::command]
pub fn git_status(
    state: State<'_, AppState>,
    request: GitProjectRequest,
) -> Result<GitRepositoryStatus, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.git_service.repository_status(&context)
}

#[tauri::command]
pub fn initialize_git_repository(
    state: State<'_, AppState>,
    request: InitializeRepositoryRequest,
) -> Result<GitRepositoryStatus, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .git_service
        .initialize_repository(&context, &request.initial_message)
}

#[tauri::command]
pub fn create_git_checkpoint(
    state: State<'_, AppState>,
    request: CreateCheckpointRequest,
) -> Result<GitCheckpoint, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .git_service
        .create_checkpoint(&context, request.purpose, &request.message)
}

#[tauri::command]
pub fn git_diff_markdown(
    state: State<'_, AppState>,
    request: GitProjectRequest,
) -> Result<GitDiff, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.git_service.diff_markdown(&context)
}

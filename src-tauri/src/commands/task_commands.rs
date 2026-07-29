use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::task::{BackendTask, TaskActivity, TaskStatus, TaskType};
use crate::tasks::task_model::LogLine;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub task_type: TaskType,
    pub project_id: Option<String>,
    pub title: String,
    pub cancellable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskByIdRequest {
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksRequest {
    pub status_filter: Option<TaskStatus>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetActiveProjectRequest {
    pub project_id: Option<String>,
    pub root_path: Option<String>,
}

#[tauri::command]
pub fn create_task(
    state: State<'_, AppState>,
    request: CreateTaskRequest,
) -> Result<BackendTask, BackendError> {
    let task = state.task_service.create_task(
        request.task_type,
        request.project_id,
        request.title,
        request.cancellable,
    );
    Ok(task)
}

#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    request: ListTasksRequest,
) -> Result<Vec<BackendTask>, BackendError> {
    Ok(state.task_service.list_tasks(request.status_filter))
}

#[tauri::command]
pub fn get_task(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Option<BackendTask>, BackendError> {
    Ok(state.task_service.get_task(&request.task_id))
}

#[tauri::command]
pub fn cancel_task(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<BackendTask, BackendError> {
    let result = if state
        .task_service
        .get_task(&request.task_id)
        .is_some_and(|task| {
            matches!(
                task.task_type,
                TaskType::LlmRequest | TaskType::SourceAiOrganize
            )
        }) {
        state.task_service.request_cancel(&request.task_id)
    } else {
        state.task_service.cancel_task(&request.task_id)
    };
    result.map_err(|msg| BackendError::new("TASK_CANCEL_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn get_task_logs(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Vec<LogLine>, BackendError> {
    state
        .task_service
        .get_logs(&request.task_id)
        .map_err(|msg| BackendError::new("TASK_LOGS_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn get_task_activities(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Vec<TaskActivity>, BackendError> {
    state
        .task_service
        .get_activities(&request.task_id)
        .map_err(|msg| BackendError::new("TASK_ACTIVITIES_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn remove_completed_tasks(state: State<'_, AppState>) -> Result<usize, BackendError> {
    Ok(state.task_service.remove_completed())
}

/// Bind (or clear) the active project root for task persistence. When a root is set,
/// previously-persisted tasks are recovered from `<root>/.app/tasks/` and returned.
#[tauri::command]
pub fn set_active_project(
    state: State<'_, AppState>,
    request: SetActiveProjectRequest,
) -> Result<Vec<BackendTask>, BackendError> {
    let root = match (request.project_id.as_deref(), request.root_path.as_deref()) {
        (Some(project_id), Some(root_path)) => {
            Some(state.resolve_project_context(project_id, root_path)?.root)
        }
        (None, None) => None,
        _ => {
            return Err(BackendError::new(
                "PROJECT_CONTEXT_MISMATCH",
                "Project id and root must be supplied together.",
                true,
                true,
            ))
        }
    };
    state
        .task_service
        .set_project_root(root)
        .map_err(|msg| BackendError::new("TASK_RECOVERY_FAILED", &msg, true, false))
}

use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::task::{BackendTask, TaskActivity, TaskStatus, TaskType};
use crate::models::workflow::WorkflowRunPage;
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
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksRequest {
    pub project_id: String,
    pub project_root_path: String,
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
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(state
        .task_service
        .list_tasks_for_root(&context.root, request.status_filter))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn get_task(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Option<BackendTask>, BackendError> {
    require_task_project(&state, &request)?;
    Ok(state.task_service.get_task(&request.task_id))
}

#[tauri::command]
pub fn cancel_task(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<BackendTask, BackendError> {
    require_task_project(&state, &request)?;
    if let Some(run) = state.task_service.get_workflow_run(&request.task_id) {
        let was_waiting = run.display_status
            == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation;
        state
            .workflow_service
            .coordinator
            .cancel(&state.task_service, &request.task_id)
            .map_err(|msg| BackendError::new("TASK_CANCEL_FAILED", &msg, true, false))?;
        if was_waiting {
            if let Some(action) = run.pending_action {
                let _ = state.confirmation_registry.confirm(
                    &action.id,
                    crate::models::confirmation::ConfirmationStatus::Cancelled,
                );
            }
            if let Err(error) = crate::services::discard_update_wiki_candidate(&request.task_id) {
                let _ = state.task_service.append_log(
                    &request.task_id,
                    crate::tasks::task_model::LogLevel::Warn,
                    format!(
                        "Workflow was cancelled, but candidate cleanup needs attention: {}",
                        error.message
                    ),
                );
            }
            if let Err(error) =
                crate::services::discard_generate_content_candidate(&request.task_id)
            {
                let _ = state.task_service.append_log(
                    &request.task_id,
                    crate::tasks::task_model::LogLevel::Warn,
                    format!(
                        "Workflow was cancelled, but generated artifact cleanup needs attention: {}",
                        error.message
                    ),
                );
            }
            let (_, next) = state
                .workflow_service
                .coordinator
                .finish_cancelled_and_claim_next(&state.task_service, &request.task_id)
                .map_err(|msg| BackendError::new("TASK_CANCEL_FAILED", &msg, true, false))?;
            if let Some(next) = next {
                state.workflow_service.dispatch_claimed_run(&next)?;
            }
        }
        return state
            .task_service
            .get_task(&request.task_id)
            .ok_or_else(|| BackendError::new("TASK_NOT_FOUND", "Task not found.", false, false));
    }
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
    require_task_project(&state, &request)?;
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
    require_task_project(&state, &request)?;
    state
        .task_service
        .get_activities(&request.task_id)
        .map_err(|msg| BackendError::new("TASK_ACTIVITIES_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn remove_completed_tasks(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<usize, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(state.task_service.remove_completed_for_root(&context.root))
}

fn require_task_project(state: &AppState, request: &TaskByIdRequest) -> Result<(), BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if !state
        .task_service
        .task_belongs_to_root(&request.task_id, &context.root)
    {
        return Err(BackendError::new(
            "TASK_PROJECT_MISMATCH",
            "Task does not belong to the asserted project.",
            true,
            true,
        ));
    }
    Ok(())
}

/// Bind (or clear) the active project root for task persistence. When a root is set,
/// previously-persisted tasks are recovered from `<root>/.app/tasks/` and returned.
#[tauri::command]
pub fn set_active_project(
    state: State<'_, AppState>,
    request: SetActiveProjectRequest,
) -> Result<Vec<BackendTask>, BackendError> {
    let project_context = match (request.project_id.as_deref(), request.root_path.as_deref()) {
        (Some(project_id), Some(root_path)) => {
            Some(state.resolve_project_context(project_id, root_path)?)
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
    match project_context {
        Some(context) => {
            let tasks = state
                .task_service
                .set_project_context(
                    context.project_id.clone(),
                    context.root.clone(),
                    context.app_dir.join("tasks"),
                )
                .map_err(|msg| BackendError::new("TASK_RECOVERY_FAILED", &msg, true, false))?;
            for task in &tasks {
                let Some(run) = state.task_service.get_workflow_run(&task.id) else {
                    continue;
                };
                if run.kind == crate::models::workflow::WorkflowKind::GenerateContent
                    && run.display_status
                        == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
                {
                    crate::services::restore_generate_content_confirmation(
                        &context,
                        &run,
                        &state.task_service,
                        &state.confirmation_registry,
                    )?;
                }
            }
            Ok(tasks)
        }
        None => state
            .task_service
            .set_project_root(None)
            .map_err(|msg| BackendError::new("TASK_RECOVERY_FAILED", &msg, true, false)),
    }
}

#[tauri::command]
pub fn continue_queued_workflows(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<WorkflowRunPage, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let identity = crate::services::project_identity(&context.root)
        .map_err(|message| BackendError::new("WORKFLOW_IDENTITY_FAILED", &message, true, false))?;
    let runs = state
        .workflow_service
        .coordinator
        .continue_queued(
            &state.task_service,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )
        .map_err(|message| BackendError::new("WORKFLOW_CONTINUE_FAILED", &message, true, false))?;
    Ok(WorkflowRunPage {
        runs,
        next_cursor: None,
    })
}

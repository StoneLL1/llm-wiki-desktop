use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::{
    CommitImportSessionRequest, ImportInput, ImportResourceMode, ImportSession,
};
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};

macro_rules! request {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name { $(pub $field: $ty),* }
    };
}
request!(CreateImportSessionV2Request {
    project_id: String,
    project_root_path: String,
    resource_mode: ImportResourceMode
});
request!(GetImportSessionV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String
});
request!(AddImportItemsV2Request { project_id: String, project_root_path: String, session_id: String, inputs: Vec<ImportInput> });
request!(SetImportItemSelectionV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    selected: bool
});
request!(StartImportItemsV2Request { project_id: String, project_root_path: String, session_id: String, item_ids: Vec<String> });

#[tauri::command]
pub fn create_import_session_v2(
    state: State<'_, AppState>,
    request: CreateImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .create_session(&context, &state.file_store, request.resource_mode)
}
#[tauri::command]
pub fn get_import_session_v2(
    state: State<'_, AppState>,
    request: GetImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}
#[tauri::command]
pub fn add_import_items_v2(
    state: State<'_, AppState>,
    request: AddImportItemsV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.add_inputs(
        &context,
        &state.file_store,
        &request.session_id,
        request.inputs,
    )
}
#[tauri::command]
pub fn set_import_item_selection_v2(
    state: State<'_, AppState>,
    request: SetImportItemSelectionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.set_item_selected(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
        request.selected,
    )?;
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

#[tauri::command]
pub fn start_import_items_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartImportItemsV2Request,
) -> Result<Vec<BackendTask>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let unique_ids: HashSet<&str> = request.item_ids.iter().map(String::as_str).collect();
    if unique_ids.len() != request.item_ids.len() {
        return Err(task_error("Import item ids must be unique."));
    }
    for item_id in &request.item_ids {
        if !session.items.iter().any(|item| item.item_id == *item_id) {
            return Err(task_error("Import item was not found."));
        }
    }
    let prepared = prepare_all(
        request.item_ids,
        |item_id| {
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == item_id)
                .expect("all item ids were validated before task creation");
            state
                .task_service
                .create_project_task(
                    TaskType::Import,
                    request.project_id.clone(),
                    context.root.clone(),
                    format!("Import {}", item.input.display_name),
                    true,
                )
                .map_err(|error| task_error(&error))
        },
        |task| {
            state
                .task_service
                .discard_unstarted_tasks(std::slice::from_ref(&task.id))
                .map_err(|error| task_error(&error))
        },
    )?;
    let mut tasks = Vec::with_capacity(prepared.len());
    for (item_id, task) in prepared {
        let (app, project_id, root, session_id, task_id) = (
            app.clone(),
            request.project_id.clone(),
            request.project_root_path.clone(),
            request.session_id.clone(),
            task.id.clone(),
        );
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let result = state
                .resolve_project_context(&project_id, &root)
                .and_then(|context| {
                    state.import_v2_service.run_item(
                        &context,
                        &state.file_store,
                        &state.task_service,
                        &session_id,
                        &item_id,
                        &task_id,
                    )
                });
            if let Err(error) = result {
                fail_task_unless_cancelled(&state, &task_id, error);
            }
        });
        tasks.push(task);
    }
    Ok(tasks)
}

fn prepare_all<T>(
    item_ids: Vec<String>,
    mut create: impl FnMut(&str) -> Result<T, BackendError>,
    mut rollback: impl FnMut(&T) -> Result<(), BackendError>,
) -> Result<Vec<(String, T)>, BackendError> {
    let mut prepared = Vec::with_capacity(item_ids.len());
    for item_id in item_ids {
        match create(&item_id) {
            Ok(task) => prepared.push((item_id, task)),
            Err(error) => {
                let mut rollback_error = None;
                for (_, task) in &prepared {
                    if let Err(error) = rollback(task) {
                        rollback_error.get_or_insert(error);
                    }
                }
                return Err(rollback_error.unwrap_or(error));
            }
        }
    }
    Ok(prepared)
}

#[tauri::command]
pub fn confirm_import_session_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CommitImportSessionRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root,
            "Confirm import session".into(),
            true,
        )
        .map_err(|error| task_error(&error))?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = (|| -> Result<TaskResult, BackendError> {
            let context =
                state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Running)
                .map_err(|error| task_error(&error))?;
            let batch = state.import_v2_service.commit_items_cancellable(
                &context,
                &state.file_store,
                &state.git_service,
                &request,
                || state.task_service.is_cancelled(&task_id),
            )?;
            Ok(TaskResult {
                summary: format!(
                    "Committed {} import item(s); {} failed.",
                    batch.committed_count, batch.failed_count
                ),
                affected_paths: vec![format!(".app/import-history/{}.json", batch.batch_id)],
                reference: None,
                pending_action: None,
            })
        })();
        match result {
            Ok(result) => {
                let _ = state.task_service.set_result(&task_id, result);
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Succeeded);
            }
            Err(error) => {
                fail_task_unless_cancelled(&state, &task_id, error);
            }
        }
    });
    Ok(task)
}

fn task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_TASK_FAILED", message, true, false)
}

fn fail_task_unless_cancelled(state: &AppState, task_id: &str, error: BackendError) {
    let _ = state.task_service.set_error(task_id, error);
    if !matches!(
        state.task_service.get_task(task_id).map(|task| task.status),
        Some(TaskStatus::Cancelled)
    ) {
        let _ = state
            .task_service
            .transition_status(task_id, TaskStatus::Failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::ImportInputKind;

    #[test]
    fn add_items_request_uses_ids_and_inputs_not_target_paths() {
        let request = AddImportItemsV2Request {
            project_id: "p1".into(),
            project_root_path: "fixture/project".into(),
            session_id: "s1".into(),
            inputs: vec![ImportInput {
                kind: ImportInputKind::File,
                display_name: "a.pdf".into(),
                locator: "fixture/in/a.pdf".into(),
                normalized_locator: None,
                source_identity: None,
            }],
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["sessionId"], "s1");
        assert!(value.get("targetPath").is_none());
        assert!(value.get("wikiPath").is_none());
    }

    #[test]
    fn task_preparation_rolls_back_every_created_task_on_later_failure() {
        let mut attempts = 0;
        let mut rolled_back = Vec::new();
        let result = prepare_all(
            ["first", "second", "third"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            |_| {
                attempts += 1;
                if attempts == 3 {
                    Err(task_error("injected persistence failure"))
                } else {
                    Ok(attempts)
                }
            },
            |task| {
                rolled_back.push(*task);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert_eq!(rolled_back, vec![1, 2]);
    }
}

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::ImportSession;
use crate::models::import_v2_file::FileScanPolicy;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::import_v2::file_discovery::{new_import_inputs, FileDiscoveryService};
use crate::tasks::task_model::LogLevel;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddImportPathsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub source_paths: Vec<String>,
}

/// Starts durable discovery work and returns immediately. The existing
/// synchronous command remains for compatibility with older frontends.
#[tauri::command]
pub fn start_add_import_paths_v2(
    app: AppHandle,
    request: AddImportPathsV2Request,
) -> Result<BackendTask, BackendError> {
    let state = app.state::<AppState>();
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root.clone(),
            "Discover import files".into(),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let run = || -> Result<(), BackendError> {
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Running)
                .map_err(task_error)?;
            state
                .task_service
                .append_log(&task_id, LogLevel::Info, "Scanning selected paths".into())
                .map_err(task_error)?;
            let context =
                state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            let session = state.import_v2_service.load_session(
                &context,
                &state.file_store,
                &request.session_id,
            )?;
            let roots = request
                .source_paths
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let scan = FileDiscoveryService::default().scan(
                &context,
                &roots,
                FileScanPolicy::default(),
                |batch| {
                    let _ = state.task_service.update_progress(
                        &task_id,
                        batch.len() as u64,
                        None,
                        Some("Discovering files".into()),
                    );
                },
                || state.task_service.is_cancelled(&task_id),
            )?;
            if state.task_service.is_cancelled(&task_id) {
                return Ok(());
            }
            let skipped = scan.skipped.len();
            let inputs = new_import_inputs(&session, scan.files);
            let added = inputs.len();
            if !inputs.is_empty() {
                state.import_v2_service.add_inputs(
                    &context,
                    &state.file_store,
                    &request.session_id,
                    inputs,
                )?;
            }
            state
                .task_service
                .append_log(
                    &task_id,
                    LogLevel::Info,
                    format!("Added {added} files; skipped {skipped}"),
                )
                .map_err(task_error)?;
            state
                .task_service
                .set_result(
                    &task_id,
                    TaskResult {
                        summary: format!("Added {added} files; skipped {skipped}."),
                        affected_paths: Vec::new(),
                        reference: None,
                        pending_action: None,
                    },
                )
                .map_err(task_error)?;
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Succeeded)
                .map_err(task_error)?;
            Ok(())
        };
        if let Err(error) = run() {
            let _ = state.task_service.set_error(&task_id, error);
            let _ = state
                .task_service
                .transition_status(&task_id, TaskStatus::Failed);
        }
    });
    Ok(task)
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_SERVICE", message, true, false)
}

#[tauri::command]
pub fn add_import_paths_v2(
    state: State<'_, AppState>,
    request: AddImportPathsV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let roots = request
        .source_paths
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let scan = FileDiscoveryService::default().scan(
        &context,
        &roots,
        FileScanPolicy::default(),
        |_| {},
        || false,
    )?;
    let inputs = new_import_inputs(&session, scan.files);
    if inputs.is_empty() {
        return Ok(session);
    }
    state
        .import_v2_service
        .add_inputs(&context, &state.file_store, &request.session_id, inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_has_sources_but_no_target_paths_or_policy_override() {
        let value = serde_json::to_value(AddImportPathsV2Request {
            project_id: "p".into(),
            project_root_path: "root".into(),
            session_id: "s".into(),
            source_paths: vec!["a.md".into()],
        })
        .unwrap();
        assert_eq!(value["sourcePaths"][0], "a.md");
        assert!(value.get("targetPath").is_none());
        assert!(value.get("policy").is_none());
    }

    #[test]
    fn background_request_is_the_same_narrow_path_contract() {
        let value = serde_json::to_value(AddImportPathsV2Request {
            project_id: "p".into(),
            project_root_path: "root".into(),
            session_id: "s".into(),
            source_paths: vec!["folder".into(), "file.pdf".into()],
        })
        .unwrap();
        assert_eq!(value["sourcePaths"].as_array().unwrap().len(), 2);
        assert!(value.get("install").is_none());
    }
}

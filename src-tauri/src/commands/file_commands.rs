use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{ConfirmationExecution, ConfirmationStatus, ConfirmedAction};
use crate::services::import_v2::source_lifecycle::{
    reject_generic_source_create, reject_generic_source_path,
};
use crate::services::WriteMode;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteMarkdownRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
    pub contents: String,
    pub mode: WriteMode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteJsonRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
    pub value: serde_json::Value,
    pub mode: WriteMode,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHashResponse {
    pub relative_path: String,
    pub hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPendingActionRequest {
    pub action_id: String,
    pub status: ConfirmationStatus,
}

#[tauri::command]
pub fn read_markdown_file(
    state: State<'_, AppState>,
    request: ProjectFileRequest,
) -> Result<String, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .file_store
        .read_markdown(&context, &request.relative_path)
}

#[tauri::command]
pub fn write_markdown_file(
    state: State<'_, AppState>,
    request: WriteMarkdownRequest,
) -> Result<FileHashResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    reject_generic_source_path(&context, &state.file_store, &request.relative_path)?;
    reject_generic_source_create(&request.relative_path, None, Some(&request.contents))?;
    state.file_store.write_markdown_checked(
        &context,
        &request.relative_path,
        &request.contents,
        request.mode,
    )?;
    let hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    Ok(FileHashResponse {
        relative_path: request.relative_path,
        hash,
    })
}

#[tauri::command]
pub fn write_json_file(
    state: State<'_, AppState>,
    request: WriteJsonRequest,
) -> Result<FileHashResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.file_store.write_json_atomic_checked(
        &context,
        &request.relative_path,
        &request.value,
        request.mode,
    )?;
    let hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    Ok(FileHashResponse {
        relative_path: request.relative_path,
        hash,
    })
}

#[tauri::command]
pub fn get_file_hash(
    state: State<'_, AppState>,
    request: ProjectFileRequest,
) -> Result<FileHashResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    Ok(FileHashResponse {
        relative_path: request.relative_path,
        hash,
    })
}

#[tauri::command]
pub fn confirm_pending_action(
    state: State<'_, AppState>,
    request: ConfirmPendingActionRequest,
) -> Result<ConfirmedAction, BackendError> {
    let stored = state
        .confirmation_registry
        .confirm(&request.action_id, request.status.clone())?;

    if request.status == ConfirmationStatus::Cancelled {
        return Ok(ConfirmedAction {
            action: stored.action,
            status: ConfirmationStatus::Cancelled,
            checkpoint_exists: false,
            project_summary: None,
        });
    }

    match stored.execution {
        Some(ConfirmationExecution::InitializeFolder {
            root_path,
            file_hashes,
        }) => {
            let (project_summary, checkpoint_exists) =
                state.project_service.confirm_folder_initialization(
                    &PathBuf::from(root_path),
                    &stored.action,
                    &file_hashes,
                )?;
            state.project_registry.register(
                project_summary.project_id.clone(),
                &PathBuf::from(&project_summary.root_path),
            )?;
            Ok(ConfirmedAction {
                action: stored.action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists,
                project_summary: Some(project_summary),
            })
        }
        Some(ConfirmationExecution::CompileMerge { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Compile conflicts must be handled by confirm_compile_action.",
            true,
            true,
        )),
        Some(ConfirmationExecution::LintFix { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Lint fixes must be handled by apply_lint_fix.",
            true,
            true,
        )),
        Some(ConfirmationExecution::ChatOverwrite { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Chat overwrites must be handled by save_answer_to_wiki.",
            true,
            true,
        )),
        Some(ConfirmationExecution::DeleteWikiPage {
            project_id,
            root_path,
            target_path,
            target_hash,
        }) => execute_wiki_page_delete(
            &state,
            stored.action,
            &project_id,
            &root_path,
            &target_path,
            &target_hash,
        ),
        None => Err(BackendError::new(
            "CONFIRMATION_EXECUTION_MISSING",
            "The pending action has no backend execution plan.",
            false,
            true,
        )
        .with_details(serde_json::json!({ "actionId": request.action_id }))),
    }
}

/// Execute a confirmed wiki page deletion: resolve the project context and
/// delegate to `SearchService::apply_page_delete`, which re-verifies the hash,
/// creates a scoped Git checkpoint, removes the file, and invalidates the graph
/// cache. The destructive logic lives in the (lib-available) service so it can
/// be unit-tested without the GUI feature; this wrapper only adapts the stored
/// confirmation execution to the service signature and assembles the
/// `ConfirmedAction`.
fn execute_wiki_page_delete(
    state: &AppState,
    action: crate::models::confirmation::PendingAction,
    project_id: &str,
    root_path: &str,
    target_path: &str,
    target_hash: &str,
) -> Result<ConfirmedAction, BackendError> {
    let context = state.resolve_project_context(project_id, root_path)?;
    let checkpoint_exists = state.search_service.apply_page_delete(
        &context,
        &state.git_service,
        target_path,
        target_hash,
    )?;
    Ok(ConfirmedAction {
        action,
        status: ConfirmationStatus::Confirmed,
        checkpoint_exists,
        project_summary: Some(state.project_service.scan_project(&context, None)),
    })
}

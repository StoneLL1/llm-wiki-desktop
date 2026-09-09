use crate::app_state::AppState;
use crate::commands::runtime::run_blocking;
use crate::errors::BackendError;
use crate::models::confirmation::{
    ConfirmationExecution, ConfirmationStatus, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::version_history::*;
use crate::services::{BlockingWorkClass, FileStore, GitService, VersionHistoryService};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub operation_id: Option<String>,
    pub path: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub save: Option<bool>,
}

impl VersionRequest {
    fn operation_id(&self) -> Result<&str, BackendError> {
        self.operation_id.as_deref().ok_or_else(|| {
            BackendError::new("VERSION_ID_INVALID", "Choose an operation.", true, true)
        })
    }
}

#[tauri::command]
pub async fn get_version_history_status(
    app: AppHandle,
    request: VersionRequest,
) -> Result<VersionHistoryStatus, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        GitService.local_history_status(&context)
    })
    .await
}

#[tauri::command]
pub async fn list_version_operations(
    app: AppHandle,
    request: VersionRequest,
) -> Result<VersionHistoryPage, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        VersionHistoryService.list(
            &context,
            request.cursor.as_deref(),
            request.limit.unwrap_or(50),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_version_operation(
    app: AppHandle,
    request: VersionRequest,
) -> Result<VersionOperation, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        VersionHistoryService.load(&context, request.operation_id()?)
    })
    .await
}

#[tauri::command]
pub async fn get_version_file_diff(
    app: AppHandle,
    request: VersionRequest,
) -> Result<VersionFileDiff, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        let path = request.path.as_deref().ok_or_else(|| {
            BackendError::new("VERSION_PATH_UNKNOWN", "Choose a file.", true, true)
        })?;
        VersionHistoryService.file_diff(&context, request.operation_id()?, path)
    })
    .await
}

#[tauri::command]
pub async fn prepare_version_action(
    app: AppHandle,
    request: VersionRequest,
) -> Result<PendingAction, BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        state.with_current_project_write_access(&request.project_id, &request.project_root_path, |_permit, context| {
            let identity = crate::services::project_identity(&context.root).map_err(|message| BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true))?;
            let (mutation, paths, checkpoint, action_type) = if let Some(id) = request.operation_id.as_deref() {
                let candidate = VersionHistoryService.load(context, id)?;
                let preview = if candidate.source_deletion.is_some() {
                    crate::services::import_v2::ImportV2Service::preview_deleted_source_restore(context, id)?
                } else { VersionHistoryService.preview_restore(context, id)? };
                if preview.already_restored || !preview.conflicts.is_empty() {
                    return Err(BackendError::new("VERSION_RESTORE_CONFLICT", "The version is already restored or its files contain newer edits.", true, true).with_details(serde_json::json!({"paths":preview.conflicts})));
                }
                let record = VersionHistoryService.load(context, id)?;
                let record_hash = FileStore.content_hash(&serde_json::to_vec(&record).map_err(|e| BackendError::new("VERSION_RECORD_INVALID", e.to_string(), true, true))?);
                let current_hashes = preview.paths.iter().map(|path| Ok((path.clone(), FileStore.file_hash_if_exists(context, path)?))).collect::<Result<_, BackendError>>()?;
                (VersionHistoryMutation::Restore { operation_id: id.into(), record_hash, current_hashes }, preview.paths, Some(record.before), PendingActionType::BatchRewrite)
            } else if request.save == Some(true) {
                let paths = VersionHistoryService.manual_scope(context)?;
                let files = VersionHistoryService.capture(context, &paths)?;
                let current_hashes = files.into_iter().map(|(path, bytes)| (path, bytes.map(|bytes| FileStore.content_hash(&bytes)))).collect();
                (VersionHistoryMutation::Save { current_hashes }, paths, None, PendingActionType::BatchRewrite)
            } else {
                (VersionHistoryMutation::Enable, vec![".git".into()], None, PendingActionType::InitializeGitRepository)
            };
            let action = PendingAction {
                id: uuid::Uuid::new_v4().to_string(), action_type,
                title: if request.operation_id.is_some() { "Restore operation" } else if request.save == Some(true) { "Save current version" } else { "Enable local version protection" }.into(),
                message: if request.operation_id.is_some() { "Restore only the listed files. Their current contents will be checked again before writing." } else if request.save == Some(true) { "Save a local version of the listed Markdown files. Sources and attachments are excluded." } else { "Save local recovery versions before application changes. No account or remote repository is required." }.into(),
                risk_level: RiskLevel::High, affected_paths: paths, preview: None, expires_at: Some((chrono::Utc::now()+chrono::Duration::minutes(10)).to_rfc3339()), checkpoint_hash: checkpoint,
            };
            state.confirmation_registry.register_with_execution(action.clone(), Some(ConfirmationExecution::VersionHistory {
                project_id: request.project_id.clone(), root_path: request.project_root_path.clone(), canonical_identity_key: identity.canonical_identity_key, identity_revision: identity.identity_revision, mutation,
            }))?;
            Ok(action)
        })
    }).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmVersionRequest {
    pub action_id: String,
    pub status: ConfirmationStatus,
}

#[tauri::command]
pub async fn confirm_version_action(
    app: AppHandle,
    request: ConfirmVersionRequest,
) -> Result<(), BackendError> {
    run_blocking(app, BlockingWorkClass::HeavyIo, move |app| {
        let state = app.state::<AppState>();
        let stored = state.confirmation_registry.peek(&request.action_id)?;
        if !matches!(stored.execution, Some(ConfirmationExecution::VersionHistory { .. })) {
            return Err(BackendError::new("CONFIRMATION_COMMAND_INVALID", "This is not a version history action.", true, true));
        }
        if request.status == ConfirmationStatus::Cancelled {
            state.confirmation_registry.confirm(&request.action_id, request.status)?;
            return Ok(());
        }
        let stored = state.confirmation_registry.claim(&request.action_id)?;
        let result = (|| {
            let Some(ConfirmationExecution::VersionHistory { project_id, root_path, canonical_identity_key, identity_revision, mutation }) = stored.execution else { unreachable!() };
            state.with_current_project_write_access(&project_id, &root_path, |permit, context| {
                let identity = crate::services::project_identity(&context.root).map_err(|message| BackendError::new("PROJECT_IDENTITY_FAILED", message, true, true))?;
                if identity.canonical_identity_key != canonical_identity_key || identity.identity_revision != identity_revision {
                    return Err(BackendError::new("PROJECT_IDENTITY_CHANGED", "This confirmation belongs to an earlier knowledge base identity.", true, true));
                }
                match mutation {
                    VersionHistoryMutation::Enable => GitService.enable_local_history(context),
                    VersionHistoryMutation::Save { current_hashes } => VersionHistoryService.save_manual(context, &current_hashes).map(|_| ()) ,
                    VersionHistoryMutation::Restore { operation_id, record_hash, current_hashes } => {
                        let record = VersionHistoryService.load(context, &operation_id)?;
                        let hash = FileStore.content_hash(&serde_json::to_vec(&record).map_err(|e| BackendError::new("VERSION_RECORD_INVALID", e.to_string(), true, true))?);
                        if hash != record_hash || current_hashes.iter().any(|(path, expected)| FileStore.file_hash_if_exists(context, path).as_ref().ok() != Some(expected)) {
                            return Err(BackendError::new("VERSION_RESTORE_CONFLICT", "Files or the recovery record changed after preview. Review the operation again.", true, true));
                        }
                        if record.source_deletion.is_some() {
                            state.import_v2_service.restore_deleted_source_authorized(permit, &operation_id, &current_hashes)?;
                        } else { VersionHistoryService.restore_confirmed(context, &operation_id, Some(&current_hashes))?; }
                        Ok(())
                    }
                }
            })
        })();
        state.confirmation_registry.finish_claim(&request.action_id, result.is_ok())?;
        result
    }).await
}

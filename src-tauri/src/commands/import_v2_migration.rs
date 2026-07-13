use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2_migration::{
    LegacyInventory, MigrationApplyResult, MigrationConfirmation, MigrationPlan,
    MigrationStatusSnapshot,
};
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::import_v2::migration::MigrationService;
use crate::tasks::task_model::{CancellationToken, LogLevel};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportV2MigrationRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanImportV2MigrationRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub inventory: LegacyInventory,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyImportV2MigrationRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub plan: MigrationPlan,
    pub confirmation: MigrationConfirmation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeImportV2MigrationRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub plan: MigrationPlan,
    pub confirmation: MigrationConfirmation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetImportV2MigrationStatusRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn scan_import_v2_migration(
    state: State<'_, AppState>,
    request: ScanImportV2MigrationRequest,
) -> Result<LegacyInventory, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    state.import_v2_service.preflight_migration_locked(&context)?;
    MigrationService::default().scan(&context.root)
}

#[tauri::command]
pub fn plan_import_v2_migration(
    state: State<'_, AppState>,
    request: PlanImportV2MigrationRequest,
) -> Result<MigrationPlan, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    state.import_v2_service.preflight_migration_locked(&context)?;
    MigrationService::default().plan(&context.root, &request.inventory)
}

#[tauri::command]
pub fn get_import_v2_migration_status(
    state: State<'_, AppState>,
    request: GetImportV2MigrationStatusRequest,
) -> Result<MigrationStatusSnapshot, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    state.import_v2_service.preflight_migration_locked(&context)?;
    MigrationService::default().status(&context)
}

#[tauri::command]
pub fn apply_import_v2_migration(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ApplyImportV2MigrationRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    state.import_v2_service.preflight_migration_locked(&context)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root,
            "Apply Import V2 migration".into(),
            true,
        )
        .map_err(|error| task_error(&error))?;
    spawn_migration_task(app, task.clone(), request.project_id, request.project_root_path, request.plan, request.confirmation, false);
    Ok(task)
}

#[tauri::command]
pub fn resume_import_v2_migration(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ResumeImportV2MigrationRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    state.import_v2_service.preflight_migration_locked(&context)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root,
            "Resume Import V2 migration".into(),
            true,
        )
        .map_err(|error| task_error(&error))?;
    spawn_migration_task(app, task.clone(), request.project_id, request.project_root_path, request.plan, request.confirmation, true);
    Ok(task)
}

fn spawn_migration_task(
    app: AppHandle,
    task: BackendTask,
    project_id: String,
    project_root_path: String,
    plan: MigrationPlan,
    confirmation: MigrationConfirmation,
    resume: bool,
) {
    let task_id = task.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = (|| -> Result<MigrationApplyResult, BackendError> {
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Running)
                .map_err(|error| task_error(&error))?;
            state
                .task_service
                .append_log(&task_id, LogLevel::Info, "Applying Import V2 migration metadata".into())
                .map_err(|error| task_error(&error))?;
            let context = state.resolve_project_context(&project_id, &project_root_path)?;
            let cancellation = state
                .task_service
                .get_cancellation_token(&task_id)
                .unwrap_or_else(CancellationToken::new);
            let service = MigrationService::default();
            if resume {
                service.resume(
                    &state.import_v2_service,
                    &state.git_service,
                    &context,
                    &plan,
                    confirmation,
                    &cancellation,
                )
            } else {
                service.apply_metadata(
                    &state.import_v2_service,
                    &state.git_service,
                    &context,
                    &plan,
                    confirmation,
                    &cancellation,
                )
            }
        })();

        match result {
            Ok(result) if result.status == crate::models::import_v2_migration::MigrationStatus::Cancelled => {
                let _ = state.task_service.transition_status(&task_id, TaskStatus::Cancelled);
            }
            Ok(result) => {
                let _ = state.task_service.set_result(
                    &task_id,
                    TaskResult {
                        summary: "Import V2 migration metadata applied.".into(),
                        affected_paths: vec![
                            ".app/source-index-v2.json".into(),
                            ".app/import-v2-migration/report.json".into(),
                        ],
                        reference: None,
                        pending_action: None,
                    },
                );
                let _ = state.task_service.transition_status(&task_id, TaskStatus::Succeeded);
                let _ = result;
            }
            Err(error) => {
                let _ = state.task_service.set_error(&task_id, error);
                if !state.task_service.is_cancelled(&task_id) {
                    let _ = state.task_service.transition_status(&task_id, TaskStatus::Failed);
                }
            }
        }
    });
}

fn task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_MIGRATION_TASK_FAILED", message, true, false)
}

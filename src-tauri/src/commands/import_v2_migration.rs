use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2_migration::{
    LegacyInventory, MigrationApplyResult, MigrationConfirmation, MigrationPlan,
    MigrationPreparation, MigrationStatusSnapshot,
};
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::import_v2::migration::MigrationService;
use crate::services::BlockingWorkClass;
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

pub fn scan_import_v2_migration(
    state: State<'_, AppState>,
    request: ScanImportV2MigrationRequest,
) -> Result<LegacyInventory, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let project_locks = state.import_v2_service.project_locks(&context)?;
    let _guard = state.import_v2_service.lock_project(&project_locks);
    state
        .import_v2_service
        .preflight_migration_locked(&context)?;
    MigrationService::default().scan(&context.root)
}

pub fn plan_import_v2_migration(
    state: State<'_, AppState>,
    request: PlanImportV2MigrationRequest,
) -> Result<MigrationPreparation, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let project_locks = state.import_v2_service.project_locks(&context)?;
    let _guard = state.import_v2_service.lock_project(&project_locks);
    state
        .import_v2_service
        .preflight_migration_locked(&context)?;
    MigrationService::default().prepare(&context.root, &request.inventory)
}

pub fn get_import_v2_migration_status(
    state: State<'_, AppState>,
    request: GetImportV2MigrationStatusRequest,
) -> Result<MigrationStatusSnapshot, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let project_locks = state.import_v2_service.project_locks(&context)?;
    let _guard = state.import_v2_service.lock_project(&project_locks);
    state
        .import_v2_service
        .preflight_migration_locked(&context)?;
    MigrationService::default().status(&context)
}

pub fn apply_import_v2_migration(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ApplyImportV2MigrationRequest,
) -> Result<BackendTask, BackendError> {
    let task = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let task_root = {
                let project_locks = state.import_v2_service.project_locks(context)?;
                let _guard = state.import_v2_service.lock_project(&project_locks);
                state
                    .import_v2_service
                    .preflight_migration_locked(context)?;
                context.root.clone()
            };
            state
                .task_service
                .create_project_task(
                    TaskType::Import,
                    request.project_id.clone(),
                    task_root,
                    "Apply Import V2 migration".into(),
                    true,
                )
                .map_err(|error| task_error(&error))
        },
    )?;
    spawn_migration_task(
        app,
        task.clone(),
        request.project_id,
        request.project_root_path,
        request.plan,
        request.confirmation,
        false,
    );
    Ok(task)
}

pub fn resume_import_v2_migration(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ResumeImportV2MigrationRequest,
) -> Result<BackendTask, BackendError> {
    let task = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let task_root = {
                let project_locks = state.import_v2_service.project_locks(context)?;
                let _guard = state.import_v2_service.lock_project(&project_locks);
                state
                    .import_v2_service
                    .preflight_migration_locked(context)?;
                context.root.clone()
            };
            state
                .task_service
                .create_project_task(
                    TaskType::Import,
                    request.project_id.clone(),
                    task_root,
                    "Resume Import V2 migration".into(),
                    true,
                )
                .map_err(|error| task_error(&error))
        },
    )?;
    spawn_migration_task(
        app,
        task.clone(),
        request.project_id,
        request.project_root_path,
        request.plan,
        request.confirmation,
        true,
    );
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
    let coordinator = app.state::<AppState>().blocking_work.clone();
    let cancellation = app
        .state::<AppState>()
        .task_service
        .get_cancellation_token(&task_id)
        .unwrap_or_default();
    let failure_app = app.clone();
    let failure_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let worker_result =
            coordinator
                .run_cancellable(BlockingWorkClass::HeavyIo, cancellation, move || {
                    let state = app.state::<AppState>();
                    let result = (|| -> Result<MigrationApplyResult, BackendError> {
                        state
                            .task_service
                            .transition_status(&task_id, TaskStatus::Running)
                            .map_err(|error| task_error(&error))?;
                        state
                            .task_service
                            .append_log(
                                &task_id,
                                LogLevel::Info,
                                "Applying Import V2 migration metadata".into(),
                            )
                            .map_err(|error| task_error(&error))?;
                        let cancellation = state
                            .task_service
                            .get_cancellation_token(&task_id)
                            .unwrap_or_else(CancellationToken::new);
                        let service = MigrationService::default();
                        state.with_current_project_write_access(
                            &project_id,
                            &project_root_path,
                            |_permit, context| {
                                let canonical_project_identity =
                                    crate::services::project_identity(&context.root)
                                        .map_err(|error| {
                                            BackendError::new(
                                                "PROJECT_IDENTITY_FAILED",
                                                error,
                                                true,
                                                false,
                                            )
                                        })?
                                        .canonical_identity_key;
                                state.blocking_work.run_project_git_blocking(
                                    canonical_project_identity,
                                    Some(&cancellation),
                                    || {
                                        if resume {
                                            service.resume(
                                                &state.import_v2_service,
                                                &state.git_service,
                                                context,
                                                &plan,
                                                confirmation,
                                                &cancellation,
                                            )
                                        } else {
                                            service.apply_metadata(
                                                &state.import_v2_service,
                                                &state.git_service,
                                                context,
                                                &plan,
                                                confirmation,
                                                &cancellation,
                                            )
                                        }
                                    },
                                )
                            },
                        )
                    })();

                    match result {
            Ok(result)
                if result.status
                    == crate::models::import_v2_migration::MigrationStatus::Cancelled =>
            {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Cancelled);
            }
            Ok(result) => {
                let checkpoint_summary = result
                    .checkpoint
                    .as_ref()
                    .and_then(|checkpoint| checkpoint.commit_hash.as_deref())
                    .map(|hash| format!("Git checkpoint {hash}"))
                    .unwrap_or_else(|| "No Git checkpoint; rollback is release-based".into());
                let _ = state.task_service.set_result(
                    &task_id,
                    TaskResult {
                        summary: format!(
                            "Import V2 migration metadata applied ({checkpoint_summary})."
                        ),
                        affected_paths: vec![
                            ".app/source-index-v2.json".into(),
                            ".app/import-v2-migration/report.json".into(),
                        ],
                        reference: None,
                        pending_action: None,
                    },
                );
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Succeeded);
            }
            Err(error) => {
                let _ = state.task_service.set_error(&task_id, error);
                if !state.task_service.is_cancelled(&task_id) {
                    let _ = state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Failed);
                }
            }
        }
                    Ok(())
                })
                .await;
        if let Err(error) = worker_result {
            let state = failure_app.state::<AppState>();
            if state.task_service.is_cancelled(&failure_task_id) {
                let _ = state.task_service.finalize_cancellation(&failure_task_id);
            } else {
                let _ = state.task_service.set_error(&failure_task_id, error);
                let _ = state
                    .task_service
                    .transition_status(&failure_task_id, TaskStatus::Failed);
            }
        }
    });
}

fn task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_MIGRATION_TASK_FAILED", message, true, false)
}

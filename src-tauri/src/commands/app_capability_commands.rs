use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::app_capability::{
    AppCapabilityContinuationState, AppCapabilityTaskControlRequest, AppCapabilityView,
    AppTaskControlScope, InstallAppCapabilityV1Request,
};
use crate::models::task::TaskActivity;
use crate::models::task::{BackendTask, TaskOperation, TaskResult, TaskStatus};
use crate::services::import_v2::capability_installer::{
    catalog_entry, discard_catalog_partial, install_catalog_entry, CapabilityCatalogEntry,
    CapabilityInstallPhase,
};
use crate::services::import_v2::capability_runtime::target_triple;
use crate::services::import_v2::product_capability::ProductCapabilityManifest;
use crate::services::BlockingWorkClass;
use crate::tasks::task_model::LogLine;

#[tauri::command]
pub async fn list_app_capabilities_v1(
    app: AppHandle,
) -> Result<Vec<AppCapabilityView>, BackendError> {
    let blocking_work = app.state::<AppState>().blocking_work.clone();
    blocking_work
        .run(BlockingWorkClass::MetadataIo, move || {
            let state = app.state::<AppState>();
            state
                .app_capability_coordinator
                .list_capabilities(&state.import_capability_runtime, &state.task_service)
        })
        .await
}

#[tauri::command]
pub fn list_app_tasks_v1(state: State<'_, AppState>) -> Vec<BackendTask> {
    state.task_service.list_app_tasks(None)
}

#[tauri::command]
pub fn get_app_capability_task_logs_v1(
    state: State<'_, AppState>,
    request: AppCapabilityTaskControlRequest,
) -> Result<Vec<LogLine>, BackendError> {
    let task = require_global_task(&state, &request.task_id, &request.scope)?;
    state
        .task_service
        .get_logs(&task.id)
        .map_err(task_control_error)
}

#[tauri::command]
pub fn get_app_capability_task_activities_v1(
    state: State<'_, AppState>,
    request: AppCapabilityTaskControlRequest,
) -> Result<Vec<TaskActivity>, BackendError> {
    let task = require_global_task(&state, &request.task_id, &request.scope)?;
    state
        .task_service
        .get_activities(&task.id)
        .map_err(task_control_error)
}

#[tauri::command]
pub async fn install_app_capability_v1(
    app: AppHandle,
    request: InstallAppCapabilityV1Request,
) -> Result<BackendTask, BackendError> {
    let blocking_work = app.state::<AppState>().blocking_work.clone();
    let worker_app = app.clone();
    blocking_work
        .run(BlockingWorkClass::HeavyIo, move || {
            let state = app.state::<AppState>();
            begin_app_capability_install(worker_app, &state, request)
        })
        .await
}

pub(crate) fn begin_app_capability_install(
    app: AppHandle,
    state: &AppState,
    request: InstallAppCapabilityV1Request,
) -> Result<BackendTask, BackendError> {
    begin_app_capability_install_inner(app, state, request, None)
}

pub(crate) fn begin_app_capability_install_for_continuation(
    app: AppHandle,
    state: &AppState,
    request: InstallAppCapabilityV1Request,
    continuation_id: &str,
) -> Result<BackendTask, BackendError> {
    begin_app_capability_install_inner(app, state, request, Some(continuation_id))
}

fn begin_app_capability_install_inner(
    app: AppHandle,
    state: &AppState,
    request: InstallAppCapabilityV1Request,
    continuation_id: Option<&str>,
) -> Result<BackendTask, BackendError> {
    let entry = catalog_entry(&request.capability_id, &target_triple()).ok_or_else(|| {
        let catalog_empty = crate::services::import_v2::capability_installer::catalog_availability()
            == crate::services::import_v2::capability_installer::CapabilityCatalogAvailability::CatalogUnavailable;
        capability_error(
            if catalog_empty {
                "APP_CAPABILITY_CATALOG_UNAVAILABLE"
            } else {
                "APP_CAPABILITY_NOT_PUBLISHED_FOR_TARGET"
            },
            if catalog_empty {
                "The signed capability catalog is unavailable in this build."
            } else {
                "No published capability release is available for this target."
            },
        )
    })?;
    require_batch4_install_route(&entry)?;
    let (task, created) = state.app_capability_coordinator.join_or_create_install(
        &state.task_service,
        &entry,
        &request.expected_version,
        &request.acknowledgement_version,
    )?;
    if let Some(continuation_id) = continuation_id {
        state
            .app_capability_coordinator
            .bind_continuation_task(continuation_id, &task.id)?;
    }
    state
        .app_capability_coordinator
        .bind_registered_continuations(&entry.capability_id, &task.id)?;
    if created {
        spawn_install_worker(app, task.clone(), entry);
    }
    Ok(task)
}

#[tauri::command]
pub async fn pause_app_capability_install_v1(
    app: AppHandle,
    request: AppCapabilityTaskControlRequest,
) -> Result<BackendTask, BackendError> {
    let blocking_work = app.state::<AppState>().blocking_work.clone();
    blocking_work
        .run(BlockingWorkClass::HeavyIo, move || {
            let state = app.state::<AppState>();
            let task = require_control_target(&state, &request)?;
            state
                .task_service
                .request_app_task_pause(&task.id, &request.task_revision)
                .map_err(task_control_error)
        })
        .await
}

#[tauri::command]
pub async fn resume_app_capability_install_v1(
    app: AppHandle,
    request: AppCapabilityTaskControlRequest,
) -> Result<BackendTask, BackendError> {
    let blocking_work = app.state::<AppState>().blocking_work.clone();
    let worker_app = app.clone();
    blocking_work
        .run(BlockingWorkClass::HeavyIo, move || {
            let state = app.state::<AppState>();
            let task = require_control_target(&state, &request)?;
            let entry = entry_for_task(&task)?;
            require_batch4_install_route(&entry)?;
            let resumed = state
                .task_service
                .resume_app_task(&task.id, &request.task_revision)
                .map_err(task_control_error)?;
            spawn_install_worker(worker_app, resumed.clone(), entry);
            Ok(resumed)
        })
        .await
}

#[tauri::command]
pub async fn cancel_app_capability_install_v1(
    state: State<'_, AppState>,
    request: AppCapabilityTaskControlRequest,
) -> Result<BackendTask, BackendError> {
    let task = require_control_target(&state, &request)?;
    if task.status == TaskStatus::Interrupted {
        let entry = entry_for_task(&task)?;
        let install_root = state
            .import_capability_runtime
            .install_root()
            .ok_or_else(|| {
                capability_error(
                    "APP_CAPABILITY_STATE_UNAVAILABLE",
                    "Capability installation state is unavailable.",
                )
            })?;
        let cancelled = state
            .task_service
            .cancel_paused_app_task(&task.id, &request.task_revision)
            .map_err(task_control_error)?;
        let cleanup = state
            .blocking_work
            .run(BlockingWorkClass::HeavyIo, move || {
                discard_catalog_partial(&install_root, &entry)
            })
            .await;
        cancel_continuations(&state, &task.id)?;
        state.app_capability_coordinator.settle_task(&cancelled);
        cleanup?;
        return Ok(cancelled);
    }
    let cancelled = state
        .task_service
        .request_app_task_cancel(&task.id, &request.task_revision)
        .map_err(task_control_error)?;
    if cancelled.status == TaskStatus::Cancelled {
        cancel_continuations(&state, &task.id)?;
        state.app_capability_coordinator.settle_task(&cancelled);
    }
    Ok(cancelled)
}

fn require_control_target(
    state: &AppState,
    request: &AppCapabilityTaskControlRequest,
) -> Result<BackendTask, BackendError> {
    let task = require_global_task(state, &request.task_id, &request.scope)?;
    if task.updated_at != request.task_revision {
        return Err(capability_error(
            "APP_CAPABILITY_TASK_REVISION_STALE",
            "The capability installation task changed. Reload it before trying again.",
        ));
    }
    Ok(task)
}

fn require_global_task(
    state: &AppState,
    task_id: &str,
    scope: &AppTaskControlScope,
) -> Result<BackendTask, BackendError> {
    if *scope != AppTaskControlScope::AppGlobal {
        return Err(capability_error(
            "APP_CAPABILITY_TASK_SCOPE_INVALID",
            "Capability installation controls require application-global scope.",
        ));
    }
    let task = state.task_service.get_task(task_id).ok_or_else(|| {
        capability_error(
            "APP_CAPABILITY_TASK_NOT_FOUND",
            "The capability installation task was not found.",
        )
    })?;
    if task.project_id.is_some()
        || task.task_type != crate::models::task::TaskType::CapabilityInstall
        || !matches!(
            task.operation,
            Some(TaskOperation::AppCapabilityInstall { .. })
        )
    {
        return Err(capability_error(
            "APP_CAPABILITY_TASK_SCOPE_INVALID",
            "The task is not an application-global capability installation.",
        ));
    }
    Ok(task)
}

fn entry_for_task(task: &BackendTask) -> Result<CapabilityCatalogEntry, BackendError> {
    let Some(TaskOperation::AppCapabilityInstall {
        capability_id,
        version,
        target_triple,
        archive_identity,
    }) = task.operation.as_ref()
    else {
        return Err(capability_error(
            "APP_CAPABILITY_TASK_SCOPE_INVALID",
            "The task is not a capability installation.",
        ));
    };
    let entry = catalog_entry(capability_id, target_triple).ok_or_else(|| {
        capability_error(
            "APP_CAPABILITY_MANIFEST_INVALID",
            "The capability release is no longer present in the product catalog.",
        )
    })?;
    if &entry.version != version
        || crate::services::app_capability_archive_identity(&entry) != *archive_identity
    {
        return Err(capability_error(
            "APP_CAPABILITY_MANIFEST_INVALID",
            "The capability task no longer matches the signed product catalog.",
        ));
    }
    Ok(entry)
}

fn require_batch4_install_route(entry: &CapabilityCatalogEntry) -> Result<String, BackendError> {
    let definition = ProductCapabilityManifest::embedded()
        .map_err(|message| capability_error("APP_CAPABILITY_MANIFEST_INVALID", &message))?
        .definitions
        .into_iter()
        .find(|definition| definition.capability_id == entry.capability_id)
        .ok_or_else(|| {
            capability_error(
                "APP_CAPABILITY_MANIFEST_INVALID",
                "The capability is not declared by the product manifest.",
            )
        })?;
    if definition.routes.len() != 1 {
        return Err(capability_error(
            "APP_CAPABILITY_ATOMIC_ACTIVATION_REQUIRED",
            "This capability requires the all-route atomic activation gate from Batch 5.",
        ));
    }
    Ok(definition.routes[0].clone())
}

fn spawn_install_worker(app: AppHandle, task: BackendTask, entry: CapabilityCatalogEntry) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let task_id = task.id.clone();
        if state
            .task_service
            .transition_status(&task_id, TaskStatus::Running)
            .is_err()
        {
            return;
        }
        let Some(token) = state.task_service.get_cancellation_token(&task_id) else {
            return;
        };
        let Some(install_root) = state.import_capability_runtime.install_root() else {
            finish_error(
                &state,
                &task_id,
                capability_error(
                    "APP_CAPABILITY_STATE_UNAVAILABLE",
                    "Capability installation state is unavailable.",
                ),
            );
            return;
        };
        let outcome = install_catalog_entry(
            &state.blocking_work,
            &install_root,
            &entry,
            &task_id,
            &token,
            |phase, current, total| {
                let label = match phase {
                    CapabilityInstallPhase::Downloading => "capability.downloading",
                    CapabilityInstallPhase::Verifying => "capability.verifying",
                    CapabilityInstallPhase::Installing => "capability.installing",
                };
                let _ = state.task_service.update_progress(
                    &task_id,
                    current,
                    Some(total),
                    Some(label.into()),
                );
            },
        )
        .await;
        let mut outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                finish_error(&state, &task_id, error);
                return;
            }
        };
        if token.is_cancelled() {
            if token.is_pause_requested() {
                let _ = state.task_service.finalize_app_task_pause(&task_id);
            } else {
                if let Err(error) = outcome.rollback(&install_root, &entry) {
                    finish_error(&state, &task_id, error);
                    return;
                }
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Cancelled);
                let _ = cancel_continuations(&state, &task_id);
                if let Some(task) = state.task_service.get_task(&task_id) {
                    state.app_capability_coordinator.settle_task(&task);
                }
            }
            return;
        }
        let route = match require_batch4_install_route(&entry) {
            Ok(route) => route,
            Err(error) => {
                let error = outcome
                    .rollback(&install_root, &entry)
                    .err()
                    .unwrap_or(error);
                finish_error(&state, &task_id, error);
                return;
            }
        };
        let _ = state.task_service.update_progress(
            &task_id,
            entry.compressed_bytes,
            Some(entry.compressed_bytes),
            Some("capability.health_check".into()),
        );
        let health_app = app.clone();
        let health_root = install_root.clone();
        let health_entry = entry.clone();
        let health_route = route.clone();
        let health_token = token.clone();
        let health_task_id = task_id.clone();
        let health = state
            .blocking_work
            .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
                let state = health_app.state::<AppState>();
                let pack = match state.import_capability_runtime.probe_version(
                    &health_root,
                    &health_entry.capability_id,
                    &health_entry.version,
                    &health_route,
                    &health_token,
                ) {
                    Ok(pack) => pack,
                    Err(error) => {
                        return Err(outcome
                            .rollback(&health_root, &health_entry)
                            .err()
                            .unwrap_or(error));
                    }
                };
                let _ = state.task_service.update_progress(
                    &health_task_id,
                    health_entry.compressed_bytes,
                    Some(health_entry.compressed_bytes),
                    Some("capability.activating".into()),
                );
                if let Err(error) = outcome.activate(&health_root) {
                    return Err(outcome
                        .rollback(&health_root, &health_entry)
                        .err()
                        .unwrap_or(error));
                }
                let activation = state.import_capability_runtime.activate_probed_version(
                    pack,
                    &health_entry.capability_id,
                    &health_route,
                    &state.import_v2_service,
                );
                match activation {
                    Ok(()) => Ok(()),
                    Err(error) => Err(outcome
                        .rollback(&health_root, &health_entry)
                        .err()
                        .unwrap_or(error)),
                }
            })
            .await;
        if let Err(error) = health {
            finish_error(&state, &task_id, error);
            return;
        }
        if token.is_cancelled() {
            if token.is_pause_requested() {
                let _ = state.task_service.finalize_app_task_pause(&task_id);
            } else {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Cancelled);
                let _ = cancel_continuations(&state, &task_id);
                if let Some(task) = state.task_service.get_task(&task_id) {
                    state.app_capability_coordinator.settle_task(&task);
                }
            }
            return;
        }
        let mut resumed = 0usize;
        let mut deferred = 0usize;
        let continuations = state
            .app_capability_coordinator
            .continuations_for_task(&task_id);
        let running_updates = continuations
            .iter()
            .map(|continuation| {
                (
                    continuation.continuation_id.clone(),
                    AppCapabilityContinuationState::Running,
                    None,
                )
            })
            .collect::<Vec<_>>();
        if let Err(error) = state
            .app_capability_coordinator
            .update_continuation_states(&running_updates)
        {
            finish_error(&state, &task_id, error);
            return;
        }
        let mut final_updates = Vec::with_capacity(continuations.len());
        for (index, continuation) in continuations.iter().enumerate() {
            let continuation_app = app.clone();
            let continuation_for_run = continuation.clone();
            let continuation_token = token.clone();
            let continuation_result = state
                .blocking_work
                .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
                    let state = continuation_app.state::<AppState>();
                    crate::commands::import_v2_presentation_commands::resume_import_capability_continuation(
                        continuation_app.clone(),
                        &state,
                        &continuation_for_run,
                        &continuation_token,
                    )
                })
                .await;
            if token.is_cancelled() {
                final_updates.extend(continuations[index..].iter().map(|pending| {
                    (
                        pending.continuation_id.clone(),
                        AppCapabilityContinuationState::Deferred,
                        Some("APP_CAPABILITY_CONTINUATION_STOPPED".into()),
                    )
                }));
                if let Err(error) = state
                    .app_capability_coordinator
                    .update_continuation_states(&final_updates)
                {
                    finish_error(&state, &task_id, error);
                    return;
                }
                if token.is_pause_requested() {
                    let _ = state.task_service.finalize_app_task_pause(&task_id);
                } else {
                    let _ = state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Cancelled);
                    let _ = cancel_continuations(&state, &task_id);
                    if let Some(task) = state.task_service.get_task(&task_id) {
                        state.app_capability_coordinator.settle_task(&task);
                    }
                }
                return;
            }
            match continuation_result {
                Ok(true) => {
                    resumed += 1;
                    final_updates.push((
                        continuation.continuation_id.clone(),
                        AppCapabilityContinuationState::Succeeded,
                        None,
                    ));
                }
                Ok(false) => {
                    deferred += 1;
                    final_updates.push((
                        continuation.continuation_id.clone(),
                        AppCapabilityContinuationState::Deferred,
                        Some("APP_CAPABILITY_CONTINUATION_DEFERRED".into()),
                    ));
                }
                Err(error) => {
                    let deferred_error = error.code.starts_with("PROJECT_")
                        || error.code.contains("STALE")
                        || error.code.contains("MISMATCH");
                    if deferred_error {
                        deferred += 1;
                    }
                    final_updates.push((
                        continuation.continuation_id.clone(),
                        if deferred_error {
                            AppCapabilityContinuationState::Deferred
                        } else {
                            AppCapabilityContinuationState::Failed
                        },
                        Some(error.code),
                    ));
                }
            }
        }
        if let Err(error) = state
            .app_capability_coordinator
            .update_continuation_states(&final_updates)
        {
            finish_error(&state, &task_id, error);
            return;
        }
        let completion = state.task_service.complete_running_with_result(
            &task_id,
            TaskResult {
                summary: format!(
                    "Installed {} {}; resumed {} continuation(s), deferred {}.",
                    entry.capability_id, entry.version, resumed, deferred
                ),
                affected_paths: Vec::new(),
                reference: None,
                pending_action: None,
            },
        );
        match completion {
            Ok(task) => state.app_capability_coordinator.settle_task(&task),
            Err(message) => finish_error(
                &state,
                &task_id,
                capability_error("APP_CAPABILITY_TASK_FINALIZE_FAILED", &message),
            ),
        }
    });
}

fn finish_error(state: &AppState, task_id: &str, error: BackendError) {
    if let Some(token) = state.task_service.get_cancellation_token(task_id) {
        if token.is_pause_requested() {
            let _ = state.task_service.finalize_app_task_pause(task_id);
            return;
        }
        if token.is_cancelled() {
            let _ = state
                .task_service
                .transition_status(task_id, TaskStatus::Cancelled);
            let _ = cancel_continuations(state, task_id);
        } else {
            let error = classify_install_error(error);
            let _ = state.task_service.set_error(task_id, error);
            let _ = state
                .task_service
                .transition_status(task_id, TaskStatus::Failed);
        }
    }
    if let Some(task) = state.task_service.get_task(task_id) {
        state.app_capability_coordinator.settle_task(&task);
    }
}

fn cancel_continuations(state: &AppState, task_id: &str) -> Result<(), BackendError> {
    let updates = state
        .app_capability_coordinator
        .continuations_for_task(task_id)
        .into_iter()
        .map(|continuation| {
            (
                continuation.continuation_id,
                AppCapabilityContinuationState::Cancelled,
                Some("APP_CAPABILITY_INSTALL_CANCELLED".into()),
            )
        })
        .collect::<Vec<_>>();
    state
        .app_capability_coordinator
        .update_continuation_states(&updates)
        .map(|_| ())
}

fn classify_install_error(error: BackendError) -> BackendError {
    if error.code.starts_with("APP_CAPABILITY_") {
        return error;
    }
    let text = format!("{} {}", error.code, error.message).to_ascii_lowercase();
    let code = if text.contains("catalog") {
        "APP_CAPABILITY_CATALOG_UNAVAILABLE"
    } else if text.contains("target") || text.contains("manifest") {
        "APP_CAPABILITY_MANIFEST_INVALID"
    } else if text.contains("hash") || text.contains("integrity") || text.contains("signature") {
        "APP_CAPABILITY_INTEGRITY_FAILED"
    } else if text.contains("range")
        || text.contains("partial content")
        || text.contains("response is invalid")
    {
        "APP_CAPABILITY_RANGE_UNSUPPORTED"
    } else if text.contains("space")
        || text.contains("disk")
        || text.contains("could not be saved")
        || text.contains("could not be checkpointed")
        || text.contains("could not be finalized")
        || text.contains("file cannot be created")
    {
        "APP_CAPABILITY_DISK_FULL"
    } else if text.contains("lock")
        || text.contains("antivirus")
        || text.contains("cannot be pinned")
    {
        "APP_CAPABILITY_FILE_LOCKED"
    } else if text.contains("health") || text.contains("probe") {
        "APP_CAPABILITY_HEALTH_CHECK_FAILED"
    } else if text.contains("rollback") {
        "APP_CAPABILITY_ROLLBACK_FAILED"
    } else if text.contains("tls")
        || text.contains("dns")
        || text.contains("proxy")
        || text.contains("download failed")
        || text.contains("download restart failed")
        || text.contains("downloader is unavailable")
        || text.contains("request failed")
    {
        "APP_CAPABILITY_NETWORK_UNAVAILABLE"
    } else {
        "APP_CAPABILITY_INSTALL_FAILED"
    };
    BackendError::new(code, &error.message, true, true).with_details(serde_json::json!({
        "sourceCode": error.code
    }))
}

fn task_control_error(message: String) -> BackendError {
    capability_error(
        if message.to_ascii_lowercase().contains("revision is stale") {
            "APP_CAPABILITY_TASK_REVISION_STALE"
        } else {
            "APP_CAPABILITY_TASK_CONTROL_FAILED"
        },
        &message,
    )
}

fn capability_error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}

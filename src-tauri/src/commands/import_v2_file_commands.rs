use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::ImportSession;
use crate::models::import_v2_file::{FileScanPolicy, FileScanResult, ImportScanIdentity};
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::import_v2::capability_runtime::CapabilityRuntimeStatus;
use crate::services::import_v2::file_discovery::FileDiscoveryService;
use crate::services::import_v2::scan_confirmation::{
    mark_scan_accepted, mark_scan_aggregate_confirmed, mark_scan_discarded,
    prepare_legacy_scan_staging, prepare_saved_scan_acceptance, prepare_scan_staging,
    SavedScanAcceptance,
};
use crate::services::BlockingWorkClass;

const DISCOVERY_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);

fn import_scan_confirmation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn import_scan_path(
    context: &crate::models::paths::ProjectContext,
    session_id: &str,
    task_id: &str,
) -> Result<String, BackendError> {
    let root = context.layout.import_state_root.as_deref().ok_or_else(|| {
        BackendError::new(
            "IMPORT_STATE_ROOT_REQUIRED",
            "Import state is unavailable for this project layout.",
            true,
            false,
        )
    })?;
    Ok(format!("{root}/{session_id}/scans/{task_id}.json"))
}

pub fn get_import_capability_statuses(state: State<'_, AppState>) -> Vec<CapabilityRuntimeStatus> {
    state.import_capability_runtime.statuses()
}
use crate::tasks::task_model::LogLevel;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddImportPathsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub large_data_confirmed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetImportScanResultV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptImportScanV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub task_id: String,
    pub confirmation_token: String,
    #[serde(default)]
    pub acknowledge_aggregate: bool,
    #[serde(default)]
    pub source_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptImportScanV2Result {
    pub session: ImportSession,
    pub scan: FileScanResult,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardImportScanV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub task_id: String,
    pub confirmation_token: String,
}

/// Starts durable discovery work and returns immediately. The existing
/// synchronous command remains for compatibility with older frontends.
pub fn start_add_import_paths_v2(
    app: AppHandle,
    request: AddImportPathsV2Request,
) -> Result<BackendTask, BackendError> {
    let state = app.state::<AppState>();
    let task = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            state.import_v2_service.ensure_session_accepts_inputs(
                context,
                &state.file_store,
                &request.session_id,
            )?;
            state
                .task_service
                .create_project_task(
                    TaskType::Import,
                    request.project_id.clone(),
                    context.root.clone(),
                    "Discover import files".into(),
                    true,
                )
                .map_err(task_error)
        },
    )?;
    let task_id = task.id.clone();
    if let Err(error) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _context| {
            state.import_v2_service.set_discovery_task_id_authorized(
                permit,
                &state.file_store,
                &request.session_id,
                Some(task_id.clone()),
            )
        },
    ) {
        let _ = state
            .task_service
            .discard_unstarted_tasks(std::slice::from_ref(&task_id));
        return Err(error);
    }
    let coordinator = state.blocking_work.clone();
    let cancellation = state
        .task_service
        .get_cancellation_token(&task_id)
        .unwrap_or_default();
    let failure_app = app.clone();
    let failure_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let worker_result = coordinator
            .run_cancellable(BlockingWorkClass::HeavyIo, cancellation, move || {
        let state = app.state::<AppState>();
        let run = || -> Result<(), BackendError> {
            let context =
                state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            state.with_current_project_write_access(
                &request.project_id,
                &request.project_root_path,
                |_permit, _context| {
                    state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Running)
                        .map_err(task_error)?;
                    state
                        .task_service
                        .append_log(&task_id, LogLevel::Info, "Scanning selected paths".into())
                        .map_err(task_error)?;
                    Ok(())
                },
            )?;
            let execution = state.begin_project_external_task(&context, &task_id)?;
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
            let mut discovered = 0_u64;
            let mut last_progress = Instant::now() - DISCOVERY_PROGRESS_MIN_INTERVAL;
            let mut scan = FileDiscoveryService::default().scan(
                &context,
                &roots,
                FileScanPolicy::default(),
                |batch| {
                    discovered += batch.len() as u64;
                    if last_progress.elapsed() >= DISCOVERY_PROGRESS_MIN_INTERVAL {
                        let _ = state.task_service.update_progress(
                            &task_id,
                            discovered,
                            None,
                            Some("Discovering files".into()),
                        );
                        last_progress = Instant::now();
                    }
                },
                || state.task_service.is_cancelled(&task_id),
            )?;
            if state.task_service.is_cancelled(&task_id) {
                return Ok(());
            }
            state.require_current_execution_epoch(&context, &execution)?;
            scan.scan_identity = Some(ImportScanIdentity {
                project_id: request.project_id.clone(),
                project_root_path: context.root.to_string_lossy().into_owned(),
                session_id: request.session_id.clone(),
                task_id: task_id.clone(),
            });
            let plan =
                prepare_scan_staging(&mut scan, &session, request.large_data_confirmed, || {
                    Uuid::new_v4().to_string()
                });
            let skipped = scan.skipped.len();
            let added = plan.inputs.len();
            let summary = if plan.aggregate_confirmation_pending {
                format!(
                    "Found {} files ({} bytes, about {} outputs); confirmation is required before adding them.",
                    scan.totals.file_count,
                    scan.totals.total_bytes,
                    scan.totals.estimated_output_files.unwrap_or(scan.totals.file_count as u64),
                )
            } else if plan.item_confirmation_pending {
                format!("Added {added} files; some large data files require confirmation.")
            } else {
                format!("Added {added} files; skipped {skipped}.")
            };
            state.with_current_project_write_access(
                &request.project_id,
                &request.project_root_path,
                |permit, context| {
                    let _ = state.task_service.update_progress(
                        &task_id,
                        discovered,
                        Some(discovered),
                        Some("Discovery complete".into()),
                    );
                    let scan_path = import_scan_path(context, &request.session_id, &task_id)?;
                    state
                        .file_store
                        .write_json_atomic(context, &scan_path, &scan)?;
                    if !plan.inputs.is_empty() {
                        state.import_v2_service.add_inputs_authorized(
                            permit,
                            &state.file_store,
                            &request.session_id,
                            plan.inputs,
                        )?;
                    }
                    state
                        .task_service
                        .append_log(&task_id, LogLevel::Info, summary.clone())
                        .map_err(task_error)?;
                    state
                        .task_service
                        .set_result(
                            &task_id,
                            TaskResult {
                                summary,
                                affected_paths: vec![scan_path],
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
                },
            )
        };
        if let Err(error) = run() {
            if error.code == "IMPORT_FILE_SCAN_CANCELLED"
                || state.task_service.is_cancelled(&task_id)
            {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Cancelled);
            } else {
                let _ = state.task_service.set_error(&task_id, error);
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
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
    Ok(task)
}

pub fn get_import_scan_result_v2(
    state: State<'_, AppState>,
    request: GetImportScanResultV2Request,
) -> Result<FileScanResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let path = import_scan_path(&context, &request.session_id, &request.task_id)?;
    state.file_store.read_json(&context, &path)
}

pub fn accept_import_scan_v2(
    state: State<'_, AppState>,
    request: AcceptImportScanV2Request,
) -> Result<AcceptImportScanV2Result, BackendError> {
    let _guard = import_scan_confirmation_lock().lock().map_err(|_| {
        BackendError::new(
            "IMPORT_SCAN_CONFIRMATION_INVALID",
            "Import scan confirmation lock is poisoned.",
            true,
            false,
        )
    })?;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let path = import_scan_path(context, &request.session_id, &request.task_id)?;
            let mut scan: FileScanResult = state.file_store.read_json(context, &path)?;
            let expected_identity = ImportScanIdentity {
                project_id: request.project_id.clone(),
                project_root_path: context.root.to_string_lossy().into_owned(),
                session_id: request.session_id.clone(),
                task_id: request.task_id.clone(),
            };
            let current = state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.session_id,
            )?;
            let acceptance = prepare_saved_scan_acceptance(
                &scan,
                &expected_identity,
                &request.confirmation_token,
                request.acknowledge_aggregate,
                request.source_paths.as_deref(),
                &current,
            )?;
            let mut fully_accepted = matches!(acceptance, SavedScanAcceptance::AlreadyAccepted);
            if let SavedScanAcceptance::Ready(plan) = acceptance {
                if !plan.inputs.is_empty() {
                    state.import_v2_service.add_inputs_authorized(
                        permit,
                        &state.file_store,
                        &request.session_id,
                        plan.inputs,
                    )?;
                }
                let now = chrono::Utc::now().to_rfc3339();
                if plan.mark_aggregate_confirmed {
                    mark_scan_aggregate_confirmed(&mut scan, now.clone());
                }
                if plan.fully_accepted {
                    mark_scan_accepted(&mut scan, now);
                    fully_accepted = true;
                }
                state.file_store.write_json_atomic(context, &path, &scan)?;
            }
            let session = if fully_accepted {
                state.import_v2_service.set_discovery_task_id_authorized(
                    permit,
                    &state.file_store,
                    &request.session_id,
                    None,
                )?
            } else {
                state.import_v2_service.load_session(
                    context,
                    &state.file_store,
                    &request.session_id,
                )?
            };
            Ok(AcceptImportScanV2Result { session, scan })
        },
    )
}

pub fn discard_import_scan_v2(
    state: State<'_, AppState>,
    request: DiscardImportScanV2Request,
) -> Result<FileScanResult, BackendError> {
    let _guard = import_scan_confirmation_lock().lock().map_err(|_| {
        BackendError::new(
            "IMPORT_SCAN_CONFIRMATION_INVALID",
            "Import scan confirmation lock is poisoned.",
            true,
            false,
        )
    })?;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let path = import_scan_path(context, &request.session_id, &request.task_id)?;
            let mut scan: FileScanResult = state.file_store.read_json(context, &path)?;
            let expected_identity = ImportScanIdentity {
                project_id: request.project_id.clone(),
                project_root_path: context.root.to_string_lossy().into_owned(),
                session_id: request.session_id.clone(),
                task_id: request.task_id.clone(),
            };
            if mark_scan_discarded(
                &mut scan,
                &expected_identity,
                &request.confirmation_token,
                chrono::Utc::now().to_rfc3339(),
            )? {
                state.file_store.write_json_atomic(context, &path, &scan)?;
            }
            state.import_v2_service.set_discovery_task_id_authorized(
                permit,
                &state.file_store,
                &request.session_id,
                None,
            )?;
            Ok(scan)
        },
    )
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_SERVICE", message, true, false)
}

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
    let mut scan = FileDiscoveryService::default().scan(
        &context,
        &roots,
        FileScanPolicy::default(),
        |_| {},
        || false,
    )?;
    let inputs = prepare_legacy_scan_staging(&mut scan, &session, request.large_data_confirmed);
    if inputs.is_empty() {
        return Ok(session);
    }
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _context| {
            state.import_v2_service.add_inputs_authorized(
                permit,
                &state.file_store,
                &request.session_id,
                inputs,
            )
        },
    )
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
            large_data_confirmed: false,
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
            large_data_confirmed: false,
        })
        .unwrap();
        assert_eq!(value["sourcePaths"].as_array().unwrap().len(), 2);
        assert!(value.get("install").is_none());
    }
}

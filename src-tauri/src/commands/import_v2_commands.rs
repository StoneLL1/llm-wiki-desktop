use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::{BackendError, IMPORT_V2_ENGINE_PANICKED};
use crate::models::import_v2::{
    CommitImportSessionRequest, ImportBatchResult, ImportCompletion, ImportInput, ImportItem,
    ImportItemResolution, ImportItemStatus, ImportRecoveryAction, ImportResourceMode,
    ImportSession, ImportThreeWayMergeContext,
};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskResultReference, TaskStatus, TaskType};
use crate::services::import_v2::agent_candidate::AgentCandidateService;
use crate::tasks::TaskService;

const DEFAULT_IMPORT_WORKER_LIMIT: usize = 2;
const MAX_IMPORT_WORKER_LIMIT: usize = 32;
static IMPORT_WORK_QUEUE: OnceLock<Mutex<VecDeque<ImportWorkerJob>>> = OnceLock::new();
static ACTIVE_IMPORT_WORKERS: AtomicUsize = AtomicUsize::new(0);
static IMPORT_WORKER_LIMIT: AtomicUsize = AtomicUsize::new(DEFAULT_IMPORT_WORKER_LIMIT);

struct ImportWorkerJob {
    app: AppHandle,
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    task_id: String,
    recovery_action: Option<ImportRecoveryAction>,
}

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
    session_id: String,
    history_batch_id: Option<String>
});
request!(GetImportRestrictedContentStatusV2Request {
    project_id: String,
    project_root_path: String
});
request!(AddImportItemsV2Request { project_id: String, project_root_path: String, session_id: String, inputs: Vec<ImportInput> });
request!(CancelImportItemV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String
});
request!(SetImportItemResolutionV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    resolution: ImportItemResolution
});
request!(CancelImportBatchV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    batch_id: String
});

pub(crate) const RESTRICTED_CONTENT_ACK_PATH: &str = ".app/import-restricted-content-ack.json";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRestrictedContentStatus {
    pub confirmation_required: bool,
}

#[tauri::command]
pub fn get_import_restricted_content_status_v2(
    state: State<'_, AppState>,
    request: GetImportRestrictedContentStatusV2Request,
) -> Result<ImportRestrictedContentStatus, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(ImportRestrictedContentStatus {
        confirmation_required: !state
            .file_store
            .exists(&context, RESTRICTED_CONTENT_ACK_PATH),
    })
}
request!(AddImportTextV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    source_name: String,
    content: String
});
request!(SetImportItemSelectionV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    selected: bool
});
request!(SelectImportSubtitleV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    file_name: String
});
request!(ImportMergeContextV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String
});
request!(StageImportManualMergeV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    merged_markdown: String
});
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartImportItemsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub recovery_action: Option<ImportRecoveryAction>,
}

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
    state.import_v2_service.recover_session(
        &context,
        &state.file_store,
        &state.task_service,
        &request.session_id,
    )?;
    AgentCandidateService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.task_service,
    )
    .recover_completed_outputs(&context, &request.session_id)
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
pub fn add_import_text_v2(
    state: State<'_, AppState>,
    request: AddImportTextV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.add_text_input(
        &context,
        &state.file_store,
        &request.session_id,
        &request.source_name,
        &request.content,
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
pub fn select_import_subtitle_v2(
    state: State<'_, AppState>,
    request: SelectImportSubtitleV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.select_subtitle_for_session(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
        &request.file_name,
    )?;
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

#[tauri::command]
pub fn get_import_merge_context_v2(
    state: State<'_, AppState>,
    request: ImportMergeContextV2Request,
) -> Result<ImportThreeWayMergeContext, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.get_three_way_merge_context(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
    )
}

#[tauri::command]
pub fn set_import_item_resolution_v2(
    state: State<'_, AppState>,
    request: SetImportItemResolutionV2Request,
) -> Result<ImportItem, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.set_item_resolution(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
        request.resolution,
    )
}

#[tauri::command]
pub fn stage_import_manual_merge_v2(
    state: State<'_, AppState>,
    request: StageImportManualMergeV2Request,
) -> Result<ImportItem, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.stage_manual_merge(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
        &request.merged_markdown,
    )
}

/// Read a historical session without recovery side effects. History views are
/// inspection-only; opening them must not resume tasks, accept candidates, or
/// rewrite staging/session evidence.
#[tauri::command]
pub fn get_import_history_session_v2(
    state: State<'_, AppState>,
    request: GetImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if let Some(batch_id) = request.history_batch_id.as_deref() {
        if let Some(snapshot) = load_history_snapshot(&context, &request.session_id, batch_id)? {
            return Ok(snapshot);
        }
    }
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

#[tauri::command]
pub fn get_import_completion_v2(
    state: State<'_, AppState>,
    request: GetImportSessionV2Request,
) -> Result<Option<ImportCompletion>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let batch_id = request.history_batch_id.as_deref().ok_or_else(|| {
        BackendError::new(
            "IMPORT_V2_HISTORY_INVALID",
            "A historical batch identity is required.",
            false,
            true,
        )
    })?;
    Ok(load_history_batch(&context, &request.session_id, batch_id)?.completion)
}

pub(crate) fn load_history_snapshot(
    context: &ProjectContext,
    session_id: &str,
    batch_id: &str,
) -> Result<Option<ImportSession>, BackendError> {
    Ok(load_history_batch(context, session_id, batch_id)?.history_snapshot)
}

pub(crate) fn load_history_batch(
    context: &ProjectContext,
    session_id: &str,
    batch_id: &str,
) -> Result<ImportBatchResult, BackendError> {
    if batch_id.is_empty()
        || batch_id.len() > 64
        || !batch_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(BackendError::new(
            "IMPORT_V2_HISTORY_INVALID",
            "The historical import identity is invalid.",
            false,
            true,
        ));
    }
    let path = context.resolve_project_path(&format!(".app/import-history/{batch_id}.json"))?;
    let bytes = std::fs::read(&path).map_err(|_| {
        BackendError::new(
            "IMPORT_V2_HISTORY_NOT_FOUND",
            "The historical import record could not be opened.",
            true,
            true,
        )
    })?;
    let batch: ImportBatchResult = serde_json::from_slice(&bytes).map_err(|_| {
        BackendError::new(
            "IMPORT_V2_HISTORY_CORRUPT",
            "The historical import record is incomplete.",
            true,
            true,
        )
    })?;
    if batch.session_id != session_id {
        return Err(BackendError::new(
            "IMPORT_V2_HISTORY_SCOPE_MISMATCH",
            "The historical import record does not belong to this session.",
            false,
            true,
        ));
    }
    Ok(batch)
}

#[tauri::command]
pub fn cancel_import_item_v2(
    state: State<'_, AppState>,
    request: CancelImportItemV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let item = session
        .items
        .iter()
        .find(|item| item.item_id == request.item_id)
        .ok_or_else(|| task_error("Import item was not found."))?;
    if item.status != ImportItemStatus::Queued {
        state.import_v2_service.cancel_queued_item(
            &context,
            &state.file_store,
            &request.session_id,
            &request.item_id,
        )?;
    }
    let bound_task_id = item.task_id.clone();
    if let Some(task_id) = &bound_task_id {
        state
            .task_service
            .cancel_task(task_id)
            .map_err(|error| task_error(&error))?;
    }
    let cancel_item = state.import_v2_service.cancel_queued_item(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
    );
    if let Err(error) = cancel_item {
        // The worker can claim the item between the session read and the
        // durable item mutation. Its task token is already cancelled, so the
        // worker owns the transition back to Cancelled and cleanup.
        if !bound_task_id
            .as_deref()
            .is_some_and(|task_id| state.task_service.is_cancelled(task_id))
        {
            return Err(error);
        }
    }
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

#[tauri::command]
pub fn skip_import_item_v2(
    state: State<'_, AppState>,
    request: CancelImportItemV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.skip_item(
        &context,
        &state.file_store,
        &state.task_service,
        &request.session_id,
        &request.item_id,
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
    start_import_items_for_state(app, &state, request)
}

pub(crate) fn start_import_items_for_state(
    app: AppHandle,
    state: &AppState,
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
    let replaced_waiting_task_ids = request
        .item_ids
        .iter()
        .filter_map(|item_id| {
            session
                .items
                .iter()
                .find(|item| item.item_id == *item_id)
                .and_then(|item| item.task_id.clone())
        })
        .collect::<Vec<_>>();
    // One IPC call is one user-visible import operation. Persist the identity
    // on every child task so parallel operations remain independently
    // observable and cancellable after navigation or app restart.
    let batch_id = Uuid::new_v4().to_string();
    let recovery_action = request.recovery_action.clone();
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
                .create_project_task_with_batch(
                    TaskType::Import,
                    request.project_id.clone(),
                    context.root.clone(),
                    format!("Import {}", item.input.display_name),
                    true,
                    batch_id.clone(),
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
    let bindings = prepared
        .iter()
        .map(|(item_id, task)| (item_id.clone(), task.id.clone()))
        .collect::<Vec<_>>();
    if let Err(error) = state.import_v2_service.bind_item_task_ids(
        &context,
        &state.file_store,
        &request.session_id,
        &bindings,
    ) {
        for (_, task) in &prepared {
            let _ = state
                .task_service
                .discard_unstarted_tasks(std::slice::from_ref(&task.id));
        }
        return Err(error);
    }
    for replaced_task_id in replaced_waiting_task_ids {
        if state
            .task_service
            .get_task(&replaced_task_id)
            .is_some_and(|task| task.status == TaskStatus::WaitingForConfirmation)
        {
            let _ = state.task_service.cancel_task(&replaced_task_id);
        }
    }
    let worker_limit = state
        .settings_service
        .read_settings(&context)
        .map(|settings| configured_import_worker_limit(settings.max_concurrent_tasks))
        .unwrap_or(DEFAULT_IMPORT_WORKER_LIMIT);
    let mut tasks = Vec::with_capacity(prepared.len());
    let mut jobs = Vec::with_capacity(prepared.len());
    for (item_id, task) in prepared {
        jobs.push(ImportWorkerJob {
            app: app.clone(),
            project_id: request.project_id.clone(),
            project_root_path: request.project_root_path.clone(),
            session_id: request.session_id.clone(),
            item_id,
            task_id: task.id.clone(),
            recovery_action: recovery_action.clone(),
        });
        tasks.push(task);
    }
    enqueue_import_jobs(jobs, worker_limit);
    Ok(tasks)
}

fn configured_import_worker_limit(value: u64) -> usize {
    usize::try_from(value)
        .unwrap_or(MAX_IMPORT_WORKER_LIMIT)
        .clamp(1, MAX_IMPORT_WORKER_LIMIT)
}

fn import_work_queue() -> &'static Mutex<VecDeque<ImportWorkerJob>> {
    IMPORT_WORK_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn enqueue_import_jobs(jobs: Vec<ImportWorkerJob>, worker_limit: usize) {
    if jobs.is_empty() {
        return;
    }
    IMPORT_WORKER_LIMIT.store(worker_limit, Ordering::Release);
    import_work_queue()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(jobs);
    schedule_import_workers();
}

fn schedule_import_workers() {
    loop {
        let worker_limit = IMPORT_WORKER_LIMIT.load(Ordering::Acquire).max(1);
        let active = ACTIVE_IMPORT_WORKERS.load(Ordering::Acquire);
        if active >= worker_limit
            || import_work_queue()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        {
            return;
        }
        if ACTIVE_IMPORT_WORKERS
            .compare_exchange(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            continue;
        }
        // ImportEngine is a synchronous boundary that performs filesystem,
        // process, and network-adapter work. A process-wide bounded queue keeps
        // it off Tokio's async workers without creating one blocking worker per
        // imported item.
        tauri::async_runtime::spawn_blocking(|| {
            loop {
                if ACTIVE_IMPORT_WORKERS.load(Ordering::Acquire)
                    > IMPORT_WORKER_LIMIT.load(Ordering::Acquire).max(1)
                {
                    break;
                }
                let job = import_work_queue()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some(job) = job else {
                    break;
                };
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_import_worker_job(&job);
                }))
                .is_err()
                {
                    let state = job.app.state::<AppState>();
                    fail_task_unless_cancelled(
                        &state,
                        &job.task_id,
                        BackendError::new(
                            IMPORT_V2_ENGINE_PANICKED,
                            "The import worker stopped unexpectedly.",
                            true,
                            false,
                        ),
                    );
                }
            }
            ACTIVE_IMPORT_WORKERS.fetch_sub(1, Ordering::AcqRel);
            // Cover the enqueue/worker-exit race: an enqueue can observe the
            // old active count just before this worker becomes idle.
            schedule_import_workers();
        });
    }
}

fn run_import_worker_job(job: &ImportWorkerJob) {
    let state = job.app.state::<AppState>();
    let result = state
        .resolve_project_context(&job.project_id, &job.project_root_path)
        .and_then(|context| {
            let item_is_still_bound = state
                .import_v2_service
                .load_session(&context, &state.file_store, &job.session_id)?
                .items
                .into_iter()
                .find(|item| item.item_id == job.item_id)
                .is_some_and(|item| {
                    item.task_id.as_deref() == Some(job.task_id.as_str())
                        && !matches!(
                            item.status,
                            ImportItemStatus::Cancelled
                                | ImportItemStatus::Skipped
                                | ImportItemStatus::Completed
                        )
                });
            if !item_is_still_bound {
                state
                    .task_service
                    .cancel_task(&job.task_id)
                    .map_err(|error| task_error(&error))?;
                return Ok(());
            }
            state.import_v2_service.run_item_with_recovery(
                &context,
                &state.file_store,
                &state.task_service,
                &job.session_id,
                &job.item_id,
                &job.task_id,
                job.recovery_action.as_ref(),
            )?;
            let restricted_content_acknowledged = state
                .file_store
                .exists(&context, RESTRICTED_CONTENT_ACK_PATH);
            if let Some(batch) = state.import_v2_service.finalize_exact_duplicate(
                &context,
                &state.file_store,
                &state.git_service,
                &job.session_id,
                &job.item_id,
                restricted_content_acknowledged,
                || {
                    state
                        .task_service
                        .transition_status(&job.task_id, TaskStatus::Running)
                        .map(|_| ())
                        .map_err(|error| task_error(&error))
                },
            )? {
                state
                    .task_service
                    .complete_running_with_result(
                        &job.task_id,
                        TaskResult {
                            summary: "Duplicate already exists; its locator was recorded.".into(),
                            affected_paths: vec![format!(
                                ".app/import-history/{}.json",
                                batch.batch_id
                            )],
                            reference: Some(TaskResultReference::ImportV2SessionPreview {
                                session_id: batch.session_id.clone(),
                                batch_id: Some(batch.batch_id.clone()),
                                completion: batch.completion.clone(),
                            }),
                            pending_action: None,
                        },
                    )
                    .map_err(|error| task_error(&error))?;
            }
            Ok(())
        });
    if let Err(error) = result {
        fail_task_unless_cancelled(&state, &job.task_id, error);
    }
}

/// Cancel only the import tasks belonging to one backend-issued batch. The
/// session lookup is intentional: a batch id alone must never reach tasks
/// from another import session in the same project.
#[tauri::command]
pub fn cancel_import_batch_v2(
    state: State<'_, AppState>,
    request: CancelImportBatchV2Request,
) -> Result<Vec<BackendTask>, BackendError> {
    if request.batch_id.trim().is_empty() {
        return Err(task_error("Import batch id must not be empty."));
    }
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    // Do not depend on the orchestrator having claimed the session item yet.
    // The task is created and returned before `claim_item_for_run` persists
    // `item.task_id`, so an immediate user cancellation must discover the
    // group from the durable task identity instead of the eventually-written
    // session mapping.
    let task_ids = state
        .task_service
        .list_tasks(None)
        .into_iter()
        .filter(|task| {
            task.project_id.as_deref() == Some(request.project_id.as_str())
                && task.batch_id.as_deref() == Some(request.batch_id.as_str())
                && task.cancellable
                && matches!(
                    task.status,
                    TaskStatus::Queued | TaskStatus::Running | TaskStatus::Cancelling
                )
        })
        .map(|task| task.id)
        .collect::<Vec<_>>();

    task_ids
        .into_iter()
        .map(|task_id| {
            state
                .task_service
                .cancel_task(&task_id)
                .map_err(|error| task_error(&error))
        })
        .collect()
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
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let includes_restricted = request.decisions.iter().any(|decision| {
        session
            .items
            .iter()
            .any(|item| item.item_id == decision.item_id && item.restricted_content)
    });
    let preview_task_ids = request
        .decisions
        .iter()
        .filter_map(|decision| {
            session
                .items
                .iter()
                .find(|item| item.item_id == decision.item_id)
                .and_then(|item| item.task_id.as_ref())
                .map(|task_id| (decision.item_id.clone(), task_id.clone()))
        })
        .collect::<HashMap<_, _>>();
    if includes_restricted
        && !state
            .file_store
            .exists(&context, RESTRICTED_CONTENT_ACK_PATH)
    {
        if !request.acknowledge_restricted_content {
            return Err(BackendError::new(
                "IMPORT_V2_RESTRICTED_CONTENT_CONFIRMATION_REQUIRED",
                "Restricted content must be acknowledged before its first project commit.",
                false,
                true,
            ));
        }
        state.file_store.write_json_atomic(
            &context,
            RESTRICTED_CONTENT_ACK_PATH,
            &serde_json::json!({
                "schemaVersion": 1,
                "acknowledgedAt": chrono::Utc::now().to_rfc3339(),
            }),
        )?;
    }
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
    let mut commit_request = request.clone();
    commit_request.batch_task_id = Some(task_id.clone());
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = (|| -> Result<TaskResult, BackendError> {
            let context =
                state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Running)
                .map_err(|error| task_error(&error))?;
            let batch = state
                .import_v2_service
                .commit_items_cancellable_with_progress(
                    &context,
                    &state.file_store,
                    &state.git_service,
                    &commit_request,
                    || state.task_service.is_cancelled(&task_id),
                    |batch| {
                        settle_confirmed_preview_tasks(
                            &state.task_service,
                            &preview_task_ids,
                            batch,
                        );
                    },
                    None,
                    || Ok(()),
                )?;
            Ok(TaskResult {
                summary: format!(
                    "Committed {} import item(s); {} failed.",
                    batch.committed_count, batch.failed_count
                ),
                affected_paths: vec![format!(".app/import-history/{}.json", batch.batch_id)],
                reference: Some(TaskResultReference::ImportV2SessionPreview {
                    session_id: batch.session_id.clone(),
                    batch_id: Some(batch.batch_id.clone()),
                    completion: batch.completion.clone(),
                }),
                pending_action: None,
            })
        })();
        match result {
            Ok(result) => {
                let _ = state
                    .task_service
                    .complete_running_with_result(&task_id, result);
            }
            Err(error) => {
                fail_task_unless_cancelled(&state, &task_id, error);
            }
        }
    });
    Ok(task)
}

fn settle_confirmed_preview_tasks(
    task_service: &TaskService,
    preview_task_ids: &HashMap<String, String>,
    batch: &ImportBatchResult,
) {
    for item in &batch.items {
        let Some(task_id) = preview_task_ids.get(&item.item_id) else {
            continue;
        };
        if !task_service
            .get_task(task_id)
            .is_some_and(|task| task.status == TaskStatus::WaitingForConfirmation)
        {
            continue;
        }
        if !item.committed && item.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED)
        {
            continue;
        }
        if task_service
            .transition_status(task_id, TaskStatus::Running)
            .is_err()
        {
            continue;
        }
        if item.committed {
            let affected_paths = item.wiki_path.clone().into_iter().collect();
            let _ = task_service.complete_running_with_result(
                task_id,
                TaskResult {
                    summary: "Import item committed.".into(),
                    affected_paths,
                    reference: None,
                    pending_action: None,
                },
            );
        } else {
            let code = item
                .error_code
                .clone()
                .unwrap_or_else(|| "IMPORT_V2_COMMIT_FAILED".into());
            let _ = task_service.set_error(
                task_id,
                BackendError::new(code, "The import item could not be committed.", true, false),
            );
            let _ = task_service.transition_status(task_id, TaskStatus::Failed);
        }
    }
}

fn task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_TASK_FAILED", message, true, false)
}

fn fail_task_unless_cancelled(state: &AppState, task_id: &str, error: BackendError) {
    let _ = state.task_service.set_error(task_id, error);
    if !matches!(
        state.task_service.get_task(task_id).map(|task| task.status),
        Some(TaskStatus::Cancelled | TaskStatus::WaitingForConfirmation)
    ) {
        let _ = state
            .task_service
            .transition_status(task_id, TaskStatus::Failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::{ImportInputKind, ImportItemCommitResult};

    #[test]
    fn import_worker_limit_is_always_bounded() {
        assert_eq!(configured_import_worker_limit(0), 1);
        assert_eq!(configured_import_worker_limit(4), 4);
        assert_eq!(
            configured_import_worker_limit(u64::MAX),
            MAX_IMPORT_WORKER_LIMIT
        );
    }

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
                media_save_mode: Default::default(),
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

    #[test]
    fn confirmation_settles_the_original_preview_tasks() {
        let root = std::env::temp_dir().join(format!("import-v2-confirm-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let tasks = TaskService::default();
        let successful = tasks
            .create_project_task(
                TaskType::Import,
                "project".into(),
                root.clone(),
                "successful preview".into(),
                true,
            )
            .unwrap();
        let failed = tasks
            .create_project_task(
                TaskType::Import,
                "project".into(),
                root.clone(),
                "failed preview".into(),
                true,
            )
            .unwrap();
        let cancelled = tasks
            .create_project_task(
                TaskType::Import,
                "project".into(),
                root.clone(),
                "cancelled preview".into(),
                true,
            )
            .unwrap();
        for task_id in [&successful.id, &failed.id, &cancelled.id] {
            tasks
                .transition_status(task_id, TaskStatus::Running)
                .unwrap();
            tasks
                .transition_status(task_id, TaskStatus::WaitingForConfirmation)
                .unwrap();
        }
        let preview_task_ids = HashMap::from([
            ("success-item".into(), successful.id.clone()),
            ("failed-item".into(), failed.id.clone()),
            ("cancelled-item".into(), cancelled.id.clone()),
        ]);
        let batch = ImportBatchResult {
            batch_id: "batch".into(),
            session_id: "session".into(),
            created_at: "2026-07-27T00:00:00Z".into(),
            batch_task_id: None,
            committed_count: 1,
            failed_count: 2,
            items: vec![
                ImportItemCommitResult {
                    item_id: "success-item".into(),
                    source_id: Some("source".into()),
                    version_id: Some("version".into()),
                    wiki_path: Some("wiki/source.md".into()),
                    content_hash: Some("a".repeat(64)),
                    disposition: None,
                    warnings: Vec::new(),
                    committed: true,
                    error_code: None,
                },
                ImportItemCommitResult {
                    item_id: "failed-item".into(),
                    source_id: None,
                    version_id: None,
                    wiki_path: None,
                    content_hash: None,
                    disposition: None,
                    warnings: Vec::new(),
                    committed: false,
                    error_code: Some("IMPORT_V2_COMMIT_CONFLICT".into()),
                },
                ImportItemCommitResult {
                    item_id: "cancelled-item".into(),
                    source_id: None,
                    version_id: None,
                    wiki_path: None,
                    content_hash: None,
                    disposition: None,
                    warnings: Vec::new(),
                    committed: false,
                    error_code: Some(crate::errors::IMPORT_V2_CANCELLED.into()),
                },
            ],
            history_snapshot: None,
            completion: None,
        };

        settle_confirmed_preview_tasks(&tasks, &preview_task_ids, &batch);

        assert_eq!(
            tasks.get_task(&successful.id).unwrap().status,
            TaskStatus::Succeeded
        );
        let failed_task = tasks.get_task(&failed.id).unwrap();
        assert_eq!(failed_task.status, TaskStatus::Failed);
        assert_eq!(
            failed_task.error.as_ref().map(|error| error.code.as_str()),
            Some("IMPORT_V2_COMMIT_CONFLICT")
        );
        assert_eq!(
            tasks.get_task(&cancelled.id).unwrap().status,
            TaskStatus::WaitingForConfirmation
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

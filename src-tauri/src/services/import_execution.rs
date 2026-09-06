//! Native task execution adapter. Commands only admit intents; this service
//! owns the queue, cancellation, preview workspaces and publication of results.
use crate::app_state::AppState;
use crate::errors::{BackendError, IMPORT_V2_ENGINE_PANICKED};
use crate::models::import_v2::{
    ImportItem, ImportItemStatus, ImportRecoveryAction, ImportResolutionKind,
    ImportSessionPatchCounts, ImportSessionPatchEvent, ImportWorkItemSnapshot,
};
use crate::models::paths::ProjectContext;
use crate::models::task::{TaskResult, TaskResultReference, TaskStatus};
use crate::services::import_v2::execution_control::{
    batch_terminal_status, BatchExecutionControl, BatchOperationState, ImportExecutionControl,
    ImportItemRunOutcome,
};
use crate::services::import_v2::NewSourceTargetReservations;
use crate::services::BlockingWorkClass;
use crate::tasks::task_model::LogLevel;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

pub(crate) const DEFAULT_IMPORT_WORKER_LIMIT: usize = 2;
pub(crate) const MAX_IMPORT_WORKER_LIMIT: usize = 32;
static IMPORT_WORK_QUEUE: OnceLock<Mutex<VecDeque<ImportWorkerJob>>> = OnceLock::new();
static ACTIVE_IMPORT_WORKERS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_TEXT_WORKERS: AtomicUsize = AtomicUsize::new(0);
static IMPORT_WORKER_LIMIT: AtomicUsize = AtomicUsize::new(DEFAULT_IMPORT_WORKER_LIMIT);

#[derive(Clone)]
pub(crate) struct ImportWorkerJob {
    pub(crate) app: AppHandle,
    pub(crate) project_id: String,
    pub(crate) project_root_path: String,
    pub(crate) session_id: String,
    pub(crate) item_id: String,
    pub(crate) task_id: String,
    pub(crate) snapshot: ImportWorkItemSnapshot,
    pub(crate) target_reservations: Arc<Mutex<NewSourceTargetReservations>>,
    pub(crate) recovery_action: Option<ImportRecoveryAction>,
    pub(crate) batch_operation: Option<BatchOperationJob>,
    pub(crate) preview_only: bool,
}

#[derive(Clone)]
pub(crate) struct TemporaryPreviewWorkspace {
    pub(crate) project_id: String,
    pub(crate) project_root: PathBuf,
    pub(crate) context: ProjectContext,
    pub(crate) last_access: SystemTime,
}

static TEMPORARY_PREVIEW_WORKSPACES: OnceLock<Mutex<HashMap<String, TemporaryPreviewWorkspace>>> =
    OnceLock::new();

pub(crate) fn temporary_preview_workspaces(
) -> &'static Mutex<HashMap<String, TemporaryPreviewWorkspace>> {
    TEMPORARY_PREVIEW_WORKSPACES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn preview_authority_error(error: &BackendError) -> bool {
    matches!(
        error.code.as_str(),
        "PROJECT_WRITE_REQUIRES_TRUST"
            | "PROJECT_WRITE_READ_ONLY"
            | "PROJECT_WRITE_STATE_UNAVAILABLE"
    )
}

pub(crate) fn create_temporary_preview_context(
    project_id: &str,
    _project_root: &Path,
) -> Result<ProjectContext, BackendError> {
    let app_temp_root = std::env::temp_dir().join("llm-wiki-desktop");
    std::fs::create_dir_all(&app_temp_root).map_err(|error| {
        BackendError::new(
            "IMPORT_PREVIEW_TEMP_UNAVAILABLE",
            error.to_string(),
            true,
            false,
        )
    })?;
    ensure_private_preview_directory(&app_temp_root)?;
    let preview_root = app_temp_root.join("import-preview");
    std::fs::create_dir_all(&preview_root).map_err(|error| {
        BackendError::new(
            "IMPORT_PREVIEW_TEMP_UNAVAILABLE",
            error.to_string(),
            true,
            false,
        )
    })?;
    ensure_private_preview_directory(&preview_root)?;
    cleanup_orphaned_temporary_preview_workspaces(&preview_root);
    let root = preview_root.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&root).map_err(|error| {
        BackendError::new(
            "IMPORT_PREVIEW_TEMP_UNAVAILABLE",
            error.to_string(),
            true,
            false,
        )
    })?;
    ensure_private_preview_directory(&root)?;
    Ok(ProjectContext::new(project_id.to_string(), root))
}

fn ensure_private_preview_directory(path: &Path) -> Result<(), BackendError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        BackendError::new(
            "IMPORT_PREVIEW_TEMP_UNAVAILABLE",
            error.to_string(),
            true,
            false,
        )
    })?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || crate::services::import_v2::transaction::is_project_reparse_point(&metadata)
    {
        return Err(BackendError::new(
            "IMPORT_PREVIEW_TEMP_UNSAFE",
            "The application preview directory is not a private regular directory.",
            false,
            true,
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(
            |error| {
                BackendError::new(
                    "IMPORT_PREVIEW_TEMP_UNAVAILABLE",
                    error.to_string(),
                    true,
                    false,
                )
            },
        )?;
    }
    Ok(())
}

fn cleanup_orphaned_temporary_preview_workspaces(preview_root: &Path) {
    const MAX_ENTRIES: usize = 128;
    const MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

    let now = SystemTime::now();
    let mut expired = Vec::new();
    let active = {
        let mut workspaces = temporary_preview_workspaces()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        workspaces.retain(|_, workspace| {
            let keep = now
                .duration_since(workspace.last_access)
                .map_or(true, |age| age < MIN_AGE);
            if !keep {
                expired.push(workspace.context.root.clone());
            }
            keep
        });
        workspaces
            .values()
            .map(|workspace| workspace.context.root.clone())
            .collect::<HashSet<_>>()
    };
    for path in expired {
        if path.parent() == Some(preview_root) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
    let Ok(entries) = std::fs::read_dir(preview_root) else {
        return;
    };
    for entry in entries.flatten().take(MAX_ENTRIES) {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() || active.contains(&path) {
            continue;
        }
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= MIN_AGE);
        if old_enough && path.parent() == Some(preview_root) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub(crate) fn import_session_context(
    state: &AppState,
    project_id: &str,
    project_root_path: &str,
    session_id: &str,
) -> Result<ProjectContext, BackendError> {
    let project_context = state.resolve_project_context(project_id, project_root_path)?;
    let mut workspaces = temporary_preview_workspaces()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(workspace) = workspaces.get_mut(session_id) {
        if workspace.project_id != project_id || workspace.project_root != project_context.root {
            return Err(BackendError::new(
                "IMPORT_PREVIEW_SCOPE_INVALID",
                "The temporary preview does not belong to the current project authority.",
                false,
                true,
            ));
        }
        workspace.last_access = SystemTime::now();
        return Ok(workspace.context.clone());
    }
    Ok(project_context)
}

pub(crate) fn release_temporary_preview_workspace(session_id: &str) {
    let workspace = temporary_preview_workspaces()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(session_id);
    let Some(workspace) = workspace else {
        return;
    };
    let root = workspace.context.root;
    let safe_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| Uuid::parse_str(name).is_ok());
    let expected_parent = root
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        == Some("import-preview");
    if safe_name && expected_parent {
        let _ = std::fs::remove_dir_all(root);
    }
}

pub(crate) fn is_temporary_preview_session(session_id: &str) -> bool {
    temporary_preview_workspaces()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(session_id)
}

#[derive(Clone)]
pub(crate) struct BatchOperationJob {
    pub(crate) state: Arc<Mutex<BatchOperationState>>,
    pub(crate) pending_items: Arc<Mutex<HashMap<String, ImportItem>>>,
    pub(crate) duplicate_item_ids: Arc<Mutex<Vec<String>>>,
    pub(crate) remaining_workers: Arc<AtomicUsize>,
}

pub(crate) fn current_import_worker_limit() -> usize {
    IMPORT_WORKER_LIMIT.load(Ordering::Acquire).max(1)
}

pub(crate) fn configured_import_worker_limit(value: u64) -> usize {
    usize::try_from(value)
        .unwrap_or(MAX_IMPORT_WORKER_LIMIT)
        .clamp(1, MAX_IMPORT_WORKER_LIMIT)
}

fn import_work_queue() -> &'static Mutex<VecDeque<ImportWorkerJob>> {
    IMPORT_WORK_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub(crate) fn enqueue_import_jobs(jobs: Vec<ImportWorkerJob>, worker_limit: usize) {
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

pub(crate) fn take_queued_import_jobs(task_id: &str) -> Vec<ImportWorkerJob> {
    let mut queue = import_work_queue()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut retained = VecDeque::with_capacity(queue.len());
    let mut removed = Vec::new();
    while let Some(job) = queue.pop_front() {
        if job.task_id == task_id {
            removed.push(job);
        } else {
            retained.push_back(job);
        }
    }
    *queue = retained;
    removed
}

fn is_small_text(job: &ImportWorkerJob) -> bool {
    let input = &job.snapshot.input;
    matches!(
        Path::new(&input.display_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md" | "markdown" | "txt" | "srt" | "vtt" | "ass" | "lrc")
    ) && input
        .source_identity
        .as_ref()
        .is_some_and(|identity| identity.size_bytes <= 1024 * 1024)
}

fn schedule_import_workers() {
    // A reserved small-text lane prevents a pair of long recognitions from
    // starving a newly added note. Both lanes still use the shared IO budget.
    for light in [true, false] {
        schedule_lane(light);
    }
}

fn schedule_lane(light: bool) {
    let active_workers = if light {
        &ACTIVE_TEXT_WORKERS
    } else {
        &ACTIVE_IMPORT_WORKERS
    };
    loop {
        let limit = if light {
            1
        } else {
            IMPORT_WORKER_LIMIT.load(Ordering::Acquire).max(1)
        };
        let active = active_workers.load(Ordering::Acquire);
        if active >= limit {
            return;
        }
        let coordinator = {
            let queue = import_work_queue()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(job) = queue.iter().find(|job| is_small_text(job) == light) else {
                return;
            };
            job.app.state::<AppState>().blocking_work.clone()
        };
        if active_workers
            .compare_exchange(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            continue;
        }
        tauri::async_runtime::spawn(async move {
            let class = if light {
                BlockingWorkClass::MetadataIo
            } else {
                BlockingWorkClass::HeavyIo
            };
            let _ = coordinator
                .run(class, move || {
                    loop {
                        let job = {
                            let mut queue = import_work_queue()
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            queue
                                .iter()
                                .position(|job| is_small_text(job) == light)
                                .and_then(|index| queue.remove(index))
                        };
                        let Some(job) = job else {
                            break;
                        };
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            run_import_worker_job(&job)
                        }))
                        .is_err()
                        {
                            let state = job.app.state::<AppState>();
                            if job.batch_operation.is_some() {
                                finish_batch_worker(
                                    &state,
                                    &job,
                                    ImportItemRunOutcome::SystemicError,
                                );
                            } else {
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
                        finish_batch_duplicate_cohort_if_drained(&job);
                    }
                    Ok(())
                })
                .await;
            active_workers.fetch_sub(1, Ordering::AcqRel);
            schedule_import_workers();
        });
    }
}

fn run_import_worker_job(job: &ImportWorkerJob) {
    let state = job.app.state::<AppState>();
    let result = import_session_context(
        &state,
        &job.project_id,
        &job.project_root_path,
        &job.session_id,
    )
    .and_then(|context| {
        let item_is_still_bound = state
            .import_v2_service
            .load_item(&context, &state.file_store, &job.session_id, &job.item_id)
            .is_ok_and(|item| {
                item.task_id.as_deref() == Some(job.task_id.as_str())
                    && !matches!(
                        item.status,
                        ImportItemStatus::Cancelled
                            | ImportItemStatus::Skipped
                            | ImportItemStatus::Completed
                    )
            });
        if !item_is_still_bound {
            if job.batch_operation.is_none() {
                state
                    .task_service
                    .cancel_task(&job.task_id)
                    .map_err(|error| task_error(&error))?;
            }
            return Ok(false);
        }
        if job.preview_only {
            state.import_v2_service.run_temporary_preview_item(
                &context,
                &state.file_store,
                &state.task_service,
                &job.session_id,
                &job.item_id,
                &job.task_id,
                job.recovery_action.as_ref(),
            )?;
            return Ok(true);
        }
        let execution = state.begin_project_external_task(&context, &job.task_id)?;
        let processed_item = if job.batch_operation.is_some() {
            state
                .import_v2_service
                .run_item_with_recovery_in_batch_authorized(
                    &execution,
                    &state.file_store,
                    &state.task_service,
                    &job.session_id,
                    &job.task_id,
                    job.snapshot.clone(),
                    &job.target_reservations,
                    job.recovery_action.as_ref(),
                )?
        } else {
            state.import_v2_service.run_item_with_recovery_authorized(
                &execution,
                &state.file_store,
                &state.task_service,
                &job.session_id,
                &job.task_id,
                job.snapshot.clone(),
                &job.target_reservations,
                job.recovery_action.as_ref(),
            )?
        };
        let deferred_exact_duplicate = job.batch_operation.is_some()
            && processed_item
                .preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .is_some_and(|resolution| resolution.kind == ImportResolutionKind::ExactDuplicate);
        if deferred_exact_duplicate {
            if let Some(operation) = &job.batch_operation {
                operation
                    .duplicate_item_ids
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(job.item_id.clone());
            }
            return Ok(true);
        }
        state.require_current_execution_epoch(&context, &execution)?;
        let restricted_content_acknowledged = state
            .file_store
            .exists(&context, RESTRICTED_CONTENT_ACK_PATH);
        let git_cancellation = state
            .task_service
            .get_cancellation_token(&job.task_id)
            .unwrap_or_default();
        let duplicate_batch = if job.batch_operation.is_some() {
            None
        } else {
            state.with_current_project_write_access(
                &job.project_id,
                &job.project_root_path,
                |_permit, write_context| {
                    let canonical_project_identity =
                        crate::services::project_identity(&write_context.root)
                            .map_err(|error| {
                                BackendError::new("PROJECT_IDENTITY_FAILED", error, true, false)
                            })?
                            .canonical_identity_key;
                    state.blocking_work.run_project_git_blocking(
                        canonical_project_identity,
                        Some(&git_cancellation),
                        || {
                            state
                                .import_v2_service
                                .finalize_exact_duplicate_cancellable_authorized(
                                    &execution,
                                    &state.file_store,
                                    &state.git_service,
                                    &job.session_id,
                                    &job.item_id,
                                    &job.task_id,
                                    restricted_content_acknowledged,
                                    || state.task_service.is_cancelled(&job.task_id),
                                    || {
                                        if job.batch_operation.is_some() {
                                            Ok(())
                                        } else {
                                            state
                                                .task_service
                                                .transition_status(
                                                    &job.task_id,
                                                    TaskStatus::Running,
                                                )
                                                .map(|_| ())
                                                .map_err(|error| task_error(&error))
                                        }
                                    },
                                )
                        },
                    )
                },
            )?
        };
        if let Some(batch) = duplicate_batch {
            if job.batch_operation.is_none() {
                let history_path = import_session_context(
                    &state,
                    &job.project_id,
                    &job.project_root_path,
                    &job.session_id,
                )?
                .layout
                .import_paths()?
                .history_entry(&batch.batch_id)?;
                state
                    .task_service
                    .complete_running_with_result(
                        &job.task_id,
                        TaskResult {
                            summary: "Duplicate already exists; its locator was recorded.".into(),
                            affected_paths: vec![history_path],
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
        }
        Ok(false)
    });
    if let Some(_) = job.batch_operation {
        let outcome = match result {
            Ok(true) => return,
            Ok(false) => classify_batch_item_outcome(&state, job),
            Err(error) if error.code == crate::errors::IMPORT_V2_CANCELLED => {
                let _ = state.with_current_project_write_access(
                    &job.project_id,
                    &job.project_root_path,
                    |permit, _context| {
                        state.import_v2_service.cancel_batch_item_authorized(
                            permit,
                            &state.file_store,
                            &job.session_id,
                            &job.item_id,
                        )
                    },
                );
                ImportItemRunOutcome::Cancelled
            }
            Err(_) => classify_batch_item_outcome(&state, job),
        };
        finish_batch_worker(&state, job, outcome);
    } else {
        if let Err(error) = result {
            fail_task_unless_cancelled(&state, &job.task_id, error);
        }
        let _ = state.with_current_project_write_access(
            &job.project_id,
            &job.project_root_path,
            |permit, _context| {
                state
                    .import_v2_service
                    .refresh_session_action_groups_authorized(
                        permit,
                        &state.file_store,
                        &job.session_id,
                    )
            },
        );
    }
}

fn finish_batch_duplicate_cohort_if_drained(job: &ImportWorkerJob) {
    let Some(operation) = &job.batch_operation else {
        return;
    };
    if operation.remaining_workers.fetch_sub(1, Ordering::AcqRel) != 1 {
        return;
    }
    let duplicate_item_ids = std::mem::take(
        &mut *operation
            .duplicate_item_ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    if duplicate_item_ids.is_empty() {
        return;
    }
    let state = job.app.state::<AppState>();
    let result =
        (|| -> Result<Option<crate::models::import_v2::ImportBatchResult>, BackendError> {
            let context = state.resolve_project_context(&job.project_id, &job.project_root_path)?;
            let execution = state.begin_project_external_task(&context, &job.task_id)?;
            state.require_current_execution_epoch(&context, &execution)?;
            let restricted_content_acknowledged = state
                .file_store
                .exists(&context, RESTRICTED_CONTENT_ACK_PATH);
            let cancellation = state
                .task_service
                .get_cancellation_token(&job.task_id)
                .unwrap_or_default();
            state.with_current_project_write_access(
                &job.project_id,
                &job.project_root_path,
                |_permit, write_context| {
                    let canonical_project_identity =
                        crate::services::project_identity(&write_context.root)
                            .map_err(|error| {
                                BackendError::new("PROJECT_IDENTITY_FAILED", error, true, false)
                            })?
                            .canonical_identity_key;
                    state.blocking_work.run_project_git_blocking(
                        canonical_project_identity,
                        Some(&cancellation),
                        || {
                            state
                                .import_v2_service
                                .finalize_exact_duplicate_cohort_cancellable_authorized(
                                    &execution,
                                    &state.file_store,
                                    &state.git_service,
                                    &job.session_id,
                                    &duplicate_item_ids,
                                    &job.task_id,
                                    restricted_content_acknowledged,
                                    || state.task_service.is_cancelled(&job.task_id),
                                )
                        },
                    )
                },
            )
        })();

    for item_id in duplicate_item_ids {
        let mut duplicate_job = job.clone();
        duplicate_job.item_id = item_id;
        let outcome = match &result {
            Ok(batch) => match batch.as_ref().and_then(|batch| {
                batch
                    .items
                    .iter()
                    .find(|item| item.item_id == duplicate_job.item_id)
            }) {
                Some(item) if item.committed => classify_batch_item_outcome(&state, &duplicate_job),
                Some(item)
                    if item.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED) =>
                {
                    let _ = state.with_current_project_write_access(
                        &job.project_id,
                        &job.project_root_path,
                        |permit, _context| {
                            state.import_v2_service.cancel_batch_item_authorized(
                                permit,
                                &state.file_store,
                                &job.session_id,
                                &duplicate_job.item_id,
                            )
                        },
                    );
                    classify_batch_item_outcome(&state, &duplicate_job)
                }
                Some(_) => ImportItemRunOutcome::Failed,
                None => classify_batch_item_outcome(&state, &duplicate_job),
            },
            Err(error) if error.code == crate::errors::IMPORT_V2_CANCELLED => {
                let _ = state.with_current_project_write_access(
                    &job.project_id,
                    &job.project_root_path,
                    |permit, _context| {
                        state.import_v2_service.cancel_batch_item_authorized(
                            permit,
                            &state.file_store,
                            &job.session_id,
                            &duplicate_job.item_id,
                        )
                    },
                );
                ImportItemRunOutcome::Cancelled
            }
            Err(_) => classify_batch_item_outcome(&state, &duplicate_job),
        };
        finish_batch_worker(&state, &duplicate_job, outcome);
    }
}

fn classify_batch_item_outcome(state: &AppState, job: &ImportWorkerJob) -> ImportItemRunOutcome {
    let Ok(context) = state.resolve_project_context(&job.project_id, &job.project_root_path) else {
        return ImportItemRunOutcome::SystemicError;
    };
    let Ok(item) = state.import_v2_service.load_item(
        &context,
        &state.file_store,
        &job.session_id,
        &job.item_id,
    ) else {
        return ImportItemRunOutcome::SystemicError;
    };
    match item.status {
        ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge => {
            ImportItemRunOutcome::Ready
        }
        ImportItemStatus::WaitingCapability
        | ImportItemStatus::WaitingLogin
        | ImportItemStatus::WaitingAuthorization
        | ImportItemStatus::Paused => ImportItemRunOutcome::Waiting,
        ImportItemStatus::Cancelled | ImportItemStatus::Skipped => ImportItemRunOutcome::Cancelled,
        ImportItemStatus::Completed => ImportItemRunOutcome::Completed,
        ImportItemStatus::Failed => ImportItemRunOutcome::Failed,
        _ => ImportItemRunOutcome::SystemicError,
    }
}

pub(crate) fn finish_batch_worker(
    state: &AppState,
    job: &ImportWorkerJob,
    outcome: ImportItemRunOutcome,
) {
    let Some(operation) = &job.batch_operation else {
        return;
    };
    let item = state
        .resolve_project_context(&job.project_id, &job.project_root_path)
        .and_then(|context| {
            state.import_v2_service.load_item(
                &context,
                &state.file_store,
                &job.session_id,
                &job.item_id,
            )
        })
        .ok();
    // Serializing the item buffer around outcome recording guarantees that a
    // terminal patch cannot overtake an earlier worker's item snapshot.
    let mut pending_items = operation
        .pending_items
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(item) = item {
        pending_items.insert(item.item_id.clone(), item);
    }
    let mut control = BatchExecutionControl::new(
        &state.task_service,
        &job.task_id,
        Arc::clone(&operation.state),
    );
    let Ok((completed, total, summary, publish)) = control.record_outcome(outcome) else {
        return;
    };
    let patch_items = if publish {
        pending_items
            .drain()
            .map(|(_, item)| item)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    drop(pending_items);
    // A terminal patch can trigger an immediate overview read. Publish only
    // after the action projection describes the completed operation.
    if completed == total {
        let projection_result = state.with_current_project_write_access(
            &job.project_id,
            &job.project_root_path,
            |permit, _context| {
                state
                    .import_v2_service
                    .refresh_session_action_groups_authorized(
                        permit,
                        &state.file_store,
                        &job.session_id,
                    )
            },
        );
        if let Err(error) = projection_result {
            let _ = control.log(
                LogLevel::Error,
                format!(
                    "Import action projection could not be finalized: {}",
                    error.message
                ),
            );
        }
    }
    if publish {
        let _ = control.flush_progress(
            completed,
            total,
            format!("Processed {completed}/{total} import items"),
        );
        let _ = control.log(
            LogLevel::Info,
            format!(
                "Import batch progress: ready {}, completed {}, waiting {}, failed {}, cancelled {}.",
                summary.ready,
                summary.completed,
                summary.waiting,
                summary.failed,
                summary.cancelled
            ),
        );
        state
            .task_service
            .emit_import_session_patch(ImportSessionPatchEvent {
                project_id: job.project_id.clone(),
                project_root_path: job.project_root_path.clone(),
                session_id: job.session_id.clone(),
                batch_id: job.task_id.clone(),
                items: patch_items,
                counts: ImportSessionPatchCounts {
                    total,
                    processed: completed,
                    succeeded: summary.ready + summary.completed,
                    waiting: summary.waiting,
                    failed: summary.failed + summary.systemic_errors,
                    cancelled: summary.cancelled,
                },
            });
    }
    if completed != total {
        return;
    }
    let _ = control.flush_progress(total, total, "Import batch complete".into());
    let (terminal_status, terminal_error) = if summary.systemic_errors > 0 {
        (
            TaskStatus::Failed,
            Some(BackendError::new(
                "IMPORT_BATCH_SYSTEMIC_FAILURE",
                "One or more batch workers stopped before reporting an item outcome.",
                true,
                true,
            )),
        )
    } else {
        let status = batch_terminal_status(&summary);
        let error = (status == TaskStatus::Failed).then(|| {
            BackendError::new(
                "IMPORT_BATCH_ITEM_FAILURE",
                "One or more import items failed.",
                true,
                true,
            )
        });
        (status, error)
    };
    let result = TaskResult {
        summary: format!(
            "Import preparation completed: {} preview-ready, {} duplicate aliases recorded, {} waiting for attention, {} failed, {} cancelled.",
            summary.ready,
            summary.completed,
            summary.waiting,
            summary.failed,
            summary.cancelled
        ),
        affected_paths: Vec::new(),
        reference: Some(TaskResultReference::ImportV2SessionPreview {
            session_id: job.session_id.clone(),
            batch_id: None,
            completion: None,
        }),
        pending_action: None,
    };
    if let Err(error) = state.task_service.finish_running_operation(
        &job.task_id,
        result,
        terminal_status,
        terminal_error,
    ) {
        let _ = state.task_service.append_log(
            &job.task_id,
            LogLevel::Error,
            format!("Import operation could not publish its final state: {error}"),
        );
    }
}

pub(crate) fn fail_task_unless_cancelled(state: &AppState, task_id: &str, error: BackendError) {
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

fn task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_TASK_FAILED", message, true, false)
}

pub(crate) const RESTRICTED_CONTENT_ACK_PATH: &str = ".app/import-restricted-content-ack.json";

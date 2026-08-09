use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::sync::atomic::AtomicBool;

use chrono::Utc;
use uuid::Uuid;

use crate::models::task::{
    BackendTask, StreamDelta, TaskActivity, TaskOperation, TaskProgress, TaskResult, TaskStatus,
    TaskType,
};
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowErrorSummary, WorkflowExecutionState, WorkflowKind,
    WorkflowPendingAction, WorkflowPersistenceMode, WorkflowPersistenceTransition, WorkflowResult,
    WorkflowRun, WorkflowRunSummary, WorkflowStageStatus, WORKFLOW_SCHEMA_VERSION,
};
use crate::services::FileStore;
use crate::tasks::cancellation::CancellationRegistry;
use crate::tasks::task_events::EventBus;
use crate::tasks::task_model::{
    validate_transition, CancellationToken, LogLevel, LogLine, TaskEntry,
};
use crate::utils::path_safety::{
    ensure_project_directory, validate_existing_project_directory, validate_existing_project_file,
    validate_project_directory,
};

const PERSISTED_TASK_SCHEMA_VERSION: u32 = 2;
const WORKFLOW_PROGRESS_PERSISTENCE_WINDOW: Duration = Duration::from_millis(250);

#[derive(Default)]
struct WorkflowPersistenceLane {
    next_revision: u64,
    persisted_revision: u64,
    last_observational_write_ms: Option<u64>,
    pending_observational_revision: Option<u64>,
    trailing_flush_scheduled: bool,
    trailing_flush_generation: u64,
    pending_error: Option<String>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct WorkflowHistoryIndexKey {
    canonical_identity_key: String,
    identity_revision: String,
    kind: Option<WorkflowKind>,
    status: Option<WorkflowDisplayStatus>,
}

#[derive(Debug, Clone)]
struct WorkflowHistoryIndexEntry {
    started_at: String,
    task_id: String,
}

type WorkflowHistoryIndex = (u64, Arc<Vec<WorkflowHistoryIndexEntry>>);

struct WorkflowPersistenceClock {
    started_at: Instant,
    #[cfg(test)]
    manual_enabled: AtomicBool,
    #[cfg(test)]
    manual_ms: AtomicU64,
}

impl Default for WorkflowPersistenceClock {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            #[cfg(test)]
            manual_enabled: AtomicBool::new(false),
            #[cfg(test)]
            manual_ms: AtomicU64::new(0),
        }
    }
}

impl WorkflowPersistenceClock {
    fn now_ms(&self) -> u64 {
        #[cfg(test)]
        if self.manual_enabled.load(Ordering::SeqCst) {
            return self.manual_ms.load(Ordering::SeqCst);
        }
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    #[cfg(test)]
    fn advance(&self, duration: Duration) {
        self.manual_enabled.store(true, Ordering::SeqCst);
        self.manual_ms.fetch_add(
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
    }

    #[cfg(test)]
    fn reset(&self) {
        self.manual_enabled.store(true, Ordering::SeqCst);
        self.manual_ms.store(0, Ordering::SeqCst);
    }

    #[cfg(test)]
    fn is_manual(&self) -> bool {
        self.manual_enabled.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkflowMutationDurability {
    Barrier,
    ObservationalProgress,
}

#[cfg(test)]
struct PersistenceWriterGate {
    entered: Mutex<Option<std::sync::mpsc::SyncSender<()>>>,
    release: (Mutex<bool>, std::sync::Condvar),
}

#[cfg(test)]
impl PersistenceWriterGate {
    fn new() -> (Self, std::sync::mpsc::Receiver<()>) {
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(0);
        (
            Self {
                entered: Mutex::new(Some(entered_tx)),
                release: (Mutex::new(false), std::sync::Condvar::new()),
            },
            entered_rx,
        )
    }

    fn block_writer(&self) {
        if let Some(entered) = self.entered.lock().expect("lock poisoned").take() {
            let _ = entered.send(());
        }
        let (released, ready) = &self.release;
        let mut released = released.lock().expect("lock poisoned");
        while !*released {
            released = ready.wait(released).expect("lock poisoned");
        }
    }

    fn release(&self) {
        let (released, ready) = &self.release;
        *released.lock().expect("lock poisoned") = true;
        ready.notify_all();
    }
}

#[cfg(test)]
thread_local! {
    static TASK_PERSISTENCE_WRITES: Cell<usize> = const { Cell::new(0) };
    static TASK_EVENT_EMISSIONS: Cell<usize> = const { Cell::new(0) };
    static TASK_PERSISTENCE_NANOS: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_task_costs() {
    TASK_PERSISTENCE_WRITES.set(0);
    TASK_EVENT_EMISSIONS.set(0);
    TASK_PERSISTENCE_NANOS.set(0);
}

#[cfg(test)]
fn task_costs() -> (usize, usize) {
    (TASK_PERSISTENCE_WRITES.get(), TASK_EVENT_EMISSIONS.get())
}

#[cfg(test)]
fn task_persistence_nanos() -> u64 {
    TASK_PERSISTENCE_NANOS.get()
}

fn legacy_task_schema_version() -> u32 {
    1
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedTaskEntry {
    #[serde(default = "legacy_task_schema_version")]
    schema_version: u32,
    task: BackendTask,
    #[serde(default)]
    log_lines: Vec<LogLine>,
    #[serde(default)]
    activities: Vec<TaskActivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workflow: Option<WorkflowExecutionState>,
}

type RecoveredTaskSnapshot = (
    BackendTask,
    Vec<LogLine>,
    Vec<TaskActivity>,
    Option<WorkflowExecutionState>,
);

fn parse_persisted_task(json: &str, persisted_id: &str) -> Result<RecoveredTaskSnapshot, String> {
    let value = serde_json::from_str::<serde_json::Value>(json)
        .map_err(|error| format!("Invalid task JSON: {error}"))?;
    let is_wrapper = value
        .as_object()
        .is_some_and(|object| object.contains_key("task"));
    if !is_wrapper {
        let snapshot = serde_json::from_value::<BackendTask>(value)
            .map(|task| (task, Vec::new(), Vec::new(), None))
            .map_err(|error| format!("Invalid legacy task snapshot: {error}"))?;
        validate_recovered_task_id(&snapshot.0.id, persisted_id)?;
        return Ok(snapshot);
    }

    // Once a document identifies itself as a wrapper it must never fall back
    // to the raw legacy shape. Otherwise malformed or future wrapper fields
    // could be silently ignored by BackendTask's permissive deserializer.
    let entry = serde_json::from_value::<PersistedTaskEntry>(value)
        .map_err(|error| format!("Invalid persisted task wrapper: {error}"))?;
    if !matches!(entry.schema_version, 1 | PERSISTED_TASK_SCHEMA_VERSION) {
        return Err(format!(
            "Unsupported persisted task schema version: {}",
            entry.schema_version
        ));
    }
    if let Some(workflow) = entry.workflow.as_ref() {
        if workflow.schema_version != WORKFLOW_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported workflow execution schema version: {}",
                workflow.schema_version
            ));
        }
    }
    let snapshot = (
        entry.task,
        entry.log_lines,
        entry.activities,
        entry.workflow,
    );
    validate_recovered_task_id(&snapshot.0.id, persisted_id)?;
    Ok(snapshot)
}

fn validate_recovered_task_id(task_id: &str, persisted_id: &str) -> Result<(), String> {
    validate_task_persistence_id(task_id)?;
    validate_task_persistence_id(persisted_id)?;
    if task_id != persisted_id {
        return Err(format!(
            "Persisted task id does not match its file name: {task_id} != {persisted_id}"
        ));
    }
    Ok(())
}

fn validate_task_persistence_id(id: &str) -> Result<(), String> {
    if id.is_empty() || matches!(id, "." | "..") {
        return Err("Task persistence id is empty or reserved".into());
    }
    if id
        .chars()
        .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return Err("Task persistence id contains an unsafe file-name character".into());
    }
    if id
        .chars()
        .last()
        .is_some_and(|character| matches!(character, ' ' | '.'))
    {
        return Err("Task persistence id has an unsafe trailing character".into());
    }
    let base = id
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            });
    if reserved {
        return Err("Task persistence id is a reserved Windows file name".into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct TaskService {
    tasks: Arc<RwLock<HashMap<String, TaskEntry>>>,
    cancellation: Arc<CancellationRegistry>,
    event_bus: Arc<RwLock<EventBus>>,
    project_root: Arc<RwLock<Option<PathBuf>>>,
    task_roots: Arc<RwLock<HashMap<String, PathBuf>>>,
    task_persistence_dirs: Arc<RwLock<HashMap<String, PathBuf>>>,
    workflow_persistence_lanes: Arc<RwLock<HashMap<String, Arc<Mutex<WorkflowPersistenceLane>>>>>,
    workflow_persistence_clock: Arc<WorkflowPersistenceClock>,
    workflow_history_revision: Arc<AtomicU64>,
    workflow_history_indices: Arc<RwLock<HashMap<WorkflowHistoryIndexKey, WorkflowHistoryIndex>>>,
    #[cfg(test)]
    injected_persistence_failures: Arc<Mutex<HashMap<String, usize>>>,
    #[cfg(test)]
    active_persistence_writers: Arc<Mutex<HashMap<String, usize>>>,
    #[cfg(test)]
    peak_persistence_writers: Arc<Mutex<HashMap<String, usize>>>,
    #[cfg(test)]
    persistence_writer_gates: Arc<Mutex<HashMap<String, Arc<PersistenceWriterGate>>>>,
    #[cfg(test)]
    persistence_writes_with_task_lock: Arc<AtomicU64>,
    #[cfg(test)]
    persistence_write_counts: Arc<Mutex<HashMap<String, usize>>>,
    #[cfg(test)]
    trailing_flush_notifications: Arc<(Mutex<Vec<(String, u64)>>, std::sync::Condvar)>,
}

impl Default for TaskService {
    fn default() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            cancellation: Arc::new(CancellationRegistry::new()),
            event_bus: Arc::new(RwLock::new(EventBus::new_noop())),
            project_root: Arc::new(RwLock::new(None)),
            task_roots: Arc::new(RwLock::new(HashMap::new())),
            task_persistence_dirs: Arc::new(RwLock::new(HashMap::new())),
            workflow_persistence_lanes: Arc::new(RwLock::new(HashMap::new())),
            workflow_persistence_clock: Arc::new(WorkflowPersistenceClock::default()),
            workflow_history_revision: Arc::new(AtomicU64::new(0)),
            workflow_history_indices: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(test)]
            injected_persistence_failures: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            active_persistence_writers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            peak_persistence_writers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            persistence_writer_gates: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            persistence_writes_with_task_lock: Arc::new(AtomicU64::new(0)),
            #[cfg(test)]
            persistence_write_counts: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            trailing_flush_notifications: Arc::new((
                Mutex::new(Vec::new()),
                std::sync::Condvar::new(),
            )),
        }
    }
}

impl TaskService {
    pub fn with_event_bus(event_bus: EventBus) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            cancellation: Arc::new(CancellationRegistry::new()),
            event_bus: Arc::new(RwLock::new(event_bus)),
            project_root: Arc::new(RwLock::new(None)),
            task_roots: Arc::new(RwLock::new(HashMap::new())),
            task_persistence_dirs: Arc::new(RwLock::new(HashMap::new())),
            workflow_persistence_lanes: Arc::new(RwLock::new(HashMap::new())),
            workflow_persistence_clock: Arc::new(WorkflowPersistenceClock::default()),
            workflow_history_revision: Arc::new(AtomicU64::new(0)),
            workflow_history_indices: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(test)]
            injected_persistence_failures: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            active_persistence_writers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            peak_persistence_writers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            persistence_writer_gates: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            persistence_writes_with_task_lock: Arc::new(AtomicU64::new(0)),
            #[cfg(test)]
            persistence_write_counts: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            trailing_flush_notifications: Arc::new((
                Mutex::new(Vec::new()),
                std::sync::Condvar::new(),
            )),
        }
    }

    pub fn set_event_bus(&self, event_bus: EventBus) {
        let mut guard = self.event_bus.write().expect("lock poisoned");
        *guard = event_bus;
    }

    /// Set the active project root. When set, terminal task transitions auto-persist to
    /// `<root>/.app/tasks/<id>.json`, and any previously-persisted tasks are recovered.
    /// Pass `None` when the project is closed. In-memory tasks and cancellation tokens
    /// remain global for the process so work from a previous project stays visible and
    /// cancellable while the user switches projects.
    pub fn set_project_root(&self, root: Option<PathBuf>) -> Result<Vec<BackendTask>, String> {
        {
            let mut guard = self.project_root.write().expect("lock poisoned");
            *guard = root.clone();
        }
        if let Some(root_path) = root {
            self.recover_tasks(&root_path)?;
            return Ok(self.list_tasks_for_root(&root_path, None));
        }
        Ok(Vec::new())
    }

    pub fn set_project_context(
        &self,
        project_id: String,
        root: PathBuf,
        task_state_root: PathBuf,
    ) -> Result<Vec<BackendTask>, String> {
        let task_state_root = validate_persistence_dir(&root, &task_state_root)?;
        *self.project_root.write().expect("lock poisoned") = Some(root.clone());
        self.recover_tasks_from(&root, &task_state_root, Some(&project_id))?;
        Ok(self.list_tasks_for_root(&root, None))
    }

    pub fn current_project_root(&self) -> Option<PathBuf> {
        self.project_root.read().ok().and_then(|g| g.clone())
    }

    fn emit<T: serde::Serialize + Clone + Send + Sync + 'static>(
        &self,
        event_type: crate::models::task::BackendEventType,
        project_id: Option<String>,
        task_id: Option<String>,
        payload: T,
    ) {
        #[cfg(test)]
        TASK_EVENT_EMISSIONS.with(|count| count.set(count.get() + 1));
        self.event_bus
            .read()
            .expect("lock poisoned")
            .emit(event_type, project_id, task_id, payload);
    }

    pub fn emit_import_session_patch(
        &self,
        payload: crate::models::import_v2::ImportSessionPatchEvent,
    ) {
        self.emit(
            crate::models::task::BackendEventType::ImportSessionPatch,
            Some(payload.project_id.clone()),
            Some(payload.batch_id.clone()),
            payload,
        );
    }

    pub fn create_task(
        &self,
        task_type: TaskType,
        project_id: Option<String>,
        title: String,
        cancellable: bool,
    ) -> BackendTask {
        let project_root = self.current_project_root();
        let persistence_dir = project_root.as_ref().map(|root| root.join(".app/tasks"));
        self.create_task_internal(
            task_type,
            project_id,
            project_root,
            title,
            cancellable,
            None,
            None,
            false,
            None,
            persistence_dir,
        )
        .expect("non-project task creation cannot require persistence")
    }

    pub fn create_project_task(
        &self,
        task_type: TaskType,
        project_id: String,
        project_root: PathBuf,
        title: String,
        cancellable: bool,
    ) -> Result<BackendTask, String> {
        let persistence_dir = project_root.join(".app/tasks");
        self.create_task_internal(
            task_type,
            Some(project_id),
            Some(project_root),
            title,
            cancellable,
            None,
            None,
            true,
            None,
            Some(persistence_dir),
        )
    }

    /// A project-scoped read-only operation that must never create `.app`
    /// state. Used for restricted compatible-vault inventory work.
    pub fn create_memory_project_task(
        &self,
        task_type: TaskType,
        project_id: String,
        project_root: PathBuf,
        title: String,
        cancellable: bool,
    ) -> Result<BackendTask, String> {
        self.create_task_internal(
            task_type,
            Some(project_id),
            Some(project_root),
            title,
            cancellable,
            None,
            None,
            false,
            None,
            None,
        )
    }

    pub fn emit_project_refreshed<T: serde::Serialize + Clone + Send + Sync + 'static>(
        &self,
        project_id: String,
        summary: T,
    ) {
        self.emit(
            crate::models::task::BackendEventType::ProjectRefreshed,
            Some(project_id),
            None,
            summary,
        );
    }

    /// Create a project task associated with one user-level operation. The
    /// identity is persisted with the task so the UI can restore and control
    /// parallel import batches after navigation or application restart.
    pub fn create_project_task_with_batch(
        &self,
        task_type: TaskType,
        project_id: String,
        project_root: PathBuf,
        title: String,
        cancellable: bool,
        batch_id: String,
    ) -> Result<BackendTask, String> {
        let persistence_dir = project_root.join(".app/tasks");
        self.create_task_internal(
            task_type,
            Some(project_id),
            Some(project_root),
            title,
            cancellable,
            Some(batch_id),
            None,
            true,
            None,
            Some(persistence_dir),
        )
    }

    /// Create the single persisted task that owns one Import V2 operation.
    /// Its id is also the operation identity; typed metadata carries the
    /// session relationship and presentation facts.
    pub fn create_project_import_operation_task(
        &self,
        project_id: String,
        project_root: PathBuf,
        task_state_root: PathBuf,
        title: String,
        session_id: String,
        item_count: u64,
        source_label: Option<String>,
    ) -> Result<BackendTask, String> {
        self.create_task_internal(
            TaskType::Import,
            Some(project_id),
            Some(project_root),
            title,
            true,
            None,
            Some(TaskOperation::ImportBatch {
                session_id,
                item_count,
                source_label,
            }),
            true,
            None,
            Some(task_state_root),
        )
    }

    pub fn create_workflow_task(
        &self,
        project_id: String,
        project_root: PathBuf,
        title: String,
        workflow: WorkflowExecutionState,
        task_state_root: Option<PathBuf>,
    ) -> Result<WorkflowRun, String> {
        let require_persistence = task_state_root.is_some();
        let task = self.create_task_internal(
            TaskType::Workflow,
            Some(project_id),
            Some(project_root),
            title,
            true,
            None,
            None,
            require_persistence,
            Some(workflow),
            task_state_root,
        )?;
        let run = self
            .get_workflow_run(&task.id)
            .ok_or_else(|| format!("Workflow task state missing: {}", task.id))?;
        self.emit(
            crate::models::task::BackendEventType::WorkflowUpdated,
            task.project_id.clone(),
            Some(task.id.clone()),
            run.clone(),
        );
        Ok(run)
    }

    fn create_task_internal(
        &self,
        task_type: TaskType,
        project_id: Option<String>,
        project_root: Option<PathBuf>,
        title: String,
        cancellable: bool,
        batch_id: Option<String>,
        operation: Option<TaskOperation>,
        require_persistence: bool,
        workflow: Option<WorkflowExecutionState>,
        persistence_dir: Option<PathBuf>,
    ) -> Result<BackendTask, String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let token = self.cancellation.register(&id);
        let batch_id = operation.as_ref().map(|_| id.clone()).or(batch_id);

        let task = BackendTask {
            id: id.clone(),
            task_type: task_type.clone(),
            project_id: project_id.clone(),
            batch_id,
            operation,
            title,
            status: TaskStatus::Queued,
            progress: None,
            started_at: now.clone(),
            updated_at: now,
            completed_at: None,
            cancellable,
            log_path: None,
            result: None,
            error: None,
        };

        let entry = TaskEntry {
            task: task.clone(),
            cancellation: token,
            log_lines: Vec::new(),
            activities: Vec::new(),
            persisted_path: None,
            workflow,
        };

        self.tasks
            .write()
            .expect("lock poisoned")
            .insert(id.clone(), entry);
        if let Some(root) = project_root {
            self.task_roots
                .write()
                .expect("lock poisoned")
                .insert(id.clone(), root);
        }
        if let Some(dir) = persistence_dir {
            self.task_persistence_dirs
                .write()
                .expect("lock poisoned")
                .insert(id.clone(), dir);
        }
        if let Err(error) = self.persist_current_task(&id) {
            if require_persistence {
                self.tasks.write().expect("lock poisoned").remove(&id);
                self.task_roots.write().expect("lock poisoned").remove(&id);
                self.task_persistence_dirs
                    .write()
                    .expect("lock poisoned")
                    .remove(&id);
                self.cancellation.remove(&id);
                return Err(error);
            }
            eprintln!("Failed to persist new task {}: {}", id, error);
        }
        if task.task_type == TaskType::Workflow {
            self.bump_workflow_history_revision();
        }

        use crate::models::task::BackendEventType::TaskUpdated;
        self.emit(TaskUpdated, project_id, Some(id.clone()), task.clone());

        Ok(task)
    }

    pub fn get_task(&self, id: &str) -> Option<BackendTask> {
        self.tasks
            .read()
            .expect("lock poisoned")
            .get(id)
            .map(|e| e.task.clone())
    }

    pub fn list_tasks(&self, status_filter: Option<TaskStatus>) -> Vec<BackendTask> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let mut list: Vec<BackendTask> = tasks.values().map(|e| e.task.clone()).collect();
        if let Some(filter) = status_filter {
            list.retain(|t| t.status == filter);
        }
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub fn list_tasks_for_root(
        &self,
        project_root: &Path,
        status_filter: Option<TaskStatus>,
    ) -> Vec<BackendTask> {
        let canonical = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let roots = self.task_roots.read().expect("lock poisoned");
        let tasks = self.tasks.read().expect("lock poisoned");
        let mut list = tasks
            .iter()
            .filter(|(id, entry)| {
                roots
                    .get(*id)
                    .map(|root| root.canonicalize().unwrap_or_else(|_| root.clone()) == canonical)
                    .unwrap_or(false)
                    && status_filter
                        .as_ref()
                        .map(|status| &entry.task.status == status)
                        .unwrap_or(true)
            })
            .map(|(_, entry)| entry.task.clone())
            .collect::<Vec<_>>();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub fn get_workflow_run(&self, id: &str) -> Option<WorkflowRun> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks.get(id)?;
        entry.workflow.as_ref()?.to_run(&entry.task)
    }

    pub(crate) fn workflow_persistence_dir(&self, id: &str) -> Option<PathBuf> {
        self.task_persistence_dirs
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned()
    }

    pub(crate) fn rebind_workflow_persistence(
        &self,
        id: &str,
        project_root: &Path,
        task_state_root: Option<PathBuf>,
    ) -> Result<WorkflowPersistenceTransition, String> {
        let task = self
            .get_task(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        if task.task_type != TaskType::Workflow {
            return Err(format!("Task is not a workflow: {id}"));
        }
        if !self.task_belongs_to_root(id, project_root) {
            return Err(format!(
                "Workflow task does not belong to the asserted project: {id}"
            ));
        }
        let task_state_root = task_state_root
            .map(|root| validate_persistence_dir(project_root, &root))
            .transpose()?;
        self.rebind_workflow_persistence_ids(&[id.to_string()], task_state_root)
            .map(|mut transitions| transitions.pop().expect("one workflow transition").1)
    }

    pub(crate) fn rebind_workflows_for_root(
        &self,
        project_root: &Path,
        task_state_root: Option<PathBuf>,
    ) -> Result<Vec<(String, WorkflowPersistenceTransition)>, String> {
        let task_state_root = task_state_root
            .map(|root| validate_persistence_dir(project_root, &root))
            .transpose()?;
        let expected = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let ids = self
            .task_roots
            .read()
            .expect("lock poisoned")
            .iter()
            .filter_map(|(id, root)| {
                let actual = root.canonicalize().unwrap_or_else(|_| root.clone());
                (actual == expected).then(|| id.clone())
            })
            .collect::<Vec<_>>();
        self.rebind_workflow_persistence_ids(&ids, task_state_root)
    }

    fn rebind_workflow_persistence_ids(
        &self,
        ids: &[String],
        task_state_root: Option<PathBuf>,
    ) -> Result<Vec<(String, WorkflowPersistenceTransition)>, String> {
        let mut serialized_ids = ids.to_vec();
        serialized_ids.sort();
        serialized_ids.dedup();
        let lanes = serialized_ids
            .iter()
            .map(|id| self.workflow_persistence_lane(id))
            .collect::<Vec<_>>();
        let mut lane_guards = lanes
            .iter()
            .map(|lane| lane.lock().expect("lock poisoned"))
            .collect::<Vec<_>>();
        let mut task_updates = Vec::new();
        let mut log_events = Vec::new();
        let mut previous_states = Vec::new();
        let mut upgrade_ids = Vec::new();
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let mut persistence = self.task_persistence_dirs.write().expect("lock poisoned");
        let mut transitions = Vec::new();
        // Validate every target before publishing any persistence mutation.
        // A malformed later workflow must not leave earlier workflows rebound
        // to a new root while the caller reports failure.
        let targets = ids
            .iter()
            .filter_map(|id| tasks.get(id).map(|entry| (id, entry)))
            .filter(|(_, entry)| entry.task.task_type == TaskType::Workflow)
            .map(|(id, entry)| {
                let workflow = entry
                    .workflow
                    .as_ref()
                    .ok_or_else(|| format!("Workflow task state missing: {id}"))?;
                workflow
                    .to_run(&entry.task)
                    .ok_or_else(|| format!("Workflow task has no project id: {id}"))?;
                Ok(id.clone())
            })
            .collect::<Result<Vec<_>, String>>()?;
        for id in targets {
            let entry = tasks
                .get_mut(&id)
                .expect("validated workflow target must remain present while locked");
            let previous = persistence.get(&id);
            let transition = persistence_transition(previous, task_state_root.as_ref());
            previous_states.push((id.clone(), entry.clone(), previous.cloned()));
            let workflow = entry
                .workflow
                .as_mut()
                .expect("validated workflow target must retain workflow state while locked");
            match task_state_root.as_ref() {
                Some(root) => {
                    persistence.insert(id.clone(), root.clone());
                    workflow.persistence = WorkflowPersistenceMode::Persistent;
                }
                None => {
                    persistence.remove(&id);
                    workflow.persistence = WorkflowPersistenceMode::MemoryOnly;
                }
            }
            if transition == WorkflowPersistenceTransition::UpgradedToPersistent {
                upgrade_ids.push(id.clone());
            }
            transitions.push((id.clone(), transition));
            let Some((level, message)) = persistence_transition_log(transition) else {
                continue;
            };
            workflow.persistence_transition = Some(transition);
            let line = LogLine {
                timestamp: Utc::now().to_rfc3339(),
                level,
                message: message.into(),
            };
            entry.log_lines.push(line.clone());
            entry.task.updated_at = Utc::now().to_rfc3339();
            let project_id = entry.task.project_id.clone();
            let run = workflow
                .to_run(&entry.task)
                .expect("validated workflow target must retain a project id while locked");
            log_events.push((project_id.clone(), id.clone(), line));
            task_updates.push((project_id, id.clone(), run));
        }
        drop(persistence);
        drop(tasks);

        let mut disk_backups = HashMap::new();
        for id in &upgrade_ids {
            let backup = (|| -> Result<(PathBuf, PathBuf, Option<PersistedTaskEntry>), String> {
                let project_root = self
                    .task_roots
                    .read()
                    .expect("lock poisoned")
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("Workflow task has no project root: {id}"))?;
                let tasks_dir = task_state_root
                    .as_ref()
                    .expect("persistence upgrades require a task-state root")
                    .clone();
                let path = tasks_dir.join(format!("{id}.json"));
                let backup = match std::fs::symlink_metadata(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(format!(
                            "Failed to inspect workflow persistence transition target {}: {error}",
                            path.display()
                        ));
                    }
                    Ok(_) => {
                        let safe_dir =
                            validate_existing_project_directory(&project_root, &tasks_dir)
                                .map_err(|error| {
                                    format!(
                                        "Workflow persistence transition root is unsafe: {error}"
                                    )
                                })?;
                        let safe_path = validate_existing_project_file(&project_root, &path)
                            .map_err(|error| {
                                format!("Workflow persistence transition target is unsafe: {error}")
                            })?;
                        let bytes = std::fs::read(&safe_path).map_err(|error| {
                            format!(
                                "Failed to back up workflow persistence transition target {}: {error}",
                                safe_path.display()
                                )
                            })?;
                        if safe_path.parent() != Some(safe_dir.as_path())
                            || safe_path.file_name()
                                != Some(std::ffi::OsStr::new(&format!("{id}.json")))
                        {
                            return Err(format!(
                                "Workflow persistence transition target does not match its binding: {}",
                                safe_path.display()
                            ));
                        }
                        let json = String::from_utf8(bytes).map_err(|error| {
                            format!(
                                "Workflow persistence transition target is not UTF-8 {}: {error}",
                                safe_path.display()
                            )
                        })?;
                        let (task, log_lines, activities, workflow) =
                            parse_persisted_task(&json, id)?;
                        Some(PersistedTaskEntry {
                            schema_version: PERSISTED_TASK_SCHEMA_VERSION,
                            task,
                            log_lines,
                            activities,
                            workflow,
                        })
                    }
                };
                Ok((project_root, tasks_dir, backup))
            })();
            match backup {
                Ok(backup) => {
                    disk_backups.insert(id.clone(), backup);
                }
                Err(error) => {
                    self.restore_workflow_rebind_state(&previous_states);
                    return Err(error);
                }
            }
        }

        let mut written_upgrades: Vec<String> = Vec::new();
        for id in &upgrade_ids {
            let lane_index = serialized_ids
                .binary_search(id)
                .expect("workflow persistence lane must remain indexed");
            if let Err(error) =
                self.persist_current_task_with_lane(id, Some(&mut lane_guards[lane_index]))
            {
                let mut rollback_errors = Vec::new();
                for written_id in &written_upgrades {
                    let (project_root, tasks_dir, backup) = disk_backups
                        .get(written_id)
                        .expect("written upgrade must retain its disk backup");
                    let rollback = if let Some(previous) = backup {
                        self.write_persisted_task_snapshot(
                            project_root,
                            tasks_dir,
                            written_id,
                            previous,
                        )
                        .map(|_| ())
                    } else {
                        remove_persisted_task_snapshot(project_root, tasks_dir, written_id)
                    };
                    if let Err(rollback_error) = rollback {
                        rollback_errors.push(rollback_error);
                    }
                }
                self.restore_workflow_rebind_state(&previous_states);
                let rollback_detail = if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("; rollback errors: {}", rollback_errors.join(" | "))
                };
                return Err(format!("{error}{rollback_detail}"));
            }
            written_upgrades.push(id.clone());
        }
        for lane in &mut lane_guards {
            lane.pending_error = None;
            lane.pending_observational_revision = None;
            lane.trailing_flush_scheduled = false;
            lane.trailing_flush_generation = lane.trailing_flush_generation.saturating_add(1);
        }
        drop(lane_guards);
        for (project_id, task_id, line) in log_events {
            self.emit(
                crate::models::task::BackendEventType::TaskLog,
                project_id,
                Some(task_id),
                line,
            );
        }
        if !task_updates.is_empty() {
            self.bump_workflow_history_revision();
        }
        for (project_id, task_id, run) in task_updates {
            self.emit(
                crate::models::task::BackendEventType::WorkflowUpdated,
                project_id,
                Some(task_id),
                run,
            );
        }
        Ok(transitions)
    }

    fn restore_workflow_rebind_state(
        &self,
        previous_states: &[(String, TaskEntry, Option<PathBuf>)],
    ) {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let mut persistence = self.task_persistence_dirs.write().expect("lock poisoned");
        for (id, entry, previous_dir) in previous_states {
            tasks.insert(id.clone(), entry.clone());
            match previous_dir {
                Some(dir) => {
                    persistence.insert(id.clone(), dir.clone());
                }
                None => {
                    persistence.remove(id);
                }
            }
        }
    }

    pub(crate) fn record_workflow_persistence_transition(
        &self,
        id: &str,
        was_persistent: bool,
        is_persistent: bool,
    ) -> Result<(), String> {
        let persistence_lane = self.workflow_persistence_lane(id);
        let mut lane = persistence_lane.lock().expect("lock poisoned");
        let transition = match (was_persistent, is_persistent) {
            (true, false) => WorkflowPersistenceTransition::DowngradedToMemoryOnly,
            (false, true) => WorkflowPersistenceTransition::UpgradedToPersistent,
            _ => WorkflowPersistenceTransition::Unchanged,
        };
        let Some((level, message)) = persistence_transition_log(transition) else {
            return Ok(());
        };
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        let previous_entry = entry.clone();
        let workflow = entry
            .workflow
            .as_mut()
            .ok_or_else(|| format!("Task is not a workflow: {id}"))?;
        workflow.persistence = if is_persistent {
            WorkflowPersistenceMode::Persistent
        } else {
            WorkflowPersistenceMode::MemoryOnly
        };
        workflow.persistence_transition = Some(transition);
        let line = LogLine {
            timestamp: Utc::now().to_rfc3339(),
            level,
            message: message.into(),
        };
        entry.log_lines.push(line.clone());
        entry.task.updated_at = Utc::now().to_rfc3339();
        let project_id = entry.task.project_id.clone();
        let task_id = entry.task.id.clone();
        let run = workflow
            .to_run(&entry.task)
            .ok_or_else(|| format!("Workflow task has no project id: {id}"))?;
        drop(tasks);
        if is_persistent {
            if self.workflow_persistence_dir(id).is_none() {
                self.tasks
                    .write()
                    .expect("lock poisoned")
                    .insert(id.to_string(), previous_entry);
                return Err(format!(
                    "Workflow persistence transition has no task-state root: {id}"
                ));
            }
            if let Err(error) = self.persist_current_task_with_lane(id, Some(&mut lane)) {
                self.tasks
                    .write()
                    .expect("lock poisoned")
                    .insert(id.to_string(), previous_entry);
                return Err(error);
            }
        } else {
            lane.pending_observational_revision = None;
            lane.trailing_flush_scheduled = false;
            lane.trailing_flush_generation = lane.trailing_flush_generation.saturating_add(1);
            lane.pending_error = None;
        }
        self.emit(
            crate::models::task::BackendEventType::TaskLog,
            project_id.clone(),
            Some(task_id.clone()),
            line,
        );
        self.emit(
            crate::models::task::BackendEventType::WorkflowUpdated,
            project_id,
            Some(task_id),
            run,
        );
        Ok(())
    }

    pub(crate) fn workflow_execution_options(
        &self,
        id: &str,
    ) -> Option<crate::models::workflow::WorkflowExecutionOptions> {
        self.tasks
            .read()
            .expect("lock poisoned")
            .get(id)?
            .workflow
            .as_ref()
            .map(|workflow| workflow.execution_options.clone())
    }

    pub(crate) fn find_workflow_run_by_execution_options<F>(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        matches: F,
    ) -> Option<WorkflowRun>
    where
        F: Fn(&crate::models::workflow::WorkflowExecutionOptions) -> bool,
    {
        let tasks = self.tasks.read().expect("lock poisoned");
        tasks.values().find_map(|entry| {
            let workflow = entry.workflow.as_ref()?;
            if workflow.canonical_identity_key != canonical_identity_key
                || workflow.identity_revision != identity_revision
                || !matches(&workflow.execution_options)
            {
                return None;
            }
            workflow.to_run(&entry.task)
        })
    }

    pub(crate) fn workflow_execution_state(
        &self,
        id: &str,
    ) -> Option<crate::models::workflow::WorkflowExecutionState> {
        self.tasks
            .read()
            .expect("lock poisoned")
            .get(id)?
            .workflow
            .clone()
    }

    pub fn task_belongs_to_root(&self, id: &str, project_root: &Path) -> bool {
        let expected = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        self.task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .map(|root| root.canonicalize().unwrap_or_else(|_| root.clone()) == expected)
            .unwrap_or(false)
    }

    pub fn project_root_for_task(&self, id: &str) -> Option<PathBuf> {
        self.task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned()
    }

    pub fn list_workflow_runs(&self) -> Vec<WorkflowRun> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let mut runs = tasks
            .values()
            .filter_map(|entry| entry.workflow.as_ref()?.to_run(&entry.task))
            .collect::<Vec<_>>();
        runs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        runs
    }

    pub fn page_workflow_runs(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        kind: Option<WorkflowKind>,
        status: Option<WorkflowDisplayStatus>,
        after: Option<(&str, &str)>,
        limit: usize,
    ) -> (Vec<WorkflowRunSummary>, bool) {
        let key = WorkflowHistoryIndexKey {
            canonical_identity_key: canonical_identity_key.to_string(),
            identity_revision: identity_revision.to_string(),
            kind,
            status,
        };
        let revision = self.workflow_history_revision.load(Ordering::Acquire);
        let cached = self
            .workflow_history_indices
            .read()
            .expect("lock poisoned")
            .get(&key)
            .filter(|(cached_revision, _)| *cached_revision == revision)
            .map(|(_, entries)| Arc::clone(entries));
        let entries = cached.unwrap_or_else(|| {
            let tasks = self.tasks.read().expect("lock poisoned");
            let mut entries = tasks
                .values()
                .filter_map(|entry| {
                    let workflow = entry.workflow.as_ref()?;
                    if workflow.canonical_identity_key != key.canonical_identity_key
                        || workflow.identity_revision != key.identity_revision
                        || key.kind.as_ref().is_some_and(|kind| &workflow.kind != kind)
                    {
                        return None;
                    }
                    let run = workflow.to_run(&entry.task)?;
                    if key
                        .status
                        .as_ref()
                        .is_some_and(|status| &run.display_status != status)
                    {
                        return None;
                    }
                    Some(WorkflowHistoryIndexEntry {
                        started_at: run.started_at,
                        task_id: run.task_id,
                    })
                })
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| {
                right
                    .started_at
                    .cmp(&left.started_at)
                    .then_with(|| right.task_id.cmp(&left.task_id))
            });
            let entries = Arc::new(entries);
            self.workflow_history_indices
                .write()
                .expect("lock poisoned")
                .insert(key.clone(), (revision, Arc::clone(&entries)));
            entries
        });
        let start = after.map_or(0, |cursor| {
            entries.partition_point(|entry| {
                (entry.started_at.as_str(), entry.task_id.as_str()) >= cursor
            })
        });
        let end = start.saturating_add(limit).min(entries.len());
        let tasks = self.tasks.read().expect("lock poisoned");
        let runs = entries[start..end]
            .iter()
            .filter_map(|entry| {
                let task = tasks.get(&entry.task_id)?;
                let run = task.workflow.as_ref()?.to_run(&task.task)?;
                Some(WorkflowRunSummary::from(&run))
            })
            .collect();
        (runs, end < entries.len())
    }

    fn bump_workflow_history_revision(&self) {
        self.workflow_history_revision
            .fetch_add(1, Ordering::AcqRel);
        self.workflow_history_indices
            .write()
            .expect("lock poisoned")
            .clear();
    }

    fn workflow_persistence_lane(&self, id: &str) -> Arc<Mutex<WorkflowPersistenceLane>> {
        if let Some(lane) = self
            .workflow_persistence_lanes
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned()
        {
            return lane;
        }
        self.workflow_persistence_lanes
            .write()
            .expect("lock poisoned")
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(WorkflowPersistenceLane::default())))
            .clone()
    }

    fn workflow_persistence_lane_if_present(
        &self,
        id: &str,
    ) -> Option<Arc<Mutex<WorkflowPersistenceLane>>> {
        self.tasks
            .read()
            .expect("lock poisoned")
            .get(id)
            .is_some_and(|entry| entry.workflow.is_some())
            .then(|| self.workflow_persistence_lane(id))
    }

    pub(crate) fn mutate_workflow<F>(&self, id: &str, mutate: F) -> Result<WorkflowRun, String>
    where
        F: FnOnce(&mut BackendTask, &mut WorkflowExecutionState) -> Result<(), String>,
    {
        self.mutate_workflow_with_durability(id, WorkflowMutationDurability::Barrier, mutate)
    }

    fn mutate_workflow_with_durability<F>(
        &self,
        id: &str,
        durability: WorkflowMutationDurability,
        mutate: F,
    ) -> Result<WorkflowRun, String>
    where
        F: FnOnce(&mut BackendTask, &mut WorkflowExecutionState) -> Result<(), String>,
    {
        let persistence_lane = self.workflow_persistence_lane(id);
        let mut lane = persistence_lane.lock().expect("lock poisoned");
        let project_root = self
            .task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let mut tasks = self.tasks.write().expect("lock poisoned");
        // Hold the task write lock while reading its persistence binding. A
        // concurrent authority rebind takes the same locks in this order, so
        // it cannot return while an in-flight mutation still owns an old dir.
        let persistence_dir = self
            .task_persistence_dirs
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        let previous_task = entry.task.clone();
        let previous_workflow = entry.workflow.clone();
        let workflow = entry
            .workflow
            .as_mut()
            .ok_or_else(|| format!("Task is not a workflow: {id}"))?;

        mutate(&mut entry.task, workflow)?;
        entry.task.updated_at = Utc::now().to_rfc3339();
        if matches!(
            entry.task.status,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) && entry.task.completed_at.is_none()
        {
            entry.task.completed_at = Some(entry.task.updated_at.clone());
        }
        let history_changed = previous_task.status != entry.task.status;

        let persisted = PersistedTaskEntry {
            schema_version: PERSISTED_TASK_SCHEMA_VERSION,
            task: entry.task.clone(),
            log_lines: entry.log_lines.clone(),
            activities: entry.activities.clone(),
            workflow: entry.workflow.clone(),
        };
        lane.next_revision = lane.next_revision.saturating_add(1);
        let revision = lane.next_revision;
        let task = entry.task.clone();
        let run = entry
            .workflow
            .as_ref()
            .and_then(|workflow| workflow.to_run(&task))
            .ok_or_else(|| format!("Workflow task has no project: {id}"))?;
        drop(tasks);

        let now_ms = self.workflow_persistence_clock.now_ms();
        let should_persist = match durability {
            WorkflowMutationDurability::Barrier => persistence_dir.is_some(),
            WorkflowMutationDurability::ObservationalProgress => {
                persistence_dir.is_some()
                    && lane.last_observational_write_ms.is_none_or(|last| {
                        now_ms.saturating_sub(last)
                            >= u64::try_from(WORKFLOW_PROGRESS_PERSISTENCE_WINDOW.as_millis())
                                .unwrap_or(u64::MAX)
                    })
            }
        };
        let mut trailing_flush = None;
        if should_persist {
            let tasks_dir = persistence_dir
                .as_deref()
                .expect("persistence was required only with a task-state root");
            let project_root = project_root
                .as_deref()
                .ok_or_else(|| format!("Workflow task has no project root: {id}"))?;
            match self.write_persisted_task_snapshot(project_root, tasks_dir, id, &persisted) {
                Ok(path) => {
                    lane.persisted_revision = revision;
                    lane.pending_error = None;
                    if durability == WorkflowMutationDurability::ObservationalProgress {
                        lane.last_observational_write_ms = Some(now_ms);
                        lane.pending_observational_revision = None;
                        lane.trailing_flush_scheduled = false;
                        lane.trailing_flush_generation =
                            lane.trailing_flush_generation.saturating_add(1);
                    } else {
                        lane.pending_observational_revision = None;
                        lane.trailing_flush_scheduled = false;
                        lane.trailing_flush_generation =
                            lane.trailing_flush_generation.saturating_add(1);
                    }
                    if let Some(entry) = self.tasks.write().expect("lock poisoned").get_mut(id) {
                        entry.persisted_path = Some(path);
                    }
                }
                Err(error) if durability == WorkflowMutationDurability::ObservationalProgress => {
                    // Progress remains a live in-memory fact. The error is retained
                    // on the task's persistence lane; the next barrier retries the
                    // latest revision before it can publish any barrier event.
                    lane.pending_error = Some(error);
                    lane.pending_observational_revision = Some(revision);
                    if !lane.trailing_flush_scheduled {
                        lane.trailing_flush_scheduled = true;
                        lane.trailing_flush_generation =
                            lane.trailing_flush_generation.saturating_add(1);
                        trailing_flush = Some((
                            WORKFLOW_PROGRESS_PERSISTENCE_WINDOW,
                            lane.trailing_flush_generation,
                        ));
                    }
                }
                Err(error) => {
                    let mut tasks = self.tasks.write().expect("lock poisoned");
                    let entry = tasks
                        .get_mut(id)
                        .ok_or_else(|| format!("Task disappeared during persistence: {id}"))?;
                    entry.task = previous_task;
                    entry.workflow = previous_workflow;
                    lane.pending_error = Some(error.clone());
                    return Err(error);
                }
            }
        } else if persistence_dir.is_none() {
            lane.persisted_revision = revision;
            lane.pending_observational_revision = None;
            lane.trailing_flush_scheduled = false;
            lane.trailing_flush_generation = lane.trailing_flush_generation.saturating_add(1);
            lane.pending_error = None;
        } else if durability == WorkflowMutationDurability::ObservationalProgress {
            lane.pending_observational_revision = Some(revision);
            if !lane.trailing_flush_scheduled {
                lane.trailing_flush_scheduled = true;
                let window_ms = u64::try_from(WORKFLOW_PROGRESS_PERSISTENCE_WINDOW.as_millis())
                    .unwrap_or(u64::MAX);
                let elapsed_ms = lane
                    .last_observational_write_ms
                    .map(|last| now_ms.saturating_sub(last))
                    .unwrap_or_default();
                lane.trailing_flush_generation = lane.trailing_flush_generation.saturating_add(1);
                trailing_flush = Some((
                    Duration::from_millis(window_ms.saturating_sub(elapsed_ms)),
                    lane.trailing_flush_generation,
                ));
            }
        }

        if history_changed {
            self.bump_workflow_history_revision();
        }
        self.emit(
            crate::models::task::BackendEventType::WorkflowUpdated,
            task.project_id.clone(),
            Some(id.to_string()),
            run.clone(),
        );
        let task_event = match task.status {
            TaskStatus::Succeeded => crate::models::task::BackendEventType::TaskCompleted,
            TaskStatus::Failed => crate::models::task::BackendEventType::TaskFailed,
            TaskStatus::Cancelled => crate::models::task::BackendEventType::TaskCancelled,
            TaskStatus::WaitingForConfirmation => {
                crate::models::task::BackendEventType::ConfirmationRequested
            }
            _ => crate::models::task::BackendEventType::TaskUpdated,
        };
        self.emit(
            task_event,
            task.project_id.clone(),
            Some(id.to_string()),
            task,
        );
        drop(lane);
        if let Some((delay, generation)) = trailing_flush {
            self.schedule_workflow_progress_flush(id.to_string(), delay, generation);
        }
        Ok(run)
    }

    fn schedule_workflow_progress_flush(&self, id: String, delay: Duration, generation: u64) {
        #[cfg(test)]
        if self.workflow_persistence_clock.is_manual() {
            return;
        }
        let service = self.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                tokio::time::sleep(delay).await;
                service.flush_pending_workflow_progress(&id, generation);
            });
        } else {
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                service.flush_pending_workflow_progress(&id, generation);
            });
        }
    }

    fn flush_pending_workflow_progress(&self, id: &str, generation: u64) {
        let Some(persistence_lane) = self.workflow_persistence_lane_if_present(id) else {
            return;
        };
        let mut lane = persistence_lane.lock().expect("lock poisoned");
        if lane.trailing_flush_generation != generation {
            return;
        }
        let Some(revision) = lane.pending_observational_revision else {
            lane.trailing_flush_scheduled = false;
            return;
        };
        let project_root = self
            .task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let persistence_dir = self
            .task_persistence_dirs
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let persisted = self
            .tasks
            .read()
            .expect("lock poisoned")
            .get(id)
            .map(|entry| PersistedTaskEntry {
                schema_version: PERSISTED_TASK_SCHEMA_VERSION,
                task: entry.task.clone(),
                log_lines: entry.log_lines.clone(),
                activities: entry.activities.clone(),
                workflow: entry.workflow.clone(),
            });
        let result = match (project_root, persistence_dir, persisted) {
            (Some(project_root), Some(tasks_dir), Some(persisted)) => {
                self.write_persisted_task_snapshot(&project_root, &tasks_dir, id, &persisted)
            }
            _ => {
                lane.pending_observational_revision = None;
                lane.trailing_flush_scheduled = false;
                return;
            }
        };
        match result {
            Ok(path) => {
                lane.persisted_revision = revision;
                lane.last_observational_write_ms = Some(self.workflow_persistence_clock.now_ms());
                lane.pending_observational_revision = None;
                lane.pending_error = None;
                if let Some(entry) = self.tasks.write().expect("lock poisoned").get_mut(id) {
                    entry.persisted_path = Some(path);
                }
            }
            Err(error) => lane.pending_error = Some(error),
        }
        lane.trailing_flush_scheduled = false;
        #[cfg(test)]
        {
            let (completed, ready) = &*self.trailing_flush_notifications;
            completed
                .lock()
                .expect("lock poisoned")
                .push((id.to_string(), generation));
            ready.notify_all();
        }
    }

    pub fn set_workflow_queue_state(
        &self,
        id: &str,
        queue_position: Option<u32>,
        continuation_required: bool,
    ) -> Result<WorkflowRun, String> {
        self.mutate_workflow(id, |_, workflow| {
            workflow.queue_position = queue_position;
            workflow.continuation_required = continuation_required;
            Ok(())
        })
    }

    pub fn transition_workflow_status(
        &self,
        id: &str,
        status: TaskStatus,
    ) -> Result<WorkflowRun, String> {
        self.mutate_workflow(id, |task, workflow| {
            validate_transition(&task.status, &status)?;
            task.status = status.clone();
            if status == TaskStatus::Running {
                workflow.queue_position = None;
                workflow.continuation_required = false;
                workflow.cancelled_from_queue = false;
                workflow.undo_cancel_until = None;
            }
            Ok(())
        })
    }

    pub fn start_workflow_stage(&self, id: &str, stage_id: &str) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            require_running_workflow(task, cancellation.as_ref(), id)?;
            if workflow.current_stage_id.is_some()
                || workflow.stages.iter().any(|stage| {
                    matches!(
                        stage.status,
                        WorkflowStageStatus::Running | WorkflowStageStatus::Waiting
                    )
                })
            {
                return Err(format!("Workflow already has an active stage: {id}"));
            }
            let target_ordinal = workflow
                .stages
                .iter()
                .find(|stage| stage.id == stage_id)
                .ok_or_else(|| format!("Workflow stage not found: {stage_id}"))?
                .ordinal;
            if workflow.stages.iter().any(|stage| {
                stage.ordinal < target_ordinal
                    && !matches!(
                        stage.status,
                        WorkflowStageStatus::Completed | WorkflowStageStatus::Skipped
                    )
            }) {
                return Err(format!(
                    "Earlier workflow stages are incomplete: {stage_id}"
                ));
            }
            let stage = workflow
                .stages
                .iter_mut()
                .find(|stage| stage.id == stage_id)
                .expect("workflow stage was resolved above");
            if stage.status != WorkflowStageStatus::Pending {
                return Err(format!("Workflow stage is not pending: {stage_id}"));
            }
            stage.status = WorkflowStageStatus::Running;
            stage
                .started_at
                .get_or_insert_with(|| Utc::now().to_rfc3339());
            stage.completed_at = None;
            workflow.current_stage_id = Some(stage_id.to_string());
            Ok(())
        })
    }

    pub fn update_workflow_stage_progress(
        &self,
        id: &str,
        stage_id: &str,
        current_item: Option<String>,
        current: u64,
        total: Option<u64>,
    ) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow_with_durability(
            id,
            WorkflowMutationDurability::ObservationalProgress,
            |task, workflow| {
                require_running_workflow(task, cancellation.as_ref(), id)?;
                require_current_stage(workflow, stage_id)?;
                let stage = workflow
                    .stages
                    .iter_mut()
                    .find(|stage| stage.id == stage_id)
                    .ok_or_else(|| format!("Workflow stage not found: {stage_id}"))?;
                if stage.status != WorkflowStageStatus::Running {
                    return Err(format!("Workflow stage is not running: {stage_id}"));
                }
                stage.current_item = current_item;
                stage.progress =
                    Some(crate::models::workflow::WorkflowCountProgress { current, total });
                Ok(())
            },
        )
    }

    pub fn complete_workflow_stage(&self, id: &str, stage_id: &str) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            require_running_workflow(task, cancellation.as_ref(), id)?;
            require_current_stage(workflow, stage_id)?;
            let stage = workflow
                .stages
                .iter_mut()
                .find(|stage| stage.id == stage_id)
                .ok_or_else(|| format!("Workflow stage not found: {stage_id}"))?;
            if stage.status != WorkflowStageStatus::Running {
                return Err(format!("Workflow stage is not running: {stage_id}"));
            }
            stage.status = WorkflowStageStatus::Completed;
            stage.completed_at = Some(Utc::now().to_rfc3339());
            stage.decision = None;
            workflow.current_stage_id = None;
            Ok(())
        })
    }

    pub fn skip_workflow_stage(&self, id: &str, stage_id: &str) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            require_running_workflow(task, cancellation.as_ref(), id)?;
            if workflow.current_stage_id.is_some() {
                return Err(format!("Workflow already has an active stage: {id}"));
            }
            let target_ordinal = workflow
                .stages
                .iter()
                .find(|stage| stage.id == stage_id)
                .ok_or_else(|| format!("Workflow stage not found: {stage_id}"))?
                .ordinal;
            if workflow.stages.iter().any(|stage| {
                stage.ordinal < target_ordinal
                    && !matches!(
                        stage.status,
                        WorkflowStageStatus::Completed | WorkflowStageStatus::Skipped
                    )
            }) {
                return Err(format!(
                    "Earlier workflow stages are incomplete: {stage_id}"
                ));
            }
            let now = Utc::now().to_rfc3339();
            let stage = workflow
                .stages
                .iter_mut()
                .find(|stage| stage.id == stage_id)
                .expect("workflow stage was resolved above");
            if stage.status != WorkflowStageStatus::Pending {
                return Err(format!("Workflow stage is not pending: {stage_id}"));
            }
            stage.status = WorkflowStageStatus::Skipped;
            stage.started_at = Some(now.clone());
            stage.completed_at = Some(now);
            stage.current_item = None;
            stage.progress = None;
            stage.decision = None;
            Ok(())
        })
    }

    pub fn set_task_cancellable(&self, id: &str, cancellable: bool) -> Result<BackendTask, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, _| {
            if !cancellable {
                require_running_workflow(task, cancellation.as_ref(), id)?;
            }
            task.cancellable = cancellable;
            Ok(())
        })?;
        self.get_task(id)
            .ok_or_else(|| format!("Task not found: {id}"))
    }

    pub fn fail_workflow_stage(
        &self,
        id: &str,
        stage_id: &str,
        error: WorkflowErrorSummary,
    ) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            require_running_workflow(task, cancellation.as_ref(), id)?;
            require_current_stage(workflow, stage_id)?;
            let stage = workflow
                .stages
                .iter_mut()
                .find(|stage| stage.id == stage_id)
                .ok_or_else(|| format!("Workflow stage not found: {stage_id}"))?;
            if stage.status != WorkflowStageStatus::Running {
                return Err(format!("Workflow stage is not running: {stage_id}"));
            }
            validate_transition(&task.status, &TaskStatus::Failed)?;
            stage.status = WorkflowStageStatus::Failed;
            stage.completed_at = Some(Utc::now().to_rfc3339());
            workflow.current_stage_id = Some(stage_id.to_string());
            workflow.error = Some(error);
            task.status = TaskStatus::Failed;
            Ok(())
        })
    }

    pub fn wait_workflow_stage(
        &self,
        id: &str,
        stage_id: &str,
        pending: WorkflowPendingAction,
    ) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            require_running_workflow(task, cancellation.as_ref(), id)?;
            require_current_stage(workflow, stage_id)?;
            let stage = workflow
                .stages
                .iter_mut()
                .find(|stage| stage.id == stage_id)
                .ok_or_else(|| format!("Workflow stage not found: {stage_id}"))?;
            if stage.status != WorkflowStageStatus::Running {
                return Err(format!("Workflow stage is not running: {stage_id}"));
            }
            validate_transition(&task.status, &TaskStatus::WaitingForConfirmation)?;
            stage.status = WorkflowStageStatus::Waiting;
            stage.decision = Some(pending.clone());
            workflow.current_stage_id = Some(stage_id.to_string());
            workflow.pending_action = Some(pending);
            task.status = TaskStatus::WaitingForConfirmation;
            Ok(())
        })
    }

    pub fn clear_workflow_pending_action(&self, id: &str) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            if cancellation
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            {
                return Err(format!("Workflow cancellation was requested: {id}"));
            }
            if task.status != TaskStatus::WaitingForConfirmation {
                return Err(format!("Workflow is not waiting for confirmation: {id}"));
            }
            workflow.pending_action = None;
            if let Some(stage_id) = workflow.current_stage_id.as_deref() {
                if let Some(stage) = workflow
                    .stages
                    .iter_mut()
                    .find(|stage| stage.id == stage_id)
                {
                    stage.decision = None;
                    stage.status = WorkflowStageStatus::Running;
                }
            }
            task.status = TaskStatus::Running;
            Ok(())
        })
    }

    pub(crate) fn begin_confirmed_workflow_apply(&self, id: &str) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            if cancellation
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            {
                return Err(format!("Workflow cancellation was requested: {id}"));
            }
            if task.status != TaskStatus::WaitingForConfirmation || !task.cancellable {
                return Err(format!("Workflow is not confirmable: {id}"));
            }
            workflow.pending_action = None;
            if let Some(stage_id) = workflow.current_stage_id.as_deref() {
                if let Some(stage) = workflow
                    .stages
                    .iter_mut()
                    .find(|stage| stage.id == stage_id)
                {
                    stage.decision = None;
                    stage.status = WorkflowStageStatus::Running;
                }
            }
            task.status = TaskStatus::Running;
            task.cancellable = false;
            Ok(())
        })
    }

    pub fn complete_workflow(
        &self,
        id: &str,
        result: WorkflowResult,
    ) -> Result<WorkflowRun, String> {
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            require_running_workflow(task, cancellation.as_ref(), id)?;
            if workflow.current_stage_id.is_some()
                || workflow.pending_action.is_some()
                || workflow.stages.iter().any(|stage| {
                    !matches!(
                        stage.status,
                        WorkflowStageStatus::Completed | WorkflowStageStatus::Skipped
                    )
                })
            {
                return Err(format!(
                    "Workflow cannot finish before every stage is completed or skipped: {id}"
                ));
            }
            validate_transition(&task.status, &TaskStatus::Succeeded)?;
            workflow.result = Some(result);
            workflow.pending_action = None;
            workflow.current_stage_id = None;
            task.status = TaskStatus::Succeeded;
            Ok(())
        })
    }

    pub fn request_workflow_cancel(&self, id: &str) -> Result<WorkflowRun, String> {
        let run = self
            .get_workflow_run(id)
            .ok_or_else(|| format!("Workflow not found: {id}"))?;
        let updated = match run.display_status {
            crate::models::workflow::WorkflowDisplayStatus::Running => {
                self.mutate_workflow(id, |task, _| {
                    if !task.cancellable {
                        return Err(format!("Task is not cancellable: {id}"));
                    }
                    task.status = TaskStatus::Cancelling;
                    Ok(())
                })
            }
            crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation => self
                .mutate_workflow(id, |task, workflow| {
                    if !task.cancellable {
                        return Err(format!("Task is not cancellable: {id}"));
                    }
                    task.status = TaskStatus::Cancelling;
                    workflow.pending_action = None;
                    Ok(())
                }),
            _ => Ok(run),
        }?;
        self.cancellation.cancel(id);
        Ok(updated)
    }

    pub(crate) fn finalize_workflow_cancellation(&self, id: &str) -> Result<WorkflowRun, String> {
        self.mutate_workflow(id, |task, workflow| {
            if matches!(
                task.status,
                TaskStatus::Succeeded
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::Interrupted
            ) {
                return Ok(());
            }
            if task.status != TaskStatus::Cancelling || !self.cancellation.is_cancelled(id) {
                return Err(format!(
                    "Workflow cancellation has not reached a finalizable state: {id}"
                ));
            }
            task.status = TaskStatus::Cancelled;
            task.cancellable = false;
            workflow.pending_action = None;
            workflow.queue_position = None;
            workflow.continuation_required = false;
            workflow.error = None;
            for stage in &mut workflow.stages {
                stage.decision = None;
            }
            Ok(())
        })
    }

    pub(crate) fn reject_workflow_dispatch(
        &self,
        id: &str,
        status: TaskStatus,
        error: WorkflowErrorSummary,
    ) -> Result<WorkflowRun, String> {
        self.reject_workflow_execution(id, status, error, false)
    }

    pub(crate) fn interrupt_workflow_confirmation(
        &self,
        id: &str,
        error: WorkflowErrorSummary,
    ) -> Result<WorkflowRun, String> {
        self.reject_workflow_execution(id, TaskStatus::Interrupted, error, true)
    }

    fn reject_workflow_execution(
        &self,
        id: &str,
        status: TaskStatus,
        error: WorkflowErrorSummary,
        allow_waiting_confirmation: bool,
    ) -> Result<WorkflowRun, String> {
        if !matches!(status, TaskStatus::Failed | TaskStatus::Interrupted) {
            return Err("Dispatch rejection must use Failed or Interrupted".into());
        }
        let cancellation = self.cancellation.get(id);
        self.mutate_workflow(id, |task, workflow| {
            if cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                return Err(format!("Workflow cancellation was requested: {id}"));
            }
            let eligible = task.status == TaskStatus::Running
                || (allow_waiting_confirmation
                    && task.status == TaskStatus::WaitingForConfirmation);
            if !eligible {
                return Err(format!(
                    "Workflow is not rejectable from its current state: {id}"
                ));
            }
            let now = Utc::now().to_rfc3339();
            let stage_index = workflow
                .current_stage_id
                .as_deref()
                .and_then(|current| workflow.stages.iter().position(|stage| stage.id == current))
                .or_else(|| {
                    workflow
                        .stages
                        .iter()
                        .position(|stage| stage.status == WorkflowStageStatus::Pending)
                });
            if let Some(stage_index) = stage_index {
                let stage = &mut workflow.stages[stage_index];
                stage.status = WorkflowStageStatus::Failed;
                stage.started_at.get_or_insert_with(|| now.clone());
                stage.completed_at = Some(now);
                stage.decision = None;
                workflow.current_stage_id = Some(stage.id.clone());
            }
            workflow.pending_action = None;
            workflow.queue_position = None;
            workflow.continuation_required = false;
            workflow.error = Some(error.clone());
            task.error = Some(crate::errors::BackendError::new(
                error.code.clone(),
                error.message_key.clone(),
                error.recoverable,
                error.user_action_required,
            ));
            task.status = status.clone();
            task.cancellable = false;
            Ok(())
        })
    }

    /// Trust revocation cancels only active workflow execution for the
    /// asserted project root. Queued runs remain queued and must pass a fresh
    /// authority check before dispatch.
    pub(crate) fn request_cancel_active_workflows_for_root(
        &self,
        project_root: &Path,
    ) -> Result<(), String> {
        let expected = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let ids = self
            .list_workflow_runs()
            .into_iter()
            .filter(|run| {
                matches!(
                    run.display_status,
                    crate::models::workflow::WorkflowDisplayStatus::Running
                        | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
                ) && self
                    .project_root_for_task(&run.task_id)
                    .map(|root| root.canonicalize().unwrap_or(root) == expected)
                    .unwrap_or(false)
            })
            .map(|run| run.task_id)
            .collect::<Vec<_>>();
        for id in ids {
            self.request_workflow_cancel(&id)?;
        }
        Ok(())
    }

    pub(crate) fn reset_workflow_cancellation(&self, id: &str) -> Result<(), String> {
        let token = self.cancellation.register(id);
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        entry.cancellation = token;
        Ok(())
    }

    pub fn transition_status(
        &self,
        id: &str,
        new_status: TaskStatus,
    ) -> Result<BackendTask, String> {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;

        validate_transition(&entry.task.status, &new_status)?;

        entry.task.status = new_status.clone();
        entry.task.updated_at = Utc::now().to_rfc3339();

        if matches!(
            new_status,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) {
            entry.task.completed_at = Some(Utc::now().to_rfc3339());
        }

        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();

        let event_type = match &new_status {
            TaskStatus::WaitingForConfirmation => {
                crate::models::task::BackendEventType::ConfirmationRequested
            }
            TaskStatus::Succeeded => crate::models::task::BackendEventType::TaskCompleted,
            TaskStatus::Failed => crate::models::task::BackendEventType::TaskFailed,
            TaskStatus::Cancelled => crate::models::task::BackendEventType::TaskCancelled,
            _ => crate::models::task::BackendEventType::TaskUpdated,
        };

        drop(tasks);
        self.emit(event_type, pid.clone(), Some(tid.clone()), task.clone());

        self.persist_current_task(&tid)?;

        Ok(task)
    }

    pub fn update_progress(
        &self,
        id: &str,
        current: u64,
        total: Option<u64>,
        label: Option<String>,
    ) -> Result<BackendTask, String> {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;

        entry.task.progress = Some(TaskProgress {
            current,
            total,
            label,
        });
        entry.task.updated_at = Utc::now().to_rfc3339();

        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();

        drop(tasks);
        use crate::models::task::BackendEventType::TaskUpdated;
        self.emit(TaskUpdated, pid, Some(tid), task.clone());
        self.persist_current_task(id)?;

        Ok(task)
    }

    pub fn append_log(&self, id: &str, level: LogLevel, message: String) -> Result<(), String> {
        let persistence_lane = self.workflow_persistence_lane_if_present(id);
        let mut lane = persistence_lane
            .as_ref()
            .map(|lane| lane.lock().expect("lock poisoned"));
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;

        let line = LogLine {
            timestamp: Utc::now().to_rfc3339(),
            level,
            message,
        };

        entry.log_lines.push(line.clone());
        entry.task.updated_at = Utc::now().to_rfc3339();
        let pid = entry.task.project_id.clone();
        let tid = entry.task.id.clone();

        drop(tasks);
        use crate::models::task::BackendEventType::TaskLog;
        if lane.is_some() {
            self.persist_current_task_with_lane(id, lane.as_deref_mut())?;
            self.emit(TaskLog, pid, Some(tid), line);
        } else {
            self.emit(TaskLog, pid, Some(tid), line);
            self.persist_current_task(id)?;
        }

        Ok(())
    }

    pub fn get_logs(&self, id: &str) -> Result<Vec<LogLine>, String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        Ok(entry.log_lines.clone())
    }

    pub fn get_activities(&self, id: &str) -> Result<Vec<TaskActivity>, String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        Ok(entry.activities.clone())
    }

    /// Emit a safe structured Agent activity and persist it with the task
    /// snapshot. Activity payloads are intentionally bounded and never carry
    /// raw model reasoning, tool arguments, file contents, or command output.
    pub fn emit_activity(&self, id: &str, activity: TaskActivity) {
        let persistence_lane = self.workflow_persistence_lane_if_present(id);
        let mut lane = persistence_lane
            .as_ref()
            .map(|lane| lane.lock().expect("lock poisoned"));
        let (pid, tid) = {
            let mut tasks = self.tasks.write().expect("lock poisoned");
            let Some(entry) = tasks.get_mut(id) else {
                return;
            };
            entry.activities.push(activity.clone());
            entry.task.updated_at = Utc::now().to_rfc3339();
            (entry.task.project_id.clone(), entry.task.id.clone())
        };
        self.emit(
            crate::models::task::BackendEventType::TaskActivity,
            pid,
            Some(tid),
            activity,
        );
        if lane.is_some() {
            let _ = self.persist_current_task_with_lane(id, lane.as_deref_mut());
        } else {
            let _ = self.persist_current_task(id);
        }
    }

    /// Emit an ephemeral streaming delta for a generative task (chat answer
    /// token-by-token). Unlike [`append_log`](Self::append_log), this does NOT
    /// push into `log_lines` and does NOT persist to disk — fine-grained
    /// generation deltas would flood the task log store with thousands of
    /// disk writes. The authoritative answer is persisted in the chat session
    /// file on completion; these deltas are only live UI hints.
    pub fn emit_stream_delta(&self, id: &str, delta: StreamDelta) {
        let (pid, tid) = {
            let tasks = self.tasks.read().expect("lock poisoned");
            let Some(entry) = tasks.get(id) else {
                return;
            };
            (entry.task.project_id.clone(), entry.task.id.clone())
        };
        self.emit(
            crate::models::task::BackendEventType::TaskStreamOutput,
            pid,
            Some(tid),
            delta,
        );
    }

    pub fn cancel_task(&self, id: &str) -> Result<BackendTask, String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        if !entry.task.cancellable {
            return Err(format!("Task is not cancellable: {}", id));
        }
        if matches!(
            entry.task.status,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) {
            // Idempotent: the task is already in a terminal state, so the
            // caller's intent (stop the task) is already satisfied. Return
            // the current snapshot instead of rejecting — otherwise a fast-
            // failing task surfaces a confusing "cannot cancel" error when
            // the user clicks Cancel after the failure already landed.
            return Ok(entry.task.clone());
        }
        let status = entry.task.status.clone();
        drop(tasks);

        self.cancellation.cancel(id);

        if matches!(status, TaskStatus::Queued | TaskStatus::Cancelling) {
            self.transition_status(id, TaskStatus::Cancelled)
        } else {
            self.transition_status(id, TaskStatus::Cancelling)?;
            self.transition_status(id, TaskStatus::Cancelled)
        }
    }

    /// Request cancellation for a long-running task without publishing a
    /// terminal state before its worker has finished cleanup. The worker is
    /// responsible for transitioning to Failed/Cancelled after it observes
    /// the token, so callers cannot start a replacement operation while the
    /// original still owns its resources.
    pub fn request_cancel(&self, id: &str) -> Result<BackendTask, String> {
        self.request_cancel_with_previous_status(id)
            .map(|(task, _)| task)
    }

    /// Atomic cancellation request plus the status observed before the
    /// request. Domain coordinators use the previous status to distinguish an
    /// active worker (which must finish cleanup) from an already-drained
    /// attention task (which the command can settle itself).
    pub fn request_cancel_with_previous_status(
        &self,
        id: &str,
    ) -> Result<(BackendTask, TaskStatus), String> {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        if !entry.task.cancellable {
            return Err(format!("Task is not cancellable: {id}"));
        }
        let previous_status = entry.task.status.clone();
        if matches!(
            entry.task.status,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) {
            return Ok((entry.task.clone(), previous_status));
        }
        entry.cancellation.cancel();
        let next_status = if previous_status == TaskStatus::Queued {
            TaskStatus::Cancelled
        } else if previous_status == TaskStatus::Cancelling {
            TaskStatus::Cancelling
        } else {
            TaskStatus::Cancelling
        };
        entry.task.status = next_status.clone();
        let now = Utc::now().to_rfc3339();
        entry.task.updated_at = now.clone();
        if next_status == TaskStatus::Cancelled {
            entry.task.completed_at = Some(now);
        }
        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();
        drop(tasks);
        self.persist_current_task(id)?;
        let event_type = if next_status == TaskStatus::Cancelled {
            crate::models::task::BackendEventType::TaskCancelled
        } else {
            crate::models::task::BackendEventType::TaskUpdated
        };
        self.emit(event_type, pid, Some(tid), task.clone());
        Ok((task, previous_status))
    }

    /// Publish the terminal state after a deferred cancellation has reached a
    /// worker-owned safe point. This deliberately does not request
    /// cancellation: callers must first use `request_cancel`, then finish any
    /// cleanup before calling this method.
    pub fn finalize_cancellation(&self, id: &str) -> Result<BackendTask, String> {
        let task = self
            .get_task(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        if matches!(
            task.status,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) {
            return Ok(task);
        }
        if task.status != TaskStatus::Cancelling || !self.cancellation.is_cancelled(id) {
            return Err(format!(
                "Task cancellation has not reached a finalizable state: {id}"
            ));
        }
        self.transition_status(id, TaskStatus::Cancelled)
    }

    pub fn set_result(&self, id: &str, result: TaskResult) -> Result<BackendTask, String> {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        entry.task.result = Some(result);
        entry.task.updated_at = Utc::now().to_rfc3339();
        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();
        drop(tasks);
        self.emit(
            crate::models::task::BackendEventType::TaskUpdated,
            pid,
            Some(tid),
            task.clone(),
        );
        self.persist_current_task(id)?;
        Ok(task)
    }

    /// Atomically installs a result and completes a running task. This is used
    /// at cancellation-sensitive boundaries where `set_result` followed by a
    /// separate status transition would allow a Cancelled task to retain a
    /// successful result.
    pub fn complete_running_with_result(
        &self,
        id: &str,
        result: TaskResult,
    ) -> Result<BackendTask, String> {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        if entry.task.status != TaskStatus::Running || entry.cancellation.is_cancelled() {
            return Err(format!("Task is no longer running: {id}"));
        }
        let previous = entry.task.clone();
        let now = Utc::now().to_rfc3339();
        entry.task.result = Some(result);
        entry.task.status = TaskStatus::Succeeded;
        entry.task.updated_at = now.clone();
        entry.task.completed_at = Some(now);
        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();
        drop(tasks);
        if let Err(error) = self.persist_current_task(id) {
            let mut tasks = self.tasks.write().expect("lock poisoned");
            if let Some(entry) = tasks.get_mut(id) {
                entry.task = previous;
            }
            drop(tasks);
            let _ = self.persist_current_task(id);
            return Err(error);
        }
        self.emit(
            crate::models::task::BackendEventType::TaskCompleted,
            pid,
            Some(tid),
            task.clone(),
        );
        Ok(task)
    }

    /// Atomically finish a multi-item operation. Cancellation wins if its
    /// shared token was set before this lock is acquired, so the final worker
    /// cannot strand the task in `cancelling` between result and status writes.
    pub fn finish_running_operation(
        &self,
        id: &str,
        result: TaskResult,
        desired_status: TaskStatus,
        error: Option<crate::errors::BackendError>,
    ) -> Result<BackendTask, String> {
        if !matches!(
            desired_status,
            TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::WaitingForConfirmation
        ) {
            return Err("Operation finish status is invalid".into());
        }
        if (desired_status == TaskStatus::Failed) != error.is_some() {
            return Err("Operation failure status and error must agree".into());
        }

        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        let cancelled =
            entry.task.status == TaskStatus::Cancelling || entry.cancellation.is_cancelled();
        if !cancelled && entry.task.status != TaskStatus::Running {
            return Err(format!("Task is no longer running: {id}"));
        }
        let previous = entry.task.clone();
        let now = Utc::now().to_rfc3339();
        if cancelled {
            entry.task.status = TaskStatus::Cancelled;
            entry.task.result = None;
            entry.task.error = None;
            entry.task.completed_at = Some(now.clone());
        } else {
            entry.task.status = desired_status.clone();
            entry.task.result = Some(result);
            entry.task.error = error;
            entry.task.completed_at =
                matches!(desired_status, TaskStatus::Succeeded | TaskStatus::Failed)
                    .then(|| now.clone());
        }
        entry.task.updated_at = now;
        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();
        drop(tasks);

        if let Err(error) = self.persist_current_task(id) {
            let mut tasks = self.tasks.write().expect("lock poisoned");
            if let Some(entry) = tasks.get_mut(id) {
                entry.task = previous;
            }
            drop(tasks);
            let _ = self.persist_current_task(id);
            return Err(error);
        }
        let event_type = match task.status {
            TaskStatus::Succeeded => crate::models::task::BackendEventType::TaskCompleted,
            TaskStatus::Failed => crate::models::task::BackendEventType::TaskFailed,
            TaskStatus::Cancelled => crate::models::task::BackendEventType::TaskCancelled,
            TaskStatus::WaitingForConfirmation => {
                crate::models::task::BackendEventType::ConfirmationRequested
            }
            _ => unreachable!("validated operation finish status"),
        };
        self.emit(event_type, pid, Some(tid), task.clone());
        Ok(task)
    }

    pub fn set_error(
        &self,
        id: &str,
        error: crate::errors::BackendError,
    ) -> Result<BackendTask, String> {
        let persistence_lane = self.workflow_persistence_lane_if_present(id);
        let mut lane = persistence_lane
            .as_ref()
            .map(|lane| lane.lock().expect("lock poisoned"));
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let entry = tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        entry.task.error = Some(error);
        entry.task.updated_at = Utc::now().to_rfc3339();
        let task = entry.task.clone();
        let pid = task.project_id.clone();
        let tid = task.id.clone();
        drop(tasks);
        if lane.is_some() {
            self.persist_current_task_with_lane(id, lane.as_deref_mut())?;
        } else {
            self.persist_current_task(id)?;
        }
        self.emit(
            crate::models::task::BackendEventType::TaskUpdated,
            pid,
            Some(tid),
            task.clone(),
        );
        Ok(task)
    }

    pub fn get_cancellation_token(&self, id: &str) -> Option<CancellationToken> {
        self.cancellation.get(id)
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        self.cancellation.is_cancelled(id)
    }

    pub fn remove_completed(&self) -> usize {
        let mut tasks = self.tasks.write().expect("lock poisoned");
        let before = tasks.len();
        let mut removed_ids = Vec::new();
        let mut removed_paths = Vec::new();
        let mut removed_workflow = false;
        tasks.retain(|id, entry| {
            let is_terminal = matches!(
                entry.task.status,
                TaskStatus::Succeeded
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::Interrupted
            );
            if is_terminal {
                removed_workflow |= entry.workflow.is_some();
                removed_ids.push(id.clone());
                if let Some(path) = &entry.persisted_path {
                    removed_paths.push((id.clone(), path.clone()));
                }
            }
            !is_terminal
        });
        let removed_count = before - tasks.len();
        drop(tasks);

        // Clean up persisted files and cancellation tokens for removed tasks.
        for id in &removed_ids {
            self.cancellation.remove(id);
        }
        for (id, path) in &removed_paths {
            let _ = self.remove_persisted_task_file(id, path);
        }
        self.task_roots
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !removed_ids.contains(id));
        self.task_persistence_dirs
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !removed_ids.contains(id));
        self.workflow_persistence_lanes
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !removed_ids.contains(id));
        if removed_workflow {
            self.bump_workflow_history_revision();
        }
        removed_count
    }

    pub fn remove_completed_for_root(&self, project_root: &Path) -> usize {
        let canonical = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let matching_ids = {
            let roots = self.task_roots.read().expect("lock poisoned");
            let tasks = self.tasks.read().expect("lock poisoned");
            tasks
                .iter()
                .filter(|(id, entry)| {
                    matches!(
                        entry.task.status,
                        TaskStatus::Succeeded
                            | TaskStatus::Failed
                            | TaskStatus::Cancelled
                            | TaskStatus::Interrupted
                    ) && roots
                        .get(*id)
                        .map(|root| {
                            root.canonicalize().unwrap_or_else(|_| root.clone()) == canonical
                        })
                        .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };
        let mut removed_paths = Vec::new();
        let mut removed_workflow = false;
        {
            let mut tasks = self.tasks.write().expect("lock poisoned");
            for id in &matching_ids {
                if let Some(entry) = tasks.remove(id) {
                    removed_workflow |= entry.workflow.is_some();
                    if let Some(path) = entry.persisted_path {
                        removed_paths.push((id.clone(), path));
                    }
                }
            }
        }
        for (id, path) in &removed_paths {
            let _ = self.remove_persisted_task_file(id, path);
        }
        {
            let mut roots = self.task_roots.write().expect("lock poisoned");
            let mut persistence_dirs = self.task_persistence_dirs.write().expect("lock poisoned");
            for id in &matching_ids {
                roots.remove(id);
                persistence_dirs.remove(id);
                self.cancellation.remove(id);
            }
        }
        self.workflow_persistence_lanes
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !matching_ids.contains(id));
        if removed_workflow {
            self.bump_workflow_history_revision();
        }
        matching_ids.len()
    }

    /// Remove tasks that were prepared for a batch which failed before any
    /// worker was started. This is intentionally narrower than user-facing
    /// completed-task cleanup and must never be used for running work.
    pub fn discard_unstarted_tasks(&self, ids: &[String]) -> Result<(), String> {
        let persisted_paths = {
            let tasks = self.tasks.read().expect("lock poisoned");
            let mut paths = Vec::new();
            for id in ids {
                let entry = tasks
                    .get(id)
                    .ok_or_else(|| format!("Task not found: {id}"))?;
                if entry.task.status != TaskStatus::Queued {
                    return Err(format!("Task is not queued: {id}"));
                }
                if let Some(path) = &entry.persisted_path {
                    paths.push((id.clone(), path.clone()));
                }
            }
            paths
        };
        for (id, path) in &persisted_paths {
            self.remove_persisted_task_file(id, path)?;
        }
        self.tasks
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !ids.contains(id));
        self.task_roots
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !ids.contains(id));
        self.task_persistence_dirs
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !ids.contains(id));
        self.workflow_persistence_lanes
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !ids.contains(id));
        for id in ids {
            self.cancellation.remove(id);
        }
        Ok(())
    }

    fn remove_persisted_task_file(&self, id: &str, path: &Path) -> Result<(), String> {
        let project_root = self
            .task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| format!("Task has no project root: {id}"))?;
        let persistence_dir = self
            .task_persistence_dirs
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| format!("Task has no persistence directory: {id}"))?;
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Failed to inspect persisted task {}: {error}",
                    path.display()
                ));
            }
            Ok(_) => {}
        }
        let persistence_dir = validate_existing_project_directory(&project_root, &persistence_dir)
            .map_err(|error| format!("Task persistence path is unsafe: {error}"))?;
        let path = validate_existing_project_file(&project_root, path)
            .map_err(|error| format!("Task persistence entry is unsafe: {error}"))?;
        let expected_name = format!("{id}.json");
        if path.parent() != Some(persistence_dir.as_path())
            || path.file_name() != Some(std::ffi::OsStr::new(&expected_name))
        {
            return Err(format!(
                "Task persistence entry does not match its task binding: {}",
                path.display()
            ));
        }

        // As with writes, the path-based standard-library API cannot hold all
        // parent directories open with no-follow semantics. Revalidating the
        // directory and exact regular file immediately before remove narrows
        // the remaining replacement window and fails closed on observed drift.
        std::fs::remove_file(&path).map_err(|error| {
            format!(
                "Failed to remove persisted task {}: {error}",
                path.display()
            )
        })
    }

    pub fn persist_task(&self, id: &str, project_root: &Path) -> Result<(), String> {
        let tasks_dir = project_root.join(".app").join("tasks");
        let tasks_dir = validate_persistence_dir(project_root, &tasks_dir)?;
        self.persist_task_to_dir(id, project_root, &tasks_dir)?;
        self.task_persistence_dirs
            .write()
            .expect("lock poisoned")
            .insert(id.to_string(), tasks_dir);
        Ok(())
    }

    fn write_persisted_task_snapshot(
        &self,
        project_root: &Path,
        tasks_dir: &Path,
        id: &str,
        persisted: &PersistedTaskEntry,
    ) -> Result<PathBuf, String> {
        #[cfg(test)]
        {
            *self
                .persistence_write_counts
                .lock()
                .expect("lock poisoned")
                .entry(id.to_string())
                .or_default() += 1;
            if self.tasks.try_write().is_err() {
                self.persistence_writes_with_task_lock
                    .fetch_add(1, Ordering::SeqCst);
            }
            let active = {
                let mut writers = self
                    .active_persistence_writers
                    .lock()
                    .expect("lock poisoned");
                let active = writers.entry(id.to_string()).or_default();
                *active += 1;
                *active
            };
            let mut peaks = self.peak_persistence_writers.lock().expect("lock poisoned");
            let peak = peaks.entry(id.to_string()).or_default();
            *peak = (*peak).max(active);
        }
        #[cfg(test)]
        let writer_gate = self
            .persistence_writer_gates
            .lock()
            .expect("lock poisoned")
            .remove(id);
        #[cfg(test)]
        if let Some(gate) = writer_gate {
            gate.block_writer();
        }
        #[cfg(test)]
        let injected_failure = {
            let mut failures = self
                .injected_persistence_failures
                .lock()
                .expect("lock poisoned");
            failures.get_mut(id).is_some_and(|remaining| {
                if *remaining == 0 {
                    return false;
                }
                *remaining -= 1;
                true
            })
        };
        #[cfg(not(test))]
        let injected_failure = false;
        let result = if injected_failure {
            Err(format!("Injected task persistence failure: {id}"))
        } else {
            write_persisted_task(project_root, tasks_dir, id, persisted)
        };
        #[cfg(test)]
        {
            let mut writers = self
                .active_persistence_writers
                .lock()
                .expect("lock poisoned");
            let active = writers
                .get_mut(id)
                .expect("writer instrumentation must remain registered");
            *active = active.saturating_sub(1);
        }
        result
    }

    #[cfg(test)]
    fn advance_workflow_persistence_clock(&self, duration: Duration) {
        self.workflow_persistence_clock.advance(duration);
    }

    #[cfg(test)]
    fn reset_workflow_progress_persistence_window(&self, id: &str) {
        self.workflow_persistence_clock.reset();
        let lane = self.workflow_persistence_lane(id);
        let mut lane = lane.lock().expect("lock poisoned");
        lane.last_observational_write_ms = None;
        lane.pending_observational_revision = None;
        lane.trailing_flush_scheduled = false;
        lane.trailing_flush_generation = lane.trailing_flush_generation.saturating_add(1);
    }

    #[cfg(test)]
    fn inject_task_persistence_failures(&self, id: &str, count: usize) {
        self.injected_persistence_failures
            .lock()
            .expect("lock poisoned")
            .insert(id.to_string(), count);
    }

    #[cfg(test)]
    fn gate_next_persistence_write(
        &self,
        id: &str,
    ) -> (Arc<PersistenceWriterGate>, std::sync::mpsc::Receiver<()>) {
        let (gate, entered) = PersistenceWriterGate::new();
        let gate = Arc::new(gate);
        self.persistence_writer_gates
            .lock()
            .expect("lock poisoned")
            .insert(id.to_string(), Arc::clone(&gate));
        (gate, entered)
    }

    #[cfg(test)]
    fn persistence_writer_metrics(&self, id: &str) -> (usize, u64) {
        let peak = self
            .peak_persistence_writers
            .lock()
            .expect("lock poisoned")
            .get(id)
            .copied()
            .unwrap_or_default();
        let writes_with_task_lock = self
            .persistence_writes_with_task_lock
            .load(Ordering::SeqCst);
        (peak, writes_with_task_lock)
    }

    #[cfg(test)]
    fn persistence_write_count(&self, id: &str) -> usize {
        self.persistence_write_counts
            .lock()
            .expect("lock poisoned")
            .get(id)
            .copied()
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn wait_for_trailing_flush(&self, id: &str, timeout: Duration) {
        let generation = self
            .workflow_persistence_lane(id)
            .lock()
            .expect("lock poisoned")
            .trailing_flush_generation;
        let (completed, ready) = &*self.trailing_flush_notifications;
        let completed = completed.lock().expect("lock poisoned");
        let (completed, wait) = ready
            .wait_timeout_while(completed, timeout, |completed| {
                !completed.iter().any(|(task_id, completed_generation)| {
                    task_id == id && *completed_generation == generation
                })
            })
            .expect("lock poisoned");
        assert!(
            !wait.timed_out()
                && completed.iter().any(|(task_id, completed_generation)| {
                    task_id == id && *completed_generation == generation
                }),
            "trailing persistence flush did not complete for {id} generation {generation}"
        );
    }

    fn persist_task_to_dir(
        &self,
        id: &str,
        project_root: &Path,
        tasks_dir: &Path,
    ) -> Result<(), String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;

        let persisted = PersistedTaskEntry {
            schema_version: PERSISTED_TASK_SCHEMA_VERSION,
            task: entry.task.clone(),
            log_lines: entry.log_lines.clone(),
            activities: entry.activities.clone(),
            workflow: entry.workflow.clone(),
        };
        let path = write_persisted_task(project_root, tasks_dir, id, &persisted)?;

        drop(tasks);
        let mut tasks = self.tasks.write().expect("lock poisoned");
        if let Some(entry) = tasks.get_mut(id) {
            entry.persisted_path = Some(path);
        }

        Ok(())
    }

    fn persist_current_task(&self, id: &str) -> Result<(), String> {
        let persistence_lane = self.workflow_persistence_lane_if_present(id);
        let mut lane = persistence_lane
            .as_ref()
            .map(|lane| lane.lock().expect("lock poisoned"));
        self.persist_current_task_with_lane(id, lane.as_deref_mut())
    }

    fn persist_current_task_with_lane(
        &self,
        id: &str,
        mut lane: Option<&mut WorkflowPersistenceLane>,
    ) -> Result<(), String> {
        let project_root = self
            .task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let Some(project_root) = project_root else {
            return Ok(());
        };
        let tasks = self.tasks.read().expect("lock poisoned");
        let persistence = self.task_persistence_dirs.read().expect("lock poisoned");
        let Some(dir) = persistence.get(id) else {
            return Ok(());
        };
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        let persisted = PersistedTaskEntry {
            schema_version: PERSISTED_TASK_SCHEMA_VERSION,
            task: entry.task.clone(),
            log_lines: entry.log_lines.clone(),
            activities: entry.activities.clone(),
            workflow: entry.workflow.clone(),
        };
        let dir = validate_persistence_dir(&project_root, dir)?;
        drop(persistence);
        drop(tasks);
        let revision = lane.as_mut().map(|lane| {
            lane.next_revision = lane.next_revision.saturating_add(1);
            lane.next_revision
        });
        let path = match self.write_persisted_task_snapshot(&project_root, &dir, id, &persisted) {
            Ok(path) => path,
            Err(error) => {
                if let Some(lane) = lane.as_mut() {
                    lane.pending_error = Some(error.clone());
                }
                return Err(error);
            }
        };
        if let (Some(lane), Some(revision)) = (lane.as_mut(), revision) {
            lane.persisted_revision = revision;
            lane.pending_observational_revision = None;
            lane.trailing_flush_scheduled = false;
            lane.trailing_flush_generation = lane.trailing_flush_generation.saturating_add(1);
            lane.pending_error = None;
        }
        if let Some(entry) = self.tasks.write().expect("lock poisoned").get_mut(id) {
            entry.persisted_path = Some(path);
        }
        Ok(())
    }

    pub fn recover_tasks(&self, project_root: &Path) -> Result<Vec<BackendTask>, String> {
        self.recover_tasks_from(project_root, &project_root.join(".app/tasks"), None)
    }

    fn recover_tasks_from(
        &self,
        project_root: &Path,
        tasks_dir: &Path,
        current_project_id: Option<&str>,
    ) -> Result<Vec<BackendTask>, String> {
        let tasks_dir = validate_persistence_dir(project_root, tasks_dir)?;
        if !tasks_dir.exists() {
            return Ok(Vec::new());
        }
        let tasks_dir = validate_existing_project_directory(project_root, &tasks_dir)
            .map_err(|error| format!("Task persistence path is unsafe: {error}"))?;

        let mut recovered = Vec::new();
        let entries = std::fs::read_dir(&tasks_dir)
            .map_err(|e| format!("Failed to read tasks dir: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let path = validate_existing_project_file(project_root, &path)
                    .map_err(|error| format!("Task persistence entry is unsafe: {error}"))?;
                let persisted_id = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| "Task persistence file name is not valid Unicode".to_string())?;
                match std::fs::read_to_string(&path) {
                    Ok(json) => {
                        let parsed = parse_persisted_task(&json, persisted_id);
                        match parsed {
                            Ok((mut task, log_lines, activities, mut workflow)) => {
                                if let Some(project_id) = current_project_id {
                                    task.project_id = Some(project_id.to_string());
                                }
                                if let Some(existing) = self.get_task(&task.id) {
                                    if !self.task_belongs_to_root(&task.id, project_root) {
                                        return Err(format!(
                                            "Recovered task id collision across project roots: {}",
                                            task.id
                                        ));
                                    }
                                    let rebound = if let Some(project_id) = current_project_id {
                                        let mut tasks = self.tasks.write().expect("lock poisoned");
                                        let entry = tasks
                                            .get_mut(&task.id)
                                            .expect("existing task must remain present");
                                        entry.task.project_id = Some(project_id.to_string());
                                        entry.task.clone()
                                    } else {
                                        existing
                                    };
                                    self.persist_current_task(&task.id)?;
                                    if let Some(run) = self.get_workflow_run(&task.id) {
                                        self.emit(
                                            crate::models::task::BackendEventType::WorkflowUpdated,
                                            Some(run.project_id.clone()),
                                            Some(run.task_id.clone()),
                                            run,
                                        );
                                    }
                                    self.emit(
                                        crate::models::task::BackendEventType::TaskUpdated,
                                        rebound.project_id.clone(),
                                        Some(rebound.id.clone()),
                                        rebound.clone(),
                                    );
                                    recovered.push(rebound);
                                    continue;
                                }
                                let token = self.cancellation.register(&task.id);
                                if let Some(state) = workflow.as_mut() {
                                    crate::services::recover_workflow(
                                        &mut task,
                                        state,
                                        project_root,
                                    );
                                } else if matches!(
                                    task.status,
                                    TaskStatus::Running
                                        | TaskStatus::Queued
                                        | TaskStatus::Cancelling
                                ) || (task.status == TaskStatus::WaitingForConfirmation
                                    && !task.is_import_operation())
                                {
                                    task.status = TaskStatus::Failed;
                                    task.error = Some(crate::errors::BackendError::new(
                                        "TASK_RECOVERY",
                                        "Task was interrupted by application restart",
                                        true,
                                        false,
                                    ));
                                    task.completed_at = Some(Utc::now().to_rfc3339());
                                    task.updated_at = Utc::now().to_rfc3339();
                                }

                                let task_entry = TaskEntry {
                                    task: task.clone(),
                                    cancellation: token,
                                    log_lines,
                                    activities,
                                    persisted_path: Some(path),
                                    workflow,
                                };

                                self.tasks
                                    .write()
                                    .expect("lock poisoned")
                                    .insert(task.id.clone(), task_entry);
                                self.task_roots
                                    .write()
                                    .expect("lock poisoned")
                                    .insert(task.id.clone(), project_root.to_path_buf());
                                self.task_persistence_dirs
                                    .write()
                                    .expect("lock poisoned")
                                    .insert(task.id.clone(), tasks_dir.to_path_buf());
                                recovered.push(task);
                                self.persist_current_task(
                                    recovered.last().expect("recovered task exists").id.as_str(),
                                )?;
                                if let Some(run) = self.get_workflow_run(
                                    recovered.last().expect("recovered task exists").id.as_str(),
                                ) {
                                    self.emit(
                                        crate::models::task::BackendEventType::WorkflowUpdated,
                                        run.project_id.clone().into(),
                                        Some(run.task_id.clone()),
                                        run,
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to parse task file {}: {}", path.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read task file {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(recovered)
    }
}

fn require_running_workflow(
    task: &BackendTask,
    cancellation: Option<&CancellationToken>,
    id: &str,
) -> Result<(), String> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        return Err(format!("Workflow cancellation was requested: {id}"));
    }
    if task.status != TaskStatus::Running {
        return Err(format!("Workflow is not running: {id}"));
    }
    Ok(())
}

fn validate_persistence_dir(
    project_root: &Path,
    persistence_dir: &Path,
) -> Result<PathBuf, String> {
    validate_project_directory(project_root, persistence_dir)
        .map_err(|error| format!("Task persistence path is unsafe: {error}"))
}

fn persistence_transition(
    previous: Option<&PathBuf>,
    next: Option<&PathBuf>,
) -> WorkflowPersistenceTransition {
    match (previous, next) {
        (None, None) => WorkflowPersistenceTransition::Unchanged,
        (Some(previous), Some(next)) if previous == next => {
            WorkflowPersistenceTransition::Unchanged
        }
        (Some(_), None) => WorkflowPersistenceTransition::DowngradedToMemoryOnly,
        (None, Some(_)) | (Some(_), Some(_)) => WorkflowPersistenceTransition::UpgradedToPersistent,
    }
}

fn persistence_transition_log(
    transition: WorkflowPersistenceTransition,
) -> Option<(LogLevel, &'static str)> {
    match transition {
        WorkflowPersistenceTransition::DowngradedToMemoryOnly => Some((
            LogLevel::Warn,
            "Project authority changed; this workflow is now memory-only and its prior task file will no longer be updated.",
        )),
        WorkflowPersistenceTransition::UpgradedToPersistent => Some((
            LogLevel::Info,
            "Project authority changed; future workflow state will use the newly derived task-state root without backfilling prior history.",
        )),
        WorkflowPersistenceTransition::Unchanged => None,
    }
}

fn write_persisted_task(
    project_root: &Path,
    persistence_dir: &Path,
    id: &str,
    persisted: &PersistedTaskEntry,
) -> Result<PathBuf, String> {
    validate_task_persistence_id(id)?;
    let persistence_dir = ensure_project_directory(project_root, persistence_dir)
        .map_err(|error| format!("Task persistence path is unsafe: {error}"))?;
    let persistence_dir = validate_existing_project_directory(project_root, &persistence_dir)
        .map_err(|error| format!("Task persistence path is unsafe: {error}"))?;
    let path = persistence_dir.join(format!("{id}.json"));
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            validate_existing_project_file(project_root, &path)
                .map_err(|error| format!("Task persistence entry is unsafe: {error}"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Task persistence entry is unavailable {}: {error}",
                path.display()
            ));
        }
    }

    // Revalidation is intentionally adjacent to the atomic writer. The
    // path-based FileStore API cannot make every parent traversal atomic with
    // no-follow flags, so this narrows rather than eliminates the remaining
    // replacement window. Its random create-new temporary file still prevents
    // pre-creating the task's staging file itself.
    #[cfg(test)]
    let persistence_started = std::time::Instant::now();
    FileStore
        .write_json_atomic_absolute(&path, persisted)
        .map_err(|error| format!("Failed to write task file: {}", error.message))?;
    #[cfg(test)]
    {
        TASK_PERSISTENCE_WRITES.with(|count| count.set(count.get() + 1));
        let elapsed = u64::try_from(persistence_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        TASK_PERSISTENCE_NANOS.with(|nanos| nanos.set(nanos.get().saturating_add(elapsed)));
    }
    Ok(path)
}

fn remove_persisted_task_snapshot(
    project_root: &Path,
    persistence_dir: &Path,
    id: &str,
) -> Result<(), String> {
    validate_task_persistence_id(id)?;
    let persistence_dir = validate_existing_project_directory(project_root, persistence_dir)
        .map_err(|error| format!("Task persistence rollback path is unsafe: {error}"))?;
    let path = persistence_dir.join(format!("{id}.json"));
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to inspect rolled-back task {}: {error}",
                path.display()
            ));
        }
        Ok(_) => {}
    }
    let path = validate_existing_project_file(project_root, &path)
        .map_err(|error| format!("Task persistence rollback entry is unsafe: {error}"))?;
    if path.parent() != Some(persistence_dir.as_path())
        || path.file_name() != Some(std::ffi::OsStr::new(&format!("{id}.json")))
    {
        return Err(format!(
            "Task persistence rollback entry does not match its binding: {}",
            path.display()
        ));
    }
    std::fs::remove_file(&path).map_err(|error| {
        format!(
            "Failed to remove rolled-back task {}: {error}",
            path.display()
        )
    })
}

fn require_current_stage(workflow: &WorkflowExecutionState, stage_id: &str) -> Result<(), String> {
    if workflow.current_stage_id.as_deref() != Some(stage_id) {
        return Err(format!("Workflow stage is not current: {stage_id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::confirmation::{PendingActionType, RiskLevel};
    use crate::models::task::BackendEventType;
    use crate::models::workflow::{
        HealthCheckMode, WorkflowExecutionOptions, WorkflowKind, WorkflowRoute, WorkflowScope,
        WorkflowStage, WorkflowStageStatus, WorkflowStartOutcome,
    };
    use crate::services::{EnqueueWorkflow, WorkflowCoordinator};
    use crate::tasks::task_events::CapturedEvent;
    use crate::tasks::task_model::LogLevel;
    use std::sync::{Arc, Barrier, Mutex};

    fn workflow_request(root: &Path, task_state_root: Option<PathBuf>) -> EnqueueWorkflow {
        EnqueueWorkflow {
            project_id: "project".into(),
            project_root: root.to_path_buf(),
            task_state_root,
            title: "Health Check".into(),
            kind: WorkflowKind::HealthCheck,
            scope: WorkflowScope::HealthCheck {
                mode: HealthCheckMode::LocalQuick,
            },
            route: Some(WorkflowRoute::Local {
                route_revision: "local".into(),
            }),
            baseline_fingerprint: "baseline".into(),
            execution_options: WorkflowExecutionOptions {
                preparation_revision: "prep-1".into(),
                ..WorkflowExecutionOptions::default()
            },
            stages: vec![WorkflowStage {
                id: "read".into(),
                ordinal: 1,
                status: WorkflowStageStatus::Pending,
                label_key: "read".into(),
                started_at: None,
                completed_at: None,
                current_item: None,
                progress: None,
                decision: None,
            }],
            retry: None,
        }
    }

    fn created_workflow(outcome: WorkflowStartOutcome) -> WorkflowRun {
        match outcome {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("expected new workflow"),
        }
    }

    #[test]
    fn history_pages_reuse_the_ordered_index_across_progress_updates() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let mut created = Vec::new();
        for index in 0..3 {
            let mut request = workflow_request(root.path(), None);
            request.baseline_fingerprint = format!("baseline-{index}");
            request.execution_options.preparation_revision = format!("prep-{index}");
            created.push(created_workflow(
                coordinator.enqueue(&service, request).unwrap(),
            ));
        }
        let identity_key = created[0].canonical_identity_key.clone();
        let identity_revision = created[0].identity_revision.clone();
        let (first_page, has_more) =
            service.page_workflow_runs(&identity_key, &identity_revision, None, None, None, 2);
        assert_eq!(first_page.len(), 2);
        assert!(has_more);
        let index_revision = service.workflow_history_revision.load(Ordering::Acquire);

        service
            .start_workflow_stage(&created[0].task_id, "read")
            .unwrap();
        service
            .update_workflow_stage_progress(&created[0].task_id, "read", None, 1, Some(3))
            .unwrap();
        assert_eq!(
            service.workflow_history_revision.load(Ordering::Acquire),
            index_revision,
            "ordinary progress must not invalidate history ordering",
        );

        let cursor = first_page.last().unwrap();
        let (second_page, _) = service.page_workflow_runs(
            &identity_key,
            &identity_revision,
            None,
            None,
            Some((&cursor.started_at, &cursor.task_id)),
            2,
        );
        assert_eq!(second_page.len(), 1);
        assert_ne!(second_page[0].task_id, first_page[0].task_id);
    }

    #[test]
    fn removing_a_completed_workflow_invalidates_the_history_index() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let mut created = Vec::new();
        for index in 0..2 {
            let mut request = workflow_request(root.path(), None);
            request.baseline_fingerprint = format!("remove-baseline-{index}");
            request.execution_options.preparation_revision = format!("remove-prep-{index}");
            created.push(created_workflow(
                coordinator.enqueue(&service, request).unwrap(),
            ));
        }
        let identity_key = created[0].canonical_identity_key.clone();
        let identity_revision = created[0].identity_revision.clone();
        let first_id = created
            .iter()
            .find(|run| run.display_status == WorkflowDisplayStatus::Running)
            .unwrap()
            .task_id
            .clone();
        service.start_workflow_stage(&first_id, "read").unwrap();
        service.complete_workflow_stage(&first_id, "read").unwrap();
        service
            .complete_workflow(
                &first_id,
                WorkflowResult::HealthCheck {
                    report_id: None,
                    persistent: false,
                    error_count: 0,
                    warning_count: 0,
                    info_count: 0,
                },
            )
            .unwrap();

        let cached = service
            .page_workflow_runs(&identity_key, &identity_revision, None, None, None, 2)
            .0;
        assert_eq!(cached.len(), 2);
        let cached_revision = service.workflow_history_revision.load(Ordering::Acquire);
        assert_eq!(service.remove_completed_for_root(root.path()), 1);
        assert!(service.workflow_history_revision.load(Ordering::Acquire) > cached_revision);

        let (remaining, has_more) =
            service.page_workflow_runs(&identity_key, &identity_revision, None, None, None, 1);
        assert_eq!(remaining.len(), 1);
        assert_ne!(remaining[0].task_id, first_id);
        assert!(!has_more);
    }

    #[test]
    fn deferred_cancellation_stays_nonterminal_until_worker_finalizes_it() {
        let service = TaskService::default();
        let task = service.create_task(TaskType::Import, None, "import".into(), true);
        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();

        let cancelling = service.request_cancel(&task.id).unwrap();
        assert_eq!(cancelling.status, TaskStatus::Cancelling);
        assert!(service.is_cancelled(&task.id));

        let cancelled = service.finalize_cancellation(&task.id).unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        assert!(cancelled.completed_at.is_some());
        assert_eq!(
            service.finalize_cancellation(&task.id).unwrap().status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn cancellation_cannot_be_finalized_without_a_request() {
        let service = TaskService::default();
        let task = service.create_task(TaskType::Import, None, "import".into(), true);
        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();

        assert!(service.finalize_cancellation(&task.id).is_err());
        assert_eq!(
            service.get_task(&task.id).unwrap().status,
            TaskStatus::Running
        );
    }

    #[test]
    fn import_operation_uses_the_resolved_compatible_task_root() {
        let project = tempfile::tempdir().unwrap();
        let compatible_tasks = project.path().join(".app/compat/tasks");
        let service = TaskService::default();
        let task = service
            .create_project_import_operation_task(
                "compatible".into(),
                project.path().to_path_buf(),
                compatible_tasks.clone(),
                "Import https://example.com".into(),
                "session-1".into(),
                1,
                Some("https://example.com".into()),
            )
            .unwrap();
        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .finish_running_operation(
                &task.id,
                TaskResult {
                    summary: "waiting".into(),
                    affected_paths: Vec::new(),
                    reference: None,
                    pending_action: None,
                },
                TaskStatus::WaitingForConfirmation,
                None,
            )
            .unwrap();

        assert!(compatible_tasks.join(format!("{}.json", task.id)).is_file());
        assert!(!project
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", task.id))
            .exists());

        let restarted = TaskService::default();
        let recovered = restarted
            .set_project_context(
                "compatible".into(),
                project.path().to_path_buf(),
                compatible_tasks,
            )
            .unwrap();
        assert!(recovered.iter().any(|candidate| {
            candidate.id == task.id
                && candidate.operation == task.operation
                && candidate.status == TaskStatus::WaitingForConfirmation
        }));
    }

    #[test]
    fn final_worker_and_cancel_request_never_leave_an_operation_cancelling() {
        for _ in 0..32 {
            let service = Arc::new(TaskService::default());
            let task = service.create_task(TaskType::Import, None, "import".into(), true);
            service
                .transition_status(&task.id, TaskStatus::Running)
                .unwrap();
            let barrier = Arc::new(Barrier::new(3));

            let cancel_service = Arc::clone(&service);
            let cancel_id = task.id.clone();
            let cancel_barrier = Arc::clone(&barrier);
            let cancel = std::thread::spawn(move || {
                cancel_barrier.wait();
                let (task, previous) =
                    cancel_service.request_cancel_with_previous_status(&cancel_id)?;
                if previous == TaskStatus::WaitingForConfirmation {
                    cancel_service.finalize_cancellation(&cancel_id)
                } else {
                    Ok(task)
                }
            });

            let finish_service = Arc::clone(&service);
            let finish_id = task.id.clone();
            let finish_barrier = Arc::clone(&barrier);
            let finish = std::thread::spawn(move || {
                finish_barrier.wait();
                finish_service.finish_running_operation(
                    &finish_id,
                    TaskResult {
                        summary: "ready".into(),
                        affected_paths: Vec::new(),
                        reference: None,
                        pending_action: None,
                    },
                    TaskStatus::WaitingForConfirmation,
                    None,
                )
            });

            barrier.wait();
            let _ = cancel.join().unwrap();
            let _ = finish.join().unwrap();
            assert!(matches!(
                service.get_task(&task.id).unwrap().status,
                TaskStatus::Cancelled | TaskStatus::WaitingForConfirmation
            ));
        }
    }

    #[test]
    fn persistence_rebind_validates_all_workflows_before_mutating_any() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let first = created_workflow(
            coordinator
                .enqueue(&service, workflow_request(root.path(), None))
                .unwrap(),
        );
        let mut second_request = workflow_request(root.path(), None);
        second_request.baseline_fingerprint = "different-baseline".into();
        let second = created_workflow(coordinator.enqueue(&service, second_request).unwrap());
        service
            .tasks
            .write()
            .unwrap()
            .get_mut(&second.task_id)
            .unwrap()
            .workflow = None;

        let error = service
            .rebind_workflows_for_root(root.path(), Some(root.path().join(".app/compat/tasks")))
            .unwrap_err();

        assert!(error.contains("Workflow task state missing"));
        assert!(service.workflow_persistence_dir(&first.task_id).is_none());
    }

    #[test]
    fn trust_revocation_cancels_active_workflows_for_the_asserted_root() {
        let root = tempfile::tempdir().unwrap();
        let other_root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let active = created_workflow(
            coordinator
                .enqueue(&service, workflow_request(root.path(), None))
                .unwrap(),
        );
        let unaffected = created_workflow(
            coordinator
                .enqueue(&service, workflow_request(other_root.path(), None))
                .unwrap(),
        );
        service
            .request_cancel_active_workflows_for_root(root.path())
            .unwrap();

        assert!(service.is_cancelled(&active.task_id));
        assert_eq!(
            service.get_task(&active.task_id).unwrap().status,
            TaskStatus::Cancelling
        );
        assert!(!service.is_cancelled(&unaffected.task_id));
        assert_eq!(
            service.get_task(&unaffected.task_id).unwrap().status,
            TaskStatus::Running
        );
    }

    #[test]
    fn queued_rebind_to_memory_only_preserves_the_old_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let tasks_root = root.path().join(".app/tasks");
        let run = created_workflow(
            coordinator
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(tasks_root.clone())),
                )
                .unwrap(),
        );
        let old_path = tasks_root.join(format!("{}.json", run.task_id));
        let old_bytes = std::fs::read(&old_path).unwrap();

        assert_eq!(
            service
                .rebind_workflow_persistence(&run.task_id, root.path(), None)
                .unwrap(),
            WorkflowPersistenceTransition::DowngradedToMemoryOnly
        );
        service
            .append_log(&run.task_id, LogLevel::Info, "memory-only log".into())
            .unwrap();
        service.start_workflow_stage(&run.task_id, "read").unwrap();

        assert_eq!(std::fs::read(old_path).unwrap(), old_bytes);
        let rebound = service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(rebound.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            rebound.persistence_transition,
            Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
        );
        assert!(service.get_logs(&run.task_id).unwrap().iter().any(|line| {
            line.level == LogLevel::Warn && line.message.contains("no longer be updated")
        }));
    }

    #[test]
    fn queued_rebind_to_persistent_is_durable_before_it_returns() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let run = created_workflow(
            coordinator
                .enqueue(&service, workflow_request(root.path(), None))
                .unwrap(),
        );
        let tasks_root = root.path().join(".app/任务");
        let task_path = tasks_root.join(format!("{}.json", run.task_id));

        assert_eq!(
            service
                .rebind_workflow_persistence(&run.task_id, root.path(), Some(tasks_root.clone()),)
                .unwrap(),
            WorkflowPersistenceTransition::UpgradedToPersistent
        );
        assert!(task_path.exists());
        let restarted = TaskService::default();
        restarted
            .recover_tasks_from(root.path(), &tasks_root, None)
            .unwrap();
        let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(recovered.persistence, WorkflowPersistenceMode::Persistent);
        assert_eq!(
            recovered.persistence_transition,
            Some(WorkflowPersistenceTransition::UpgradedToPersistent)
        );
        let rebound = service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(rebound.persistence, WorkflowPersistenceMode::Persistent);
        assert_eq!(
            rebound.persistence_transition,
            Some(WorkflowPersistenceTransition::UpgradedToPersistent)
        );
        assert!(service.get_logs(&run.task_id).unwrap().iter().any(|line| {
            line.level == LogLevel::Info && line.message.contains("newly derived")
        }));
    }

    #[test]
    fn queued_rebind_write_failure_keeps_memory_only_state_and_publishes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let (service, events) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(&service, workflow_request(root.path(), None))
                .unwrap(),
        );
        events.lock().unwrap().clear();
        service.inject_task_persistence_failures(&run.task_id, 1);
        let tasks_root = root.path().join(".app/任务");

        let error = service
            .rebind_workflow_persistence(&run.task_id, root.path(), Some(tasks_root.clone()))
            .unwrap_err();

        assert!(error.contains("Injected task persistence failure"));
        assert!(!tasks_root.join(format!("{}.json", run.task_id)).exists());
        assert_eq!(service.workflow_persistence_dir(&run.task_id), None);
        let current = service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(current.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(current.persistence_transition, None);
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn project_authority_rebind_updates_every_workflow_for_the_same_root() {
        let root = tempfile::tempdir().unwrap();
        let other_root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let tasks_root = root.path().join(".app/tasks");
        let first = created_workflow(
            coordinator
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(tasks_root.clone())),
                )
                .unwrap(),
        );
        let mut second_request = workflow_request(root.path(), Some(tasks_root));
        second_request.execution_options.preparation_revision = "prep-2".into();
        second_request.baseline_fingerprint = "baseline-2".into();
        let second = created_workflow(coordinator.enqueue(&service, second_request).unwrap());
        let other = created_workflow(
            coordinator
                .enqueue(&service, workflow_request(other_root.path(), None))
                .unwrap(),
        );

        let transitions = service
            .rebind_workflows_for_root(root.path(), None)
            .unwrap();

        assert_eq!(transitions.len(), 2);
        for id in [&first.task_id, &second.task_id] {
            let run = service.get_workflow_run(id).unwrap();
            assert_eq!(run.persistence, WorkflowPersistenceMode::MemoryOnly);
            assert_eq!(
                run.persistence_transition,
                Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
            );
            assert_eq!(service.workflow_persistence_dir(id), None);
        }
        let other = service.get_workflow_run(&other.task_id).unwrap();
        assert_eq!(other.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(other.persistence_transition, None);
    }

    #[test]
    fn cancellation_and_atomic_completion_never_leave_a_cancelled_result() {
        for _ in 0..64 {
            let service = Arc::new(TaskService::default());
            let task = service.create_task(TaskType::Import, None, "race".into(), true);
            service
                .transition_status(&task.id, TaskStatus::Running)
                .unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let cancel_service = Arc::clone(&service);
            let cancel_id = task.id.clone();
            let cancel_barrier = Arc::clone(&barrier);
            let cancel = std::thread::spawn(move || {
                cancel_barrier.wait();
                cancel_service.cancel_task(&cancel_id)
            });
            let complete_service = Arc::clone(&service);
            let complete_id = task.id.clone();
            let complete_barrier = Arc::clone(&barrier);
            let complete = std::thread::spawn(move || {
                complete_barrier.wait();
                complete_service.complete_running_with_result(
                    &complete_id,
                    TaskResult {
                        summary: "candidate".into(),
                        affected_paths: Vec::new(),
                        reference: None,
                        pending_action: None,
                    },
                )
            });
            barrier.wait();
            let _ = cancel.join().unwrap();
            let _ = complete.join().unwrap();
            let final_task = service.get_task(&task.id).unwrap();
            match final_task.status {
                TaskStatus::Succeeded => assert!(final_task.result.is_some()),
                TaskStatus::Cancelled => assert!(final_task.result.is_none()),
                status => panic!("unexpected race terminal state: {status:?}"),
            }
        }
    }

    fn make_service() -> (TaskService, Arc<Mutex<Vec<CapturedEvent>>>) {
        let (event_bus, events) = EventBus::new_test_capture();
        let service = TaskService::with_event_bus(event_bus);
        (service, events)
    }

    #[test]
    fn workflow_progress_coalesces_persistence_without_coalescing_live_events() {
        let root = tempfile::tempdir().unwrap();
        let (service, events) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        events.lock().unwrap().clear();
        reset_task_costs();

        for current in 1..=500 {
            service
                .update_workflow_stage_progress(
                    &run.task_id,
                    "read",
                    Some(format!("wiki/scale/page-{current:04}.md")),
                    current,
                    Some(500),
                )
                .unwrap();
        }

        let (persistence_writes, event_emissions) = task_costs();
        assert!(
            persistence_writes <= 1,
            "500 progress updates inside one persistence window wrote {persistence_writes} task snapshots"
        );
        assert_eq!(event_emissions, 1_000);
        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1_000);
        assert_eq!(
            captured
                .iter()
                .filter(|event| event.event_type == BackendEventType::WorkflowUpdated)
                .count(),
            500,
        );
        assert_eq!(
            captured
                .iter()
                .filter(|event| event.event_type == BackendEventType::TaskUpdated)
                .count(),
            500,
        );
    }

    #[test]
    fn workflow_progress_uses_the_250ms_window_and_stage_barrier_flushes_latest_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        service.reset_workflow_progress_persistence_window(&run.task_id);
        reset_task_costs();

        for current in 1..=500 {
            service
                .update_workflow_stage_progress(
                    &run.task_id,
                    "read",
                    Some(format!("wiki/规模/页面-{current:04}.md")),
                    current,
                    Some(500),
                )
                .unwrap();
            service.advance_workflow_persistence_clock(Duration::from_millis(20));
        }

        let progress_writes = task_costs().0;
        assert!(
            progress_writes <= 41,
            "10 seconds of progress exceeded the 250ms persistence budget: {progress_writes}"
        );
        service
            .complete_workflow_stage(&run.task_id, "read")
            .unwrap();
        assert_eq!(task_costs().0, progress_writes + 1);

        let restarted = TaskService::default();
        restarted.recover_tasks(root.path()).unwrap();
        let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
        let stage = recovered
            .stages
            .iter()
            .find(|stage| stage.id == "read")
            .unwrap();
        assert_eq!(stage.status, WorkflowStageStatus::Completed);
        assert_eq!(
            stage.current_item.as_deref(),
            Some("wiki/规模/页面-0500.md")
        );
        assert_eq!(
            stage.progress.as_ref().map(|progress| progress.current),
            Some(500)
        );
    }

    #[test]
    fn workflow_progress_trailing_flush_bounds_idle_crash_loss_to_one_window() {
        let root = tempfile::tempdir().unwrap();
        let (service, _) = make_service();
        let tasks_root = root.path().join(".app/tasks");
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(tasks_root.clone())),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        service
            .update_workflow_stage_progress(
                &run.task_id,
                "read",
                Some("wiki/first.md".into()),
                1,
                Some(2),
            )
            .unwrap();
        service
            .update_workflow_stage_progress(
                &run.task_id,
                "read",
                Some("wiki/尾随.md".into()),
                2,
                Some(2),
            )
            .unwrap();

        service.wait_for_trailing_flush(&run.task_id, Duration::from_secs(2));

        let restarted = TaskService::default();
        restarted
            .recover_tasks_from(root.path(), &tasks_root, None)
            .unwrap();
        let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
        let stage = recovered
            .stages
            .iter()
            .find(|stage| stage.id == "read")
            .unwrap();
        assert_eq!(stage.current_item.as_deref(), Some("wiki/尾随.md"));
        assert_eq!(
            stage.progress.as_ref().map(|progress| progress.current),
            Some(2)
        );
    }

    #[test]
    fn generic_workflow_barrier_cancels_the_pending_trailing_generation() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        for current in 1..=2 {
            service
                .update_workflow_stage_progress(
                    &run.task_id,
                    "read",
                    Some(format!("wiki/barrier-{current}.md")),
                    current,
                    Some(2),
                )
                .unwrap();
        }
        let persistence_lane = service.workflow_persistence_lane(&run.task_id);
        let pending_generation = {
            let lane = persistence_lane.lock().unwrap();
            assert!(lane.pending_observational_revision.is_some());
            assert!(lane.trailing_flush_scheduled);
            lane.trailing_flush_generation
        };
        let writes_before_barrier = service.persistence_write_count(&run.task_id);

        service
            .append_log(
                &run.task_id,
                LogLevel::Info,
                "barrier flushes latest progress".into(),
            )
            .unwrap();
        assert_eq!(
            service.persistence_write_count(&run.task_id),
            writes_before_barrier + 1
        );
        service.flush_pending_workflow_progress(&run.task_id, pending_generation);
        assert_eq!(
            service.persistence_write_count(&run.task_id),
            writes_before_barrier + 1,
            "a cancelled trailing generation performed a duplicate write"
        );
        let lane = persistence_lane.lock().unwrap();
        assert!(lane.pending_observational_revision.is_none());
        assert!(!lane.trailing_flush_scheduled);
        assert!(lane.persisted_revision <= lane.next_revision);
    }

    #[test]
    fn failed_observational_write_is_retried_by_the_next_barrier() {
        let root = tempfile::tempdir().unwrap();
        let (service, events) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        events.lock().unwrap().clear();
        service.inject_task_persistence_failures(&run.task_id, 1);

        let live = service
            .update_workflow_stage_progress(
                &run.task_id,
                "read",
                Some("wiki/恢复.md".into()),
                7,
                Some(10),
            )
            .unwrap();
        assert_eq!(live.stages[0].progress.as_ref().unwrap().current, 7);
        assert_eq!(events.lock().unwrap().len(), 2);

        service
            .complete_workflow_stage(&run.task_id, "read")
            .unwrap();
        let restarted = TaskService::default();
        restarted.recover_tasks(root.path()).unwrap();
        let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(recovered.stages[0].status, WorkflowStageStatus::Completed);
        assert_eq!(
            recovered.stages[0]
                .progress
                .as_ref()
                .map(|progress| progress.current),
            Some(7)
        );
    }

    #[test]
    fn confirmation_and_cancellation_barriers_persist_the_latest_progress_before_events() {
        let root = tempfile::tempdir().unwrap();
        let (service, events) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        service.reset_workflow_progress_persistence_window(&run.task_id);
        service
            .update_workflow_stage_progress(
                &run.task_id,
                "read",
                Some("wiki/first.md".into()),
                1,
                Some(2),
            )
            .unwrap();
        service
            .update_workflow_stage_progress(
                &run.task_id,
                "read",
                Some("wiki/第二页.md".into()),
                2,
                Some(2),
            )
            .unwrap();
        events.lock().unwrap().clear();
        let pending = WorkflowPendingAction {
            id: "pending-1".into(),
            action_type: PendingActionType::BatchRewrite,
            risk_level: RiskLevel::High,
            affected_paths: vec!["wiki/第二页.md".into()],
            candidate: None,
            expires_at: None,
            checkpoint_hash: Some("checkpoint".into()),
        };

        service
            .wait_workflow_stage(&run.task_id, "read", pending)
            .unwrap();
        let task_path = root
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", run.task_id));
        let json = std::fs::read_to_string(&task_path).unwrap();
        let (task, _, _, workflow) = parse_persisted_task(&json, &run.task_id).unwrap();
        let workflow = workflow.unwrap();
        assert_eq!(task.status, TaskStatus::WaitingForConfirmation);
        assert_eq!(workflow.pending_action.as_ref().unwrap().id, "pending-1");
        assert_eq!(workflow.stages[0].progress.as_ref().unwrap().current, 2);
        assert_eq!(
            events.lock().unwrap()[0].event_type,
            BackendEventType::WorkflowUpdated
        );

        service.request_workflow_cancel(&run.task_id).unwrap();
        service
            .finalize_workflow_cancellation(&run.task_id)
            .unwrap();
        let json = std::fs::read_to_string(task_path).unwrap();
        let (task, _, _, workflow) = parse_persisted_task(&json, &run.task_id).unwrap();
        assert_eq!(task.status, TaskStatus::Cancelled);
        assert!(workflow.unwrap().pending_action.is_none());
    }

    #[test]
    fn terminal_barrier_write_failure_rolls_back_without_publishing_terminal_state() {
        let root = tempfile::tempdir().unwrap();
        let (service, events) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        service
            .complete_workflow_stage(&run.task_id, "read")
            .unwrap();
        events.lock().unwrap().clear();
        service.inject_task_persistence_failures(&run.task_id, 1);
        let result = WorkflowResult::HealthCheck {
            report_id: Some("report-1".into()),
            persistent: true,
            error_count: 0,
            warning_count: 1,
            info_count: 2,
        };

        let error = service
            .complete_workflow(&run.task_id, result.clone())
            .unwrap_err();
        assert!(error.contains("Injected task persistence failure"));
        let rolled_back = service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(
            rolled_back.display_status,
            crate::models::workflow::WorkflowDisplayStatus::Running
        );
        assert!(rolled_back.result.is_none());
        assert!(events.lock().unwrap().is_empty());

        let completed = service.complete_workflow(&run.task_id, result).unwrap();
        assert_eq!(
            completed.display_status,
            crate::models::workflow::WorkflowDisplayStatus::Completed
        );
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|event| { event.event_type == BackendEventType::TaskCompleted }));
    }

    #[test]
    fn failed_terminal_barrier_rollback_cannot_overwrite_a_concurrent_log() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        service
            .complete_workflow_stage(&run.task_id, "read")
            .unwrap();
        service.inject_task_persistence_failures(&run.task_id, 1);
        let (writer_gate, writer_entered) = service.gate_next_persistence_write(&run.task_id);
        let completion_service = service.clone();
        let completion_id = run.task_id.clone();
        let completion = std::thread::spawn(move || {
            completion_service.complete_workflow(
                &completion_id,
                WorkflowResult::HealthCheck {
                    report_id: None,
                    persistent: true,
                    error_count: 0,
                    warning_count: 0,
                    info_count: 1,
                },
            )
        });
        writer_entered
            .recv_timeout(Duration::from_secs(2))
            .expect("terminal writer did not reach the deterministic gate");

        let (log_started_tx, log_started_rx) = std::sync::mpsc::channel();
        let (log_done_tx, log_done_rx) = std::sync::mpsc::channel();
        let log_service = service.clone();
        let log_id = run.task_id.clone();
        let log_writer = std::thread::spawn(move || {
            log_started_tx.send(()).unwrap();
            let result =
                log_service.append_log(&log_id, LogLevel::Info, "concurrent after rollback".into());
            log_done_tx.send(()).unwrap();
            result
        });
        log_started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(log_done_rx.try_recv().is_err());
        writer_gate.release();

        let error = completion.join().unwrap().unwrap_err();
        assert!(error.contains("Injected task persistence failure"));
        log_writer.join().unwrap().unwrap();
        log_done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let current = service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(
            current.display_status,
            crate::models::workflow::WorkflowDisplayStatus::Running
        );
        assert!(current.result.is_none());
        assert!(service
            .get_logs(&run.task_id)
            .unwrap()
            .iter()
            .any(|line| line.message == "concurrent after rollback"));

        let restarted = TaskService::default();
        restarted.recover_tasks(root.path()).unwrap();
        assert!(restarted
            .get_logs(&run.task_id)
            .unwrap()
            .iter()
            .any(|line| line.message == "concurrent after rollback"));
    }

    #[test]
    fn concurrent_progress_cannot_leave_a_stale_snapshot_after_a_barrier() {
        let root = tempfile::tempdir().unwrap();
        let service = Arc::new(TaskService::default());
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        let start = Arc::new(Barrier::new(9));
        let mut workers = Vec::new();
        for current in 1..=8 {
            let service = Arc::clone(&service);
            let task_id = run.task_id.clone();
            let start = Arc::clone(&start);
            workers.push(std::thread::spawn(move || {
                start.wait();
                service
                    .update_workflow_stage_progress(
                        &task_id,
                        "read",
                        Some(format!("wiki/concurrent-{current}.md")),
                        current,
                        Some(8),
                    )
                    .unwrap();
            }));
        }
        start.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        service
            .complete_workflow_stage(&run.task_id, "read")
            .unwrap();
        let expected = service.get_workflow_run(&run.task_id).unwrap().stages[0].clone();

        let restarted = TaskService::default();
        restarted.recover_tasks(root.path()).unwrap();
        let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(recovered.stages[0], expected);
        assert_eq!(service.persistence_writer_metrics(&run.task_id), (1, 0));
    }

    #[test]
    fn paused_progress_writer_serializes_before_newer_stage_barrier() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        service.reset_workflow_progress_persistence_window(&run.task_id);
        let (writer_gate, writer_entered) = service.gate_next_persistence_write(&run.task_id);
        let progress_service = service.clone();
        let progress_id = run.task_id.clone();
        let progress = std::thread::spawn(move || {
            progress_service.update_workflow_stage_progress(
                &progress_id,
                "read",
                Some("wiki/old-writer.md".into()),
                1,
                Some(1),
            )
        });
        writer_entered
            .recv_timeout(Duration::from_secs(2))
            .expect("progress writer did not reach the deterministic gate");

        let (barrier_started_tx, barrier_started_rx) = std::sync::mpsc::channel();
        let (barrier_done_tx, barrier_done_rx) = std::sync::mpsc::channel();
        let barrier_service = service.clone();
        let barrier_id = run.task_id.clone();
        let barrier = std::thread::spawn(move || {
            barrier_started_tx.send(()).unwrap();
            let result = barrier_service.complete_workflow_stage(&barrier_id, "read");
            barrier_done_tx.send(()).unwrap();
            result
        });
        barrier_started_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(barrier_done_rx.try_recv().is_err());

        writer_gate.release();
        progress.join().unwrap().unwrap();
        barrier.join().unwrap().unwrap();
        barrier_done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();

        let restarted = TaskService::default();
        restarted.recover_tasks(root.path()).unwrap();
        let stage = &restarted.get_workflow_run(&run.task_id).unwrap().stages[0];
        assert_eq!(stage.status, WorkflowStageStatus::Completed);
        assert_eq!(stage.current_item.as_deref(), Some("wiki/old-writer.md"));
        assert_eq!(service.persistence_writer_metrics(&run.task_id), (1, 0));
    }

    #[test]
    fn persistence_lanes_do_not_serialize_writers_from_different_projects() {
        let first_root = tempfile::tempdir().unwrap();
        let second_root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let first = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(
                        first_root.path(),
                        Some(first_root.path().join(".app/tasks")),
                    ),
                )
                .unwrap(),
        );
        let second = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(
                        second_root.path(),
                        Some(second_root.path().join(".app/tasks")),
                    ),
                )
                .unwrap(),
        );
        for id in [&first.task_id, &second.task_id] {
            service.start_workflow_stage(id, "read").unwrap();
            service.reset_workflow_progress_persistence_window(id);
        }
        let (first_gate, first_entered) = service.gate_next_persistence_write(&first.task_id);
        let (second_gate, second_entered) = service.gate_next_persistence_write(&second.task_id);

        let spawn_progress = |id: String, current_item: &'static str| {
            let service = service.clone();
            std::thread::spawn(move || {
                service.update_workflow_stage_progress(
                    &id,
                    "read",
                    Some(current_item.into()),
                    1,
                    Some(1),
                )
            })
        };
        let first_writer = spawn_progress(first.task_id.clone(), "wiki/first.md");
        let second_writer = spawn_progress(second.task_id.clone(), "wiki/second.md");
        first_entered
            .recv_timeout(Duration::from_secs(2))
            .expect("first project writer did not start");
        second_entered
            .recv_timeout(Duration::from_secs(2))
            .expect("second project writer was serialized behind the first project");
        first_gate.release();
        second_gate.release();
        first_writer.join().unwrap().unwrap();
        second_writer.join().unwrap().unwrap();
        assert_eq!(service.persistence_writer_metrics(&first.task_id), (1, 0));
        assert_eq!(service.persistence_writer_metrics(&second.task_id), (1, 0));
    }

    #[test]
    fn persistence_rollback_rejects_a_replaced_parent_alias_before_delete() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let tasks_dir = root.path().join(".app").join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::remove_dir(&tasks_dir).unwrap();
        create_directory_alias(outside.path(), &tasks_dir);
        let outside_task = outside.path().join("rollback-task.json");
        std::fs::write(&outside_task, b"outside must survive").unwrap();

        let error =
            remove_persisted_task_snapshot(root.path(), &tasks_dir, "rollback-task").unwrap_err();

        assert!(error.contains("unsafe"));
        assert_eq!(
            std::fs::read(&outside_task).unwrap(),
            b"outside must survive"
        );
        remove_directory_alias(&tasks_dir);
    }

    #[test]
    fn workflow_progress_write_budget_is_deterministic_across_ten_fixed_samples() {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        let mut samples = Vec::new();
        for _ in 0..10 {
            service.reset_workflow_progress_persistence_window(&run.task_id);
            reset_task_costs();
            for current in 1..=500 {
                service
                    .update_workflow_stage_progress(
                        &run.task_id,
                        "read",
                        Some(format!("wiki/sample-{current:04}.md")),
                        current,
                        Some(500),
                    )
                    .unwrap();
                service.advance_workflow_persistence_clock(Duration::from_millis(20));
            }
            samples.push(task_costs().0);
        }
        assert!(samples.iter().all(|writes| *writes == samples[0]));
        assert!(
            samples[0] <= 41,
            "unexpected persistence samples: {samples:?}"
        );
    }

    #[test]
    #[ignore = "local release performance reference for the Batch 5C stop/go gate"]
    fn workflow_progress_release_reference_reports_persistence_share() {
        const FIXTURES_PER_SAMPLE: usize = 10;
        assert!(
            !cfg!(debug_assertions),
            "Batch 5C reference must run with cargo test --release"
        );
        let root = tempfile::tempdir().unwrap();
        let (service, events) = make_service();
        let run = created_workflow(
            WorkflowCoordinator::default()
                .enqueue(
                    &service,
                    workflow_request(root.path(), Some(root.path().join(".app/tasks"))),
                )
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "read").unwrap();
        let update = || {
            service.reset_workflow_progress_persistence_window(&run.task_id);
            for current in 1..=500 {
                service
                    .update_workflow_stage_progress(
                        &run.task_id,
                        "read",
                        Some(format!("wiki/scale/page-{current:04}.md")),
                        current,
                        Some(500),
                    )
                    .unwrap();
                service.advance_workflow_persistence_clock(Duration::from_millis(20));
            }
        };
        for _ in 0..5 {
            events.lock().unwrap().clear();
            reset_task_costs();
            for _ in 0..FIXTURES_PER_SAMPLE {
                update();
            }
        }
        let mut total_ms = Vec::with_capacity(50);
        let mut persistence_ms = Vec::with_capacity(50);
        for _ in 0..50 {
            events.lock().unwrap().clear();
            reset_task_costs();
            let started = std::time::Instant::now();
            for _ in 0..FIXTURES_PER_SAMPLE {
                update();
            }
            total_ms.push(started.elapsed().as_secs_f64() * 1_000.0 / FIXTURES_PER_SAMPLE as f64);
            persistence_ms
                .push(task_persistence_nanos() as f64 / 1_000_000.0 / FIXTURES_PER_SAMPLE as f64);
        }
        let total = task_sample_stats(&total_ms);
        let persistence = task_sample_stats(&persistence_ms);
        eprintln!(
            "BATCH5_PROGRESS_REFERENCE profile=release cache_mode=os_warm storage=tempdir_same_volume os={} arch={} parallelism={} samples=50 fixtures_per_sample={} updates_per_fixture=500 total_mean_ms={:.3} total_p95_ms={:.3} total_cv={:.4} persistence_mean_ms={:.3} persistence_p95_ms={:.3} persistence_share={:.4}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::available_parallelism().map_or(0, |value| value.get()),
            FIXTURES_PER_SAMPLE,
            total.mean,
            total.p95,
            total.cv,
            persistence.mean,
            persistence.p95,
            persistence.mean / total.mean,
        );
        assert!(task_costs().0 <= 41 * FIXTURES_PER_SAMPLE);
        assert_eq!(task_costs().1, 1_000 * FIXTURES_PER_SAMPLE);
        assert!(
            total.cv < 0.15,
            "release measurement CV was {:.4}",
            total.cv
        );
    }

    struct TaskSampleStats {
        mean: f64,
        p95: f64,
        cv: f64,
    }

    fn task_sample_stats(samples: &[f64]) -> TaskSampleStats {
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance = samples
            .iter()
            .map(|sample| (sample - mean).powi(2))
            .sum::<f64>()
            / samples.len() as f64;
        let mut ordered = samples.to_vec();
        ordered.sort_by(f64::total_cmp);
        let p95_index = ((ordered.len() as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(ordered.len() - 1);
        TaskSampleStats {
            mean,
            p95: ordered[p95_index],
            cv: variance.sqrt() / mean,
        }
    }

    #[cfg(unix)]
    fn create_directory_alias(target: &Path, alias: &Path) {
        std::os::unix::fs::symlink(target, alias).unwrap();
    }

    #[cfg(windows)]
    fn create_directory_alias(target: &Path, alias: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(alias)
            .arg(target)
            .status()
            .unwrap();
        assert!(status.success(), "failed to create test junction");
    }

    #[cfg(unix)]
    fn remove_directory_alias(alias: &Path) {
        std::fs::remove_file(alias).unwrap();
    }

    #[cfg(windows)]
    fn remove_directory_alias(alias: &Path) {
        std::fs::remove_dir(alias).unwrap();
    }

    #[test]
    fn test_create_task() {
        let (service, events) = make_service();

        let task = service.create_task(
            TaskType::GraphBuild,
            Some("project-1".to_string()),
            "Build graph".to_string(),
            true,
        );

        assert_eq!(task.task_type, TaskType::GraphBuild);
        assert_eq!(task.project_id, Some("project-1".to_string()));
        assert_eq!(task.title, "Build graph");
        assert_eq!(task.status, TaskStatus::Queued);
        assert!(task.cancellable);
        assert!(task.completed_at.is_none());
        assert!(!task.id.is_empty());

        let stored = service.get_task(&task.id).unwrap();
        assert_eq!(stored.id, task.id);

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].event_type, BackendEventType::TaskUpdated);
        assert_eq!(captured[0].task_id.as_ref().unwrap(), &task.id);
    }

    #[test]
    fn test_state_transitions_valid() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Import files".to_string(), true);

        assert!(service
            .transition_status(&task.id, TaskStatus::Running)
            .is_ok());
        let t = service.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Running);

        assert!(service
            .transition_status(&task.id, TaskStatus::Succeeded)
            .is_ok());
        let t = service.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Succeeded);
        assert!(t.completed_at.is_some());
    }

    #[test]
    fn test_state_transitions_invalid() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Import files".to_string(), true);

        assert!(service
            .transition_status(&task.id, TaskStatus::Succeeded)
            .is_err());

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();

        assert!(service
            .transition_status(&task.id, TaskStatus::Queued)
            .is_err());

        assert!(service
            .transition_status(&task.id, TaskStatus::WaitingForConfirmation)
            .is_ok());

        assert!(service
            .transition_status(&task.id, TaskStatus::Running)
            .is_ok());
    }

    #[test]
    fn test_update_progress() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::GraphBuild, None, "Build".to_string(), true);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .update_progress(&task.id, 5, Some(10), Some("Halfway".to_string()))
            .unwrap();

        let t = service.get_task(&task.id).unwrap();
        let progress = t.progress.unwrap();
        assert_eq!(progress.current, 5);
        assert_eq!(progress.total, Some(10));
        assert_eq!(progress.label, Some("Halfway".to_string()));
    }

    #[test]
    fn test_append_log() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Import".to_string(), true);

        service
            .append_log(&task.id, LogLevel::Info, "Starting import".to_string())
            .unwrap();
        service
            .append_log(
                &task.id,
                LogLevel::Warn,
                "Duplicate file skipped".to_string(),
            )
            .unwrap();

        let logs = service.get_logs(&task.id).unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, LogLevel::Info);
        assert_eq!(logs[0].message, "Starting import");
        assert_eq!(logs[1].level, LogLevel::Warn);
        assert_eq!(logs[1].message, "Duplicate file skipped");
    }

    #[test]
    fn test_logs_are_append_only() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Import".to_string(), true);

        service
            .append_log(&task.id, LogLevel::Info, "Line 1".to_string())
            .unwrap();
        service
            .append_log(&task.id, LogLevel::Info, "Line 2".to_string())
            .unwrap();

        let logs = service.get_logs(&task.id).unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "Line 1");
        assert_eq!(logs[1].message, "Line 2");
    }

    #[test]
    fn test_cancel_task() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::GraphBuild, None, "Build graph".to_string(), true);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        let result = service.cancel_task(&task.id);
        assert!(result.is_ok());

        let t = service.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
        assert!(t.completed_at.is_some());
    }

    #[test]
    fn test_cancel_non_cancellable_task_fails() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Export, None, "Export".to_string(), false);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        let result = service.cancel_task(&task.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not cancellable"));
    }

    #[test]
    fn test_cancel_already_completed_task_is_idempotent() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Import".to_string(), true);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&task.id, TaskStatus::Succeeded)
            .unwrap();

        // Cancelling a task that already reached a terminal state must not
        // error — the caller's intent (stop the task) is already satisfied,
        // and rejecting surfaces a confusing "cannot cancel" toast when a
        // fast-failing task lands before the user's click.
        let result = service.cancel_task(&task.id).unwrap();
        assert_eq!(result.status, TaskStatus::Succeeded);

        // Same idempotency for Failed and Cancelled terminal states.
        let failed = service.create_task(TaskType::Import, None, "Import".to_string(), true);
        service
            .transition_status(&failed.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&failed.id, TaskStatus::Failed)
            .unwrap();
        assert_eq!(
            service.cancel_task(&failed.id).unwrap().status,
            TaskStatus::Failed
        );

        let cancelled = service.create_task(TaskType::Import, None, "Import".to_string(), true);
        service
            .transition_status(&cancelled.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&cancelled.id, TaskStatus::Cancelling)
            .unwrap();
        service
            .transition_status(&cancelled.id, TaskStatus::Cancelled)
            .unwrap();
        assert_eq!(
            service.cancel_task(&cancelled.id).unwrap().status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn test_cancellation_token_signals() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::AgentRun, None, "Agent task".to_string(), true);

        let token = service.get_cancellation_token(&task.id).unwrap();
        assert!(!token.is_cancelled());

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service.cancel_task(&task.id).unwrap();

        assert!(token.is_cancelled());
        assert!(service.is_cancelled(&task.id));
    }

    #[test]
    fn test_set_result() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::WikiCompile, None, "Compile".to_string(), true);

        let result = TaskResult {
            summary: "Compiled 10 pages".to_string(),
            affected_paths: vec!["wiki/index.md".to_string(), "wiki/overview.md".to_string()],
            reference: None,
            pending_action: None,
        };

        service.set_result(&task.id, result.clone()).unwrap();
        let t = service.get_task(&task.id).unwrap();
        let r = t.result.unwrap();
        assert_eq!(r.summary, "Compiled 10 pages");
        assert_eq!(r.affected_paths.len(), 2);
    }

    #[test]
    fn test_list_tasks_with_filter() {
        let (service, _events) = make_service();

        let t1 = service.create_task(TaskType::Import, None, "Import".to_string(), true);
        let t2 = service.create_task(TaskType::Export, None, "Export".to_string(), true);

        service
            .transition_status(&t1.id, TaskStatus::Running)
            .unwrap();

        let all = service.list_tasks(None);
        assert_eq!(all.len(), 2);

        let running = service.list_tasks(Some(TaskStatus::Running));
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, t1.id);

        let queued = service.list_tasks(Some(TaskStatus::Queued));
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].id, t2.id);
    }

    #[test]
    fn test_list_tasks_orders_by_updated_time_desc() {
        let (service, _events) = make_service();
        let old = service.create_task(TaskType::Import, None, "Old".to_string(), true);
        let new = service.create_task(TaskType::Export, None, "New".to_string(), true);

        service
            .transition_status(&old.id, TaskStatus::Running)
            .unwrap();
        service
            .append_log(&old.id, LogLevel::Info, "old update".to_string())
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
        service
            .transition_status(&new.id, TaskStatus::Running)
            .unwrap();
        service
            .append_log(&new.id, LogLevel::Info, "new update".to_string())
            .unwrap();

        let listed = service.list_tasks(None);
        assert_eq!(listed[0].id, new.id);
    }

    #[test]
    fn test_persist_and_recover_tasks() {
        let temp = std::env::temp_dir().join("llm-wiki-task-test-persist");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        // Create and persist tasks using first service
        {
            let (service, _events) = make_service();
            let t1 = service.create_task(
                TaskType::Import,
                Some("project-1".to_string()),
                "Import files".to_string(),
                true,
            );
            service
                .transition_status(&t1.id, TaskStatus::Running)
                .unwrap();
            service
                .append_log(&t1.id, LogLevel::Info, "started import".to_string())
                .unwrap();
            service.persist_task(&t1.id, &temp).unwrap();

            let t2 = service.create_task(
                TaskType::GraphBuild,
                Some("project-1".to_string()),
                "Build graph".to_string(),
                true,
            );
            service
                .transition_status(&t2.id, TaskStatus::Running)
                .unwrap();
            service
                .transition_status(&t2.id, TaskStatus::Succeeded)
                .unwrap();
            service
                .set_result(
                    &t2.id,
                    TaskResult {
                        summary: "Done".to_string(),
                        affected_paths: vec![],
                        reference: None,
                        pending_action: None,
                    },
                )
                .unwrap();
            service.persist_task(&t2.id, &temp).unwrap();
        }

        // Recover with a new service
        {
            let (service2, _events) = make_service();
            let recovered = service2.recover_tasks(&temp).unwrap();
            assert_eq!(recovered.len(), 2);

            // Running task should be marked as Failed after recovery
            let r1 = service2
                .get_task(
                    &recovered
                        .iter()
                        .find(|t| t.task_type == TaskType::Import)
                        .unwrap()
                        .id,
                )
                .unwrap();
            assert_eq!(r1.status, TaskStatus::Failed);
            assert!(r1.error.is_some());
            assert_eq!(
                service2.get_logs(&r1.id).unwrap()[0].message,
                "started import"
            );

            // Succeeded task should stay Succeeded
            let r2 = service2
                .get_task(
                    &recovered
                        .iter()
                        .find(|t| t.task_type == TaskType::GraphBuild)
                        .unwrap()
                        .id,
                )
                .unwrap();
            assert_eq!(r2.status, TaskStatus::Succeeded);
            assert!(r2.result.is_some());
        }

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_running_task_state_and_logs_persist_before_terminal_status() {
        let temp =
            std::env::temp_dir().join(format!("llm-wiki-task-test-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp).unwrap();

        let (service, _events) = make_service();
        service.set_project_root(Some(temp.clone())).unwrap();
        let task = service.create_task(
            TaskType::Import,
            Some("project-live".to_string()),
            "Live import".to_string(),
            true,
        );
        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .append_log(&task.id, LogLevel::Info, "copying sources".to_string())
            .unwrap();

        let persisted = temp.join(".app/tasks").join(format!("{}.json", task.id));
        assert!(
            persisted.exists(),
            "running tasks must be durable before completion"
        );

        let (restarted, _events) = make_service();
        restarted.recover_tasks(&temp).unwrap();
        assert_eq!(
            restarted.get_task(&task.id).unwrap().status,
            TaskStatus::Failed
        );
        assert_eq!(
            restarted.get_logs(&task.id).unwrap()[0].message,
            "copying sources"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn queued_task_recovers_as_retryable_interrupted_work() {
        let root = std::env::temp_dir().join(format!("task-recover-queued-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let (service, _) = make_service();
        service.set_project_root(Some(root.clone())).unwrap();
        let queued = service.create_task(
            TaskType::Import,
            Some("p".into()),
            "Queued import".into(),
            true,
        );
        service.persist_task(&queued.id, &root).unwrap();

        let (restarted, _) = make_service();
        restarted.recover_tasks(&root).unwrap();
        let recovered = restarted.get_task(&queued.id).unwrap();
        assert_eq!(recovered.status, TaskStatus::Failed);
        let error = recovered.error.unwrap();
        assert_eq!(error.code, "TASK_RECOVERY");
        assert!(error.recoverable);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn source_ai_recovery_preserves_the_exact_retry_binding_and_settings() {
        let root = std::env::temp_dir().join(format!("task-recover-source-ai-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let (service, _) = make_service();
        service.set_project_root(Some(root.clone())).unwrap();
        let task = service.create_task(
            TaskType::SourceAiOrganize,
            Some("project-1".into()),
            "Source AI".into(),
            true,
        );
        service
            .set_result(
                &task.id,
                TaskResult {
                    summary: "queued".into(),
                    affected_paths: Vec::new(),
                    reference: Some(crate::models::task::TaskResultReference::SourceAiOrganize {
                        source_id: "source-中文".into(),
                        base_version_id: "version-1".into(),
                        base_markdown_hash: "abc123".into(),
                        candidate_id: None,
                        route: Some(crate::models::compile::CompileRoutePreference::Byok),
                        agent: None,
                        provider: Some(crate::models::llm::LlmProviderKind::OpenAi),
                        custom_instructions: Some("保留原始引文".into()),
                        project_root_path: Some(root.to_string_lossy().into_owned()),
                        resolved_engine: Some("open_ai".into()),
                        resolved_model: Some("gpt-source".into()),
                    }),
                    pending_action: None,
                },
            )
            .unwrap();
        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service.persist_task(&task.id, &root).unwrap();

        let (restarted, _) = make_service();
        restarted.recover_tasks(&root).unwrap();
        let recovered = restarted.get_task(&task.id).unwrap();
        assert_eq!(recovered.status, TaskStatus::Failed);
        assert!(recovered.error.as_ref().unwrap().recoverable);
        match recovered.result.unwrap().reference.unwrap() {
            crate::models::task::TaskResultReference::SourceAiOrganize {
                source_id,
                base_version_id,
                base_markdown_hash,
                route,
                provider,
                custom_instructions,
                ..
            } => {
                assert_eq!(source_id, "source-中文");
                assert_eq!(base_version_id, "version-1");
                assert_eq!(base_markdown_hash, "abc123");
                assert_eq!(
                    route,
                    Some(crate::models::compile::CompileRoutePreference::Byok)
                );
                assert_eq!(provider, Some(crate::models::llm::LlmProviderKind::OpenAi));
                assert_eq!(custom_instructions.as_deref(), Some("保留原始引文"));
            }
            other => panic!("unexpected recovery reference: {other:?}"),
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn test_project_task_root_is_bound_explicitly_not_from_ambient_active_project() {
        let project_a = std::env::temp_dir().join(format!("task-root-a-{}", Uuid::new_v4()));
        let project_b = std::env::temp_dir().join(format!("task-root-b-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&project_a).unwrap();
        std::fs::create_dir_all(&project_b).unwrap();
        let (service, _events) = make_service();
        service.set_project_root(Some(project_a.clone())).unwrap();

        let task = service
            .create_project_task(
                TaskType::Export,
                "project-b".to_string(),
                project_b.clone(),
                "B export".to_string(),
                true,
            )
            .unwrap();

        assert!(project_b
            .join(".app/tasks")
            .join(format!("{}.json", task.id))
            .exists());
        assert!(!project_a
            .join(".app/tasks")
            .join(format!("{}.json", task.id))
            .exists());
        let _ = std::fs::remove_dir_all(project_a);
        let _ = std::fs::remove_dir_all(project_b);
    }

    #[test]
    fn test_switching_project_keeps_background_tasks_alive_but_returns_scoped_snapshots() {
        let proj_a = std::env::temp_dir().join("llm-wiki-task-test-isolation-a");
        let proj_b = std::env::temp_dir().join("llm-wiki-task-test-isolation-b");
        let _ = std::fs::remove_dir_all(&proj_a);
        let _ = std::fs::remove_dir_all(&proj_b);
        std::fs::create_dir_all(&proj_a).unwrap();
        std::fs::create_dir_all(&proj_b).unwrap();

        let (service, _events) = make_service();

        // Activate project A and create a task there.
        let recovered_a = service.set_project_root(Some(proj_a.clone())).unwrap();
        assert!(recovered_a.is_empty());
        let task_a =
            service.create_task(TaskType::Import, None, "Project A import".to_string(), true);
        service
            .transition_status(&task_a.id, TaskStatus::Running)
            .unwrap();
        // Register a cancellation token so we can assert it is cleared too.
        let token_a = service.get_cancellation_token(&task_a.id).unwrap();
        assert_eq!(service.list_tasks(None).len(), 1);

        // Switch to project B: A keeps running in the backend, but the
        // frontend-facing snapshot for B must not expose it.
        let recovered_b = service.set_project_root(Some(proj_b.clone())).unwrap();
        assert!(recovered_b.is_empty());
        assert_eq!(service.list_tasks(None).len(), 1);
        assert!(service.get_cancellation_token(&task_a.id).is_some());
        let returned_to_a = service.set_project_root(Some(proj_a.clone())).unwrap();
        assert_eq!(returned_to_a.len(), 1);
        assert_eq!(
            service.get_task(&task_a.id).unwrap().status,
            TaskStatus::Running
        );
        assert!(service.cancel_task(&task_a.id).is_ok());
        assert!(token_a.is_cancelled());
        assert!(proj_a
            .join(".app/tasks")
            .join(format!("{}.json", task_a.id))
            .exists());
        assert!(!proj_b
            .join(".app/tasks")
            .join(format!("{}.json", task_a.id))
            .exists());

        // Project B tasks coexist with A tasks.
        let task_b =
            service.create_task(TaskType::Export, None, "Project B export".to_string(), true);
        let listed = service.list_tasks(None);
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|task| task.id == task_b.id));
        assert!(listed.iter().any(|task| task.id == task_a.id));

        // Closing the workspace must not make background work disappear.
        let recovered_none = service.set_project_root(None).unwrap();
        assert!(recovered_none.is_empty());
        assert_eq!(service.list_tasks(None).len(), 2);

        let _ = std::fs::remove_dir_all(&proj_a);
        let _ = std::fs::remove_dir_all(&proj_b);
    }

    #[test]
    fn test_remove_completed_tasks() {
        let (service, _events) = make_service();

        let t1 = service.create_task(TaskType::Import, None, "Import".to_string(), true);
        let t2 = service.create_task(TaskType::Export, None, "Export".to_string(), true);
        let t3 = service.create_task(TaskType::GraphBuild, None, "Graph".to_string(), true);

        service
            .transition_status(&t1.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&t1.id, TaskStatus::Succeeded)
            .unwrap();

        service
            .transition_status(&t2.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&t2.id, TaskStatus::Failed)
            .unwrap();

        let removed = service.remove_completed();
        assert_eq!(removed, 2);

        let remaining = service.list_tasks(None);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, t3.id);
    }

    #[test]
    fn discard_unstarted_tasks_removes_memory_and_persisted_files() {
        let (service, _events) = make_service();
        let root = std::env::temp_dir().join(format!("task-discard-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let first = service
            .create_project_task(
                TaskType::Import,
                "project".into(),
                root.clone(),
                "first".into(),
                true,
            )
            .unwrap();
        let second = service
            .create_project_task(
                TaskType::Import,
                "project".into(),
                root.clone(),
                "second".into(),
                true,
            )
            .unwrap();

        service
            .discard_unstarted_tasks(&[first.id.clone(), second.id.clone()])
            .unwrap();

        assert!(service.get_task(&first.id).is_none());
        assert!(service.get_task(&second.id).is_none());
        assert!(!root
            .join(".app/tasks")
            .join(format!("{}.json", first.id))
            .exists());
        assert!(!root
            .join(".app/tasks")
            .join(format!("{}.json", second.id))
            .exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn discard_rejects_replaced_persistence_parent_without_deleting_outside_file() {
        let (service, _events) = make_service();
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let task = service
            .create_project_task(
                TaskType::Import,
                "project".into(),
                root.path().to_path_buf(),
                "queued".into(),
                true,
            )
            .unwrap();
        let app_dir = root.path().join(".app");
        std::fs::remove_dir_all(&app_dir).unwrap();
        let outside_tasks = outside.path().join("tasks");
        std::fs::create_dir_all(&outside_tasks).unwrap();
        let outside_file = outside_tasks.join(format!("{}.json", task.id));
        std::fs::write(&outside_file, b"outside sentinel").unwrap();
        create_directory_alias(outside.path(), &app_dir);

        let result = service.discard_unstarted_tasks(std::slice::from_ref(&task.id));
        let outside_contents = std::fs::read(&outside_file).unwrap();
        let task_was_preserved = service.get_task(&task.id).is_some();
        remove_directory_alias(&app_dir);

        assert!(result.is_err());
        assert_eq!(outside_contents, b"outside sentinel");
        assert!(task_was_preserved);
    }

    #[test]
    fn recovery_rejects_json_named_directory_alias() {
        let (service, _events) = make_service();
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let tasks_dir = root.path().join(".app").join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let alias = tasks_dir.join("outside.json");
        create_directory_alias(outside.path(), &alias);

        let result = service.recover_tasks(root.path());
        remove_directory_alias(&alias);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsafe"));
    }

    #[test]
    fn recovery_rejects_task_ids_that_do_not_match_the_safe_file_stem() {
        let root = tempfile::tempdir().unwrap();
        let tasks_dir = root.path().join(".app").join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let (source, _) = make_service();
        let mut task = source.create_task(
            TaskType::Import,
            Some("project".into()),
            "malicious".into(),
            true,
        );
        task.id = "../../outside".into();
        std::fs::write(
            tasks_dir.join("malicious.json"),
            serde_json::to_vec_pretty(&task).unwrap(),
        )
        .unwrap();

        let (restarted, _) = make_service();
        let recovered = restarted.recover_tasks(root.path()).unwrap();

        assert!(recovered.is_empty());
        assert!(!root.path().join("outside.json").exists());
    }

    #[test]
    fn recovery_accepts_a_cjk_task_id_with_a_matching_safe_file_stem() {
        let root = tempfile::tempdir().unwrap();
        let tasks_dir = root.path().join(".app").join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        let (source, _) = make_service();
        let mut task = source.create_task(
            TaskType::Import,
            Some("project".into()),
            "unicode".into(),
            true,
        );
        task.id = "任务-一".into();
        std::fs::write(
            tasks_dir.join("任务-一.json"),
            serde_json::to_vec_pretty(&task).unwrap(),
        )
        .unwrap();

        let (restarted, _) = make_service();
        let recovered = restarted.recover_tasks(root.path()).unwrap();

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, "任务-一");
    }

    #[test]
    fn test_events_emitted_on_transitions() {
        let (service, events) = make_service();
        let task = service.create_task(
            TaskType::Import,
            Some("p1".to_string()),
            "Test".to_string(),
            true,
        );

        events.lock().unwrap().clear();

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&task.id, TaskStatus::Succeeded)
            .unwrap();

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].event_type, BackendEventType::TaskUpdated);
        assert_eq!(captured[1].event_type, BackendEventType::TaskCompleted);
    }

    #[test]
    fn test_confirmation_requested_event() {
        let (service, events) = make_service();
        let task = service.create_task(TaskType::AgentRun, None, "Needs confirm".to_string(), true);

        events.lock().unwrap().clear();
        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        events.lock().unwrap().clear();

        service
            .transition_status(&task.id, TaskStatus::WaitingForConfirmation)
            .unwrap();

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(
            captured[0].event_type,
            BackendEventType::ConfirmationRequested
        );
    }

    #[test]
    fn test_cancelling_to_cancelled_transition() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::GraphBuild, None, "Cancel me".to_string(), true);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service.cancel_task(&task.id).unwrap();

        let t = service.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
    }

    #[test]
    fn test_waiting_for_confirmation_to_cancelling() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::AgentRun, None, "Needs confirm".to_string(), true);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&task.id, TaskStatus::WaitingForConfirmation)
            .unwrap();
        service.cancel_task(&task.id).unwrap();

        let t = service.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
    }

    #[test]
    fn test_nonexistent_task_errors() {
        let (service, _events) = make_service();

        assert!(service.get_task("nonexistent").is_none());
        assert!(service
            .transition_status("nonexistent", TaskStatus::Running)
            .is_err());
        assert!(service
            .update_progress("nonexistent", 0, None, None)
            .is_err());
        assert!(service
            .append_log("nonexistent", LogLevel::Info, "msg".to_string())
            .is_err());
        assert!(service.get_logs("nonexistent").is_err());
    }

    #[test]
    fn test_log_event_emitted() {
        let (service, events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Log test".to_string(), true);

        events.lock().unwrap().clear();
        service
            .append_log(&task.id, LogLevel::Info, "Hello".to_string())
            .unwrap();

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].event_type, BackendEventType::TaskLog);
    }

    #[test]
    fn emit_stream_delta_emits_without_persisting_to_logs() {
        let (service, events) = make_service();
        let task = service.create_task(
            TaskType::LlmRequest,
            Some("p1".to_string()),
            "Chat".to_string(),
            true,
        );

        events.lock().unwrap().clear();
        service.emit_stream_delta(
            &task.id,
            StreamDelta {
                delta: "Hel".into(),
                route: Some("byok".into()),
            },
        );
        service.emit_stream_delta(
            &task.id,
            StreamDelta {
                delta: "lo".into(),
                route: None,
            },
        );

        // Two stream-output events fired...
        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].event_type, BackendEventType::TaskStreamOutput);
        assert_eq!(captured[1].event_type, BackendEventType::TaskStreamOutput);
        // ...but the task log store stays empty (deltas are ephemeral).
        assert!(service.get_logs(&task.id).unwrap().is_empty());
    }

    #[test]
    fn emit_stream_delta_for_unknown_task_is_noop() {
        let (service, events) = make_service();
        events.lock().unwrap().clear();
        // Must not panic on a missing task id.
        service.emit_stream_delta(
            "does-not-exist",
            StreamDelta {
                delta: "x".into(),
                route: None,
            },
        );
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn emit_activity_is_structured_and_recoverable_from_task_state() {
        let (service, events) = make_service();
        let task = service.create_task(TaskType::AgentRun, None, "Agent run".to_string(), true);
        events.lock().unwrap().clear();

        service.emit_activity(
            &task.id,
            TaskActivity::ToolCall {
                call_id: "tool-1".into(),
                name: "Read".into(),
                detail: Some("wiki/page.md".into()),
            },
        );

        assert_eq!(service.get_activities(&task.id).unwrap().len(), 1);
        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].event_type, BackendEventType::TaskActivity);
    }

    #[test]
    fn test_cancel_queued_task_goes_directly_to_cancelled() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Cancel queued".to_string(), true);

        // Task is Queued, cancel should go directly to Cancelled
        let result = service.cancel_task(&task.id);
        assert!(result.is_ok());

        let t = service.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
    }
}

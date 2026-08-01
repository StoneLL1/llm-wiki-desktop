use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use uuid::Uuid;

use crate::models::task::{
    BackendTask, StreamDelta, TaskActivity, TaskProgress, TaskResult, TaskStatus, TaskType,
};
use crate::models::workflow::{
    WorkflowErrorSummary, WorkflowExecutionState, WorkflowPendingAction, WorkflowResult,
    WorkflowRun, WorkflowStageStatus,
};
use crate::services::FileStore;
use crate::tasks::cancellation::CancellationRegistry;
use crate::tasks::task_events::EventBus;
use crate::tasks::task_model::{
    validate_transition, CancellationToken, LogLevel, LogLine, TaskEntry,
};

const PERSISTED_TASK_SCHEMA_VERSION: u32 = 2;

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

pub struct TaskService {
    tasks: RwLock<HashMap<String, TaskEntry>>,
    cancellation: CancellationRegistry,
    event_bus: RwLock<EventBus>,
    project_root: RwLock<Option<PathBuf>>,
    task_roots: RwLock<HashMap<String, PathBuf>>,
    task_persistence_dirs: RwLock<HashMap<String, PathBuf>>,
}

impl Default for TaskService {
    fn default() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            cancellation: CancellationRegistry::new(),
            event_bus: RwLock::new(EventBus::new_noop()),
            project_root: RwLock::new(None),
            task_roots: RwLock::new(HashMap::new()),
            task_persistence_dirs: RwLock::new(HashMap::new()),
        }
    }
}

impl TaskService {
    pub fn with_event_bus(event_bus: EventBus) -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            cancellation: CancellationRegistry::new(),
            event_bus: RwLock::new(event_bus),
            project_root: RwLock::new(None),
            task_roots: RwLock::new(HashMap::new()),
            task_persistence_dirs: RwLock::new(HashMap::new()),
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
        validate_persistence_dir(&root, &task_state_root)?;
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
        self.event_bus
            .read()
            .expect("lock poisoned")
            .emit(event_type, project_id, task_id, payload);
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
            true,
            None,
            Some(persistence_dir),
        )
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
            true,
            None,
            Some(persistence_dir),
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
        require_persistence: bool,
        workflow: Option<WorkflowExecutionState>,
        persistence_dir: Option<PathBuf>,
    ) -> Result<BackendTask, String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let token = self.cancellation.register(&id);

        let task = BackendTask {
            id: id.clone(),
            task_type: task_type.clone(),
            project_id: project_id.clone(),
            batch_id,
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

    pub(crate) fn mutate_workflow<F>(&self, id: &str, mutate: F) -> Result<WorkflowRun, String>
    where
        F: FnOnce(&mut BackendTask, &mut WorkflowExecutionState) -> Result<(), String>,
    {
        let persistence_dir = self
            .task_persistence_dirs
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let project_root = self
            .task_roots
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        let mut tasks = self.tasks.write().expect("lock poisoned");
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

        let persisted = PersistedTaskEntry {
            schema_version: PERSISTED_TASK_SCHEMA_VERSION,
            task: entry.task.clone(),
            log_lines: entry.log_lines.clone(),
            activities: entry.activities.clone(),
            workflow: entry.workflow.clone(),
        };
        if let Some(tasks_dir) = persistence_dir {
            let project_root = project_root
                .as_deref()
                .ok_or_else(|| format!("Workflow task has no project root: {id}"))?;
            validate_persistence_dir(project_root, &tasks_dir)?;
            let write_result = std::fs::create_dir_all(&tasks_dir)
                .map_err(|error| format!("Failed to create tasks dir: {error}"))
                .and_then(|_| {
                    let path = tasks_dir.join(format!("{id}.json"));
                    FileStore
                        .write_json_atomic_absolute(&path, &persisted)
                        .map_err(|error| format!("Failed to write task file: {}", error.message))?;
                    entry.persisted_path = Some(path);
                    Ok(())
                });
            if let Err(error) = write_result {
                entry.task = previous_task;
                entry.workflow = previous_workflow;
                return Err(error);
            }
        }

        let task = entry.task.clone();
        let run = entry
            .workflow
            .as_ref()
            .and_then(|workflow| workflow.to_run(&task))
            .ok_or_else(|| format!("Workflow task has no project: {id}"))?;
        drop(tasks);

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
        Ok(run)
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
            stage.current_item = current_item;
            stage.progress =
                Some(crate::models::workflow::WorkflowCountProgress { current, total });
            Ok(())
        })
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
        self.emit(TaskLog, pid, Some(tid), line);
        self.persist_current_task(id)?;

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
        let _ = self.persist_current_task(id);
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

        if status == TaskStatus::Queued {
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
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {id}"))?;
        if !entry.task.cancellable {
            return Err(format!("Task is not cancellable: {id}"));
        }
        if matches!(
            entry.task.status,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) {
            return Ok(entry.task.clone());
        }
        let status = entry.task.status.clone();
        drop(tasks);
        self.cancellation.cancel(id);
        if status == TaskStatus::Queued {
            self.transition_status(id, TaskStatus::Cancelled)
        } else if status == TaskStatus::Cancelling {
            self.get_task(id)
                .ok_or_else(|| format!("Task not found: {id}"))
        } else {
            self.transition_status(id, TaskStatus::Cancelling)
        }
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

    pub fn set_error(
        &self,
        id: &str,
        error: crate::errors::BackendError,
    ) -> Result<BackendTask, String> {
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
        self.persist_current_task(id)?;
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
        tasks.retain(|id, entry| {
            let is_terminal = matches!(
                entry.task.status,
                TaskStatus::Succeeded
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::Interrupted
            );
            if is_terminal {
                removed_ids.push(id.clone());
                if let Some(path) = &entry.persisted_path {
                    removed_paths.push(path.clone());
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
        self.task_roots
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !removed_ids.contains(id));
        self.task_persistence_dirs
            .write()
            .expect("lock poisoned")
            .retain(|id, _| !removed_ids.contains(id));
        for path in removed_paths {
            let _ = std::fs::remove_file(path);
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
        {
            let mut tasks = self.tasks.write().expect("lock poisoned");
            for id in &matching_ids {
                if let Some(entry) = tasks.remove(id) {
                    if let Some(path) = entry.persisted_path {
                        removed_paths.push(path);
                    }
                }
            }
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
        for path in removed_paths {
            let _ = std::fs::remove_file(path);
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
                    paths.push(path.clone());
                }
            }
            paths
        };
        for path in &persisted_paths {
            if path.exists() {
                std::fs::remove_file(path).map_err(|error| {
                    format!(
                        "Failed to discard prepared task {}: {error}",
                        path.display()
                    )
                })?;
            }
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
        for id in ids {
            self.cancellation.remove(id);
        }
        Ok(())
    }

    pub fn persist_task(&self, id: &str, project_root: &Path) -> Result<(), String> {
        let tasks_dir = project_root.join(".app").join("tasks");
        validate_persistence_dir(project_root, &tasks_dir)?;
        self.persist_task_to_dir(id, &tasks_dir)?;
        self.task_persistence_dirs
            .write()
            .expect("lock poisoned")
            .insert(id.to_string(), tasks_dir);
        Ok(())
    }

    fn persist_task_to_dir(&self, id: &str, tasks_dir: &Path) -> Result<(), String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;

        std::fs::create_dir_all(tasks_dir)
            .map_err(|e| format!("Failed to create tasks dir: {}", e))?;

        let path = tasks_dir.join(format!("{}.json", id));
        let persisted = PersistedTaskEntry {
            schema_version: PERSISTED_TASK_SCHEMA_VERSION,
            task: entry.task.clone(),
            log_lines: entry.log_lines.clone(),
            activities: entry.activities.clone(),
            workflow: entry.workflow.clone(),
        };
        FileStore
            .write_json_atomic_absolute(&path, &persisted)
            .map_err(|error| format!("Failed to write task file: {}", error.message))?;

        drop(tasks);
        let mut tasks = self.tasks.write().expect("lock poisoned");
        if let Some(entry) = tasks.get_mut(id) {
            entry.persisted_path = Some(path);
        }

        Ok(())
    }

    fn persist_current_task(&self, id: &str) -> Result<(), String> {
        let persistence_dir = self
            .task_persistence_dirs
            .read()
            .expect("lock poisoned")
            .get(id)
            .cloned();
        match persistence_dir {
            Some(dir) => {
                let project_root = self
                    .task_roots
                    .read()
                    .expect("lock poisoned")
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("Task has no project root: {id}"))?;
                validate_persistence_dir(&project_root, &dir)?;
                self.persist_task_to_dir(id, &dir)
            }
            None => Ok(()),
        }
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
        if !tasks_dir.exists() {
            return Ok(Vec::new());
        }

        let mut recovered = Vec::new();
        let entries = std::fs::read_dir(&tasks_dir)
            .map_err(|e| format!("Failed to read tasks dir: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                match std::fs::read_to_string(&path) {
                    Ok(json) => {
                        let parsed = serde_json::from_str::<PersistedTaskEntry>(&json)
                            .map(|entry| {
                                (
                                    entry.task,
                                    entry.log_lines,
                                    entry.activities,
                                    entry.workflow,
                                )
                            })
                            .or_else(|_| {
                                serde_json::from_str::<BackendTask>(&json)
                                    .map(|task| (task, Vec::new(), Vec::new(), None))
                            });
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
                                        | TaskStatus::WaitingForConfirmation
                                ) {
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

fn validate_persistence_dir(project_root: &Path, persistence_dir: &Path) -> Result<(), String> {
    let canonical_root = project_root
        .canonicalize()
        .map_err(|error| format!("Project root is unavailable: {error}"))?;
    let mut ancestor = persistence_dir;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| "Task persistence path has no existing ancestor".to_string())?;
    }
    let canonical_ancestor = ancestor
        .canonicalize()
        .map_err(|error| format!("Task persistence ancestor is unavailable: {error}"))?;
    if !canonical_ancestor.starts_with(canonical_root) {
        return Err("Task persistence path resolves outside the project root".into());
    }
    Ok(())
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
    use crate::models::task::BackendEventType;
    use crate::tasks::task_events::CapturedEvent;
    use crate::tasks::task_model::LogLevel;
    use std::sync::{Arc, Mutex};

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

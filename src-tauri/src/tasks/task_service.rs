use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use uuid::Uuid;

use crate::models::task::{BackendTask, TaskProgress, TaskResult, TaskStatus, TaskType};
use crate::tasks::cancellation::CancellationRegistry;
use crate::tasks::task_events::EventBus;
use crate::tasks::task_model::{
    validate_transition, CancellationToken, LogLevel, LogLine, TaskEntry,
};

pub struct TaskService {
    tasks: RwLock<HashMap<String, TaskEntry>>,
    cancellation: CancellationRegistry,
    event_bus: RwLock<EventBus>,
    project_root: RwLock<Option<PathBuf>>,
}

impl Default for TaskService {
    fn default() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            cancellation: CancellationRegistry::new(),
            event_bus: RwLock::new(EventBus::new_noop()),
            project_root: RwLock::new(None),
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
        }
    }

    pub fn set_event_bus(&self, event_bus: EventBus) {
        let mut guard = self.event_bus.write().expect("lock poisoned");
        *guard = event_bus;
    }

    /// Set the active project root. When set, terminal task transitions auto-persist to
    /// `<root>/.app/tasks/<id>.json`, and any previously-persisted tasks are recovered.
    /// Pass `None` when the project is closed. The in-memory task cache and cancellation
    /// registry are cleared on every call, since tasks are project-scoped — switching
    /// projects must not leak the previous project's tasks into `list_tasks`.
    pub fn set_project_root(&self, root: Option<PathBuf>) -> Result<Vec<BackendTask>, String> {
        {
            let mut guard = self.project_root.write().expect("lock poisoned");
            *guard = root.clone();
        }
        self.tasks.write().expect("lock poisoned").clear();
        self.cancellation.clear();
        match root {
            Some(root_path) => self.recover_tasks(&root_path),
            None => Ok(Vec::new()),
        }
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
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let token = self.cancellation.register(&id);

        let task = BackendTask {
            id: id.clone(),
            task_type: task_type.clone(),
            project_id: project_id.clone(),
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
            persisted_path: None,
        };

        self.tasks
            .write()
            .expect("lock poisoned")
            .insert(id.clone(), entry);

        use crate::models::task::BackendEventType::TaskUpdated;
        self.emit(TaskUpdated, project_id, Some(id.clone()), task.clone());

        task
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
            TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
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

        // Auto-persist on terminal transitions when a project root is bound.
        if matches!(
            new_status,
            TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            if let Some(root) = self.current_project_root() {
                if let Err(e) = self.persist_task(&tid, &root) {
                    eprintln!("Failed to auto-persist task {}: {}", tid, e);
                }
            }
        }

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

        Ok(())
    }

    pub fn get_logs(&self, id: &str) -> Result<Vec<LogLine>, String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;
        Ok(entry.log_lines.clone())
    }

    pub fn cancel_task(&self, id: &str) -> Result<BackendTask, String> {
        let status = {
            let tasks = self.tasks.read().expect("lock poisoned");
            let entry = tasks
                .get(id)
                .ok_or_else(|| format!("Task not found: {}", id))?;
            if !entry.task.cancellable {
                return Err(format!("Task is not cancellable: {}", id));
            }
            if matches!(
                entry.task.status,
                TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
            ) {
                return Err(format!(
                    "Cannot cancel task in terminal state: {:?}",
                    entry.task.status
                ));
            }
            entry.task.status.clone()
        };

        self.cancellation.cancel(id);

        if status == TaskStatus::Queued {
            self.transition_status(id, TaskStatus::Cancelled)
        } else {
            self.transition_status(id, TaskStatus::Cancelling)?;
            self.transition_status(id, TaskStatus::Cancelled)
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
        Ok(entry.task.clone())
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
        tasks.retain(|id, entry| {
            let is_terminal = matches!(
                entry.task.status,
                TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
            );
            if is_terminal {
                removed_ids.push(id.clone());
            }
            !is_terminal
        });

        // Clean up persisted files and cancellation tokens for removed tasks.
        for id in &removed_ids {
            self.cancellation.remove(id);
        }
        if let Some(root) = self.current_project_root() {
            let tasks_dir = root.join(".app").join("tasks");
            for id in &removed_ids {
                let path = tasks_dir.join(format!("{}.json", id));
                let _ = std::fs::remove_file(&path);
            }
        }

        before - tasks.len()
    }

    pub fn persist_task(&self, id: &str, project_root: &Path) -> Result<(), String> {
        let tasks = self.tasks.read().expect("lock poisoned");
        let entry = tasks
            .get(id)
            .ok_or_else(|| format!("Task not found: {}", id))?;

        let tasks_dir = project_root.join(".app").join("tasks");
        std::fs::create_dir_all(&tasks_dir)
            .map_err(|e| format!("Failed to create tasks dir: {}", e))?;

        let path = tasks_dir.join(format!("{}.json", id));
        let json = serde_json::to_string_pretty(&entry.task)
            .map_err(|e| format!("Failed to serialize task: {}", e))?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to write task file: {}", e))?;

        drop(tasks);
        let mut tasks = self.tasks.write().expect("lock poisoned");
        if let Some(entry) = tasks.get_mut(id) {
            entry.persisted_path = Some(path);
        }

        Ok(())
    }

    pub fn recover_tasks(&self, project_root: &Path) -> Result<Vec<BackendTask>, String> {
        let tasks_dir = project_root.join(".app").join("tasks");
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
                    Ok(json) => match serde_json::from_str::<BackendTask>(&json) {
                        Ok(mut task) => {
                            let token = self.cancellation.register(&task.id);
                            if matches!(
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
                                log_lines: Vec::new(),
                                persisted_path: Some(path),
                            };

                            self.tasks
                                .write()
                                .expect("lock poisoned")
                                .insert(task.id.clone(), task_entry);
                            recovered.push(task);
                        }
                        Err(e) => {
                            eprintln!("Failed to parse task file {}: {}", path.display(), e);
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to read task file {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(recovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::task::BackendEventType;
    use crate::tasks::task_events::CapturedEvent;
    use crate::tasks::task_model::LogLevel;
    use std::sync::{Arc, Mutex};

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
    fn test_cancel_already_completed_task_fails() {
        let (service, _events) = make_service();
        let task = service.create_task(TaskType::Import, None, "Import".to_string(), true);

        service
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        service
            .transition_status(&task.id, TaskStatus::Succeeded)
            .unwrap();

        let result = service.cancel_task(&task.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("terminal state"));
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
    fn test_set_project_root_clears_previous_tasks() {
        // Tasks are project-scoped: switching the active project root must not leak the
        // previous project's in-memory tasks (or cancellation tokens) into list_tasks.
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

        // Switch to project B: cache must be cleared, no A tasks visible.
        let recovered_b = service.set_project_root(Some(proj_b.clone())).unwrap();
        assert!(recovered_b.is_empty());
        assert!(service.list_tasks(None).is_empty());
        // The previous project's cancellation token must no longer be tracked.
        assert!(service.get_cancellation_token(&task_a.id).is_none());
        // The shared AtomicBool behind the stale token is unaffected (still readable),
        // but it is no longer reachable through the registry — i.e. switching projects
        // cannot cancel the previous project's still-running work.
        assert!(!token_a.is_cancelled());

        // Create a task in project B; it must coexist with neither A task.
        let task_b =
            service.create_task(TaskType::Export, None, "Project B export".to_string(), true);
        let listed = service.list_tasks(None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, task_b.id);

        // Closing the project (None) must also clear the cache.
        let recovered_none = service.set_project_root(None).unwrap();
        assert!(recovered_none.is_empty());
        assert!(service.list_tasks(None).is_empty());

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

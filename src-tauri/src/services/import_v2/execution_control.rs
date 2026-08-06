use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::models::task::{TaskResult, TaskStatus};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

/// Execution controls carry operation-level facts only. Import item state is
/// always persisted by the orchestrator/session store, never inferred from a
/// task lifecycle.
pub(crate) trait ImportExecutionControl {
    fn operation_id(&self) -> &str;
    fn is_cancelled(&self) -> bool;
    fn progress(&mut self, current: u64, total: Option<u64>, label: String) -> Result<(), String>;
    fn log(&mut self, level: LogLevel, message: String) -> Result<(), String>;
}

/// The operation task deliberately aggregates terminal item facts instead of
/// attempting to model every item state with `TaskStatus`.  The counters are
/// shared by the bounded workers, but are not a task registry: there is one
/// cancellation token and one persistent task for the complete operation.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ImportOperationSummary {
    pub ready: u64,
    pub completed: u64,
    pub waiting: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub systemic_errors: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportItemRunOutcome {
    Ready,
    Completed,
    Waiting,
    Failed,
    Cancelled,
    SystemicError,
}

pub(crate) fn batch_terminal_status(summary: &ImportOperationSummary) -> TaskStatus {
    if summary.failed > 0 || summary.systemic_errors > 0 {
        TaskStatus::Failed
    } else if summary.waiting > 0 {
        TaskStatus::WaitingForConfirmation
    } else {
        TaskStatus::Succeeded
    }
}

#[derive(Debug)]
pub(crate) struct BatchOperationState {
    total: u64,
    completed: u64,
    summary: ImportOperationSummary,
    last_publish: Instant,
}

impl BatchOperationState {
    pub(crate) fn new(total: u64) -> Self {
        Self {
            total,
            completed: 0,
            summary: ImportOperationSummary::default(),
            last_publish: Instant::now() - Duration::from_millis(100),
        }
    }

    pub(crate) fn record(
        &mut self,
        outcome: ImportItemRunOutcome,
    ) -> (u64, u64, ImportOperationSummary, bool) {
        self.record_at(outcome, Instant::now())
    }

    fn record_at(
        &mut self,
        outcome: ImportItemRunOutcome,
        now: Instant,
    ) -> (u64, u64, ImportOperationSummary, bool) {
        self.completed += 1;
        match outcome {
            ImportItemRunOutcome::Ready => self.summary.ready += 1,
            ImportItemRunOutcome::Completed => self.summary.completed += 1,
            ImportItemRunOutcome::Waiting => self.summary.waiting += 1,
            ImportItemRunOutcome::Failed => self.summary.failed += 1,
            ImportItemRunOutcome::Cancelled => self.summary.cancelled += 1,
            ImportItemRunOutcome::SystemicError => self.summary.systemic_errors += 1,
        }
        let terminal = self.completed == self.total;
        let publish =
            terminal || now.duration_since(self.last_publish) >= Duration::from_millis(100);
        if publish {
            self.last_publish = now;
        }
        (self.completed, self.total, self.summary.clone(), publish)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::task::TaskType;

    #[test]
    fn batch_progress_is_throttled_with_a_deterministic_clock_and_flushes_terminal() {
        let mut state = BatchOperationState::new(1_000);
        let start = Instant::now();
        state.last_publish = start;
        let mut publishes = 0;
        for index in 0..1_000 {
            let at = start + Duration::from_millis((index / 100) as u64 * 100);
            let (_, _, _, publish) = state.record_at(ImportItemRunOutcome::Ready, at);
            publishes += usize::from(publish);
        }
        assert_eq!(
            publishes, 10,
            "nine throttled updates plus terminal flush in one second"
        );
    }

    #[test]
    fn partial_failure_or_attention_never_reports_success() {
        assert_eq!(
            batch_terminal_status(&ImportOperationSummary {
                failed: 1,
                ..Default::default()
            }),
            TaskStatus::Failed
        );
        assert_eq!(
            batch_terminal_status(&ImportOperationSummary {
                waiting: 1,
                ..Default::default()
            }),
            TaskStatus::WaitingForConfirmation
        );
        assert_eq!(
            batch_terminal_status(&ImportOperationSummary {
                ready: 1,
                completed: 1,
                ..Default::default()
            }),
            TaskStatus::Succeeded
        );
    }

    #[test]
    fn batch_execution_control_observes_the_one_shared_cancellation_token() {
        let root = tempfile::tempdir().unwrap();
        let tasks = TaskService::default();
        let task = tasks
            .create_project_task_with_batch(
                TaskType::Import,
                "batch-cancel".into(),
                root.path().to_path_buf(),
                "Import batch".into(),
                true,
                "import-v2-operation:session".into(),
            )
            .unwrap();
        let state = Arc::new(Mutex::new(BatchOperationState::new(2)));
        let control = BatchExecutionControl::new(&tasks, &task.id, state);
        assert!(!control.is_cancelled());
        tasks.cancel_task(&task.id).unwrap();
        assert!(control.is_cancelled());
        assert_eq!(tasks.list_tasks(None).len(), 1);
    }
}

pub(crate) struct LegacyTaskExecutionControl<'a> {
    tasks: &'a TaskService,
    task_id: &'a str,
}

impl<'a> LegacyTaskExecutionControl<'a> {
    pub(crate) fn new(tasks: &'a TaskService, task_id: &'a str) -> Self {
        Self { tasks, task_id }
    }

    pub(crate) fn transition(&self, status: TaskStatus) -> Result<(), String> {
        self.tasks
            .transition_status(self.task_id, status)
            .map(|_| ())
    }

    pub(crate) fn set_result(&self, result: TaskResult) -> Result<(), String> {
        self.tasks.set_result(self.task_id, result).map(|_| ())
    }
}

impl ImportExecutionControl for LegacyTaskExecutionControl<'_> {
    fn operation_id(&self) -> &str {
        self.task_id
    }
    fn is_cancelled(&self) -> bool {
        self.tasks.is_cancelled(self.task_id)
    }
    fn progress(&mut self, current: u64, total: Option<u64>, label: String) -> Result<(), String> {
        self.tasks
            .update_progress(self.task_id, current, total, Some(label))
            .map(|_| ())
    }
    fn log(&mut self, level: LogLevel, message: String) -> Result<(), String> {
        self.tasks
            .append_log(self.task_id, level, message)
            .map(|_| ())
    }
}

pub(crate) struct BatchExecutionControl<'a> {
    tasks: &'a TaskService,
    task_id: &'a str,
    state: Arc<Mutex<BatchOperationState>>,
}

impl<'a> BatchExecutionControl<'a> {
    pub(crate) fn new(
        tasks: &'a TaskService,
        task_id: &'a str,
        state: Arc<Mutex<BatchOperationState>>,
    ) -> Self {
        Self {
            tasks,
            task_id,
            state,
        }
    }

    pub(crate) fn flush_progress(
        &mut self,
        current: u64,
        total: u64,
        label: String,
    ) -> Result<(), String> {
        self.state
            .lock()
            .map_err(|_| "Import batch state lock poisoned.".to_string())?
            .last_publish = Instant::now();
        self.tasks
            .update_progress(self.task_id, current, Some(total), Some(label))
            .map(|_| ())
    }

    pub(crate) fn record_outcome(
        &mut self,
        outcome: ImportItemRunOutcome,
    ) -> Result<(u64, u64, ImportOperationSummary, bool), String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "Import batch state lock poisoned.".to_string())?
            .record(outcome))
    }
}

impl ImportExecutionControl for BatchExecutionControl<'_> {
    fn operation_id(&self) -> &str {
        self.task_id
    }
    fn is_cancelled(&self) -> bool {
        self.tasks.is_cancelled(self.task_id)
    }
    fn progress(&mut self, current: u64, total: Option<u64>, label: String) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Import batch state lock poisoned.".to_string())?;
        if state.last_publish.elapsed() >= Duration::from_millis(100) {
            state.last_publish = Instant::now();
            self.tasks
                .update_progress(self.task_id, current, total, Some(label))
                .map(|_| ())?;
        }
        Ok(())
    }
    fn log(&mut self, level: LogLevel, message: String) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Import batch state lock poisoned.".to_string())?;
        if state.last_publish.elapsed() >= Duration::from_millis(100) {
            state.last_publish = Instant::now();
            self.tasks
                .append_log(self.task_id, level, message)
                .map(|_| ())?;
        }
        Ok(())
    }
}

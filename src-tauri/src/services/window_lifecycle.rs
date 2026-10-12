use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::confirmation::{UpdateInstallBarrier, UpdateInstallLease};
use crate::models::task::{BackendTask, TaskStatus};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowCloseIntent {
    Ask,
    Quit,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowCloseAction {
    Cancel,
    Background,
    Quit,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveWindowCloseRequest {
    pub action: WindowCloseAction,
    pub unsaved_changes: bool,
    pub discard_unsaved_changes: bool,
    pub editor_saving: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowCloseSnapshot {
    pub intent: Option<WindowCloseIntent>,
    pub active_tasks: Vec<BackendTask>,
}

#[derive(Default)]
struct CloseState {
    pending: Option<WindowCloseIntent>,
    // Keep admission closed until process exit, rather than releasing the
    // guard after AppHandle::exit merely queues the exit request.
    shutdown_lease: Option<UpdateInstallLease>,
}

#[derive(Default)]
pub struct WindowLifecycle {
    state: Mutex<CloseState>,
}

impl WindowLifecycle {
    pub fn request(&self, intent: WindowCloseIntent) -> Result<(), BackendError> {
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        if state.shutdown_lease.is_none() {
            state.pending.get_or_insert(intent);
        }
        Ok(())
    }

    pub fn snapshot(&self, tasks: Vec<BackendTask>) -> Result<WindowCloseSnapshot, BackendError> {
        let state = self.state.lock().map_err(|_| unavailable())?;
        Ok(WindowCloseSnapshot {
            intent: state.pending,
            active_tasks: tasks.into_iter().filter(is_active_task).collect(),
        })
    }

    pub fn dismiss(&self) -> Result<(), BackendError> {
        self.state.lock().map_err(|_| unavailable())?.pending = None;
        Ok(())
    }

    pub(crate) fn reserve_quit(
        &self,
        barrier: &UpdateInstallBarrier,
        validate: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), BackendError> {
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        if state.pending.is_none() {
            return Err(BackendError::new(
                "APP_CLOSE_NOT_REQUESTED",
                "Request window close before confirming exit.",
                true,
                true,
            ));
        }
        let lease = barrier.reserve_install_or_restart(validate)?;
        state.shutdown_lease = Some(lease);
        state.pending = None;
        Ok(())
    }
}

pub fn is_active_task(task: &BackendTask) -> bool {
    matches!(
        task.status,
        TaskStatus::Queued
            | TaskStatus::Running
            | TaskStatus::Cancelling
            | TaskStatus::WaitingForConfirmation
    )
}

pub(crate) fn validate_quit(
    request: &ResolveWindowCloseRequest,
    tasks: &[BackendTask],
    workflow_apply_active: bool,
) -> Result<(), BackendError> {
    if request.editor_saving {
        return Err(BackendError::new(
            "APP_CLOSE_EDITOR_SAVING",
            "Wait for the editor save to finish before quitting.",
            true,
            true,
        ));
    }
    if request.unsaved_changes && !request.discard_unsaved_changes {
        return Err(BackendError::new(
            "APP_CLOSE_UNSAVED_CHANGES",
            "Confirm discarding unsaved changes before quitting.",
            true,
            true,
        ));
    }
    if workflow_apply_active || tasks.iter().any(BackendTask::blocks_update_install) {
        return Err(BackendError::new(
            "APP_CLOSE_CRITICAL_TASK",
            "Wait for the current file operation to finish before quitting.",
            true,
            true,
        ));
    }
    Ok(())
}

fn unavailable() -> BackendError {
    BackendError::new(
        "APP_CLOSE_UNAVAILABLE",
        "Window close state is unavailable.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::task::{TaskOperation, TaskType};

    fn request() -> ResolveWindowCloseRequest {
        ResolveWindowCloseRequest {
            action: WindowCloseAction::Quit,
            unsaved_changes: false,
            discard_unsaved_changes: false,
            editor_saving: false,
        }
    }

    fn task() -> BackendTask {
        BackendTask {
            id: "test".into(),
            task_type: TaskType::LlmRequest,
            project_id: None,
            batch_id: None,
            operation: None,
            title: "Test".into(),
            status: TaskStatus::Running,
            progress: None,
            started_at: String::new(),
            updated_at: String::new(),
            completed_at: None,
            cancellable: true,
            log_path: None,
            result: None,
            error: None,
        }
    }

    #[test]
    fn duplicate_close_requests_keep_one_pending_intent_and_cancel_clears_it() {
        let lifecycle = WindowLifecycle::default();
        lifecycle.request(WindowCloseIntent::Ask).unwrap();
        lifecycle.request(WindowCloseIntent::Quit).unwrap();
        assert_eq!(
            lifecycle.snapshot(vec![]).unwrap().intent,
            Some(WindowCloseIntent::Ask)
        );
        lifecycle.dismiss().unwrap();
        assert_eq!(lifecycle.snapshot(vec![]).unwrap().intent, None);
    }

    #[test]
    fn quit_requires_explicit_discard_but_never_interrupts_editor_saving() {
        let mut request = request();
        request.unsaved_changes = true;
        assert_eq!(
            validate_quit(&request, &[], false).unwrap_err().code,
            "APP_CLOSE_UNSAVED_CHANGES"
        );
        request.discard_unsaved_changes = true;
        validate_quit(&request, &[], false).unwrap();
        request.editor_saving = true;
        assert_eq!(
            validate_quit(&request, &[], false).unwrap_err().code,
            "APP_CLOSE_EDITOR_SAVING"
        );
    }

    #[test]
    fn quit_blocks_critical_tasks_and_workflow_apply_but_allows_safe_background_work() {
        let mut task = task();
        validate_quit(&request(), &[task.clone()], false).unwrap();
        task.operation = Some(TaskOperation::ImportCommit {
            session_id: "session".into(),
        });
        assert!(validate_quit(&request(), &[task.clone()], false).is_err());
        task.status = TaskStatus::Succeeded;
        validate_quit(&request(), &[task], false).unwrap();
        assert!(validate_quit(&request(), &[], true).is_err());
    }

    #[test]
    fn quit_reuses_mutation_barrier_and_keeps_it_reserved_until_shutdown() {
        let lifecycle = WindowLifecycle::default();
        let barrier = UpdateInstallBarrier::default();
        lifecycle.request(WindowCloseIntent::Ask).unwrap();
        let mutation = barrier.enter_project_mutation().unwrap();
        assert!(lifecycle.reserve_quit(&barrier, || Ok(())).is_err());
        assert_eq!(
            lifecycle.snapshot(vec![]).unwrap().intent,
            Some(WindowCloseIntent::Ask)
        );
        drop(mutation);
        lifecycle.reserve_quit(&barrier, || Ok(())).unwrap();
        assert!(barrier.enter_project_mutation().is_err());
        assert!(barrier.reserve_install_or_restart(|| Ok(())).is_err());
        drop(lifecycle);
        assert!(barrier.enter_project_mutation().is_ok());
    }
}

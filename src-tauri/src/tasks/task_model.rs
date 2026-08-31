use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::models::task::{BackendTask, TaskActivity, TaskStatus};
use crate::models::workflow::WorkflowExecutionState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

#[derive(Clone)]
pub struct TaskEntry {
    pub task: BackendTask,
    pub cancellation: CancellationToken,
    pub log_lines: Vec<LogLine>,
    pub activities: Vec<TaskActivity>,
    pub persisted_path: Option<PathBuf>,
    pub workflow: Option<WorkflowExecutionState>,
}

#[derive(Debug, Clone)]
pub struct CancellationToken {
    state: Arc<AtomicU8>,
}

const TASK_SIGNAL_RUNNING: u8 = 0;
const TASK_SIGNAL_CANCELLED: u8 = 1;
const TASK_SIGNAL_PAUSED: u8 = 2;

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(TASK_SIGNAL_RUNNING)),
        }
    }

    pub fn cancel(&self) {
        self.state.store(TASK_SIGNAL_CANCELLED, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst) != TASK_SIGNAL_RUNNING
    }

    pub fn request_pause(&self) {
        let _ = self.state.compare_exchange(
            TASK_SIGNAL_RUNNING,
            TASK_SIGNAL_PAUSED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }

    pub fn is_pause_requested(&self) -> bool {
        self.state.load(Ordering::SeqCst) == TASK_SIGNAL_PAUSED
    }

    pub fn reset(&self) {
        self.state.store(TASK_SIGNAL_RUNNING, Ordering::SeqCst);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

pub fn validate_transition(current: &TaskStatus, next: &TaskStatus) -> Result<(), String> {
    use TaskStatus::*;
    match (current, next) {
        (Queued, Running)
        | (Queued, Interrupted)
        | (Queued, Cancelled)
        | (Running, WaitingForConfirmation)
        | (Running, Succeeded)
        | (Running, Failed)
        | (Running, Interrupted)
        | (Running, Cancelling)
        | (WaitingForConfirmation, Running)
        | (WaitingForConfirmation, Cancelling)
        | (Cancelling, Cancelled)
        | (Cancelling, Failed)
        | (Cancelling, Interrupted)
        | (Interrupted, Queued)
        | (Interrupted, Cancelled) => Ok(()),
        _ => Err(format!(
            "Invalid state transition: {:?} -> {:?}",
            current, next
        )),
    }
}

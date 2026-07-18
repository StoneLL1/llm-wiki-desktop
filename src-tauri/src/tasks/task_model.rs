use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::models::task::{BackendTask, TaskActivity, TaskStatus};

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

pub struct TaskEntry {
    pub task: BackendTask,
    pub cancellation: CancellationToken,
    pub log_lines: Vec<LogLine>,
    pub activities: Vec<TaskActivity>,
    pub persisted_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
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
        | (Queued, Cancelled)
        | (Running, WaitingForConfirmation)
        | (Running, Succeeded)
        | (Running, Failed)
        | (Running, Cancelling)
        | (WaitingForConfirmation, Running)
        | (WaitingForConfirmation, Cancelling)
        | (Cancelling, Cancelled)
        | (Cancelling, Failed) => Ok(()),
        _ => Err(format!(
            "Invalid state transition: {:?} -> {:?}",
            current, next
        )),
    }
}

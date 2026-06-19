use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use uuid::Uuid;

use crate::models::task::{BackendEvent, BackendEventType};

#[derive(Clone)]
pub enum EventBus {
    Tauri(Arc<tauri::AppHandle>),
    Noop,
    #[doc(hidden)]
    TestCapture(Arc<Mutex<Vec<CapturedEvent>>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedEvent {
    pub event_type: BackendEventType,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}

impl EventBus {
    pub fn new_tauri(app_handle: tauri::AppHandle) -> Self {
        EventBus::Tauri(Arc::new(app_handle))
    }

    pub fn new_noop() -> Self {
        EventBus::Noop
    }

    pub fn new_test_capture() -> (Self, Arc<Mutex<Vec<CapturedEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (EventBus::TestCapture(events.clone()), events)
    }

    pub fn emit<T: Serialize + Clone + Send + Sync + 'static>(
        &self,
        event_type: BackendEventType,
        project_id: Option<String>,
        task_id: Option<String>,
        payload: T,
    ) {
        let event = BackendEvent {
            event_id: Uuid::new_v4().to_string(),
            event_type,
            project_id,
            task_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
        };
        match self {
            EventBus::Tauri(app_handle) => {
                let event_name = event_type_to_tauri_name(&event.event_type);
                let _ = app_handle.emit(&event_name, event);
            }
            EventBus::Noop => {}
            EventBus::TestCapture(events) => {
                events.lock().unwrap().push(CapturedEvent {
                    event_type: event.event_type,
                    project_id: event.project_id,
                    task_id: event.task_id,
                });
            }
        }
    }
}

fn event_type_to_tauri_name(event_type: &BackendEventType) -> String {
    use BackendEventType::*;
    match event_type {
        TaskUpdated => "task://updated",
        TaskLog => "task://log",
        TaskCompleted => "task://completed",
        TaskFailed => "task://failed",
        TaskCancelled => "task://cancelled",
        ConfirmationRequested => "confirmation://requested",
        ProjectRefreshed => "project://refreshed",
        WikiChanged => "wiki://changed",
        GraphUpdated => "graph://updated",
        AgentOutput => "agent://output",
    }
    .to_string()
}

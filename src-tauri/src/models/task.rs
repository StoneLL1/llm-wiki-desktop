use serde::{Deserialize, Serialize};

use crate::errors::BackendError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackendTask {
    pub id: String,
    pub task_type: TaskType,
    pub project_id: Option<String>,
    pub title: String,
    pub status: TaskStatus,
    pub progress: Option<TaskProgress>,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancellable: bool,
    pub log_path: Option<String>,
    pub result: Option<TaskResult>,
    pub error: Option<BackendError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Import,
    WikiCompile,
    AgentRun,
    LlmRequest,
    GraphBuild,
    DeepLint,
    AutoFix,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    WaitingForConfirmation,
    Cancelling,
    Cancelled,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub current: u64,
    pub total: Option<u64>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub summary: String,
    pub affected_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<crate::models::confirmation::PendingAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackendEvent<T> {
    pub event_id: String,
    pub event_type: BackendEventType,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub timestamp: String,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendEventType {
    TaskUpdated,
    TaskLog,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
    ConfirmationRequested,
    ProjectRefreshed,
    WikiChanged,
    GraphUpdated,
    AgentOutput,
    /// Ephemeral streaming delta for a long-running generative task (chat
    /// answer token-by-token). Emitted via `TaskService::emit_stream_delta`,
    /// NOT persisted into `log_lines` — the authoritative answer lands in the
    /// chat session file on task completion; these deltas are only live UI
    /// hints. Frontend channel: `task://stream-output`.
    TaskStreamOutput,
}

/// Payload of a [`BackendEventType::TaskStreamOutput`] event. `delta` is the
/// incremental text (the frontend concatenates); `route` is an optional
/// human-readable label ("agent" / "byok") so the UI can render a live model
/// badge before the persisted answer exists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamDelta {
    pub delta: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        BackendEvent, BackendEventType, BackendTask, TaskProgress, TaskResult, TaskStatus, TaskType,
    };
    use crate::errors::BackendError;
    use serde_json::json;

    #[test]
    fn serializes_task_enums_as_stable_strings() {
        use crate::models::task::StreamDelta;
        assert_eq!(
            serde_json::to_string(&TaskStatus::WaitingForConfirmation).unwrap(),
            "\"waiting_for_confirmation\""
        );
        assert_eq!(
            serde_json::to_string(&TaskType::WikiCompile).unwrap(),
            "\"wiki_compile\""
        );
        assert_eq!(
            serde_json::to_string(&BackendEventType::ConfirmationRequested).unwrap(),
            "\"confirmation_requested\""
        );
        assert_eq!(
            serde_json::to_string(&BackendEventType::TaskStreamOutput).unwrap(),
            "\"task_stream_output\""
        );
        // Full delta round-trips with camelCase and omits a missing route.
        let delta = StreamDelta {
            delta: "Hi".into(),
            route: None,
        };
        let value = serde_json::to_value(&delta).unwrap();
        assert_eq!(value["delta"], json!("Hi"));
        assert!(value.get("route").is_none());
        let with_route = StreamDelta {
            delta: "Hi".into(),
            route: Some("byok".into()),
        };
        assert_eq!(
            serde_json::to_value(&with_route).unwrap()["route"],
            json!("byok")
        );
    }

    #[test]
    fn serializes_backend_task_with_camel_case_fields() {
        let task = BackendTask {
            id: "task-1".to_string(),
            task_type: TaskType::GraphBuild,
            project_id: Some("project-1".to_string()),
            title: "Build graph".to_string(),
            status: TaskStatus::Running,
            progress: Some(TaskProgress {
                current: 1,
                total: Some(3),
                label: Some("Scanning wiki".to_string()),
            }),
            started_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:01Z".to_string(),
            completed_at: None,
            cancellable: true,
            log_path: Some(".app/tasks/task-1.log".to_string()),
            result: Some(TaskResult {
                summary: "Started".to_string(),
                affected_paths: vec![".app/graph-cache.json".to_string()],
                pending_action: None,
            }),
            error: Some(BackendError::new("TASK_TEST", "Example", true, false)),
        };

        let value = serde_json::to_value(task).unwrap();

        assert_eq!(value["taskType"], json!("graph_build"));
        assert_eq!(value["projectId"], json!("project-1"));
        assert_eq!(value["startedAt"], json!("2026-06-19T00:00:00Z"));
        assert_eq!(value["progress"]["current"], json!(1));
        assert_eq!(
            value["result"]["affectedPaths"][0],
            json!(".app/graph-cache.json")
        );
        assert_eq!(value["error"]["userActionRequired"], json!(false));
        assert!(value.get("task_type").is_none());
    }

    #[test]
    fn serializes_backend_event_with_typed_event_name() {
        let event = BackendEvent {
            event_id: "event-1".to_string(),
            event_type: BackendEventType::TaskUpdated,
            project_id: Some("project-1".to_string()),
            task_id: Some("task-1".to_string()),
            timestamp: "2026-06-19T00:00:00Z".to_string(),
            payload: json!({ "status": "running" }),
        };

        let value = serde_json::to_value(event).unwrap();

        assert_eq!(value["eventType"], json!("task_updated"));
        assert_eq!(value["projectId"], json!("project-1"));
        assert_eq!(value["taskId"], json!("task-1"));
        assert!(value.get("event_type").is_none());
    }
}

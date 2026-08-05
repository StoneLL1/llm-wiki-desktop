use serde::{Deserialize, Serialize};

use crate::errors::BackendError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackendTask {
    pub id: String,
    pub task_type: TaskType,
    pub project_id: Option<String>,
    /// Optional operation identity shared by tasks created from one user
    /// action. Existing task files omit this field, so it must remain
    /// backwards-compatible and absent for non-grouped tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
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
    SourceAiOrganize,
    ProjectInventory,
    Workflow,
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
    Interrupted,
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
    pub reference: Option<TaskResultReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<crate::models::confirmation::PendingAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TaskResultReference {
    ImportPreview {
        session_id: String,
        item_id: String,
    },
    ImportV2SessionPreview {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        batch_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completion: Option<crate::models::import_v2::ImportCompletion>,
    },
    Compile {
        result: crate::models::compile::CompileResult,
    },
    SourceAiOrganize {
        source_id: String,
        base_version_id: String,
        base_markdown_hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route: Option<crate::models::compile::CompileRoutePreference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<crate::models::agent::AgentKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<crate::models::llm::LlmProviderKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_root_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved_engine: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved_model: Option<String>,
    },
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
    /// Safe, structured progress for Agent/LLM runs. Raw hidden reasoning,
    /// tool arguments, file contents, and command output never cross this
    /// boundary. Frontend channel: `task://activity`.
    TaskActivity,
    /// Persisted structured workflow state changed. The payload is the full
    /// bounded [`WorkflowRun`](crate::models::workflow::WorkflowRun) snapshot.
    WorkflowUpdated,
    ImportSessionPatch,
}

/// Payload of a [`BackendEventType::TaskStreamOutput`] event. `delta` is the
/// incremental text (the frontend concatenates); `route` is an optional
/// internal stream label ("chat-agent", "chat-byok", or "task-agent") so the
/// appropriate live surface can render it before the persisted result exists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamDelta {
    pub delta: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

/// A safe presentation event for an Agent run. Raw hidden reasoning, tool
/// arguments, file contents, and command output never cross this boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TaskActivity {
    Phase {
        name: String,
        status: TaskActivityStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    Thinking {
        status: TaskActivityStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    ToolCall {
        call_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    ToolResult {
        call_id: String,
        success: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskActivityStatus {
    Started,
    Completed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::{
        BackendEvent, BackendEventType, BackendTask, TaskProgress, TaskResult, TaskResultReference,
        TaskStatus, TaskType,
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
            batch_id: None,
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
                reference: None,
                pending_action: None,
            }),
            error: Some(BackendError::new("TASK_TEST", "Example", true, false)),
        };

        let value = serde_json::to_value(task).unwrap();

        assert_eq!(value["taskType"], json!("graph_build"));
        assert_eq!(value["projectId"], json!("project-1"));
        assert!(value.get("batchId").is_none());
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
    fn accepts_a_persisted_task_batch_identity() {
        let value = serde_json::json!({
            "id": "task-1",
            "taskType": "import",
            "projectId": "project-1",
            "batchId": "batch-1",
            "title": "Import notes.md",
            "status": "queued",
            "progress": null,
            "startedAt": "2026-07-15T00:00:00Z",
            "updatedAt": "2026-07-15T00:00:00Z",
            "completedAt": null,
            "cancellable": true,
            "logPath": null,
            "result": null,
            "error": null
        });
        let task: BackendTask = serde_json::from_value(value).unwrap();
        assert_eq!(task.batch_id.as_deref(), Some("batch-1"));
    }

    #[test]
    fn import_v2_batch_reference_round_trips_and_accepts_legacy_shape() {
        let reference = TaskResultReference::ImportV2SessionPreview {
            session_id: "session-1".into(),
            batch_id: Some("batch-1".into()),
            completion: None,
        };
        let value = serde_json::to_value(&reference).unwrap();
        assert_eq!(value["type"], json!("import_v2_session_preview"));
        assert_eq!(value["sessionId"], json!("session-1"));
        assert_eq!(value["batchId"], json!("batch-1"));

        let legacy: TaskResultReference = serde_json::from_value(json!({
            "type": "import_v2_session_preview",
            "sessionId": "session-1"
        }))
        .unwrap();
        assert_eq!(
            legacy,
            TaskResultReference::ImportV2SessionPreview {
                session_id: "session-1".into(),
                batch_id: None,
                completion: None,
            }
        );
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

    #[test]
    fn source_ai_task_reference_keeps_candidate_binding_typed() {
        let reference = TaskResultReference::SourceAiOrganize {
            source_id: "source-中文".into(),
            base_version_id: "version-1".into(),
            base_markdown_hash: "abc123".into(),
            candidate_id: Some("candidate-1".into()),
            route: Some(crate::models::compile::CompileRoutePreference::Byok),
            agent: None,
            provider: Some(crate::models::llm::LlmProviderKind::OpenAi),
            custom_instructions: Some("保留引文".into()),
            project_root_path: Some(r"D:\知识库".into()),
            resolved_engine: Some("open_ai".into()),
            resolved_model: Some("gpt-source".into()),
        };
        let value = serde_json::to_value(&reference).unwrap();
        assert_eq!(value["type"], json!("source_ai_organize"));
        assert_eq!(value["sourceId"], json!("source-中文"));
        assert_eq!(value["baseVersionId"], json!("version-1"));
        assert_eq!(value["baseMarkdownHash"], json!("abc123"));
        assert_eq!(value["candidateId"], json!("candidate-1"));
        assert_eq!(value["route"], json!("byok"));
        assert_eq!(value["provider"], json!("open_ai"));
        assert_eq!(value["customInstructions"], json!("保留引文"));
        assert_eq!(value["projectRootPath"], json!(r"D:\知识库"));
        assert_eq!(value["resolvedEngine"], json!("open_ai"));
        assert_eq!(value["resolvedModel"], json!("gpt-source"));
        assert_eq!(
            serde_json::from_value::<TaskResultReference>(value).unwrap(),
            reference
        );
        assert_eq!(
            serde_json::to_string(&TaskType::SourceAiOrganize).unwrap(),
            "\"source_ai_organize\""
        );
    }
}

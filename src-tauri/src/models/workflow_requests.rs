//! Stable Workflow IPC request shapes, shared by commands and interaction services.
use serde::Deserialize;

use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowKind, WorkflowRouteSelection, WorkflowScope,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub kind: WorkflowKind,
    pub scope: Option<WorkflowScope>,
    pub route_selection: Option<WorkflowRouteSelection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub preparation_id: String,
    pub preparation_revision: String,
    #[serde(default)]
    pub acknowledge_restricted_content: bool,
    #[serde(default)]
    pub acknowledge_remote_provider: bool,
    #[serde(default)]
    pub retry_of_task_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkflowRunsRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub workflow_kind: Option<WorkflowKind>,
    pub display_status: Option<WorkflowDisplayStatus>,
    pub cursor: Option<String>,
    #[serde(default = "default_history_page_limit")]
    pub limit: usize,
}

pub(crate) const DEFAULT_HISTORY_PAGE_LIMIT: usize = 50;

fn default_history_page_limit() -> usize {
    DEFAULT_HISTORY_PAGE_LIMIT
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFileDiffRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
    pub pending_action_id: String,
    pub file_id: String,
    #[serde(default)]
    pub cursor: Option<usize>,
    #[serde(default = "default_diff_chunk_bytes")]
    pub limit_bytes: usize,
}

pub(crate) const DEFAULT_DIFF_CHUNK_BYTES: usize = 64 * 1024;

fn default_diff_chunk_bytes() -> usize {
    DEFAULT_DIFF_CHUNK_BYTES
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderQueuedWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
    pub before_task_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkflowActionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
    pub action_id: String,
}

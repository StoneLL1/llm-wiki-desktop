//! Bounded project history paging; no preparation or confirmation hydration.
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::workflow::{WorkflowDisplayStatus, WorkflowKind, WorkflowRunHistoryPage};
use crate::models::workflow_requests::{ListWorkflowRunsRequest, DEFAULT_HISTORY_PAGE_LIMIT};

const MAX_HISTORY_PAGE_LIMIT: usize = 100;
const MAX_HISTORY_PAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct WorkflowHistoryCursor {
    canonical_identity_key: String,
    identity_revision: String,
    workflow_kind: Option<WorkflowKind>,
    display_status: Option<WorkflowDisplayStatus>,
    started_at: String,
    task_id: String,
}

pub(crate) fn list_workflow_runs_for_state(
    state: &AppState,
    request: ListWorkflowRunsRequest,
) -> Result<WorkflowRunHistoryPage, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let identity = crate::services::project_identity(&context.root)
        .map_err(|message| workflow_error("WORKFLOW_IDENTITY_FAILED", message))?;
    let cursor = request
        .cursor
        .as_deref()
        .map(decode_history_cursor)
        .transpose()?;
    if let Some(cursor) = &cursor {
        if cursor.canonical_identity_key != identity.canonical_identity_key
            || cursor.identity_revision != identity.identity_revision
            || cursor.workflow_kind != request.workflow_kind
            || cursor.display_status != request.display_status
        {
            return Err(workflow_error(
                "WORKFLOW_CURSOR_SCOPE_MISMATCH",
                "The workflow history cursor belongs to a different project identity or filter.",
            ));
        }
    }
    let limit = if request.limit == 0 {
        DEFAULT_HISTORY_PAGE_LIMIT
    } else {
        request.limit.clamp(1, MAX_HISTORY_PAGE_LIMIT)
    };
    let after = cursor
        .as_ref()
        .map(|cursor| (cursor.started_at.as_str(), cursor.task_id.as_str()));
    let (mut runs, mut has_more) = state.task_service.page_workflow_runs(
        &identity.canonical_identity_key,
        &identity.identity_revision,
        request.workflow_kind.clone(),
        request.display_status.clone(),
        after,
        limit,
    );
    loop {
        let next_cursor = if has_more {
            runs.last()
                .map(|run| {
                    encode_history_cursor(&WorkflowHistoryCursor {
                        canonical_identity_key: identity.canonical_identity_key.clone(),
                        identity_revision: identity.identity_revision.clone(),
                        workflow_kind: request.workflow_kind.clone(),
                        display_status: request.display_status.clone(),
                        started_at: run.started_at.clone(),
                        task_id: run.task_id.clone(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        let page = WorkflowRunHistoryPage {
            runs: runs.clone(),
            next_cursor,
        };
        if serde_json::to_vec(&page).is_ok_and(|payload| payload.len() <= MAX_HISTORY_PAGE_BYTES) {
            return Ok(page);
        }
        if runs.len() <= 1 {
            return Err(workflow_error(
                "WORKFLOW_HISTORY_PAGE_TOO_LARGE",
                "A workflow history summary exceeds the response size limit.",
            ));
        }
        runs.pop();
        has_more = true;
    }
}

fn encode_history_cursor(cursor: &WorkflowHistoryCursor) -> Result<String, BackendError> {
    let bytes = serde_json::to_vec(cursor).map_err(|error| {
        workflow_error(
            "WORKFLOW_CURSOR_INVALID",
            format!("Could not encode workflow cursor: {error}"),
        )
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn decode_history_cursor(cursor: &str) -> Result<WorkflowHistoryCursor, BackendError> {
    if cursor.is_empty() || !cursor.is_ascii() || cursor.len() % 2 != 0 {
        return Err(workflow_error(
            "WORKFLOW_CURSOR_INVALID",
            "The workflow history cursor is invalid.",
        ));
    }
    let bytes = (0..cursor.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&cursor[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            workflow_error(
                "WORKFLOW_CURSOR_INVALID",
                "The workflow history cursor is invalid.",
            )
        })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        workflow_error(
            "WORKFLOW_CURSOR_INVALID",
            "The workflow history cursor is invalid.",
        )
    })
}

fn workflow_error(code: &str, message: impl Into<String>) -> BackendError {
    BackendError::new(code, message, true, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_cursor_round_trips_identity_revision_and_filters() {
        let cursor = WorkflowHistoryCursor {
            canonical_identity_key: "identity-中文|safe".into(),
            identity_revision: "revision-a".into(),
            workflow_kind: Some(WorkflowKind::HealthCheck),
            display_status: Some(WorkflowDisplayStatus::Failed),
            started_at: "2026-08-10T00:00:00Z".into(),
            task_id: "task|unicode-任务".into(),
        };
        let encoded = encode_history_cursor(&cursor).unwrap();
        assert_eq!(decode_history_cursor(&encoded).unwrap(), cursor);
        assert!(decode_history_cursor("not-hex").is_err());
        assert!(decode_history_cursor("中文").is_err());
    }
}

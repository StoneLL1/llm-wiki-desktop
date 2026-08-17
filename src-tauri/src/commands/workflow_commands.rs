use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{ConfirmationExecution, PendingActionType, StoredPendingAction};
use crate::models::workflow::{
    WorkflowDecisionCounts, WorkflowDecisionReview, WorkflowDisplayStatus, WorkflowErrorSummary,
    WorkflowFileDiff, WorkflowFileDiffKind, WorkflowFileDiffPage, WorkflowKind,
    WorkflowPreparation, WorkflowPrerequisiteAction, WorkflowProjectMutationState,
    WorkflowRouteSelection, WorkflowRun, WorkflowRunHistoryPage, WorkflowRunPage, WorkflowScope,
    WorkflowStartOutcome, WorkflowsOverview,
};
use crate::services::{
    resolve_workflow_persistence_binding, restore_agent_lint_repair_confirmation,
    restore_generate_content_confirmation, restore_update_wiki_confirmation,
    AgentLintRepairExecutionServices, CompileExecutionServices, GenerateContentExecutionServices,
    PrepareWorkflowInput, UpdateWikiExecutionServices, WorkflowPersistenceBinding,
    WorkflowPreparationEnvironment,
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

const DEFAULT_HISTORY_PAGE_LIMIT: usize = 50;
const MAX_HISTORY_PAGE_LIMIT: usize = 100;
const MAX_HISTORY_PAGE_BYTES: usize = 1024 * 1024;

fn default_history_page_limit() -> usize {
    DEFAULT_HISTORY_PAGE_LIMIT
}

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

const DEFAULT_DIFF_CHUNK_BYTES: usize = 64 * 1024;
const MAX_DIFF_CHUNK_BYTES: usize = 240 * 1024;
const MAX_DIFF_RESPONSE_BYTES: usize = 256 * 1024;
const LARGE_DIFF_BYTES: usize = 256 * 1024;
const LARGE_REVIEW_BYTES: usize = 1024 * 1024;
const SLOW_REVIEW_HYDRATION: Duration = Duration::from_millis(50);

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

#[tauri::command]
pub fn get_workflows_overview(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<WorkflowsOverview, BackendError> {
    if request.project_id.trim().is_empty() && request.project_root_path.trim().is_empty() {
        return Ok(state.workflow_service.no_project_overview());
    }
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = state.resolve_workflow_access(&context)?;
    state.workflow_service.project_overview(
        &context,
        access,
        &state.settings_service,
        &state.secret_service,
        &state.agent_service,
        &state.task_service,
    )
}

#[tauri::command]
pub fn prepare_workflow(
    state: State<'_, AppState>,
    request: PrepareWorkflowRequest,
) -> Result<WorkflowPreparation, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = state.resolve_workflow_access(&context)?;
    state.workflow_service.prepare(
        &WorkflowPreparationEnvironment {
            context: &context,
            access,
            settings_service: &state.settings_service,
            secret_service: &state.secret_service,
            agent_service: &state.agent_service,
        },
        PrepareWorkflowInput {
            kind: request.kind,
            scope: request.scope,
            route_selection: request.route_selection,
        },
    )
}

#[tauri::command]
pub fn start_workflow(
    state: State<'_, AppState>,
    request: StartWorkflowRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    let outcome = state.with_current_project_task_access(
        &request.project_id,
        &request.project_root_path,
        |permit| {
            state.workflow_service.enqueue_with_acknowledgements(
                permit,
                &state.settings_service,
                &state.secret_service,
                &state.agent_service,
                &state.task_service,
                &request.preparation_id,
                &request.preparation_revision,
                request.acknowledge_restricted_content,
                request.acknowledge_remote_provider,
            )
        },
    )?;
    if let WorkflowStartOutcome::Created { run } = &outcome {
        if run.display_status == WorkflowDisplayStatus::Running {
            state.workflow_service.dispatch_claimed_run_with_settings(
                &state.task_service,
                &state.settings_service,
                run,
            )?;
        }
    }
    Ok(outcome)
}

#[tauri::command]
pub fn list_workflow_runs(
    state: State<'_, AppState>,
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
    if cursor.is_empty() || cursor.len() % 2 != 0 {
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

#[tauri::command]
pub fn get_workflow_run(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    let context = require_workflow_project(&state, &request)?;
    let mut run = workflow_run(&state, &request.task_id)?;
    let next = if let Some(pending) = run.pending_action.clone() {
        state.with_workflow_access(&context, |_| {
            let hydration_started = Instant::now();
            match hydrate_workflow_confirmation(&state, &context, &run, &pending, false) {
                Ok(review) => {
                    run.decision_review = Some(prepare_decision_review_for_transport(
                        review,
                        hydration_started.elapsed(),
                    ));
                    Ok(None)
                }
                Err(error) => {
                    let (interrupted, next) =
                        interrupt_unconfirmable_workflow(&state, &context, &run, &pending, error)?;
                    run = interrupted;
                    Ok(next)
                }
            }
        })?
    } else {
        None
    };
    if let Some(next) = next {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            &next,
        )?;
    }
    Ok(run)
}

#[tauri::command]
pub fn get_workflow_file_diff(
    state: State<'_, AppState>,
    request: WorkflowFileDiffRequest,
) -> Result<WorkflowFileDiffPage, BackendError> {
    let run_request = WorkflowRunRequest {
        project_id: request.project_id,
        project_root_path: request.project_root_path,
        task_id: request.task_id,
    };
    let context = require_workflow_project(&state, &run_request)?;
    let run = workflow_run(&state, &run_request.task_id)?;
    let start = request.cursor.unwrap_or(0);
    let limit = if request.limit_bytes == 0 {
        DEFAULT_DIFF_CHUNK_BYTES
    } else {
        request.limit_bytes.clamp(1, MAX_DIFF_CHUNK_BYTES)
    };
    let terminal_repair_diff = matches!(
        &run.result,
        Some(crate::models::workflow::WorkflowResult::AgentLintRepair {
            diff_available: true,
            ..
        })
    ) && run.pending_action.is_none();
    if terminal_repair_diff {
        let mut bounded_limit = limit;
        loop {
            let page = state.with_workflow_access(&context, |_| {
                crate::services::agent_lint_repair_terminal_file_diff_page(
                    &context,
                    &run,
                    &agent_lint_repair_services(&state),
                    &request.file_id,
                    start,
                    bounded_limit,
                )?
                .ok_or_else(|| {
                    workflow_error(
                        "WORKFLOW_DIFF_NOT_FOUND",
                        "The requested terminal repair diff does not exist.",
                    )
                })
            })?;
            if serde_json::to_vec(&page)
                .is_ok_and(|payload| payload.len() <= MAX_DIFF_RESPONSE_BYTES)
            {
                return Ok(page);
            }
            if bounded_limit == 1 {
                return Err(workflow_error(
                    "WORKFLOW_DIFF_RESPONSE_TOO_LARGE",
                    "The workflow diff metadata exceeds the response size limit.",
                ));
            }
            bounded_limit = (bounded_limit / 2).max(1);
        }
    }
    let pending = run.pending_action.as_ref().ok_or_else(|| {
        workflow_error(
            "WORKFLOW_CONFIRMATION_STALE",
            "The workflow is no longer waiting for confirmation.",
        )
    })?;
    if run.display_status != WorkflowDisplayStatus::WaitingForConfirmation
        || pending.id != request.pending_action_id
    {
        return Err(workflow_error(
            "WORKFLOW_CONFIRMATION_STALE",
            "The pending workflow action changed before this diff was read.",
        ));
    }
    if run.kind == WorkflowKind::UpdateWiki {
        let mut bounded_limit = limit;
        loop {
            let page = state.with_workflow_access(&context, |_| {
                validate_workflow_confirmation(&state, &context, &run, pending)?;
                let workflow = state
                    .task_service
                    .workflow_execution_state(&run.task_id)
                    .ok_or_else(|| {
                        workflow_error(
                            "WORKFLOW_CANDIDATE_STALE",
                            "The persisted workflow candidate is no longer valid.",
                        )
                    })?;
                crate::services::update_wiki_file_diff_page_for_workflow(
                    &run.task_id,
                    &context.root,
                    &workflow,
                    &request.file_id,
                    start,
                    bounded_limit,
                )?
                .ok_or_else(|| {
                    workflow_error(
                        "WORKFLOW_DIFF_NOT_FOUND",
                        "The requested workflow diff does not exist.",
                    )
                })
            })?;
            if serde_json::to_vec(&page)
                .is_ok_and(|payload| payload.len() <= MAX_DIFF_RESPONSE_BYTES)
            {
                return Ok(page);
            }
            if bounded_limit == 1 {
                return Err(workflow_error(
                    "WORKFLOW_DIFF_RESPONSE_TOO_LARGE",
                    "The workflow diff metadata exceeds the response size limit.",
                ));
            }
            bounded_limit = (bounded_limit / 2).max(1);
        }
    }
    if matches!(
        run.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    ) {
        let mut bounded_limit = limit;
        loop {
            let page = state.with_workflow_access(&context, |_| {
                validate_workflow_confirmation(&state, &context, &run, pending)?;
                crate::services::agent_lint_repair_file_diff_page(
                    &context,
                    &run,
                    &agent_lint_repair_services(&state),
                    &request.file_id,
                    start,
                    bounded_limit,
                )?
                .ok_or_else(|| {
                    workflow_error(
                        "WORKFLOW_DIFF_NOT_FOUND",
                        "The requested repair diff does not exist.",
                    )
                })
            })?;
            if serde_json::to_vec(&page)
                .is_ok_and(|payload| payload.len() <= MAX_DIFF_RESPONSE_BYTES)
            {
                return Ok(page);
            }
            if bounded_limit == 1 {
                return Err(workflow_error(
                    "WORKFLOW_DIFF_RESPONSE_TOO_LARGE",
                    "The workflow diff metadata exceeds the response size limit.",
                ));
            }
            bounded_limit = (bounded_limit / 2).max(1);
        }
    }
    let file = state.with_workflow_access(&context, |_| {
        let review = normalize_decision_review_files(hydrate_workflow_confirmation(
            &state, &context, &run, pending, true,
        )?);
        review
            .file_diffs
            .into_iter()
            .find(|file| file.file_id == request.file_id)
            .ok_or_else(|| {
                workflow_error(
                    "WORKFLOW_DIFF_NOT_FOUND",
                    "The requested workflow diff does not exist.",
                )
            })
    })?;
    paginate_workflow_diff(file, start, limit)
}

fn paginate_workflow_diff(
    file: WorkflowFileDiff,
    start: usize,
    limit: usize,
) -> Result<WorkflowFileDiffPage, BackendError> {
    let diff = file.diff.ok_or_else(|| {
        workflow_error(
            "WORKFLOW_DIFF_NOT_FOUND",
            "The requested workflow diff is unavailable.",
        )
    })?;
    if start > diff.len() || !diff.is_char_boundary(start) {
        return Err(workflow_error(
            "WORKFLOW_DIFF_CURSOR_INVALID",
            "The workflow diff cursor is invalid.",
        ));
    }
    let mut end = start.saturating_add(limit).min(diff.len());
    while end > start && !diff.is_char_boundary(end) {
        end -= 1;
    }
    let minimum_end = if start < diff.len() {
        let mut boundary = start + 1;
        while !diff.is_char_boundary(boundary) {
            boundary += 1;
        }
        boundary
    } else {
        start
    };
    end = end.max(minimum_end);
    let build_page = |end: usize| {
        let truncated = end < diff.len();
        WorkflowFileDiffPage {
            file_id: file.file_id.clone(),
            path: file.path.clone(),
            kind: file.kind,
            diff: diff[start..end].to_string(),
            next_cursor: truncated.then_some(end),
            truncated,
        }
    };
    loop {
        let page = build_page(end);
        if serde_json::to_vec(&page).is_ok_and(|payload| payload.len() <= MAX_DIFF_RESPONSE_BYTES) {
            return Ok(page);
        }
        if end == minimum_end {
            return Err(workflow_error(
                "WORKFLOW_DIFF_RESPONSE_TOO_LARGE",
                "The workflow diff metadata exceeds the response size limit.",
            ));
        }
        end = start + (end - start) / 2;
        while end > start && !diff.is_char_boundary(end) {
            end -= 1;
        }
        end = end.max(minimum_end);
    }
}

fn normalize_decision_review_files(mut review: WorkflowDecisionReview) -> WorkflowDecisionReview {
    for (index, file) in review.file_diffs.iter_mut().enumerate() {
        file.file_id = format!("file-{index:08x}");
        file.diff_bytes = file.diff.as_ref().map_or(file.diff_bytes, String::len);
    }
    review
}

fn prepare_decision_review_for_transport(
    review: WorkflowDecisionReview,
    hydration_elapsed: Duration,
) -> WorkflowDecisionReview {
    let mut review = normalize_decision_review_files(review);
    let large_file = review
        .file_diffs
        .iter()
        .any(|file| file.diff_bytes > LARGE_DIFF_BYTES);
    let large_review =
        serde_json::to_vec(&review).map_or(true, |payload| payload.len() > LARGE_REVIEW_BYTES);
    if large_file || large_review || hydration_elapsed > SLOW_REVIEW_HYDRATION {
        for file in &mut review.file_diffs {
            file.diff = None;
        }
    }
    review
}

pub(crate) fn interrupt_unconfirmable_workflow(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    pending: &crate::models::workflow::WorkflowPendingAction,
    error: BackendError,
) -> Result<(WorkflowRun, Option<WorkflowRun>), BackendError> {
    let _ = state
        .confirmation_registry
        .cancel_workflow_binding(context, run, pending);
    let mut repair_result = None;
    let mut project_mutation_state = WorkflowProjectMutationState::Unknown;
    match &run.kind {
        WorkflowKind::UpdateWiki => {
            let _ = crate::services::discard_update_wiki_candidate(&run.task_id);
        }
        WorkflowKind::GenerateContent => {
            let _ = crate::services::discard_generate_content_candidate(&run.task_id);
        }
        WorkflowKind::HealthCheck => {
            if matches!(
                &run.operation,
                crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
            ) {
                match crate::services::rollback_and_discard_agent_lint_repair_candidate(
                    context,
                    run,
                    &agent_lint_repair_services(state),
                ) {
                    Ok(result) => {
                        project_mutation_state = if matches!(
                            result,
                            crate::models::workflow::WorkflowResult::AgentLintRepair {
                                outcome: crate::models::lint::AgentLintRepairOutcome::RolledBack,
                                ..
                            }
                        ) {
                            WorkflowProjectMutationState::RolledBack
                        } else {
                            WorkflowProjectMutationState::NotModified
                        };
                        repair_result = Some(result);
                    }
                    Err(_) => {
                        project_mutation_state = WorkflowProjectMutationState::Modified;
                        repair_result =
                            Some(crate::services::agent_lint_repair_interrupted_result(run));
                    }
                }
            }
        }
    }
    state
        .workflow_service
        .coordinator
        .interrupt_invalid_confirmation_with_result(
            &state.task_service,
            &run.task_id,
            WorkflowErrorSummary {
                code: error.code,
                message_key: error.message,
                recoverable: false,
                user_action_required: true,
                suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
                project_mutation_state,
            },
            repair_result,
        )
        .map_err(|message| workflow_error("WORKFLOW_CONFIRMATION_RECOVERY_FAILED", message))
}

fn hydrate_workflow_confirmation(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    pending: &crate::models::workflow::WorkflowPendingAction,
    include_update_wiki_diffs: bool,
) -> Result<WorkflowDecisionReview, BackendError> {
    let stored = validate_workflow_confirmation(state, context, run, pending)?;
    let affected = stored.action.affected_paths.len() as u32;
    let counts = match stored.action.action_type {
        PendingActionType::OverwriteFile => WorkflowDecisionCounts {
            overwritten: affected,
            ..WorkflowDecisionCounts::default()
        },
        PendingActionType::DeleteFile => WorkflowDecisionCounts {
            deleted: affected,
            ..WorkflowDecisionCounts::default()
        },
        _ => WorkflowDecisionCounts {
            modified: affected,
            ..WorkflowDecisionCounts::default()
        },
    };
    let file_diffs = stored
        .action
        .preview
        .as_ref()
        .and_then(|preview| preview.diff.as_ref())
        .map(|diff| {
            vec![WorkflowFileDiff {
                file_id: String::new(),
                path: stored
                    .action
                    .affected_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "candidate".into()),
                diff_bytes: diff.len(),
                diff: Some(diff.clone()),
                kind: WorkflowFileDiffKind::TwoWay,
            }]
        })
        .unwrap_or_default();
    if matches!(
        run.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    ) {
        crate::services::agent_lint_repair_decision_review(
            context,
            run,
            &agent_lint_repair_services(state),
            include_update_wiki_diffs,
        )
        .ok_or_else(|| {
            workflow_error(
                "WORKFLOW_CANDIDATE_STALE",
                "The persisted Agent lint repair candidate is no longer valid.",
            )
        })
    } else if run.kind == WorkflowKind::UpdateWiki {
        let workflow = state
            .task_service
            .workflow_execution_state(&run.task_id)
            .ok_or_else(|| {
                workflow_error(
                    "WORKFLOW_CANDIDATE_STALE",
                    "The persisted workflow candidate is no longer valid.",
                )
            })?;
        if include_update_wiki_diffs {
            crate::services::update_wiki_decision_review_for_workflow(
                &run.task_id,
                &context.root,
                &workflow,
            )
        } else {
            let summary = crate::services::update_wiki_decision_review_summary_for_workflow(
                &run.task_id,
                &context.root,
                &workflow,
            )
            .ok_or_else(|| {
                workflow_error(
                    "WORKFLOW_CANDIDATE_STALE",
                    "The persisted workflow candidate is no longer valid.",
                )
            })?;
            if crate::services::update_wiki_review_can_inline(
                &summary,
                LARGE_DIFF_BYTES,
                LARGE_REVIEW_BYTES,
            ) {
                crate::services::update_wiki_decision_review_for_workflow(
                    &run.task_id,
                    &context.root,
                    &workflow,
                )
            } else {
                Some(summary)
            }
        }
        .ok_or_else(|| {
            workflow_error(
                "WORKFLOW_CANDIDATE_STALE",
                "The persisted workflow candidate is no longer valid.",
            )
        })
    } else {
        Ok(WorkflowDecisionReview {
            reason: stored.action.message,
            counts,
            user_edits_detected: stored.action.action_type == PendingActionType::MergeConflict,
            file_diffs,
        })
    }
}

fn validate_workflow_confirmation(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    pending: &crate::models::workflow::WorkflowPendingAction,
) -> Result<StoredPendingAction, BackendError> {
    match &run.kind {
        WorkflowKind::UpdateWiki => restore_update_wiki_confirmation(
            context,
            run,
            &state.task_service,
            &state.confirmation_registry,
        )?,
        WorkflowKind::GenerateContent => restore_generate_content_confirmation(
            context,
            run,
            &state.task_service,
            &state.confirmation_registry,
        )?,
        WorkflowKind::HealthCheck => {
            if matches!(
                run.operation,
                crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
            ) {
                restore_agent_lint_repair_confirmation(
                    context,
                    run,
                    &state.confirmation_registry,
                    &state.settings_service,
                    &state.task_service,
                )?;
            }
        }
    }
    let stored = state.confirmation_registry.peek(&pending.id)?;
    if !crate::models::confirmation::workflow_execution_matches(
        &run.kind,
        stored.execution.as_ref(),
        context,
        run,
        pending,
    ) {
        return Err(workflow_error(
            "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
            "The confirmation execution plan does not match this workflow run.",
        ));
    }
    Ok(stored)
}

#[tauri::command]
pub fn cancel_workflow_run(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    require_workflow_project(&state, &request)?;
    let (run, next) = state.with_current_project_task_access(
        &request.project_id,
        &request.project_root_path,
        |permit| {
            cancel_or_discard_workflow(
                &state,
                permit.context(),
                &request.task_id,
                false,
                permit.workflow_access().persistence
                    == crate::models::workflow::WorkflowPersistenceMode::Persistent,
            )
        },
    )?;
    dispatch_next(&state, next)?;
    Ok(run)
}

#[tauri::command]
pub fn undo_cancel_queued_workflow(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    require_workflow_project(&state, &request)?;
    let before = workflow_run(&state, &request.task_id)?;
    let is_repair = matches!(
        before.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    );
    let (run, claimed) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, current| {
            let access = permit.workflow_access();
            if !is_repair {
                return state
                    .workflow_service
                    .coordinator
                    .undo_cancel(&state.task_service, &request.task_id)
                    .map_err(|message| workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message));
            }
            crate::commands::lint_commands::validate_agent_lint_repair_replay_facts(
                &state,
                current,
                &before,
                &access,
                crate::commands::lint_commands::AgentLintRepairReplayIntent::Undo,
            )?;
            let (held, _) = state
                .workflow_service
                .coordinator
                .undo_cancel_pending_approval(&state.task_service, &request.task_id)
                .map_err(|message| workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message))?;
            if let Err(error) =
                crate::commands::lint_commands::restore_agent_lint_repair_attestation_for_run(
                    &state, &held,
                )
            {
                let _ = state
                    .workflow_service
                    .coordinator
                    .cancel_created_without_undo_and_claim_next(&state.task_service, &held.task_id);
                return Err(error);
            }
            match state
                .workflow_service
                .coordinator
                .release_initial_approval_hold_and_claim_next(&state.task_service, &held.task_id)
            {
                Ok(result) => Ok(result),
                Err(message) => {
                    let _ =
                    crate::commands::lint_commands::cancel_agent_lint_repair_attestation_for_run(
                        &state, &held,
                    );
                    let _ = state
                        .workflow_service
                        .coordinator
                        .cancel_created_without_undo_and_claim_next(
                            &state.task_service,
                            &held.task_id,
                        );
                    Err(workflow_error("WORKFLOW_UNDO_CANCEL_FAILED", message))
                }
            }
        },
    )?;
    dispatch_next(&state, claimed)?;
    Ok(run)
}

#[tauri::command]
pub fn reorder_queued_workflow(
    state: State<'_, AppState>,
    request: ReorderQueuedWorkflowRequest,
) -> Result<WorkflowRunPage, BackendError> {
    let task_request = WorkflowRunRequest {
        project_id: request.project_id,
        project_root_path: request.project_root_path,
        task_id: request.task_id,
    };
    require_workflow_project(&state, &task_request)?;
    if let Some(before_task_id) = request.before_task_id.as_deref() {
        require_workflow_project(
            &state,
            &WorkflowRunRequest {
                project_id: task_request.project_id.clone(),
                project_root_path: task_request.project_root_path.clone(),
                task_id: before_task_id.to_string(),
            },
        )?;
    }
    let runs = state.with_current_project_write_access(
        &task_request.project_id,
        &task_request.project_root_path,
        |_permit, _context| {
            state
                .workflow_service
                .coordinator
                .reorder_queued(
                    &state.task_service,
                    &task_request.task_id,
                    request.before_task_id.as_deref(),
                )
                .map_err(|message| workflow_error("WORKFLOW_REORDER_FAILED", message))
        },
    )?;
    Ok(WorkflowRunPage {
        runs,
        next_cursor: None,
    })
}

#[tauri::command]
pub fn retry_workflow(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    retry_workflow_for_state(&state, request)
}

pub(crate) fn retry_workflow_for_state(
    state: &AppState,
    request: WorkflowRunRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    require_workflow_project(state, &request)?;
    let original = workflow_run(state, &request.task_id)?;
    let repair_retry = matches!(
        original.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    );
    let (outcome, released_claim) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, current| {
            let access = permit.workflow_access();
            let replay = revalidate_workflow_replay_with_access(
                state,
                current,
                &original,
                access,
                super::lint_commands::AgentLintRepairReplayIntent::Retry,
            )?;
            if let Err(error) = replay.eligibility {
                state
                    .workflow_service
                    .coordinator
                    .apply_persistence_and_continue_queued(
                        &state.task_service,
                        &original.canonical_identity_key,
                        &original.identity_revision,
                        &[(original.task_id.clone(), replay.persistence.task_state_root)],
                        false,
                    )
                    .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
                return Err(error);
            }
            let outcome = if repair_retry {
                state.workflow_service.coordinator.retry_pending_approval(
                    &state.task_service,
                    &request.task_id,
                    current.project_id.clone(),
                    current.root.clone(),
                    replay.persistence.task_state_root,
                )
            } else {
                state.workflow_service.coordinator.retry(
                    &state.task_service,
                    &request.task_id,
                    current.project_id.clone(),
                    current.root.clone(),
                    replay.persistence.task_state_root,
                )
            }
            .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
            if repair_retry {
                let WorkflowStartOutcome::Created { run } = outcome else {
                    return Ok((outcome, None));
                };
                if let Err(error) = super::lint_commands::attest_agent_lint_repair_run(state, &run)
                {
                    let (_, next) = state
                        .workflow_service
                        .coordinator
                        .cancel_created_without_undo_and_claim_next(
                            &state.task_service,
                            &run.task_id,
                        )
                        .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
                    if let Some(next) = next {
                        state.workflow_service.dispatch_claimed_run_with_settings(
                            &state.task_service,
                            &state.settings_service,
                            &next,
                        )?;
                    }
                    return Err(error);
                }
                let (released, claimed) = state
                    .workflow_service
                    .coordinator
                    .release_initial_approval_hold_and_claim_next(&state.task_service, &run.task_id)
                    .map_err(|message| workflow_error("WORKFLOW_RETRY_FAILED", message))?;
                return Ok((WorkflowStartOutcome::Created { run: released }, claimed));
            }
            Ok((outcome, None))
        },
    )?;
    let run = match &outcome {
        WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => run,
    };
    if let Some(claimed) = released_claim {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            &claimed,
        )?;
    } else if !repair_retry && matches!(outcome, WorkflowStartOutcome::Created { .. }) {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            run,
        )?;
    }
    Ok(outcome)
}

pub(crate) struct WorkflowReplayValidation {
    pub persistence: WorkflowPersistenceBinding,
    pub eligibility: Result<(), BackendError>,
}

pub(crate) fn revalidate_workflow_replay_with_access(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
    access: crate::services::WorkflowAccessSnapshot,
    repair_intent: super::lint_commands::AgentLintRepairReplayIntent,
) -> Result<WorkflowReplayValidation, BackendError> {
    ensure_workflow_identity(context, run)?;
    if matches!(
        run.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    ) {
        let persistence =
            resolve_workflow_persistence_binding(context, access.persistence.clone())?;
        let eligibility = super::lint_commands::validate_agent_lint_repair_replay_facts(
            state,
            context,
            run,
            &access,
            repair_intent,
        )
        .map_err(|error| {
            BackendError::new(
                "WORKFLOW_REPREPARATION_REQUIRED",
                "Agent lint repair access, route, Git state, or authorized Wiki paths changed. Prepare and approve the repair again.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "action": crate::models::workflow::WorkflowPrerequisiteAction::PrepareAgain,
                "reasonCode": error.code,
            }))
        });
        return Ok(WorkflowReplayValidation {
            persistence,
            eligibility,
        });
    }
    state.require_workflow_content_write_root(context, &run.kind)?;
    let route_selection = run.route.as_ref().and_then(|route| match route {
        crate::models::workflow::WorkflowRoute::Agent { agent, .. } => {
            Some(WorkflowRouteSelection::Agent {
                agent: agent.clone(),
            })
        }
        crate::models::workflow::WorkflowRoute::Byok { provider, .. } => {
            Some(WorkflowRouteSelection::Byok {
                provider: provider.clone(),
            })
        }
        crate::models::workflow::WorkflowRoute::Local { .. } => None,
    });
    if matches!(
        run.route,
        Some(crate::models::workflow::WorkflowRoute::Agent { .. })
    ) {
        // Retry and project-open continuation share this replay validator.
        // Neither boundary may accept the short-lived route-presentation cache
        // when deciding whether an Agent attempt can be created or claimed.
        state.agent_service.invalidate_workflow_route_cache();
    }
    let preparation = state.workflow_service.prepare(
        &WorkflowPreparationEnvironment {
            context,
            access,
            settings_service: &state.settings_service,
            secret_service: &state.secret_service,
            agent_service: &state.agent_service,
        },
        PrepareWorkflowInput {
            kind: run.kind.clone(),
            scope: Some(run.scope.clone()),
            route_selection,
        },
    )?;
    let persistence = resolve_workflow_persistence_binding(
        context,
        preparation.project_access.persistence.clone(),
    )?;
    let eligibility = (|| {
        let blocking = preparation.prerequisites.iter().find(|item| {
            item.blocking
                && !matches!(
                    item.action,
                    crate::models::workflow::WorkflowPrerequisiteAction::AcknowledgeRemoteProvider
                        | crate::models::workflow::WorkflowPrerequisiteAction::AcknowledgeRestrictedContent
                )
        });
        if preparation.baseline.fingerprint != run.baseline_fingerprint
            || preparation.route != run.route
            || blocking.is_some()
        {
            return Err(BackendError::new(
                "WORKFLOW_REPREPARATION_REQUIRED",
                "Project access, inputs, Git state, or execution route changed. Prepare the workflow again.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "action": crate::models::workflow::WorkflowPrerequisiteAction::PrepareAgain,
                "prerequisite": blocking,
            })));
        }
        Ok(())
    })();
    Ok(WorkflowReplayValidation {
        persistence,
        eligibility,
    })
}

#[tauri::command]
pub fn confirm_workflow_action(
    state: State<'_, AppState>,
    request: ConfirmWorkflowActionRequest,
) -> Result<WorkflowRun, BackendError> {
    let run_request = WorkflowRunRequest {
        project_id: request.project_id.clone(),
        project_root_path: request.project_root_path.clone(),
        task_id: request.task_id,
    };
    let context = require_workflow_project(&state, &run_request)?;
    let run = workflow_run(&state, &run_request.task_id)?;
    let pending = run.pending_action.clone().ok_or_else(|| {
        workflow_error(
            "WORKFLOW_CONFIRMATION_NOT_FOUND",
            "The workflow is not waiting for a confirmation.",
        )
    })?;
    if pending.id != request.action_id {
        return Err(workflow_error(
            "WORKFLOW_CONFIRMATION_MISMATCH",
            "The confirmation does not belong to this workflow run.",
        ));
    }
    if matches!(
        run.operation,
        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
    ) {
        let settings = state.settings_service.read_settings(&context)?;
        let selected_agent = match run.route.as_ref() {
            Some(crate::models::workflow::WorkflowRoute::Agent { agent, .. }) => *agent,
            _ => {
                return Err(workflow_error(
                    "LINT_AGENT_ROUTE_REQUIRED",
                    "Agent lint repair has no exact Agent route.",
                ))
            }
        };
        let services = agent_lint_repair_services(&state);
        let authority_run = run.clone();
        let result = crate::services::confirm_agent_lint_repair_review_authorized(
            &context,
            &run.task_id,
            &services,
            &settings.language,
            settings.agent_default == Some(selected_agent),
            || state.publish_workflow_external_launch(&context, &authority_run),
        );
        return match result {
            Ok((current, next)) => {
                dispatch_next(&state, next)?;
                Ok(current)
            }
            Err(failure) => {
                dispatch_next(&state, failure.next)?;
                Err(failure.error)
            }
        };
    }
    let execution_result = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let access = permit.workflow_access();
            if access.trust != crate::models::workflow::WorkflowProjectTrust::Trusted {
                return Err(workflow_error(
                    "WORKFLOW_PROJECT_UNTRUSTED",
                    "Workflow confirmation requires a trusted project.",
                ));
            }
            if access.filesystem_access
                != crate::models::workflow::WorkflowFilesystemAccess::Writable
            {
                return Err(workflow_error(
                    "WORKFLOW_PROJECT_READ_ONLY",
                    "Workflow confirmation requires writable project access.",
                ));
            }
            state.require_workflow_content_write_root(&context, &run.kind)?;
            let stored = state.confirmation_registry.claim(&request.action_id)?;
            if !crate::models::confirmation::workflow_execution_matches(
                &run.kind,
                stored.execution.as_ref(),
                &context,
                &run,
                &pending,
            ) {
                let _ = state
                    .confirmation_registry
                    .finish_claim(&request.action_id, false);
                let _ = state
                    .confirmation_registry
                    .cancel_workflow_binding(&context, &run, &pending);
                match &run.kind {
                    WorkflowKind::UpdateWiki => {
                        let _ = crate::services::discard_update_wiki_candidate(&run.task_id);
                    }
                    WorkflowKind::GenerateContent => {
                        let _ = crate::services::discard_generate_content_candidate(&run.task_id);
                    }
                    WorkflowKind::HealthCheck => {}
                }
                let (_, next) = state
                    .workflow_service
                    .coordinator
                    .interrupt_invalid_confirmation(
                        &state.task_service,
                        &run.task_id,
                        WorkflowErrorSummary {
                            code: "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH".into(),
                            message_key: "workflows.error.prepareAgain".into(),
                            recoverable: false,
                            user_action_required: true,
                            suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
                            project_mutation_state: WorkflowProjectMutationState::NotModified,
                        },
                    )
                    .map_err(|message| workflow_error("WORKFLOW_CONFIRMATION_FAILED", message))?;
                return Ok(Err((
                    workflow_error(
                        "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
                        "The confirmation execution plan does not match this workflow run.",
                    ),
                    next,
                )));
            }
            let execution_result = match (run.kind.clone(), stored.execution) {
                (
                    WorkflowKind::GenerateContent,
                    Some(ConfirmationExecution::GenerateContentOverwrite {
                        project_id,
                        root_path,
                        task_id,
                        ..
                    }),
                ) if project_id == context.project_id
                    && root_path == context.root.to_string_lossy()
                    && task_id == run_request.task_id =>
                {
                    match crate::services::confirm_generate_content_overwrite(
                        &context,
                        &run_request.task_id,
                        &generate_content_services(&state),
                    ) {
                        Ok(value) => Ok(value),
                        Err(failure) => Err((failure.error, failure.next)),
                    }
                }
                (
                    WorkflowKind::UpdateWiki,
                    Some(ConfirmationExecution::UpdateWikiReview {
                        project_id,
                        root_path,
                        task_id,
                        ..
                    }),
                ) if project_id == context.project_id
                    && root_path == context.root.to_string_lossy()
                    && task_id == run_request.task_id =>
                {
                    let compile = CompileExecutionServices {
                        agent_service: &state.agent_service,
                        llm_service: &state.llm_service,
                        secret_service: &state.secret_service,
                        settings_service: &state.settings_service,
                        task_service: &state.task_service,
                    };
                    match crate::services::confirm_update_wiki_review(
                        &context,
                        &run_request.task_id,
                        &UpdateWikiExecutionServices {
                            compile,
                            git_service: &state.git_service,
                            file_store: &state.file_store,
                            bookmark_service: &state.bookmark_service,
                            search_service: &state.search_service,
                            confirmation_registry: &state.confirmation_registry,
                            coordinator: &state.workflow_service.coordinator,
                        },
                    ) {
                        Ok(value) => Ok(value),
                        Err(failure) => Err((failure.error, failure.next)),
                    }
                }
                _ => Err((
                    workflow_error(
                        "WORKFLOW_CONFIRMATION_EXECUTION_MISMATCH",
                        "The confirmation execution plan does not match this workflow run.",
                    ),
                    None,
                )),
            };
            let consume_confirmation = execution_result.is_ok()
                || state
                    .task_service
                    .get_workflow_run(&run_request.task_id)
                    .is_none_or(|current| {
                        current
                            .pending_action
                            .as_ref()
                            .is_none_or(|pending| pending.id != request.action_id)
                    });
            state
                .confirmation_registry
                .finish_claim(&request.action_id, consume_confirmation)?;
            Ok(execution_result)
        },
    )?;
    match execution_result {
        Ok((completed, next)) => {
            dispatch_next(&state, next)?;
            Ok(completed)
        }
        Err((error, next)) => {
            dispatch_next(&state, next)?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn discard_workflow_result(
    state: State<'_, AppState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    require_workflow_project(&state, &request)?;
    let (run, next) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, current| {
            cancel_or_discard_workflow(&state, current, &request.task_id, true, true)
        },
    )?;
    dispatch_next(&state, next)?;
    Ok(run)
}

fn require_workflow_project(
    state: &AppState,
    request: &WorkflowRunRequest,
) -> Result<crate::models::paths::ProjectContext, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if !state
        .task_service
        .task_belongs_to_root(&request.task_id, &context.root)
    {
        return Err(workflow_error(
            "WORKFLOW_PROJECT_MISMATCH",
            "Workflow does not belong to the asserted project.",
        ));
    }
    let run = workflow_run(state, &request.task_id)?;
    ensure_workflow_identity(&context, &run)?;
    Ok(context)
}

fn ensure_workflow_identity(
    context: &crate::models::paths::ProjectContext,
    run: &WorkflowRun,
) -> Result<(), BackendError> {
    let identity = crate::services::project_identity(&context.root)
        .map_err(|message| workflow_error("WORKFLOW_IDENTITY_FAILED", message))?;
    if run.canonical_identity_key != identity.canonical_identity_key
        || run.identity_revision != identity.identity_revision
    {
        return Err(workflow_error(
            "WORKFLOW_PROJECT_IDENTITY_CHANGED",
            "The project folder identity changed after this workflow was created.",
        ));
    }
    Ok(())
}

fn workflow_run(state: &AppState, task_id: &str) -> Result<WorkflowRun, BackendError> {
    state
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| workflow_error("WORKFLOW_NOT_FOUND", "The workflow run was not found."))
}

pub(crate) fn cancel_or_discard_workflow(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    task_id: &str,
    require_waiting: bool,
    allow_project_cleanup: bool,
) -> Result<(WorkflowRun, Option<WorkflowRun>), BackendError> {
    let before = workflow_run(state, task_id)?;
    if require_waiting && before.display_status != WorkflowDisplayStatus::WaitingForConfirmation {
        return Err(workflow_error(
            "WORKFLOW_RESULT_NOT_DISCARDABLE",
            "Only a workflow result waiting for confirmation can be discarded.",
        ));
    }
    state
        .workflow_service
        .coordinator
        .cancel(&state.task_service, task_id)
        .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
    if allow_project_cleanup
        && matches!(
            before.display_status,
            WorkflowDisplayStatus::Queued
                | WorkflowDisplayStatus::Running
                | WorkflowDisplayStatus::WaitingForConfirmation
        )
    {
        // The task owner decides whether cancellation can win (notably, a
        // checked apply is temporarily non-cancellable) before the app-owned
        // receipt is tombstoned. A late success publication only accepts a
        // still-Dispatched receipt, so a winning cancellation cannot be
        // overwritten by the final commit path.
        crate::commands::lint_commands::cancel_agent_lint_repair_attestation_for_run(
            state, &before,
        )?;
    }
    let cancelling = workflow_run(state, task_id)?;
    if let Some(action) = before.pending_action.as_ref() {
        if let Err(error) = state
            .confirmation_registry
            .cancel_workflow_binding(context, &before, action)
        {
            if error.code == "CONFIRMATION_IN_USE" {
                return Ok((cancelling, None));
            }
            return Err(error);
        }
        if allow_project_cleanup
            && matches!(
                &before.operation,
                crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
            )
        {
            let result = crate::services::rollback_and_discard_agent_lint_repair_candidate(
                context,
                &before,
                &agent_lint_repair_services(state),
            )?;
            let (cancelled, next) = state
                .workflow_service
                .coordinator
                .finish_cancelled_and_claim_next_with_result(
                    &state.task_service,
                    task_id,
                    Some(result),
                )
                .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
            return Ok((cancelled, next));
        } else {
            let _ = crate::services::discard_update_wiki_candidate(task_id);
            let _ = crate::services::discard_generate_content_candidate(task_id);
        }
        let (cancelled, next) = state
            .workflow_service
            .coordinator
            .finish_cancelled_and_claim_next(&state.task_service, task_id)
            .map_err(|message| workflow_error("WORKFLOW_CANCEL_FAILED", message))?;
        return Ok((cancelled, next));
    }
    Ok((cancelling, None))
}

fn generate_content_services(state: &AppState) -> GenerateContentExecutionServices<'_> {
    GenerateContentExecutionServices {
        export_service: &state.export_service,
        search_service: &state.search_service,
        settings_service: &state.settings_service,
        secret_service: &state.secret_service,
        agent_service: &state.agent_service,
        llm_service: &state.llm_service,
        git_service: &state.git_service,
        confirmation_registry: &state.confirmation_registry,
        task_service: &state.task_service,
        coordinator: &state.workflow_service.coordinator,
    }
}

pub(super) fn agent_lint_repair_services(state: &AppState) -> AgentLintRepairExecutionServices<'_> {
    AgentLintRepairExecutionServices {
        agent_service: &state.agent_service,
        lint_service: &state.lint_service,
        git_service: &state.git_service,
        file_store: &state.file_store,
        bookmark_service: &state.bookmark_service,
        search_service: &state.search_service,
        confirmation_registry: &state.confirmation_registry,
        settings_service: &state.settings_service,
        task_service: &state.task_service,
        coordinator: &state.workflow_service.coordinator,
    }
}

fn dispatch_next(state: &AppState, next: Option<WorkflowRun>) -> Result<(), BackendError> {
    if let Some(next) = next {
        state.workflow_service.dispatch_claimed_run_with_settings(
            &state.task_service,
            &state.settings_service,
            &next,
        )?;
    }
    Ok(())
}

fn workflow_error(code: &str, message: impl Into<String>) -> BackendError {
    BackendError::new(code, message, true, true)
}

#[cfg(test)]
mod batch6_tests {
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
    }

    #[test]
    fn large_review_transport_keeps_only_stable_file_summaries() {
        let review = WorkflowDecisionReview {
            reason: "large".into(),
            counts: WorkflowDecisionCounts::default(),
            user_edits_detected: true,
            file_diffs: (0..500)
                .map(|index| WorkflowFileDiff {
                    file_id: String::new(),
                    path: format!("wiki/规模/页面-{index:04}.md"),
                    diff_bytes: 0,
                    diff: Some("x".repeat(20 * 1024)),
                    kind: WorkflowFileDiffKind::TwoWay,
                })
                .collect(),
        };
        let transported = prepare_decision_review_for_transport(review, Duration::ZERO);
        assert_eq!(transported.file_diffs.len(), 500);
        assert_eq!(transported.file_diffs[0].file_id, "file-00000000");
        assert_eq!(transported.file_diffs[499].file_id, "file-000001f3");
        assert!(transported
            .file_diffs
            .iter()
            .all(|file| file.diff.is_none()));
        assert!(serde_json::to_vec(&transported).unwrap().len() < LARGE_REVIEW_BYTES);
    }

    #[test]
    fn update_wiki_detail_keeps_small_reviews_inline_and_large_reviews_lazy() {
        let review = |sizes: &[usize]| WorkflowDecisionReview {
            reason: "review".into(),
            counts: WorkflowDecisionCounts::default(),
            user_edits_detected: false,
            file_diffs: sizes
                .iter()
                .enumerate()
                .map(|(index, size)| WorkflowFileDiff {
                    file_id: format!("file-{index:08x}"),
                    path: format!("wiki/page-{index}.md"),
                    diff_bytes: *size,
                    diff: None,
                    kind: WorkflowFileDiffKind::TwoWay,
                })
                .collect(),
        };
        let can_inline = |review: &WorkflowDecisionReview| {
            crate::services::update_wiki_review_can_inline(
                review,
                LARGE_DIFF_BYTES,
                LARGE_REVIEW_BYTES,
            )
        };
        assert!(can_inline(&review(&[1024, 2048])));
        assert!(!can_inline(&review(&[LARGE_DIFF_BYTES + 1])));
        assert!(!can_inline(&review(&[200 * 1024; 6])));

        let mut three_way = review(&[1]);
        three_way.file_diffs[0].kind = WorkflowFileDiffKind::ThreeWay;
        assert!(!can_inline(&three_way));
    }

    #[test]
    fn diff_pages_are_utf8_safe_and_bounded() {
        let file = WorkflowFileDiff {
            file_id: "file-00000000".into(),
            path: "wiki/中文/很长的路径.md".into(),
            diff_bytes: 12,
            diff: Some("甲乙丙丁".into()),
            kind: WorkflowFileDiffKind::TwoWay,
        };
        let first = paginate_workflow_diff(file.clone(), 0, 5).unwrap();
        assert_eq!(first.diff, "甲");
        assert_eq!(first.next_cursor, Some(3));
        let second = paginate_workflow_diff(file, first.next_cursor.unwrap(), 9).unwrap();
        assert_eq!(second.diff, "乙丙丁");
        assert!(!second.truncated);
        assert!(serde_json::to_vec(&first).unwrap().len() < 256 * 1024);
    }

    #[test]
    fn diff_pages_bound_the_serialized_payload_with_escape_heavy_content() {
        let file = WorkflowFileDiff {
            file_id: "file-00000000".into(),
            path: format!("wiki/{}\\page.md", "路径".repeat(256)),
            diff_bytes: 400 * 1024,
            diff: Some("\"\\\n\t".repeat(100 * 1024)),
            kind: WorkflowFileDiffKind::TwoWay,
        };
        let first = paginate_workflow_diff(file, 0, MAX_DIFF_CHUNK_BYTES).unwrap();
        assert!(first.truncated);
        assert!(first.next_cursor.is_some());
        assert!(serde_json::to_vec(&first).unwrap().len() <= MAX_DIFF_RESPONSE_BYTES);
    }

    #[test]
    fn diff_pages_always_advance_over_a_multibyte_character() {
        let file = WorkflowFileDiff {
            file_id: "file-00000000".into(),
            path: "wiki/中文.md".into(),
            diff_bytes: 8,
            diff: Some("中文🚀".into()),
            kind: WorkflowFileDiffKind::TwoWay,
        };
        let first = paginate_workflow_diff(file, 0, 1).unwrap();
        assert_eq!(first.diff, "中");
        assert_eq!(first.next_cursor, Some("中".len()));
    }

    #[test]
    fn retry_and_continue_shared_replay_gate_reprobe_agent_route_before_claim() {
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        use crate::models::agent::AgentKind;
        use crate::models::project::ProjectTrustKind;
        use crate::models::settings::Settings;
        use crate::models::workflow::{
            HealthCheckMode, WorkflowExecutionOptions, WorkflowFilesystemAccess, WorkflowGitState,
            WorkflowPersistenceMode, WorkflowProjectTrust, WorkflowScope, WorkflowStartOutcome,
        };
        use crate::services::{
            AgentInvocation, AgentProbeTarget, AgentService, EnqueueWorkflow, ProcessRunner,
            SettingsService, WorkflowAccessSnapshot,
        };
        use crate::tasks::TaskService;

        struct MutableCodex {
            version: AtomicUsize,
            invocations: AtomicUsize,
        }

        impl ProcessRunner for MutableCodex {
            fn find_executable(&self, command: &str) -> Option<PathBuf> {
                (command == "codex").then(|| PathBuf::from("codex"))
            }

            fn resolve_probe_target(&self, command: &str) -> AgentProbeTarget {
                AgentProbeTarget {
                    logical_command: command.into(),
                    executable_path: self.find_executable(command),
                    program: command.into(),
                    leading_args: Vec::new(),
                }
            }

            fn run_with_timeout(
                &self,
                _: &str,
                args: &[&str],
                _: Duration,
            ) -> Result<String, BackendError> {
                if args == ["--version"] {
                    return Ok(format!("codex {}.0.0", self.version.load(Ordering::SeqCst)));
                }
                Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
            }

            fn run_capture(&self, _: &AgentInvocation) -> Result<(String, String), BackendError> {
                unreachable!()
            }

            fn run_task_streaming(
                &self,
                _: &AgentInvocation,
                _: &TaskService,
                _: &str,
            ) -> Result<String, BackendError> {
                self.invocations.fetch_add(1, Ordering::SeqCst);
                Ok("[]".into())
            }
        }

        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki")).unwrap();
        std::fs::write(root.path().join("wiki/index.md"), "# Index\n").unwrap();
        let context =
            crate::models::paths::ProjectContext::new("replay-route", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let runner = Arc::new(MutableCodex {
            version: AtomicUsize::new(1),
            invocations: AtomicUsize::new(0),
        });
        let state = AppState {
            agent_service: AgentService::with_runner(runner.clone()),
            settings_service: SettingsService::with_config_dir(config.path().to_path_buf()),
            ..AppState::default()
        };
        state
            .settings_service
            .save_settings(
                &context,
                &Settings {
                    agent_default: Some(AgentKind::Codex),
                    ..Settings::default()
                },
            )
            .unwrap();
        let access = WorkflowAccessSnapshot {
            trust: WorkflowProjectTrust::Trusted,
            trust_kind: Some(ProjectTrustKind::Native),
            filesystem_access: WorkflowFilesystemAccess::Writable,
            persistence: WorkflowPersistenceMode::Persistent,
            git_state: WorkflowGitState::Clean,
            authority_revision: "authority-v1".into(),
        };
        let preparation = state
            .workflow_service
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &context,
                    access: access.clone(),
                    settings_service: &state.settings_service,
                    secret_service: &state.secret_service,
                    agent_service: &state.agent_service,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::HealthCheck,
                    scope: Some(WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    }),
                    route_selection: Some(WorkflowRouteSelection::Agent {
                        agent: AgentKind::Codex,
                    }),
                },
            )
            .unwrap();
        let outcome = state
            .workflow_service
            .coordinator
            .enqueue(
                &state.task_service,
                EnqueueWorkflow {
                    project_id: context.project_id.clone(),
                    project_root: context.root.clone(),
                    task_state_root: None,
                    title: "Health Check".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope: WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    },
                    route: preparation.route,
                    baseline_fingerprint: preparation.baseline.fingerprint,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: preparation.preparation_revision,
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: crate::services::workflow_stages(&WorkflowKind::HealthCheck),
                    retry: None,
                },
            )
            .unwrap();
        let run = match outcome {
            WorkflowStartOutcome::Created { run } => run,
            _ => panic!("fixture must create a workflow run"),
        };

        runner.version.store(2, Ordering::SeqCst);
        let replay = revalidate_workflow_replay_with_access(
            &state,
            &context,
            &run,
            access,
            crate::commands::lint_commands::AgentLintRepairReplayIntent::Continue,
        )
        .unwrap();
        assert!(replay.persistence.task_state_root.is_none());
        assert_eq!(
            replay.eligibility.unwrap_err().code,
            "WORKFLOW_REPREPARATION_REQUIRED"
        );
        assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    }
}

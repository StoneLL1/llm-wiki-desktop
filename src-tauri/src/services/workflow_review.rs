//! On-demand Workflow confirmation reviews, paged diffs and checked confirmation.
//! TaskService owns task facts; this facade only loads candidate data for a requested run.
use std::time::{Duration, Instant};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{ConfirmationExecution, PendingActionType, StoredPendingAction};
use crate::models::workflow::{
    WorkflowDecisionCounts, WorkflowDecisionReview, WorkflowDisplayStatus, WorkflowErrorSummary,
    WorkflowFileDiff, WorkflowFileDiffKind, WorkflowFileDiffPage, WorkflowKind,
    WorkflowPrerequisiteAction, WorkflowProjectMutationState, WorkflowRun,
};
use crate::models::workflow_requests::{
    ConfirmWorkflowActionRequest, WorkflowFileDiffRequest, WorkflowRunRequest,
    DEFAULT_DIFF_CHUNK_BYTES,
};
use crate::services::{
    restore_agent_lint_repair_confirmation, restore_generate_content_confirmation,
    restore_update_wiki_confirmation, AgentLintRepairExecutionServices, CompileExecutionServices,
    GenerateContentExecutionServices, UpdateWikiExecutionServices,
};

const MAX_DIFF_CHUNK_BYTES: usize = 240 * 1024;
const MAX_DIFF_RESPONSE_BYTES: usize = 256 * 1024;
const LARGE_DIFF_BYTES: usize = 256 * 1024;
const LARGE_REVIEW_BYTES: usize = 1024 * 1024;
const SLOW_REVIEW_HYDRATION: Duration = Duration::from_millis(50);

pub(crate) fn get_workflow_run_for_state(
    state: &AppState,
    request: WorkflowRunRequest,
) -> Result<WorkflowRun, BackendError> {
    let context = require_workflow_project(state, &request)?;
    let mut run = workflow_run(state, &request.task_id)?;
    // Scope review has no generated candidate or executable confirmation.
    if run
        .pending_action
        .as_ref()
        .is_some_and(|pending| pending.action_type == PendingActionType::ReviewScope)
    {
        return Ok(run);
    }
    let next = if let Some(pending) = run.pending_action.clone() {
        state.with_workflow_access(&context, |_| {
            let hydration_started = Instant::now();
            match hydrate_workflow_confirmation(state, &context, &run, &pending, false) {
                Ok(review) => {
                    run.decision_review = Some(prepare_decision_review_for_transport(
                        review,
                        hydration_started.elapsed(),
                    ));
                    Ok(None)
                }
                Err(error) => {
                    let (interrupted, next) =
                        interrupt_unconfirmable_workflow(state, &context, &run, &pending, error)?;
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

pub(crate) fn get_workflow_file_diff_for_state(
    state: &AppState,
    request: WorkflowFileDiffRequest,
) -> Result<WorkflowFileDiffPage, BackendError> {
    let run_request = WorkflowRunRequest {
        project_id: request.project_id,
        project_root_path: request.project_root_path,
        task_id: request.task_id,
    };
    let context = require_workflow_project(state, &run_request)?;
    let run = workflow_run(state, &run_request.task_id)?;
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
                    &agent_lint_repair_services(state),
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
    if pending.action_type == PendingActionType::ReviewScope {
        return Err(workflow_error(
            "WORKFLOW_DIFF_NOT_FOUND",
            "Scope review has no generated file diff.",
        ));
    }
    if run.kind == WorkflowKind::UpdateWiki {
        let mut bounded_limit = limit;
        loop {
            let page = state.with_workflow_access(&context, |_| {
                validate_workflow_confirmation(state, &context, &run, pending)?;
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
                validate_workflow_confirmation(state, &context, &run, pending)?;
                crate::services::agent_lint_repair_file_diff_page(
                    &context,
                    &run,
                    &agent_lint_repair_services(state),
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
            state, &context, &run, pending, true,
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
        crate::services::update_wiki_decision_review_for_workflow(
            &run.task_id,
            &context.root,
            &workflow,
            (!include_update_wiki_diffs).then_some((LARGE_DIFF_BYTES, LARGE_REVIEW_BYTES)),
        )
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
    if run.kind == WorkflowKind::UpdateWiki {
        if let Ok(stored) = state.confirmation_registry.peek(&pending.id) {
            if crate::models::confirmation::workflow_execution_matches(
                &run.kind,
                stored.execution.as_ref(),
                context,
                run,
                pending,
            ) {
                return Ok(stored);
            }
        }
    }
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

pub(crate) fn confirm_workflow_action_for_state(
    state: &AppState,
    request: ConfirmWorkflowActionRequest,
) -> Result<WorkflowRun, BackendError> {
    let run_request = WorkflowRunRequest {
        project_id: request.project_id.clone(),
        project_root_path: request.project_root_path.clone(),
        task_id: request.task_id,
    };
    let context = require_workflow_project(state, &run_request)?;
    let run = workflow_run(state, &run_request.task_id)?;
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
    if pending.action_type == PendingActionType::ReviewScope {
        return Err(workflow_error(
            "WORKFLOW_REPREPARATION_REQUIRED",
            "Review the changed input scope and prepare the workflow again before starting.",
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
        let services = agent_lint_repair_services(state);
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
                dispatch_next(state, next)?;
                Ok(current)
            }
            Err(failure) => {
                dispatch_next(state, failure.next)?;
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
                        &generate_content_services(state),
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
            dispatch_next(state, next)?;
            Ok(completed)
        }
        Err((error, next)) => {
            dispatch_next(state, next)?;
            Err(error)
        }
    }
}

pub(crate) fn require_workflow_project(
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

pub(crate) fn ensure_workflow_identity(
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

pub(crate) fn workflow_run(state: &AppState, task_id: &str) -> Result<WorkflowRun, BackendError> {
    state
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| workflow_error("WORKFLOW_NOT_FOUND", "The workflow run was not found."))
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

pub(crate) fn agent_lint_repair_services(state: &AppState) -> AgentLintRepairExecutionServices<'_> {
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

pub(crate) fn dispatch_next(
    state: &AppState,
    next: Option<WorkflowRun>,
) -> Result<(), BackendError> {
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
mod tests {
    use super::*;

    #[test]
    fn scope_review_detail_stays_waiting_without_candidate_and_rejects_apply() {
        use crate::models::confirmation::RiskLevel;
        use crate::models::workflow::{
            UpdateWikiMode, WorkflowExecutionOptions, WorkflowPendingAction, WorkflowScope,
            WorkflowStartOutcome,
        };
        use crate::services::{workflow_stages, EnqueueWorkflow};

        let root = tempfile::tempdir().unwrap();
        for path in ["raw/sources", "wiki", ".app/tasks", "exports", "skills"] {
            std::fs::create_dir_all(root.path().join(path)).unwrap();
        }
        std::fs::write(root.path().join("purpose.md"), "# Purpose\n").unwrap();
        std::fs::write(root.path().join("schema.md"), "# Schema\n").unwrap();
        let state = AppState::default();
        let context = state
            .project_registry
            .register("scope-review", root.path())
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
                    title: "Update Wiki".into(),
                    kind: WorkflowKind::UpdateWiki,
                    scope: WorkflowScope::UpdateWiki {
                        mode: UpdateWikiMode::ChangedSources,
                        source_versions: Vec::new(),
                    },
                    route: None,
                    baseline_fingerprint: "prior-inputs".into(),
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: "scope-review-v1".into(),
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: workflow_stages(&WorkflowKind::UpdateWiki),
                    retry: None,
                },
            )
            .unwrap();
        let WorkflowStartOutcome::Created { run } = outcome else {
            panic!("fixture should create a workflow");
        };
        let pending = WorkflowPendingAction {
            id: "scope-review-action".into(),
            action_type: PendingActionType::ReviewScope,
            risk_level: RiskLevel::Low,
            affected_paths: vec!["raw/sources/中文.md".into()],
            candidate: None,
            expires_at: None,
            checkpoint_hash: None,
        };
        state
            .task_service
            .start_workflow_stage(&run.task_id, "analyze_sources")
            .unwrap();
        state
            .task_service
            .wait_workflow_stage(&run.task_id, "analyze_sources", pending)
            .unwrap();
        let request = WorkflowRunRequest {
            project_id: context.project_id.clone(),
            project_root_path: context.root.to_string_lossy().into_owned(),
            task_id: run.task_id.clone(),
        };
        let detail = get_workflow_run_for_state(&state, request.clone()).unwrap();
        assert_eq!(
            detail.display_status,
            WorkflowDisplayStatus::WaitingForConfirmation
        );
        assert!(detail.decision_review.is_none());
        let confirmation = confirm_workflow_action_for_state(
            &state,
            ConfirmWorkflowActionRequest {
                project_id: request.project_id.clone(),
                project_root_path: request.project_root_path.clone(),
                task_id: request.task_id.clone(),
                action_id: "scope-review-action".into(),
            },
        )
        .unwrap_err();
        assert_eq!(confirmation.code, "WORKFLOW_REPREPARATION_REQUIRED");
        let diff = get_workflow_file_diff_for_state(
            &state,
            WorkflowFileDiffRequest {
                project_id: request.project_id,
                project_root_path: request.project_root_path,
                task_id: request.task_id,
                pending_action_id: "scope-review-action".into(),
                file_id: "file-00000000".into(),
                cursor: None,
                limit_bytes: DEFAULT_DIFF_CHUNK_BYTES,
            },
        )
        .unwrap_err();
        assert_eq!(diff.code, "WORKFLOW_DIFF_NOT_FOUND");
        assert_eq!(
            state
                .task_service
                .get_workflow_run(&run.task_id)
                .unwrap()
                .display_status,
            WorkflowDisplayStatus::WaitingForConfirmation
        );
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
}

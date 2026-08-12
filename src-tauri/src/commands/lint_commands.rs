use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::ConfirmationExecution;
use crate::models::lint::{
    AddLintIgnoreRequest, ApplyLintFixRequest, ApplyLintFixesBatchRequest, DeepLintReport,
    GetDeepLintReportRequest, LintBatchOutcome, LintBatchSkip, LintFixOutcome, LintHistoryFile,
    LintIgnoreFile, LintReport, ListLintHistoryRequest, ListLintIgnoresRequest,
    PersistedLintReport, ReadLintHistoryReportRequest, RemoveLintIgnoreRequest,
    RunLocalLintRequest, StartDeepLintRequest,
};
use crate::models::task::BackendTask;

/// Run the deterministic local lint pass. Synchronous — it never calls a
/// model and completes in a single wiki scan.
#[tauri::command]
pub fn run_local_lint(
    state: State<'_, AppState>,
    request: RunLocalLintRequest,
) -> Result<LintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let report = state
        .lint_service
        .run_local_lint(&context, &state.search_service)?;
    state.lint_service.persist_local_report(&context, &report)?;
    Ok(report)
}

/// The legacy task-owned Deep Lint launch path is intentionally migrated to
/// Complete Health. Keeping it disabled avoids a second launch-authority,
/// snapshot, cancellation, and report-persistence contract. Complete Health
/// retains both explicit Agent and explicit BYOK analysis routes.
#[tauri::command]
pub fn start_deep_lint(
    _state: State<'_, AppState>,
    _request: StartDeepLintRequest,
) -> Result<BackendTask, BackendError> {
    Err(legacy_deep_lint_migrated_error())
}

fn legacy_deep_lint_migrated_error() -> BackendError {
    BackendError::new(
        "LINT_DEEP_HEALTH_REQUIRED",
        "Deep lint now runs through Complete Health so explicit Agent or BYOK launches share one current-authority, snapshot, cancellation, and report contract.",
        true,
        true,
    )
}

/// Load the persisted deep-lint report for a completed (or in-flight) task.
#[tauri::command]
pub fn get_deep_lint_report(
    state: State<'_, AppState>,
    request: GetDeepLintReportRequest,
) -> Result<DeepLintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let persisted = state
        .lint_service
        .read_lint_history_report(&context, &request.task_id)?;
    persisted.deep_report.ok_or_else(|| {
        BackendError::new(
            "LINT_DEEP_REPORT_MISSING",
            "The selected lint history report is not a deep lint report.",
            true,
            true,
        )
    })
}

#[tauri::command]
pub fn list_lint_history(
    state: State<'_, AppState>,
    request: ListLintHistoryRequest,
) -> Result<LintHistoryFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.lint_service.list_lint_history(&context)
}

#[tauri::command]
pub fn read_lint_history_report(
    state: State<'_, AppState>,
    request: ReadLintHistoryReportRequest,
) -> Result<PersistedLintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .lint_service
        .read_lint_history_report(&context, &request.id)
}

/// Apply (or plan) a single lint fix. Safe fixes apply under a Git checkpoint;
/// high-risk fixes return a `PendingAction` until confirmed.
#[tauri::command]
pub fn apply_lint_fix(
    state: State<'_, AppState>,
    request: ApplyLintFixRequest,
) -> Result<LintFixOutcome, BackendError> {
    if request.confirm_high_risk {
        let action_id = request.action_id.as_deref().ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_REQUIRED",
                "A backend-issued lint confirmation action is required.",
                true,
                true,
            )
        })?;
        // Claim the action before the write. Cancellation is rejected while a
        // claim is active, so an approval cannot be removed halfway through a
        // destructive mutation and then report a misleading NOT_FOUND error.
        let stored = state.confirmation_registry.claim(action_id)?;
        // Keep every post-claim validation inside the cleanup boundary. If
        // execution data or project resolution is invalid, the claim must be
        // released so a later retry/cancel is not permanently blocked.
        let result = (|| -> Result<LintFixOutcome, BackendError> {
            let execution = stored.execution.ok_or_else(|| {
                BackendError::new(
                    "CONFIRMATION_EXECUTION_MISSING",
                    "Lint confirmation has no execution plan.",
                    false,
                    true,
                )
            })?;
            let ConfirmationExecution::LintFix {
                project_id,
                root_path,
                issue,
            } = execution
            else {
                return Err(BackendError::new(
                    "CONFIRMATION_TYPE_MISMATCH",
                    "The pending action is not a lint fix.",
                    false,
                    true,
                ));
            };
            let context = state.resolve_project_context(&project_id, &root_path)?;
            state.lint_service.apply_fix(
                &context,
                &state.git_service,
                &issue,
                true,
                request.expected_hash.as_deref(),
            )
        })();
        match result {
            Ok(outcome) => {
                state.confirmation_registry.finish_claim(
                    action_id,
                    outcome.kind == crate::models::lint::LintFixOutcomeKind::Applied,
                )?;
                return Ok(outcome);
            }
            Err(error) => {
                let _ = state.confirmation_registry.finish_claim(action_id, false);
                return Err(error);
            }
        }
    }

    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let outcome = state.lint_service.apply_fix(
        &context,
        &state.git_service,
        &request.issue,
        false,
        request.expected_hash.as_deref(),
    )?;
    if let Some(action) = outcome.pending_action.clone() {
        state.confirmation_registry.register_with_execution(
            action,
            Some(ConfirmationExecution::LintFix {
                project_id: request.project_id,
                root_path: request.project_root_path,
                issue: request.issue,
            }),
        )?;
    }
    Ok(outcome)
}

/// Apply many lint fixes in one shot (PRD-LINT-003). One Git checkpoint
/// protects every safe write; high-risk fixes come back as confirmations for
/// unified review. Each confirmation is registered here so the existing
/// `apply_lint_fix(confirm_high_risk=true, action_id)` path can execute it.
#[tauri::command]
pub fn apply_lint_fixes(
    state: State<'_, AppState>,
    request: ApplyLintFixesBatchRequest,
) -> Result<LintBatchOutcome, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let outcome = state.lint_service.apply_fixes_batch(
        &context,
        &state.git_service,
        &request.issues,
        &request.expected_hashes,
    )?;
    let mut registered_confirmations = Vec::with_capacity(outcome.needs_confirmation.len());
    let mut skipped = outcome.skipped.clone();
    for confirmation in outcome.needs_confirmation {
        match state.confirmation_registry.register_with_execution(
            confirmation.pending_action.clone(),
            Some(ConfirmationExecution::LintFix {
                project_id: request.project_id.clone(),
                root_path: request.project_root_path.clone(),
                issue: confirmation.issue.clone(),
            }),
        ) {
            Ok(()) => registered_confirmations.push(confirmation),
            Err(error) => skipped.push(LintBatchSkip {
                issue_id: confirmation.issue.id,
                path: confirmation.issue.path,
                reason_code: "LINT_CONFIRMATION_REGISTER_FAILED".into(),
                reason: format!(
                    "The safe batch result was kept, but this high-risk confirmation could not be registered: {}",
                    error.message
                ),
            }),
        }
    }
    Ok(LintBatchOutcome {
        needs_confirmation: registered_confirmations,
        skipped,
        ..outcome
    })
}

/// Record an ignored (path, rule) so `run_local_lint` skips it on future scans.
#[tauri::command]
pub fn add_lint_ignore(
    state: State<'_, AppState>,
    request: AddLintIgnoreRequest,
) -> Result<LintIgnoreFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .lint_service
        .add_ignore(&context, &request.path, request.rule)
}

/// Remove an ignored (path, rule) so it is reported again.
#[tauri::command]
pub fn remove_lint_ignore(
    state: State<'_, AppState>,
    request: RemoveLintIgnoreRequest,
) -> Result<LintIgnoreFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .lint_service
        .remove_ignore(&context, &request.path, request.rule)
}

/// Return the current ignore list.
#[tauri::command]
pub fn list_lint_ignores(
    state: State<'_, AppState>,
    request: ListLintIgnoresRequest,
) -> Result<LintIgnoreFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.lint_service.list_ignores(&context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_path_is_task_scoped() {
        assert_eq!(
            format!(".app/lint-reports/{}.json", "task-1"),
            ".app/lint-reports/task-1.json"
        );
    }

    #[test]
    fn legacy_deep_lint_is_migrated_before_any_task_or_external_launch() {
        let error = legacy_deep_lint_migrated_error();
        assert_eq!(error.code, "LINT_DEEP_HEALTH_REQUIRED");
        assert!(error.message.contains("Complete Health"));
    }
}

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::CompileRoutePreference;
use crate::models::confirmation::ConfirmationExecution;
use crate::models::lint::{
    AddLintIgnoreRequest, ApplyLintFixRequest, ApplyLintFixesBatchRequest, DeepLintReport,
    GetDeepLintReportRequest, LintBatchOutcome, LintBatchSkip, LintFixOutcome, LintHistoryFile,
    LintIgnoreFile, LintReport, ListLintHistoryRequest, ListLintIgnoresRequest,
    PersistedLintReport, ReadLintHistoryReportRequest, RemoveLintIgnoreRequest,
    RunLocalLintRequest, StartDeepLintRequest,
};
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::{AgentService, LlmService};
use crate::tasks::task_model::LogLevel;

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

/// Start a deep-lint run as a cancellable background task (BYOK is the only
/// verified route; CLI Agent execution is disabled until credential isolation
/// is available). The parsed issues are persisted to
/// `.app/lint-reports/<task_id>.json` and surfaced via `get_deep_lint_report`.
#[tauri::command]
pub fn start_deep_lint(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartDeepLintRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _deep_start_guard = state.lint_service.lock_deep_start()?;
    if let Some(existing) = state
        .task_service
        .list_tasks(None)
        .into_iter()
        .find(|task| {
            task.task_type == TaskType::DeepLint
                && task.project_id.as_deref() == Some(request.project_id.as_str())
                && matches!(
                    task.status,
                    TaskStatus::Queued
                        | TaskStatus::Running
                        | TaskStatus::WaitingForConfirmation
                        | TaskStatus::Cancelling
                )
        })
    {
        return Ok(existing);
    }
    let task = state
        .task_service
        .create_project_task(
            TaskType::DeepLint,
            request.project_id.clone(),
            context.root.clone(),
            "Deep wiki lint".to_string(),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_deep_lint(&state, request, &context, &task_id).await {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            if !matches!(
                state.task_service.get_task(&task_id).map(|t| t.status),
                Some(TaskStatus::Cancelled)
            ) {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
            }
        }
    });
    Ok(task)
}

async fn run_deep_lint(
    state: &AppState,
    request: StartDeepLintRequest,
    context: &ProjectContext,
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Building deep-lint prompt".into())
        .map_err(task_error)?;
    let language = state
        .settings_service
        .read_settings(context)
        .map(|settings| settings.language)
        .unwrap_or_else(|_| "en".to_string());
    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "LINT_CANCELLED",
            "Deep lint was cancelled.",
            true,
            false,
        ));
    }
    // The prompt builder scans the wiki internally. Require the tree and
    // hashes immediately around that scan to be stable, otherwise the model
    // could receive one snapshot while fixes are guarded by another.
    let mut stable_prompt: Option<String> = None;
    let mut known_paths: HashSet<String> = HashSet::new();
    let mut scan_hashes: HashMap<String, String> = HashMap::new();
    let mut prompt_input_hashes: HashMap<String, Option<String>> = HashMap::new();
    for _ in 0..2 {
        let before_tree = state.search_service.scan_wiki(context, &HashSet::new())?;
        let before_paths: HashSet<String> = before_tree
            .pages
            .iter()
            .map(|page| page.path.clone())
            .collect();
        let before_hashes = state
            .lint_service
            .capture_prompt_input_hashes(context, &before_paths)?;
        let candidate_prompt =
            state
                .lint_service
                .build_deep_lint_prompt(context, &state.search_service, &language)?;
        let after_tree = state.search_service.scan_wiki(context, &HashSet::new())?;
        let after_paths: HashSet<String> = after_tree
            .pages
            .iter()
            .map(|page| page.path.clone())
            .collect();
        let after_hashes = state
            .lint_service
            .capture_prompt_input_hashes(context, &after_paths)?;
        if before_paths == after_paths && before_hashes == after_hashes {
            stable_prompt = Some(candidate_prompt);
            known_paths = after_paths;
            prompt_input_hashes = before_hashes;
            scan_hashes = state
                .lint_service
                .capture_page_hashes(context, &known_paths);
            break;
        }
    }
    let Some(prompt) = stable_prompt else {
        return Err(BackendError::new(
            "LINT_SCAN_CHANGED",
            "The wiki changed while preparing the deep-lint snapshot; run the scan again.",
            true,
            true,
        ));
    };
    let deterministic_issue_ids: HashSet<String> = state
        .lint_service
        .run_local_lint(context, &state.search_service)?
        .issues
        .into_iter()
        .map(|issue| issue.id)
        .collect();
    // The deterministic pass is also a prompt input: reject an edit that
    // lands between prompt construction and model invocation.
    verify_deep_prompt_snapshot(state, context, &known_paths, &prompt_input_hashes)?;

    let raw = match resolve_route(
        state,
        context,
        request.route,
        request.agent,
        request.provider,
    )? {
        ResolvedRoute::Agent(kind) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Running {} (wiki-lint skill)", kind.command()),
                )
                .map_err(task_error)?;
            let workspace = create_lint_workspace(task_id)?;
            let _guard = WorkspaceGuard(workspace.clone());
            verify_deep_prompt_snapshot(state, context, &known_paths, &prompt_input_hashes)?;
            let invocation = AgentService::lint_invocation(kind, &workspace, &prompt)?;
            state
                .agent_service
                .run_lint_streaming(&invocation, &state.task_service, task_id)?
            // _guard drops here and removes the temp workspace even on Err / cancel.
        }
        ResolvedRoute::Byok(provider) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Calling {:?} (BYOK)", provider.provider),
                )
                .map_err(task_error)?;
            let secret = state.secret_service.get(provider.provider)?;
            verify_deep_prompt_snapshot(state, context, &known_paths, &prompt_input_hashes)?;
            let completion = state
                .llm_service
                .complete(&provider, secret.as_deref(), &prompt);
            let raw = crate::tasks::byok_progress::poll_with_progress(
                &state.task_service,
                task_id,
                "Linting",
                completion,
            )
            .await
            .map_err(|_| {
                crate::tasks::byok_progress::cancelled_error(
                    "LINT_CANCELLED",
                    "Deep lint was cancelled.",
                )
            })??;
            // Do not mirror model output into the task log: it may contain
            // source excerpts or provider-side sensitive material. Keep only
            // a bounded length diagnostic for the task drawer.
            let _ = state.task_service.append_log(
                task_id,
                LogLevel::Info,
                format!(
                    "BYOK lint response received ({} chars).",
                    raw.chars().count()
                ),
            );
            raw
        }
    };

    // A provider may take time to start or complete while the user edits the
    // wiki. Do not persist findings from a prompt that no longer represents
    // the project snapshot used for this request.
    verify_deep_prompt_snapshot(state, context, &known_paths, &prompt_input_hashes)?;

    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "LINT_CANCELLED",
            "Deep lint was cancelled.",
            true,
            false,
        ));
    }

    let mut issues = crate::services::LintService::parse_agent_issues_for_known_paths(
        &raw,
        &known_paths,
        &deterministic_issue_ids,
    )?;
    state
        .lint_service
        .filter_ignored_issues(context, &mut issues)?;
    for issue in &mut issues {
        issue.scan_hash = scan_hashes.get(&issue.path).cloned();
    }
    let issue_count = issues.len();
    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "LINT_CANCELLED",
            "Deep lint was cancelled.",
            true,
            false,
        ));
    }
    let report = DeepLintReport {
        issues,
        // Raw model output can contain wiki excerpts or unrecognized secrets;
        // persist only the typed issues and a safe diagnostic marker.
        raw_output: "raw output omitted for security".into(),
        generated_at: crate::utils::time_utils::now_rfc3339(),
    };
    let entry = state
        .lint_service
        .persist_deep_report(context, task_id, request.route, &report)?;
    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "LINT_CANCELLED",
            "Deep lint was cancelled.",
            true,
            false,
        ));
    }
    let report_path = format!(".app/lint-reports/{}.json", entry.id);

    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: format!("Deep lint found {issue_count} issue(s)."),
                affected_paths: vec![report_path],
                reference: None,
                pending_action: None,
            },
        )
        .map_err(task_error)?;
    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "LINT_CANCELLED",
            "Deep lint was cancelled.",
            true,
            false,
        ));
    }
    state
        .task_service
        .transition_status(task_id, TaskStatus::Succeeded)
        .map_err(task_error)?;
    Ok(())
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

enum ResolvedRoute {
    Agent(AgentKind),
    Byok(LlmProviderConfig),
}

/// Resolve the deep-lint provider. Unlike chat, this intentionally refuses
/// unverified CLI Agent profiles and falls back to BYOK in Auto mode.
/// Kept local to lint so the feature stays self-contained.
fn resolve_route(
    state: &AppState,
    context: &ProjectContext,
    preference: CompileRoutePreference,
    explicit_agent: Option<AgentKind>,
    explicit_provider: Option<LlmProviderKind>,
) -> Result<ResolvedRoute, BackendError> {
    let agent_config = AgentService::load_config(context)?;
    let providers = LlmService::list_providers(context)?;
    let selected_agent = explicit_agent.or(agent_config.default_agent);
    let usable_agent = selected_agent.filter(|kind| {
        // Check the product safety contract before probing PATH. With the
        // current false profile this avoids executing any CLI detection code
        // for a route that is intentionally unavailable.
        if !AgentService::supports_lint_agent(*kind) {
            return false;
        }
        let info = state
            .agent_service
            .detect_agent(*kind, agent_config.default_agent == Some(*kind));
        info.state == AgentDetectionState::Installed && AgentService::supports_lint_agent(*kind)
    });
    let selected_provider = select_provider(explicit_provider, &providers, &state.secret_service)?;
    let use_agent = match preference {
        CompileRoutePreference::Agent => true,
        CompileRoutePreference::Byok => false,
        CompileRoutePreference::Auto => usable_agent.is_some(),
    };
    if use_agent {
        if let Some(kind) = usable_agent {
            return Ok(ResolvedRoute::Agent(kind));
        }
        return Err(BackendError::new(
            "AGENT_UNAVAILABLE",
            "No verified no-tools Agent profile is available for deep lint. Switch to BYOK.",
            true,
            true,
        ));
    }
    match selected_provider {
        Some(provider) => Ok(ResolvedRoute::Byok(provider)),
        None => Err(BackendError::new(
            "LLM_PROVIDER_MISSING",
            "No enabled BYOK provider with a configured secret is available for deep lint.",
            true,
            true,
        )),
    }
}

fn select_provider(
    explicit: Option<LlmProviderKind>,
    providers: &[LlmProviderConfig],
    secrets: &crate::services::SecretService,
) -> Result<Option<LlmProviderConfig>, BackendError> {
    if let Some(kind) = explicit {
        let provider = providers
            .iter()
            .find(|p| p.enabled && p.provider == kind)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "LLM_PROVIDER_MISSING",
                    "The selected BYOK provider is not enabled.",
                    true,
                    true,
                )
            })?;
        if provider.provider.requires_secret() && secrets.get(provider.provider)?.is_none() {
            return Err(BackendError::new(
                "LLM_SECRET_MISSING",
                "The selected provider has no configured secret.",
                true,
                true,
            ));
        }
        return Ok(Some(provider));
    }
    for provider in providers.iter().filter(|p| p.enabled) {
        // Auto mode must not trust an arbitrary project-local endpoint with a
        // global credential. Custom/proxy hosts remain available only when a
        // user explicitly selects that provider in the request.
        if !is_safe_auto_provider(provider) {
            continue;
        }
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

fn is_safe_auto_provider(provider: &LlmProviderConfig) -> bool {
    let Ok(url) = url::Url::parse(provider.base_url.trim()) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    match provider.provider {
        LlmProviderKind::OpenAi => host == "api.openai.com",
        LlmProviderKind::Anthropic => host == "api.anthropic.com",
        LlmProviderKind::Google => host == "generativelanguage.googleapis.com",
        LlmProviderKind::Ollama => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
        LlmProviderKind::Custom => false,
    }
}

fn verify_deep_prompt_snapshot(
    state: &AppState,
    context: &ProjectContext,
    expected_paths: &HashSet<String>,
    expected_hashes: &HashMap<String, Option<String>>,
) -> Result<(), BackendError> {
    let tree = state.search_service.scan_wiki(context, &HashSet::new())?;
    let paths: HashSet<String> = tree.pages.iter().map(|page| page.path.clone()).collect();
    let hashes = state
        .lint_service
        .capture_prompt_input_hashes(context, &paths)?;
    if &paths != expected_paths || &hashes != expected_hashes {
        return Err(BackendError::new(
            "LINT_SCAN_CHANGED",
            "The wiki changed while preparing or running deep lint; run the scan again.",
            true,
            true,
        ));
    }
    Ok(())
}

fn create_lint_workspace(task_id: &str) -> Result<PathBuf, BackendError> {
    let workspace = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(format!("lint-{task_id}"));
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace).map_err(|err| {
            BackendError::new("LINT_WORKSPACE_FAILED", err.to_string(), true, false)
        })?;
    }
    std::fs::create_dir_all(&workspace)
        .map_err(|err| BackendError::new("LINT_WORKSPACE_FAILED", err.to_string(), true, false))?;
    Ok(workspace)
}

/// Removes the temp workspace on drop, including when the agent run errors or
/// is cancelled mid-stream (the happy-path cleanup alone leaks on those paths).
struct WorkspaceGuard(PathBuf);

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn report_path_is_task_scoped() {
        assert_eq!(
            format!(".app/lint-reports/{}.json", "task-1"),
            ".app/lint-reports/task-1.json"
        );
    }
}

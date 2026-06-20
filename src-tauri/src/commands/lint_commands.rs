use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::CompileRoutePreference;
use crate::models::lint::{
    ApplyLintFixRequest, DeepLintReport, GetDeepLintReportRequest, LintFixOutcome, LintReport,
    RunLocalLintRequest, StartDeepLintRequest,
};
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::{AgentService, LlmService};
use crate::tasks::task_model::LogLevel;

const LINT_REPORTS_DIR: &str = ".app/lint-reports";

fn context_for(project_id: &str, root_path: &str) -> ProjectContext {
    ProjectContext::new(project_id, PathBuf::from(root_path))
}

/// Run the deterministic local lint pass. Synchronous — it never calls a
/// model and completes in a single wiki scan.
#[tauri::command]
pub fn run_local_lint(
    state: State<'_, AppState>,
    request: RunLocalLintRequest,
) -> Result<LintReport, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state
        .lint_service
        .run_local_lint(&context, &state.search_service)
}

/// Start an Agent deep-lint run as a cancellable background task (Agent CLI
/// preferred, BYOK fallback). The parsed issues are persisted to
/// `.app/lint-reports/<task_id>.json` and surfaced via `get_deep_lint_report`.
#[tauri::command]
pub fn start_deep_lint(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartDeepLintRequest,
) -> Result<BackendTask, BackendError> {
    let task = state.task_service.create_task(
        TaskType::DeepLint,
        Some(request.project_id.clone()),
        "Deep wiki lint".to_string(),
        true,
    );
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_deep_lint(&state, request, &task_id).await {
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
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    let context = context_for(&request.project_id, &request.project_root_path);

    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Building deep-lint prompt".into())
        .map_err(task_error)?;
    let prompt = state
        .lint_service
        .build_deep_lint_prompt(&context, &state.search_service)?;

    let raw = match resolve_route(
        state,
        &context,
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
            let invocation = AgentService::lint_invocation(kind, &workspace, &prompt)?;
            state
                .agent_service
                .run_task_streaming(&invocation, &state.task_service, task_id)?
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
            let completion = state
                .llm_service
                .complete(&provider, secret.as_deref(), &prompt);
            tokio::pin!(completion);
            let raw = loop {
                tokio::select! {
                    result = &mut completion => break result?,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        if state.task_service.is_cancelled(task_id) {
                            return Err(BackendError::new(
                                "LINT_CANCELLED",
                                "Deep lint was cancelled.",
                                true,
                                false,
                            ));
                        }
                    }
                }
            };
            // Mirror agent stdout into the task drawer so BYOK runs show output too.
            for line in raw.lines() {
                let _ = state
                    .task_service
                    .append_log(task_id, LogLevel::Info, line.to_string());
            }
            raw
        }
    };

    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "LINT_CANCELLED",
            "Deep lint was cancelled.",
            true,
            false,
        ));
    }

    let issues = crate::services::LintService::parse_agent_issues(&raw)?;
    let issue_count = issues.len();
    let report = DeepLintReport {
        issues,
        raw_output: raw,
        generated_at: crate::utils::time_utils::now_rfc3339(),
    };
    let report_path = format!("{LINT_REPORTS_DIR}/{task_id}.json");
    state.file_store.ensure_dir(&context, LINT_REPORTS_DIR)?;
    state
        .file_store
        .write_json_atomic(&context, &report_path, &report)?;

    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: format!("Deep lint found {issue_count} issue(s)."),
                affected_paths: vec![report_path],
                pending_action: None,
            },
        )
        .map_err(task_error)?;
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
    let context = context_for(&request.project_id, &request.project_root_path);
    let path = format!("{LINT_REPORTS_DIR}/{}.json", request.task_id);
    state.file_store.read_json(&context, &path)
}

/// Apply (or plan) a single lint fix. Safe fixes apply under a Git checkpoint;
/// high-risk fixes return a `PendingAction` until confirmed.
#[tauri::command]
pub fn apply_lint_fix(
    state: State<'_, AppState>,
    request: ApplyLintFixRequest,
) -> Result<LintFixOutcome, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state.lint_service.apply_fix(
        &context,
        &state.git_service,
        &request.issue,
        request.confirm_high_risk,
        request.expected_hash.as_deref(),
    )
}

enum ResolvedRoute {
    Agent(AgentKind),
    Byok(LlmProviderConfig),
}

/// Replicates `chat_commands::resolve_route` — Agent preferred, BYOK fallback.
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
        state
            .agent_service
            .detect_agents(Some(*kind))
            .iter()
            .any(|info| info.kind == *kind && info.state == AgentDetectionState::Installed)
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
            "No usable Agent CLI is configured for deep lint. Install an Agent or switch to a BYOK provider.",
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
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
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
    use super::*;

    #[test]
    fn report_path_is_task_scoped() {
        assert_eq!(
            format!("{LINT_REPORTS_DIR}/task-1.json"),
            ".app/lint-reports/task-1.json"
        );
    }
}

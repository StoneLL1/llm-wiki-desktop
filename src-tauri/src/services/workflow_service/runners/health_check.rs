use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::errors::BackendError;
use crate::models::agent::AgentDetectionState;
use crate::models::lint::{
    Fixability, HealthCheckCoverage, HealthCheckReport, LintIssue, LintIssueSource, LintIssueType,
    LintSeverity,
};
use crate::models::paths::ProjectContext;
use crate::models::task::TaskStatus;
use crate::models::workflow::{
    HealthCheckMode, WorkflowErrorSummary, WorkflowKind, WorkflowPrerequisiteAction,
    WorkflowResult, WorkflowRoute, WorkflowRun, WorkflowScope,
};
use crate::services::{
    health_source_paths, AgentService, LintService, LlmService, LocalLintPhase, SearchService,
    SecretService, SettingsService,
};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

use super::super::{
    fingerprint::{canonical_json, hex_sha256},
    preparation::workflow_baseline_for_scope,
    WorkflowCoordinator, WorkflowRunner, WorkflowStageSink,
};

const READ_MARKDOWN: &str = "read_markdown";
const CHECK_MARKDOWN: &str = "check_markdown";
const CHECK_LINKS: &str = "check_links";
const DEEP_CHECK: &str = "deep_check";
const MERGE_FINDINGS: &str = "merge_findings";
const CLASSIFY_FINDINGS: &str = "classify_findings";
const WRITE_REPORT: &str = "write_report";
const COMPLETE: &str = "complete";

type StartCallback = dyn Fn(WorkflowRun) + Send + Sync;

pub struct HealthCheckRunner {
    start_callback: Arc<StartCallback>,
}

impl HealthCheckRunner {
    pub fn new(callback: impl Fn(WorkflowRun) + Send + Sync + 'static) -> Self {
        Self {
            start_callback: Arc::new(callback),
        }
    }
}

impl WorkflowRunner for HealthCheckRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::HealthCheck
    }

    fn start(&self, run: WorkflowRun) {
        (self.start_callback)(run);
    }
}

pub struct HealthCheckExecutionServices<'a> {
    pub lint_service: &'a LintService,
    pub search_service: &'a SearchService,
    pub settings_service: &'a SettingsService,
    pub secret_service: &'a SecretService,
    pub agent_service: &'a AgentService,
    pub llm_service: &'a LlmService,
    pub task_service: &'a TaskService,
    pub coordinator: &'a WorkflowCoordinator,
}

pub async fn run_health_check(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
) -> Option<WorkflowRun> {
    let task_id = run.task_id.clone();
    run_health_check_with_deep(context, run, services, move |prompt, route| async move {
        execute_prepared_deep_route(context, services, &task_id, &route, prompt).await
    })
    .await
}

/// Testable core for the composed runner. The injected function represents
/// exactly one already-prepared route; it cannot select or fall back to a
/// different engine.
pub async fn run_health_check_with_deep<F, Fut>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    deep_check: F,
) -> Option<WorkflowRun>
where
    F: FnOnce(String, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
{
    match execute_health_check(context, &run, services, deep_check).await {
        Ok(next) => next,
        Err(error) => finish_error(&run, services, error),
    }
}

async fn execute_health_check<F, Fut>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    deep_check: F,
) -> Result<Option<WorkflowRun>, BackendError>
where
    F: FnOnce(String, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
{
    let started = Instant::now();
    let task_id = run.task_id.as_str();
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, task_id);
    let mode = health_mode(run)?;

    sink.start(READ_MARKDOWN).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    let baseline = workflow_baseline_for_scope(context, &run.scope)?;
    if baseline.fingerprint != run.baseline_fingerprint {
        return Err(baseline_changed());
    }
    let tree = services
        .search_service
        .scan_wiki(context, &std::collections::HashSet::new())?;
    let raw_source_paths = health_source_paths(context)?;
    let wiki_source_pages = tree
        .pages
        .iter()
        .filter(|page| page.path.starts_with("wiki/sources/"))
        .count();
    let source_pages = wiki_source_pages + raw_source_paths.len();
    let wiki_pages = tree.pages.len().saturating_sub(wiki_source_pages);
    let mut not_applicable_rules = Vec::new();
    if wiki_pages == 0 && source_pages > 0 {
        not_applicable_rules.push("index_drift".to_string());
    }
    sink.progress(
        READ_MARKDOWN,
        tree.pages.first().map(|page| page.path.clone()),
        (tree.pages.len() + raw_source_paths.len()) as u64,
        Some((tree.pages.len() + raw_source_paths.len()) as u64),
    )
    .map_err(task_error)?;
    sink.complete(READ_MARKDOWN).map_err(task_error)?;

    sink.start(CHECK_MARKDOWN).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    let local_report = services.lint_service.run_health_local_lint_with_phase(
        context,
        services.search_service,
        |phase| match phase {
            LocalLintPhase::MarkdownComplete => {
                sink.progress(
                    CHECK_MARKDOWN,
                    None,
                    (tree.pages.len() + raw_source_paths.len()) as u64,
                    Some((tree.pages.len() + raw_source_paths.len()) as u64),
                )
                .map_err(task_error)?;
                sink.complete(CHECK_MARKDOWN).map_err(task_error)?;
                sink.start(CHECK_LINKS).map_err(task_error)?;
                ensure_not_cancelled(services.task_service, task_id)
            }
        },
    )?;
    sink.progress(
        CHECK_LINKS,
        local_report
            .issues
            .iter()
            .find(|issue| is_link_rule(issue.issue_type))
            .map(|issue| issue.path.clone()),
        local_report.scanned_pages as u64,
        Some(local_report.scanned_pages as u64),
    )
    .map_err(task_error)?;
    sink.complete(CHECK_LINKS).map_err(task_error)?;

    let mut deep_issues = Vec::new();
    let mut deep_covered_pages = None;
    let mut deep_truncated = false;
    match mode {
        HealthCheckMode::LocalQuick => {
            validate_local_route(run.route.as_ref())?;
            sink.skip(DEEP_CHECK).map_err(task_error)?;
        }
        HealthCheckMode::Complete => {
            sink.start(DEEP_CHECK).map_err(task_error)?;
            ensure_not_cancelled(services.task_service, task_id)?;
            let route = run.route.clone().ok_or_else(route_unavailable)?;
            if matches!(route, WorkflowRoute::Local { .. }) {
                return Err(route_unavailable());
            }
            validate_prepared_route(context, services, &route)?;
            let language = services
                .settings_service
                .read_settings(context)
                .map(|settings| settings.language)
                .unwrap_or_else(|_| "en".into());
            let snapshot = services
                .lint_service
                .prepare_health_deep_lint_snapshot(
                    context,
                    services.search_service,
                    &language,
                    &local_report,
                )
                .map_err(map_deep_snapshot_error)?;
            deep_covered_pages = Some(snapshot.deep_covered_pages);
            deep_truncated = snapshot.deep_truncated;
            services
                .lint_service
                .verify_deep_lint_snapshot(context, services.search_service, &snapshot)
                .map_err(map_deep_snapshot_error)?;
            if workflow_baseline_for_scope(context, &run.scope)?.fingerprint
                != run.baseline_fingerprint
            {
                return Err(baseline_changed());
            }
            let raw = deep_check(snapshot.prompt.clone(), route).await?;
            ensure_not_cancelled(services.task_service, task_id)?;
            deep_issues = services
                .lint_service
                .finish_deep_lint_snapshot(context, services.search_service, &snapshot, &raw, false)
                .map_err(map_deep_snapshot_error)?;
            sink.progress(
                DEEP_CHECK,
                deep_issues.first().map(|issue| issue.path.clone()),
                deep_issues.len() as u64,
                Some(tree.pages.len() as u64),
            )
            .map_err(task_error)?;
            sink.complete(DEEP_CHECK).map_err(task_error)?;
        }
    }

    sink.start(MERGE_FINDINGS).map_err(task_error)?;
    let (issues, finding_origins) = merge_findings(local_report.issues, deep_issues);
    sink.progress(
        MERGE_FINDINGS,
        issues.first().map(|issue| issue.path.clone()),
        issues.len() as u64,
        Some(issues.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(MERGE_FINDINGS).map_err(task_error)?;

    sink.start(CLASSIFY_FINDINGS).map_err(task_error)?;
    let (error_count, warning_count, info_count, findings_by_type) = classify(&issues);
    sink.progress(
        CLASSIFY_FINDINGS,
        None,
        issues.len() as u64,
        Some(issues.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(CLASSIFY_FINDINGS).map_err(task_error)?;

    sink.start(WRITE_REPORT).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    if workflow_baseline_for_scope(context, &run.scope)?.fingerprint != run.baseline_fingerprint {
        return Err(baseline_changed());
    }
    let persistent = services
        .task_service
        .workflow_persistence_dir(task_id)
        .is_some();
    let report = HealthCheckReport {
        report_id: task_id.to_string(),
        task_id: task_id.to_string(),
        mode: mode.clone(),
        route: run.route.clone().ok_or_else(route_unavailable)?,
        persistent,
        issues,
        finding_origins,
        coverage: HealthCheckCoverage {
            scanned_pages: local_report.scanned_pages,
            source_pages,
            wiki_pages,
            deep_covered_pages,
            deep_truncated,
            not_applicable_rules,
        },
        error_count,
        warning_count,
        info_count,
        findings_by_type,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        generated_at: crate::utils::time_utils::now_rfc3339(),
    };
    services
        .lint_service
        .store_health_check_report_guarded(context, &report, || {
            ensure_not_cancelled(services.task_service, task_id)?;
            if workflow_baseline_for_scope(context, &run.scope)?.fingerprint
                != run.baseline_fingerprint
            {
                return Err(baseline_changed());
            }
            Ok(())
        })?;
    sink.progress(WRITE_REPORT, Some(report.report_id.clone()), 1, Some(1))
        .map_err(task_error)?;
    sink.complete(WRITE_REPORT).map_err(task_error)?;

    sink.start(COMPLETE).map_err(task_error)?;
    sink.complete(COMPLETE).map_err(task_error)?;
    let (_, next) = sink
        .finish(WorkflowResult::HealthCheck {
            report_id: Some(report.report_id),
            persistent,
            error_count: error_count as u64,
            warning_count: warning_count as u64,
            info_count: info_count as u64,
        })
        .map_err(task_error)?;
    Ok(next)
}

async fn execute_prepared_deep_route(
    context: &ProjectContext,
    services: &HealthCheckExecutionServices<'_>,
    task_id: &str,
    route: &WorkflowRoute,
    prompt: String,
) -> Result<String, BackendError> {
    match route {
        WorkflowRoute::Local { .. } => Err(route_unavailable()),
        WorkflowRoute::Agent { agent, .. } => {
            if !AgentService::supports_lint_agent(*agent) {
                return Err(route_unavailable());
            }
            let settings = services.settings_service.read_settings(context)?;
            let info = services
                .agent_service
                .detect_agent(*agent, settings.agent_default == Some(*agent));
            if info.state != AgentDetectionState::Installed {
                return Err(route_unavailable());
            }
            let workspace = create_lint_workspace(task_id)?;
            let _guard = WorkspaceGuard(workspace.clone());
            let invocation = AgentService::lint_invocation(*agent, &workspace, &prompt)?;
            services
                .agent_service
                .run_lint_streaming(&invocation, services.task_service, task_id)
        }
        WorkflowRoute::Byok {
            provider, model, ..
        } => {
            let config = services
                .settings_service
                .read_settings(context)?
                .llm_providers
                .into_iter()
                .find(|candidate| {
                    candidate.enabled
                        && candidate.provider == *provider
                        && candidate.model == *model
                })
                .ok_or_else(route_unavailable)?;
            let secret = services.secret_service.get(*provider)?;
            if provider.requires_secret() && secret.is_none() {
                return Err(route_unavailable());
            }
            let completion = services
                .llm_service
                .complete(&config, secret.as_deref(), &prompt);
            crate::tasks::byok_progress::poll_with_progress(
                services.task_service,
                task_id,
                "Checking",
                completion,
            )
            .await
            .map_err(|_| {
                crate::tasks::byok_progress::cancelled_error(
                    "WORKFLOW_CANCELLED",
                    "Health Check was cancelled.",
                )
            })?
        }
    }
}

fn validate_prepared_route(
    context: &ProjectContext,
    services: &HealthCheckExecutionServices<'_>,
    route: &WorkflowRoute,
) -> Result<(), BackendError> {
    match route {
        WorkflowRoute::Local { route_revision } => (route_revision == "local-v1")
            .then_some(())
            .ok_or_else(route_unavailable),
        WorkflowRoute::Agent {
            agent,
            route_revision,
            ..
        } => {
            if !AgentService::supports_lint_agent(*agent) {
                return Err(route_unavailable());
            }
            let settings = services.settings_service.read_settings(context)?;
            let info = services
                .agent_service
                .detect_agent(*agent, settings.agent_default == Some(*agent));
            let revision =
                canonical_json(&(agent, &info.state, &info.version, &info.executable_path))
                    .map(|value| hex_sha256(value.as_bytes()))
                    .map_err(|_| route_unavailable())?;
            (info.state == AgentDetectionState::Installed && revision == *route_revision)
                .then_some(())
                .ok_or_else(route_unavailable)
        }
        WorkflowRoute::Byok {
            provider,
            model,
            route_revision,
        } => {
            let settings = services.settings_service.read_settings(context)?;
            let config = settings
                .llm_providers
                .into_iter()
                .find(|candidate| candidate.provider == *provider && candidate.model == *model)
                .ok_or_else(route_unavailable)?;
            let configured_secret =
                !provider.requires_secret() || services.secret_service.get(*provider)?.is_some();
            let available = config.enabled
                && !config.model.trim().is_empty()
                && {
                    let url = config.base_url.trim().to_ascii_lowercase();
                    url.starts_with("https://") || url.starts_with("http://")
                }
                && configured_secret;
            let revision = canonical_json(&(
                config.provider,
                &config.model,
                &config.base_url,
                config.context_window,
                config.enabled,
                configured_secret,
            ))
            .map(|value| hex_sha256(value.as_bytes()))
            .map_err(|_| route_unavailable())?;
            (available && revision == *route_revision)
                .then_some(())
                .ok_or_else(route_unavailable)
        }
    }
}

fn health_mode(run: &WorkflowRun) -> Result<HealthCheckMode, BackendError> {
    match &run.scope {
        WorkflowScope::HealthCheck { mode } => Ok(mode.clone()),
        _ => Err(BackendError::new(
            "WORKFLOW_SCOPE_KIND_MISMATCH",
            "Health Check received a different workflow scope.",
            false,
            true,
        )),
    }
}

fn validate_local_route(route: Option<&WorkflowRoute>) -> Result<(), BackendError> {
    if matches!(route, Some(WorkflowRoute::Local { route_revision }) if route_revision == "local-v1")
    {
        Ok(())
    } else {
        Err(route_unavailable())
    }
}

fn merge_findings(
    local: Vec<LintIssue>,
    deep: Vec<LintIssue>,
) -> (Vec<LintIssue>, BTreeMap<String, Vec<LintIssueSource>>) {
    let mut merged: HashMap<String, (LintIssue, Vec<LintIssueSource>)> = HashMap::new();
    for issue in local.into_iter().chain(deep) {
        let base_key = finding_identity(&issue);
        let key = match merged.get(&base_key) {
            Some((existing, origins))
                if origins.contains(&issue.source) && !same_finding(existing, &issue) =>
            {
                format!("{base_key}|{}", issue.id)
            }
            _ => base_key,
        };
        match merged.get_mut(&key) {
            Some((existing, origins)) => {
                if !origins.contains(&issue.source) {
                    origins.push(issue.source);
                }
                if severity_rank(issue.severity) < severity_rank(existing.severity) {
                    existing.severity = issue.severity;
                    existing.message = issue.message.clone();
                }
                existing.evidence = merge_text(existing.evidence.take(), issue.evidence);
                existing.suggested_action =
                    merge_text(existing.suggested_action.take(), issue.suggested_action);
                if fixability_rank(issue.fixability) > fixability_rank(existing.fixability) {
                    existing.fixability = issue.fixability;
                    existing.scan_hash = issue.scan_hash;
                }
            }
            None => {
                merged.insert(key, (issue.clone(), vec![issue.source]));
            }
        }
    }
    let mut entries = merged.into_values().collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| {
        severity_rank(left.severity)
            .cmp(&severity_rank(right.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut origins = BTreeMap::new();
    let issues = entries
        .into_iter()
        .map(|(issue, mut issue_origins)| {
            issue_origins.sort_by_key(|source| match source {
                LintIssueSource::Local => 0,
                LintIssueSource::Agent => 1,
            });
            origins.insert(issue.id.clone(), issue_origins);
            issue
        })
        .collect();
    (issues, origins)
}

fn same_finding(left: &LintIssue, right: &LintIssue) -> bool {
    left.message.trim() == right.message.trim()
        && left.evidence.as_deref().map(str::trim) == right.evidence.as_deref().map(str::trim)
        && left.suggested_action.as_deref().map(str::trim)
            == right.suggested_action.as_deref().map(str::trim)
}

fn finding_identity(issue: &LintIssue) -> String {
    let issue_type = serde_json::to_string(&issue.issue_type).unwrap_or_default();
    let range = issue
        .range
        .as_ref()
        .map(|range| format!("{}:{}", range.line, range.column.unwrap_or(0)))
        .unwrap_or_default();
    format!(
        "{}|{}|{}|{}",
        issue_type,
        issue.path,
        issue.target.as_deref().unwrap_or_default(),
        range
    )
}

fn classify(issues: &[LintIssue]) -> (usize, usize, usize, BTreeMap<String, usize>) {
    let mut errors = 0;
    let mut warnings = 0;
    let mut infos = 0;
    let mut by_type = BTreeMap::new();
    for issue in issues {
        match issue.severity {
            LintSeverity::Error => errors += 1,
            LintSeverity::Warning => warnings += 1,
            LintSeverity::Info => infos += 1,
        }
        let key = serde_json::to_value(issue.issue_type)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".into());
        *by_type.entry(key).or_insert(0) += 1;
    }
    (errors, warnings, infos, by_type)
}

fn is_link_rule(issue_type: LintIssueType) -> bool {
    matches!(
        issue_type,
        LintIssueType::DeadLink | LintIssueType::OrphanPage | LintIssueType::IndexDrift
    )
}

fn severity_rank(severity: LintSeverity) -> u8 {
    match severity {
        LintSeverity::Error => 0,
        LintSeverity::Warning => 1,
        LintSeverity::Info => 2,
    }
}

fn fixability_rank(fixability: Fixability) -> u8 {
    match fixability {
        Fixability::None => 0,
        Fixability::Safe => 1,
        Fixability::HighRisk => 2,
    }
}

fn merge_text(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) if left.trim() != right.trim() => {
            Some(format!("{}\n\n{}", left.trim(), right.trim()))
        }
        (Some(left), _) => Some(left),
        (_, Some(right)) => Some(right),
        _ => None,
    }
}

fn ensure_not_cancelled(tasks: &TaskService, task_id: &str) -> Result<(), BackendError> {
    if tasks.is_cancelled(task_id)
        || tasks.get_task(task_id).is_some_and(|task| {
            matches!(task.status, TaskStatus::Cancelling | TaskStatus::Cancelled)
        })
    {
        Err(BackendError::new(
            "WORKFLOW_CANCELLED",
            "Health Check was cancelled.",
            true,
            false,
        ))
    } else {
        Ok(())
    }
}

fn baseline_changed() -> BackendError {
    BackendError::new(
        "WORKFLOW_INPUT_BASELINE_CHANGED",
        "Readable Markdown changed while Health Check was running. Prepare and run again.",
        true,
        true,
    )
}

fn map_deep_snapshot_error(error: BackendError) -> BackendError {
    if error.code == "LINT_SCAN_CHANGED" {
        baseline_changed()
    } else {
        error
    }
}

fn route_unavailable() -> BackendError {
    BackendError::new(
        "WORKFLOW_ROUTE_UNAVAILABLE",
        "The prepared Health Check route is no longer available. Review Settings and retry.",
        true,
        true,
    )
}

fn finish_error(
    run: &WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    error: BackendError,
) -> Option<WorkflowRun> {
    let _ = services
        .task_service
        .append_log(&run.task_id, LogLevel::Error, error.message.clone());
    let cancelled = services.task_service.is_cancelled(&run.task_id)
        || services
            .task_service
            .get_task(&run.task_id)
            .is_some_and(|task| {
                matches!(task.status, TaskStatus::Cancelling | TaskStatus::Cancelled)
            });
    let outcome = if cancelled {
        services
            .coordinator
            .finish_cancelled_and_claim_next(services.task_service, &run.task_id)
    } else {
        let _ = services.task_service.set_error(&run.task_id, error.clone());
        let sink =
            WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);
        let refreshed = services.task_service.get_workflow_run(&run.task_id);
        let current = refreshed
            .as_ref()
            .and_then(|run| {
                run.stages
                    .iter()
                    .find(|stage| {
                        stage.status == crate::models::workflow::WorkflowStageStatus::Running
                    })
                    .or_else(|| {
                        run.stages.iter().find(|stage| {
                            stage.status == crate::models::workflow::WorkflowStageStatus::Pending
                        })
                    })
                    .map(|stage| stage.id.clone())
            })
            .unwrap_or_else(|| READ_MARKDOWN.into());
        if refreshed.as_ref().is_some_and(|run| {
            run.stages.iter().any(|stage| {
                stage.id == current
                    && stage.status == crate::models::workflow::WorkflowStageStatus::Pending
            })
        }) {
            let _ = sink.start(&current);
        }
        sink.fail(
            &current,
            WorkflowErrorSummary {
                code: error.code.clone(),
                message_key: if error.code.contains("BASELINE")
                    || error.code.contains("SCAN_CHANGED")
                {
                    "workflows.error.prepareAgain".into()
                } else if error.code.contains("ROUTE")
                    || error.code.contains("PROVIDER")
                    || error.code.contains("AGENT")
                {
                    "workflows.error.configureExecutionRoute".into()
                } else {
                    "workflows.error.healthCheckFailed".into()
                },
                recoverable: error.recoverable,
                user_action_required: error.user_action_required,
                suggested_action: if error.code.contains("BASELINE")
                    || error.code.contains("SCAN_CHANGED")
                {
                    Some(WorkflowPrerequisiteAction::PrepareAgain)
                } else if error.code.contains("ROUTE")
                    || error.code.contains("PROVIDER")
                    || error.code.contains("AGENT")
                {
                    Some(WorkflowPrerequisiteAction::ConfigureExecutionRoute)
                } else {
                    None
                },
            },
        )
    };
    outcome.ok().and_then(|(_, next)| next)
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn create_lint_workspace(label: &str) -> Result<PathBuf, BackendError> {
    let workspace = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(format!("lint-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workspace).map_err(|error| {
        BackendError::new("LINT_WORKSPACE_FAILED", error.to_string(), true, false)
    })?;
    Ok(workspace)
}

struct WorkspaceGuard(PathBuf);

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

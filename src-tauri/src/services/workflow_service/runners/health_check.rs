use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::errors::BackendError;
use crate::models::agent::AgentDetectionState;
use crate::models::lint::{
    Fixability, HealthCheckCoverage, HealthCheckExecution, HealthCheckReport, HealthDeepStatus,
    HealthReportFreshness, LintIssue, LintIssueSource, LintSeverity,
};
use crate::models::llm::LlmProviderConfig;
use crate::models::paths::ProjectContext;
use crate::models::task::TaskStatus;
use crate::models::workflow::{
    HealthCheckMode, WorkflowErrorSummary, WorkflowHealthCoverageSummary, WorkflowKind,
    WorkflowPrerequisiteAction, WorkflowProjectMutationState, WorkflowResult, WorkflowRoute,
    WorkflowRun, WorkflowScope,
};
use crate::services::{
    AgentService, DeepLintSnapshot, HealthScanPhase, LintService, LlmService, SearchService,
    SecretService, SettingsService,
};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

use super::super::{
    fingerprint::{canonical_json, hex_sha256},
    preparation::workflow_baseline_for_scope,
    WorkflowCoordinator, WorkflowExternalLaunchPermit, WorkflowRunner, WorkflowStageSink,
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
    let external_permit = WorkflowExternalLaunchPermit::prevalidated(&run);
    let report_run = run.clone();
    run_health_check_authorized(
        context,
        run,
        services,
        || Ok(external_permit),
        move || {
            Ok(services
                .task_service
                .workflow_persistence_dir(&report_run.task_id)
                .is_some()
                .then(|| WorkflowExternalLaunchPermit::prevalidated(&report_run)))
        },
    )
    .await
}

pub async fn run_health_check_authorized<F, P>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    authorize_external_launch: F,
    authorize_report_write: P,
) -> Option<WorkflowRun>
where
    F: FnOnce() -> Result<WorkflowExternalLaunchPermit, BackendError>,
    P: FnMut() -> Result<Option<WorkflowExternalLaunchPermit>, BackendError>,
{
    let task_id = run.task_id.clone();
    let launch_run = run.clone();
    run_health_check_with_deep_and_report_authority(
        context,
        run,
        services,
        move |snapshot, route| async move {
            let publication = authorize_external_launch()?.begin()?;
            execute_prepared_deep_route(
                context,
                services,
                &task_id,
                &route,
                &snapshot,
                &launch_run.scope,
                &launch_run.baseline_fingerprint,
                publication,
            )
            .await
        },
        authorize_report_write,
    )
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
    F: FnOnce(DeepLintSnapshot, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
{
    let report_run = run.clone();
    run_health_check_with_deep_and_report_authority(context, run, services, deep_check, move || {
        Ok(services
            .task_service
            .workflow_persistence_dir(&report_run.task_id)
            .is_some()
            .then(|| WorkflowExternalLaunchPermit::prevalidated(&report_run)))
    })
    .await
}

pub async fn run_health_check_with_deep_and_report_authority<F, Fut, P>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    deep_check: F,
    report_authority: P,
) -> Option<WorkflowRun>
where
    F: FnOnce(DeepLintSnapshot, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
    P: FnMut() -> Result<Option<WorkflowExternalLaunchPermit>, BackendError>,
{
    match execute_health_check(context, &run, services, deep_check, report_authority).await {
        Ok(next) => next,
        Err(error) => finish_error(context, &run, services, error),
    }
}

async fn execute_health_check<F, Fut, P>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    deep_check: F,
    mut report_authority: P,
) -> Result<Option<WorkflowRun>, BackendError>
where
    F: FnOnce(DeepLintSnapshot, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
    P: FnMut() -> Result<Option<WorkflowExternalLaunchPermit>, BackendError>,
{
    let started = Instant::now();
    let task_id = run.task_id.as_str();
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, task_id);
    let mode = health_mode(run)?;

    sink.start(READ_MARKDOWN).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    if mode == HealthCheckMode::LocalQuick {
        validate_local_route(run.route.as_ref())?;
    }
    // Inventory, content and deterministic rules belong to one execution-time
    // read pass. An admission fingerprint never freezes a local read request.
    sink.complete(READ_MARKDOWN).map_err(task_error)?;
    sink.start(CHECK_MARKDOWN).map_err(task_error)?;
    let mut checking_links = false;
    let scan = services.lint_service.run_health_local_scan(
        context,
        services.search_service,
        |progress| {
            ensure_not_cancelled(services.task_service, task_id)?;
            if progress.phase != HealthScanPhase::Markdown && !checking_links {
                sink.complete(CHECK_MARKDOWN).map_err(task_error)?;
                sink.start(CHECK_LINKS).map_err(task_error)?;
                checking_links = true;
            }
            // Cancellation is observed for every page. Publish count updates
            // only at real batch edges; verification must not reset link counts.
            if progress.phase == HealthScanPhase::Verify
                || (progress.completed % 16 != 0 && progress.completed != progress.total)
            {
                return Ok(());
            }
            sink.progress(
                if checking_links {
                    CHECK_LINKS
                } else {
                    CHECK_MARKDOWN
                },
                progress.path.clone(),
                progress.completed as u64,
                Some(progress.total as u64),
            )
            .map_err(task_error)?;
            Ok(())
        },
    )?;
    if !checking_links {
        sink.complete(CHECK_MARKDOWN).map_err(task_error)?;
        sink.start(CHECK_LINKS).map_err(task_error)?;
    }
    sink.complete(CHECK_LINKS).map_err(task_error)?;
    let (issues, finding_origins) = merge_findings(scan.report.issues.clone(), Vec::new());
    let (error_count, warning_count, info_count, findings_by_type) = classify(&issues);
    let mut report = HealthCheckReport {
        execution: Some(HealthCheckExecution {
            input_fingerprint: scan.input_fingerprint.clone(),
            input_hashes: scan.input_hashes.clone(),
            scanned_at: scan.scanned_at.clone(),
            freshness: if scan.current {
                HealthReportFreshness::Current
            } else {
                HealthReportFreshness::Stale
            },
            deep_status: if mode == HealthCheckMode::LocalQuick {
                HealthDeepStatus::NotRequested
            } else {
                HealthDeepStatus::Pending
            },
            deep_error_code: None,
        }),
        report_id: task_id.to_string(),
        task_id: task_id.to_string(),
        mode: mode.clone(),
        route: run.route.clone().ok_or_else(route_unavailable)?,
        persistent: services
            .task_service
            .workflow_persistence_dir(task_id)
            .is_some(),
        issues,
        finding_origins,
        coverage: HealthCheckCoverage {
            scanned_pages: scan.report.scanned_pages,
            source_pages: scan.source_pages,
            wiki_pages: scan.wiki_pages,
            deep_covered_pages: None,
            deep_truncated: false,
            not_applicable_rules: scan.not_applicable_rules.clone(),
        },
        error_count,
        warning_count,
        info_count,
        findings_by_type,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        generated_at: crate::utils::time_utils::now_rfc3339(),
    };
    let mut deep_issues = Vec::new();
    if mode == HealthCheckMode::LocalQuick {
        sink.skip(DEEP_CHECK).map_err(task_error)?;
    } else {
        // Save the local portion before any external work. It remains in Lint
        // history even if the process or external route stops here.
        store_report(context, run, services, &mut report, &mut report_authority)?;
        sink.start(DEEP_CHECK).map_err(task_error)?;
        if workflow_baseline_for_scope(context, &run.scope)?.fingerprint != run.baseline_fingerprint
        {
            services
                .task_service
                .wait_workflow_stage_with_result(
                    task_id,
                    DEEP_CHECK,
                    crate::models::workflow::WorkflowPendingAction {
                        id: uuid::Uuid::new_v4().to_string(),
                        action_type: crate::models::confirmation::PendingActionType::ReviewScope,
                        risk_level: crate::models::confirmation::RiskLevel::Low,
                        affected_paths: Vec::new(),
                        candidate: None,
                        expires_at: None,
                        checkpoint_hash: None,
                    },
                    Some(health_result(&report)?),
                )
                .map_err(task_error)?;
            return Ok(None);
        }
        let deep_result = async {
            ensure_not_cancelled(services.task_service, task_id)?;
            let route = run.route.clone().ok_or_else(route_unavailable)?;
            validate_prepared_route(context, services, &route)?;
            let language = services
                .settings_service
                .read_settings(context)
                .map(|settings| settings.language)
                .unwrap_or_else(|_| "en".into());
            let snapshot = services
                .lint_service
                .prepare_health_deep_snapshot_from_scan(&scan, &language);
            report.coverage.deep_truncated = snapshot.deep_truncated;
            services
                .lint_service
                .verify_deep_lint_snapshot(context, services.search_service, &snapshot)
                .map_err(map_deep_snapshot_error)?;
            let raw = deep_check(snapshot.clone(), route).await?;
            ensure_not_cancelled(services.task_service, task_id)?;
            let issues = services
                .lint_service
                .finish_deep_lint_snapshot(context, services.search_service, &snapshot, &raw, false)
                .map_err(map_deep_snapshot_error)?;
            report.coverage.deep_covered_pages = Some(snapshot.deep_covered_pages);
            Ok::<_, BackendError>(issues)
        }
        .await;
        match deep_result {
            Ok(issues) => {
                deep_issues = issues;
                report.execution.as_mut().unwrap().deep_status = HealthDeepStatus::Completed;
                sink.complete(DEEP_CHECK).map_err(task_error)?;
            }
            Err(error) => {
                ensure_not_cancelled(services.task_service, task_id)?;
                let execution = report.execution.as_mut().unwrap();
                execution.deep_status = HealthDeepStatus::Failed;
                execution.deep_error_code = Some(error.code.clone());
                refresh_report_freshness(
                    context,
                    services.lint_service,
                    &mut report,
                    services.task_service,
                    task_id,
                )?;
                report.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                store_report(context, run, services, &mut report, &mut report_authority)?;
                services
                    .task_service
                    .append_log(task_id, LogLevel::Error, error.message.clone())
                    .map_err(task_error)?;
                services
                    .task_service
                    .set_error(task_id, error.clone())
                    .map_err(task_error)?;
                let (_, next) = sink
                    .fail_with_result(
                        DEEP_CHECK,
                        health_error_summary(&error),
                        health_result(&report)?,
                    )
                    .map_err(task_error)?;
                return Ok(next);
            }
        }
    }
    sink.start(MERGE_FINDINGS).map_err(task_error)?;
    (report.issues, report.finding_origins) = merge_findings(report.issues, deep_issues);
    sink.complete(MERGE_FINDINGS).map_err(task_error)?;
    sink.start(CLASSIFY_FINDINGS).map_err(task_error)?;
    (
        report.error_count,
        report.warning_count,
        report.info_count,
        report.findings_by_type,
    ) = classify(&report.issues);
    sink.complete(CLASSIFY_FINDINGS).map_err(task_error)?;
    sink.start(WRITE_REPORT).map_err(task_error)?;
    refresh_report_freshness(
        context,
        services.lint_service,
        &mut report,
        services.task_service,
        task_id,
    )?;
    report.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    store_report(context, run, services, &mut report, &mut report_authority)?;
    sink.progress(WRITE_REPORT, Some(report.report_id.clone()), 1, Some(1))
        .map_err(task_error)?;
    sink.complete(WRITE_REPORT).map_err(task_error)?;
    sink.start(COMPLETE).map_err(task_error)?;
    sink.complete(COMPLETE).map_err(task_error)?;
    let (_, next) = sink.finish(health_result(&report)?).map_err(task_error)?;
    Ok(next)
}

fn refresh_report_freshness(
    context: &ProjectContext,
    lint: &LintService,
    report: &mut HealthCheckReport,
    tasks: &TaskService,
    task_id: &str,
) -> Result<(), BackendError> {
    let execution = report
        .execution
        .as_mut()
        .expect("new report has execution evidence");
    let verification = lint.verify_health_inputs(context, &execution.input_hashes, |_| {
        ensure_not_cancelled(tasks, task_id)
    });
    execution.freshness = match verification {
        Ok(true) => HealthReportFreshness::Current,
        Ok(false) => HealthReportFreshness::Stale,
        Err(error) if error.code == "WORKFLOW_CANCELLED" => return Err(error),
        Err(_) => HealthReportFreshness::Unknown,
    };
    Ok(())
}

fn store_report<P>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &HealthCheckExecutionServices<'_>,
    report: &mut HealthCheckReport,
    report_authority: &mut P,
) -> Result<(), BackendError>
where
    P: FnMut() -> Result<Option<WorkflowExternalLaunchPermit>, BackendError>,
{
    let task_id = run.task_id.as_str();
    loop {
        ensure_not_cancelled(services.task_service, task_id)?;
        let publication = report_authority()?;
        report.persistent = publication.is_some();
        let publication = publication
            .map(WorkflowExternalLaunchPermit::begin)
            .transpose()?;
        let persistent = report.persistent;
        let stored =
            services
                .lint_service
                .store_health_check_report_guarded(context, report, || {
                    ensure_not_cancelled(services.task_service, task_id)?;
                    if services
                        .task_service
                        .workflow_persistence_dir(task_id)
                        .is_some()
                        != persistent
                    {
                        return Err(BackendError::new(
                            "WORKFLOW_PERSISTENCE_CHANGED",
                            "Report persistence authority changed.",
                            true,
                            true,
                        ));
                    }
                    Ok(())
                });
        match stored {
            Ok(_) => {
                if let Some(publication) = publication {
                    publication.started();
                }
                return Ok(());
            }
            Err(error) if error.code == "WORKFLOW_PERSISTENCE_CHANGED" => continue,
            Err(error) => return Err(error),
        }
    }
}

fn health_result(report: &HealthCheckReport) -> Result<WorkflowResult, BackendError> {
    Ok(WorkflowResult::HealthCheck {
        report_id: Some(report.report_id.clone()),
        persistent: report.persistent,
        report_digest: Some(LintService::health_check_report_digest(report)?),
        error_count: report.error_count as u64,
        warning_count: report.warning_count as u64,
        info_count: report.info_count as u64,
        coverage: Some(WorkflowHealthCoverageSummary {
            deep_status: report
                .execution
                .as_ref()
                .map(|execution| execution.deep_status),
            mode: report.mode.clone(),
            scanned_pages: report.coverage.scanned_pages as u64,
            deep_covered_pages: report.coverage.deep_covered_pages.map(|count| count as u64),
            deep_truncated: report.coverage.deep_truncated,
        }),
        findings_by_type: report
            .findings_by_type
            .iter()
            .map(|(kind, count)| (kind.clone(), *count as u64))
            .collect(),
    })
}

async fn execute_prepared_deep_route(
    context: &ProjectContext,
    services: &HealthCheckExecutionServices<'_>,
    task_id: &str,
    route: &WorkflowRoute,
    snapshot: &DeepLintSnapshot,
    scope: &WorkflowScope,
    baseline_fingerprint: &str,
    publication: super::super::WorkflowLaunchPublication,
) -> Result<String, BackendError> {
    // Revalidate at the actual launch boundary as well as before snapshot
    // construction. Agent/provider version, executable, profile, model, URL,
    // or secret drift during the local pass must yield invocation zero.
    match route {
        WorkflowRoute::Local { .. } => Err(route_unavailable()),
        WorkflowRoute::Agent {
            agent,
            route_revision,
            ..
        } => {
            let workspace = create_lint_workspace(task_id)?;
            let _guard = WorkspaceGuard(workspace.clone());
            let settings = services.settings_service.read_settings(context)?;
            let prepared = services
                .agent_service
                .prepare_lint_analysis(
                    *agent,
                    settings.agent_default == Some(*agent),
                    &workspace,
                    &snapshot.prompt,
                )
                .map_err(|_| route_unavailable())?;
            validate_agent_route_revision(
                *agent,
                prepared.info(),
                prepared.target_revision(),
                route_revision,
            )?;
            validate_launch_snapshot(context, services, snapshot, scope, baseline_fingerprint)?;
            let result = services.agent_service.run_prepared_lint_streaming(
                &prepared,
                services.task_service,
                task_id,
            );
            publication.started();
            result
        }
        WorkflowRoute::Byok { .. } => {
            let PreparedDeepRoute::Byok { config, secret } =
                validate_prepared_route(context, services, route)?
            else {
                return Err(route_unavailable());
            };
            if services.task_service.is_cancelled(task_id) {
                return Err(crate::tasks::byok_progress::cancelled_error(
                    "WORKFLOW_CANCELLED",
                    "Health Check was cancelled.",
                ));
            }
            validate_launch_snapshot(context, services, snapshot, scope, baseline_fingerprint)?;
            let completion =
                services
                    .llm_service
                    .complete(&config, secret.as_deref(), &snapshot.prompt);
            let result = crate::tasks::byok_progress::poll_with_progress(
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
            });
            publication.started();
            result?
        }
    }
}

fn validate_launch_snapshot(
    context: &ProjectContext,
    services: &HealthCheckExecutionServices<'_>,
    snapshot: &DeepLintSnapshot,
    scope: &WorkflowScope,
    baseline_fingerprint: &str,
) -> Result<(), BackendError> {
    services
        .lint_service
        .verify_deep_lint_snapshot(context, services.search_service, snapshot)
        .map_err(map_deep_snapshot_error)?;
    if workflow_baseline_for_scope(context, scope)?.fingerprint != baseline_fingerprint {
        return Err(baseline_changed());
    }
    Ok(())
}

enum PreparedDeepRoute {
    Local,
    Agent,
    Byok {
        config: LlmProviderConfig,
        secret: Option<String>,
    },
}

fn validate_prepared_route(
    context: &ProjectContext,
    services: &HealthCheckExecutionServices<'_>,
    route: &WorkflowRoute,
) -> Result<PreparedDeepRoute, BackendError> {
    match route {
        WorkflowRoute::Local { route_revision } => (route_revision == "local-v1")
            .then_some(PreparedDeepRoute::Local)
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
            let (info, target_revision) = services
                .agent_service
                .lint_analysis_route_facts(*agent, settings.agent_default == Some(*agent))?;
            validate_agent_route_revision(*agent, &info, &target_revision, route_revision)?;
            Ok(PreparedDeepRoute::Agent)
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
            let binding = crate::services::LlmService::credential_binding(context, &config)?;
            let secret = crate::services::LlmService::bound_secret_for_config(
                context,
                services.secret_service,
                &config,
            )
            .map_err(|_| route_unavailable())?;
            let configured_secret = !provider.requires_secret() || secret.is_some();
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
                binding.as_ref().map(|binding| &binding.config_id),
                binding.as_ref().map(|binding| binding.revision),
            ))
            .map(|value| hex_sha256(value.as_bytes()))
            .map_err(|_| route_unavailable())?;
            (available && revision == *route_revision)
                .then_some(PreparedDeepRoute::Byok { config, secret })
                .ok_or_else(route_unavailable)
        }
    }
}

fn validate_agent_route_revision(
    agent: crate::models::agent::AgentKind,
    info: &crate::models::agent::AgentInfo,
    target_revision: &str,
    route_revision: &str,
) -> Result<(), BackendError> {
    let profile_revision =
        AgentService::lint_route_profile_revision(agent).ok_or_else(route_unavailable)?;
    let revision = canonical_json(&(
        agent,
        &info.state,
        &info.version,
        &info.executable_path,
        profile_revision,
        target_revision,
    ))
    .map(|value| hex_sha256(value.as_bytes()))
    .map_err(|_| route_unavailable())?;
    (info.state == AgentDetectionState::Installed && revision == route_revision)
        .then_some(())
        .ok_or_else(route_unavailable)
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
    context: &ProjectContext,
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
    let retained = services
        .lint_service
        .read_current_health_report(context, &run.task_id)
        .ok()
        .and_then(|report| health_result(&report).ok());
    let outcome = if cancelled {
        services
            .coordinator
            .finish_cancelled_and_claim_next_with_result(
                services.task_service,
                &run.task_id,
                retained,
            )
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
        match retained {
            Some(result) => sink.fail_with_result(&current, health_error_summary(&error), result),
            None => sink.fail(&current, health_error_summary(&error)),
        }
    };
    outcome.ok().and_then(|(_, next)| next)
}

fn health_error_summary(error: &BackendError) -> WorkflowErrorSummary {
    WorkflowErrorSummary {
        code: error.code.clone(),
        message_key: if error.code.contains("BASELINE") || error.code.contains("SCAN_CHANGED") {
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
        suggested_action: if error.code.contains("BASELINE") || error.code.contains("SCAN_CHANGED")
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
        project_mutation_state: WorkflowProjectMutationState::NotModified,
    }
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

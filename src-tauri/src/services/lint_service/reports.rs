use crate::app_state::ProjectWritePermit;
use crate::errors::BackendError;
use crate::models::compile::CompileRoutePreference;
use crate::models::lint::{
    DeepLintReport, HealthCheckReport, LintHistoryEntry, LintHistoryFile, LintIssue, LintReport,
    LintReportKind, LintSeverity, PersistedLintReport,
};
use crate::models::paths::ProjectContext;
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowKind, WorkflowOperation, WorkflowPersistenceMode,
    WorkflowResult, WorkflowRoute, WorkflowRun,
};
use crate::utils::safe_project_dir::remove_project_file;
use sha2::{Digest, Sha256};

use super::{LintService, LINT_REPORTS_DIR};

const LINT_HISTORY_PATH: &str = ".app/lint-history.json";
const LINT_HISTORY_LIMIT: usize = 50;
const LINT_MEMORY_PROJECT_LIMIT: usize = 64;

#[cfg(all(test, unix))]
thread_local! {
    static AFTER_MEMORY_NAMESPACE_TRIM: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(all(test, unix))]
fn set_after_memory_namespace_trim(hook: impl FnOnce() + 'static) {
    AFTER_MEMORY_NAMESPACE_TRIM.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_after_memory_namespace_trim() {
    #[cfg(all(test, unix))]
    AFTER_MEMORY_NAMESPACE_TRIM.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

impl LintService {
    pub fn health_check_report_digest(report: &HealthCheckReport) -> Result<String, BackendError> {
        let canonical = crate::services::canonical_json(report).map_err(|error| {
            BackendError::new(
                "LINT_REPORT_DIGEST_FAILED",
                format!("Could not canonicalize the Health report: {error}"),
                false,
                true,
            )
        })?;
        Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
    }

    fn persist_local_report_unchecked(
        &self,
        context: &ProjectContext,
        report: &LintReport,
    ) -> Result<LintHistoryEntry, BackendError> {
        let id = format!("local-{}", uuid::Uuid::new_v4());
        let entry = lint_history_entry_for_local(&id, report);
        let persisted = PersistedLintReport {
            entry: entry.clone(),
            local_report: Some(report.clone()),
            deep_report: None,
            health_check_report: None,
        };
        let _guard = self.metadata_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_METADATA_LOCKED",
                "Lint metadata is currently being updated by another operation.",
                true,
                true,
            )
        })?;
        self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
        self.file_store.write_json_atomic(
            context,
            &format!("{LINT_REPORTS_DIR}/{id}.json"),
            &persisted,
        )?;
        self.record_history_entry_locked(context, entry.clone())?;
        Ok(entry)
    }

    pub(crate) fn persist_local_report_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        report: &LintReport,
    ) -> Result<LintHistoryEntry, BackendError> {
        self.persist_local_report_unchecked(permit.context(), report)
    }

    /// Compatibility surface for integration and service tests. Production
    /// callers must enter through `persist_local_report_authorized`.
    #[cfg(debug_assertions)]
    pub fn persist_local_report(
        &self,
        context: &ProjectContext,
        report: &LintReport,
    ) -> Result<LintHistoryEntry, BackendError> {
        self.persist_local_report_unchecked(context, report)
    }

    #[cfg(debug_assertions)]
    pub fn persist_deep_report(
        &self,
        context: &ProjectContext,
        task_id: &str,
        route: CompileRoutePreference,
        report: &DeepLintReport,
    ) -> Result<LintHistoryEntry, BackendError> {
        let entry = lint_history_entry_for_deep(task_id, route, report);
        let persisted = PersistedLintReport {
            entry: entry.clone(),
            local_report: None,
            deep_report: Some(report.clone()),
            health_check_report: None,
        };
        let _guard = self.metadata_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_METADATA_LOCKED",
                "Lint metadata is currently being updated by another operation.",
                true,
                true,
            )
        })?;
        self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
        self.file_store.write_json_atomic(
            context,
            &format!("{LINT_REPORTS_DIR}/{task_id}.json"),
            &persisted,
        )?;
        self.record_history_entry_locked(context, entry.clone())?;
        Ok(entry)
    }

    pub fn list_lint_history(
        &self,
        context: &ProjectContext,
    ) -> Result<LintHistoryFile, BackendError> {
        let mut file = self.load_history(context)?;
        let project_key = self.memory_project_key(context)?;
        if let Ok(memory) = self.memory_reports.read() {
            if let Some(reports) = memory.get(&project_key) {
                file.entries
                    .extend(reports.values().map(|report| report.entry.clone()));
            }
        }
        file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        file.entries.dedup_by(|left, right| left.id == right.id);
        file.entries.truncate(LINT_HISTORY_LIMIT);
        Ok(file)
    }

    pub fn read_lint_history_report(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<PersistedLintReport, BackendError> {
        reject_report_id(id)?;
        let project_key = self.memory_project_key(context)?;
        if let Ok(memory) = self.memory_reports.read() {
            if let Some(report) = memory.get(&project_key).and_then(|reports| reports.get(id)) {
                return Ok(report.clone());
            }
        }
        let path = format!("{LINT_REPORTS_DIR}/{id}.json");
        match self
            .file_store
            .read_json::<PersistedLintReport>(context, &path)
        {
            Ok(report) => Ok(report),
            Err(wrapper_error) => {
                let legacy = self.file_store.read_json::<DeepLintReport>(context, &path);
                legacy
                    .map(|deep_report| PersistedLintReport {
                        entry: lint_history_entry_for_deep(
                            id,
                            CompileRoutePreference::Auto,
                            &deep_report,
                        ),
                        local_report: None,
                        deep_report: Some(deep_report),
                        health_check_report: None,
                    })
                    .map_err(|_| wrapper_error)
            }
        }
    }

    /// Return the current Health report owned by the existing workflow
    /// authority. Memory-only reports are process-local; persistent reports
    /// are read from the atomic report file so repair preparation still works
    /// after the workflow/report crossed a process boundary. The caller must
    /// continue through `validate_current_health_report_owner` before using
    /// the report for repair authorization.
    pub fn read_current_health_report(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<HealthCheckReport, BackendError> {
        reject_report_id(id)?;
        let project_key = self.memory_project_key(context)?;
        let memory_report = {
            let memory = self.memory_reports.read().map_err(|_| {
                BackendError::new(
                    "LINT_MEMORY_REPORTS_LOCKED",
                    "In-memory Lint reports are temporarily unavailable.",
                    true,
                    false,
                )
            })?;
            memory
                .get(&project_key)
                .and_then(|reports| reports.get(id))
                .and_then(|persisted| persisted.health_check_report.as_ref())
                .filter(|report| {
                    !report.persistent
                        && report.report_id == id
                        && report.task_id == id
                        && !report.report_id.is_empty()
                })
                .cloned()
        };
        if let Some(report) = memory_report {
            return Ok(report);
        }

        let path = format!("{LINT_REPORTS_DIR}/{id}.json");
        let persisted = self
            .file_store
            .read_json::<PersistedLintReport>(context, &path)
            .map_err(|_| current_health_report_required())?;
        persisted
            .health_check_report
            .as_ref()
            .filter(|report| is_current_persistent_health_report(&persisted, id, report))
            .cloned()
            .ok_or_else(current_health_report_required)
    }

    pub fn validate_current_health_report_owner(
        report: &HealthCheckReport,
        owner: &WorkflowRun,
        project_id: &str,
        canonical_identity_key: &str,
        identity_revision: &str,
        current_health_route: &WorkflowRoute,
        current_baseline_fingerprint: &str,
    ) -> Result<(), BackendError> {
        let report_digest = Self::health_check_report_digest(report)
            .map_err(|_| current_health_report_required())?;
        let exact_health_result = matches!(
            owner.result.as_ref(),
            Some(WorkflowResult::HealthCheck {
                report_id: Some(result_report_id),
                persistent,
                report_digest: Some(result_report_digest),
                ..
            }) if result_report_id == &report.report_id
                && *persistent == report.persistent
                && result_report_digest == &report_digest
        );
        let persistence_matches = match report.persistent {
            true => owner.persistence == WorkflowPersistenceMode::Persistent,
            false => owner.persistence == WorkflowPersistenceMode::MemoryOnly,
        };
        if report.task_id != owner.task_id
            || owner.project_id != project_id
            || owner.kind != WorkflowKind::HealthCheck
            || owner.operation != WorkflowOperation::BuiltIn
            || owner.display_status != WorkflowDisplayStatus::Completed
            || !persistence_matches
            || owner.route.as_ref() != Some(&report.route)
            || !exact_health_result
        {
            return Err(BackendError::new(
                "LINT_REPAIR_HEALTH_REPORT_REQUIRED",
                "The report is not owned by a completed built-in Health Workflow.",
                true,
                true,
            ));
        }
        if owner.canonical_identity_key != canonical_identity_key
            || owner.identity_revision != identity_revision
            || &report.route != current_health_route
            || owner.baseline_fingerprint != current_baseline_fingerprint
        {
            return Err(BackendError::new(
                "LINT_REPAIR_REPORT_STALE",
                "The Health report no longer matches the current project, route, or baseline.",
                true,
                true,
            ));
        }
        Ok(())
    }

    /// Save one composed Health Check report. Persistent runs use the existing
    /// atomic Lint report/history files; read-only/restricted runs remain
    /// process-local and never attempt to create `.app`.
    fn store_health_check_report(
        &self,
        context: &ProjectContext,
        report: &HealthCheckReport,
    ) -> Result<LintHistoryEntry, BackendError> {
        self.store_health_check_report_guarded(context, report, || Ok(()))
    }

    pub(crate) fn store_health_check_report_guarded<F>(
        &self,
        context: &ProjectContext,
        report: &HealthCheckReport,
        mut validate: F,
    ) -> Result<LintHistoryEntry, BackendError>
    where
        F: FnMut() -> Result<(), BackendError>,
    {
        validate()?;
        let entry = lint_history_entry_for_health_check(report);
        let persisted = PersistedLintReport {
            entry: entry.clone(),
            local_report: None,
            deep_report: None,
            health_check_report: Some(report.clone()),
        };
        if !report.persistent {
            #[cfg(unix)]
            let (project_key, project_anchor) = {
                let identity =
                    crate::services::project_identity(&context.root).map_err(|message| {
                        BackendError::new("LINT_PROJECT_IDENTITY_UNAVAILABLE", message, true, true)
                    })?;
                self.memory_project_identity(identity)?
            };
            #[cfg(not(unix))]
            let project_key = self.memory_project_key(context)?;
            let mut memory = self.memory_reports.write().map_err(|_| {
                BackendError::new(
                    "LINT_MEMORY_REPORTS_LOCKED",
                    "In-memory Lint reports are temporarily unavailable.",
                    true,
                    false,
                )
            })?;
            // All namespace/anchor mutations use one fixed lock order and stay
            // within the same memory write critical section. This keeps
            // eviction linearizable: no writer can recreate an evicted report
            // namespace between its removal and the matching anchor release.
            #[cfg(unix)]
            let mut roots = self.memory_project_roots.lock().map_err(|_| {
                BackendError::new(
                    "LINT_PROJECT_IDENTITY_UNAVAILABLE",
                    "In-memory project identity registry is unavailable.",
                    true,
                    false,
                )
            })?;
            #[cfg(unix)]
            roots.entry(project_key.clone()).or_insert(project_anchor);
            let reports = memory.entry(project_key.clone()).or_default();
            reports.insert(report.report_id.clone(), persisted);
            trim_memory_reports(reports);
            if let Err(error) = validate() {
                reports.remove(&report.report_id);
                let remove_namespace = reports.is_empty();
                if remove_namespace {
                    memory.remove(&project_key);
                }
                #[cfg(unix)]
                if remove_namespace {
                    roots.remove(&project_key);
                }
                return Err(error);
            }
            let evicted = trim_memory_project_namespaces(&mut memory, &project_key);
            #[cfg(not(unix))]
            let _ = &evicted;
            run_after_memory_namespace_trim();
            #[cfg(unix)]
            for key in &evicted {
                roots.remove(key);
            }
            return Ok(entry);
        }

        let _guard = self.metadata_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_METADATA_LOCKED",
                "Lint metadata is currently being updated by another operation.",
                true,
                true,
            )
        })?;
        self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
        self.file_store.write_json_atomic(
            context,
            &format!("{LINT_REPORTS_DIR}/{}.json", report.report_id),
            &persisted,
        )?;
        self.record_history_entry_locked(context, entry.clone())?;
        if let Err(error) = validate() {
            self.rollback_health_check_report_locked(context, &report.report_id)?;
            return Err(error);
        }
        Ok(entry)
    }

    fn rollback_health_check_report_locked(
        &self,
        context: &ProjectContext,
        report_id: &str,
    ) -> Result<(), BackendError> {
        reject_report_id(report_id)?;
        let path =
            context.resolve_project_write_path(&format!("{LINT_REPORTS_DIR}/{report_id}.json"))?;
        if path.is_file() {
            remove_project_file(&context.root, &path).map_err(|error| {
                BackendError::new(
                    "LINT_REPORT_ROLLBACK_FAILED",
                    format!("Could not remove stale Health Check report: {error}"),
                    true,
                    true,
                )
            })?;
        }
        let mut history = self.load_history(context)?;
        history.entries.retain(|entry| entry.id != report_id);
        self.file_store
            .write_json_atomic(context, LINT_HISTORY_PATH, &history)
    }

    fn load_history(&self, context: &ProjectContext) -> Result<LintHistoryFile, BackendError> {
        match self
            .file_store
            .read_json::<LintHistoryFile>(context, LINT_HISTORY_PATH)
        {
            Ok(mut file) => {
                file.version = 1;
                file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                file.entries.truncate(LINT_HISTORY_LIMIT);
                Ok(file)
            }
            Err(err) if err.code == "FILE_READ_FAILED" => {
                if context.root.join(LINT_HISTORY_PATH).exists() {
                    Err(BackendError::new(
                        "LINT_HISTORY_READ_FAILED",
                        format!("Could not read {LINT_HISTORY_PATH}: {}", err.message),
                        true,
                        true,
                    ))
                } else {
                    Ok(LintHistoryFile {
                        version: 1,
                        entries: Vec::new(),
                    })
                }
            }
            Err(err) => Err(BackendError::new(
                "LINT_HISTORY_READ_FAILED",
                format!("Could not read {LINT_HISTORY_PATH}: {}", err.message),
                true,
                true,
            )),
        }
    }

    fn record_history_entry_locked(
        &self,
        context: &ProjectContext,
        entry: LintHistoryEntry,
    ) -> Result<(), BackendError> {
        let mut file = self.load_history(context)?;
        let previous_ids: std::collections::HashSet<String> = file
            .entries
            .iter()
            .map(|existing| existing.id.clone())
            .collect();
        file.entries.retain(|existing| existing.id != entry.id);
        file.entries.insert(0, entry);
        file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        file.entries.truncate(LINT_HISTORY_LIMIT);
        self.file_store
            .write_json_atomic(context, LINT_HISTORY_PATH, &file)?;
        let retained_ids: std::collections::HashSet<&str> = file
            .entries
            .iter()
            .map(|existing| existing.id.as_str())
            .collect();
        let removed_ids: Vec<String> = previous_ids
            .into_iter()
            .filter(|id| !retained_ids.contains(id.as_str()))
            .collect();
        self.prune_report_bodies(context, &removed_ids)?;
        Ok(())
    }

    fn prune_report_bodies(
        &self,
        context: &ProjectContext,
        removed_ids: &[String],
    ) -> Result<(), BackendError> {
        for id in removed_ids {
            reject_report_id(id)?;
            let path =
                context.resolve_project_write_path(&format!("{LINT_REPORTS_DIR}/{id}.json"))?;
            if path.exists() {
                remove_project_file(&context.root, &path).map_err(|err| {
                    BackendError::new(
                        "LINT_HISTORY_PRUNE_FAILED",
                        format!("Could not remove stale lint report {id}: {err}"),
                        true,
                        true,
                    )
                })?;
            }
        }
        Ok(())
    }
}

fn current_health_report_required() -> BackendError {
    BackendError::new(
        "LINT_MEMORY_HEALTH_REPORT_REQUIRED",
        "Agent lint repair requires a current authoritative Health report.",
        true,
        true,
    )
}

fn is_current_persistent_health_report(
    persisted: &PersistedLintReport,
    id: &str,
    report: &HealthCheckReport,
) -> bool {
    report.persistent
        && !report.report_id.is_empty()
        && report.report_id == id
        && persisted.entry.id == id
        && persisted.entry.kind == LintReportKind::HealthCheck
        && persisted.entry.persistent
        && report.task_id == id
        && persisted.entry.task_id.as_deref() == Some(report.task_id.as_str())
        && persisted.entry.workflow_route.as_ref() == Some(&report.route)
        && persisted.entry.health_check_mode.as_ref() == Some(&report.mode)
        && persisted.entry.report_digest.as_deref()
            == LintService::health_check_report_digest(report)
                .ok()
                .as_deref()
}

fn lint_history_entry_for_local(id: &str, report: &LintReport) -> LintHistoryEntry {
    let (error_count, warning_count, info_count) = count_issue_severities(&report.issues);
    LintHistoryEntry {
        id: id.to_string(),
        kind: LintReportKind::Local,
        created_at: report.generated_at.clone(),
        issue_count: report.issues.len(),
        error_count,
        warning_count,
        info_count,
        scanned_pages: Some(report.scanned_pages),
        task_id: None,
        route: None,
        workflow_route: None,
        health_check_mode: None,
        duration_ms: None,
        persistent: true,
        report_digest: None,
    }
}

fn lint_history_entry_for_deep(
    task_id: &str,
    route: CompileRoutePreference,
    report: &DeepLintReport,
) -> LintHistoryEntry {
    let (error_count, warning_count, info_count) = count_issue_severities(&report.issues);
    LintHistoryEntry {
        id: task_id.to_string(),
        kind: LintReportKind::Deep,
        created_at: report.generated_at.clone(),
        issue_count: report.issues.len(),
        error_count,
        warning_count,
        info_count,
        scanned_pages: None,
        task_id: Some(task_id.to_string()),
        route: Some(route),
        workflow_route: None,
        health_check_mode: None,
        duration_ms: None,
        persistent: true,
        report_digest: None,
    }
}

fn lint_history_entry_for_health_check(report: &HealthCheckReport) -> LintHistoryEntry {
    LintHistoryEntry {
        id: report.report_id.clone(),
        kind: LintReportKind::HealthCheck,
        created_at: report.generated_at.clone(),
        issue_count: report.issues.len(),
        error_count: report.error_count,
        warning_count: report.warning_count,
        info_count: report.info_count,
        scanned_pages: Some(report.coverage.scanned_pages),
        task_id: Some(report.task_id.clone()),
        route: None,
        workflow_route: Some(report.route.clone()),
        health_check_mode: Some(report.mode.clone()),
        duration_ms: Some(report.duration_ms),
        persistent: report.persistent,
        report_digest: LintService::health_check_report_digest(report).ok(),
    }
}

impl LintService {
    fn memory_project_key(&self, context: &ProjectContext) -> Result<String, BackendError> {
        let identity = crate::services::project_identity(&context.root).map_err(|message| {
            BackendError::new("LINT_PROJECT_IDENTITY_UNAVAILABLE", message, true, true)
        })?;

        #[cfg(unix)]
        {
            self.memory_project_identity(identity).map(|(key, _)| key)
        }

        #[cfg(not(unix))]
        {
            Ok(format!(
                "{}:{}",
                identity.canonical_identity_key, identity.identity_revision
            ))
        }
    }

    #[cfg(unix)]
    fn memory_project_identity(
        &self,
        identity: crate::services::ProjectWorkflowIdentity,
    ) -> Result<(String, super::MemoryProjectRootAnchor), BackendError> {
        use std::os::unix::fs::MetadataExt;

        let anchor = std::fs::File::open(&identity.canonical_root).map_err(|error| {
            BackendError::new(
                "LINT_PROJECT_IDENTITY_UNAVAILABLE",
                format!("Project root could not be pinned: {error}"),
                true,
                true,
            )
        })?;
        let metadata = anchor.metadata().map_err(|error| {
            BackendError::new(
                "LINT_PROJECT_IDENTITY_UNAVAILABLE",
                format!("Pinned project root metadata is unavailable: {error}"),
                true,
                true,
            )
        })?;
        let device = metadata.dev();
        let inode = metadata.ino();
        let key = format!("{}:unix:{device}:{inode}", identity.canonical_identity_key);
        Ok((key, super::MemoryProjectRootAnchor { _anchor: anchor }))
    }
}

fn trim_memory_reports(reports: &mut std::collections::HashMap<String, PersistedLintReport>) {
    if reports.len() <= LINT_HISTORY_LIMIT {
        return;
    }
    let mut ids = reports
        .values()
        .map(|report| (report.entry.created_at.clone(), report.entry.id.clone()))
        .collect::<Vec<_>>();
    ids.sort_by(|left, right| right.cmp(left));
    let retained = ids
        .into_iter()
        .take(LINT_HISTORY_LIMIT)
        .map(|(_, id)| id)
        .collect::<std::collections::HashSet<_>>();
    reports.retain(|id, _| retained.contains(id));
}

fn trim_memory_project_namespaces(
    memory: &mut std::collections::HashMap<
        String,
        std::collections::HashMap<String, PersistedLintReport>,
    >,
    protected: &str,
) -> Vec<String> {
    let mut evicted = Vec::new();
    while memory.len() > LINT_MEMORY_PROJECT_LIMIT {
        let Some(oldest) = memory
            .iter()
            .filter(|(key, _)| key.as_str() != protected)
            .min_by_key(|(_, reports)| {
                reports
                    .values()
                    .map(|report| report.entry.created_at.as_str())
                    .max()
                    .unwrap_or_default()
            })
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        memory.remove(&oldest);
        evicted.push(oldest);
    }
    evicted
}

fn reject_report_id(id: &str) -> Result<(), BackendError> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(BackendError::new(
            "LINT_HISTORY_ID_INVALID",
            "Lint report id is invalid.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "id": id })));
    }
    Ok(())
}

fn count_issue_severities(issues: &[LintIssue]) -> (usize, usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    let mut infos = 0;
    for issue in issues {
        match issue.severity {
            LintSeverity::Error => errors += 1,
            LintSeverity::Warning => warnings += 1,
            LintSeverity::Info => infos += 1,
        }
    }
    (errors, warnings, infos)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::BTreeMap;

    use super::super::test_support::{tmp_context, write_file};
    use super::super::LintService;
    use crate::errors::BackendError;
    use crate::models::agent::AgentKind;
    use crate::models::lint::{HealthCheckCoverage, HealthCheckReport, LintReport};
    use crate::models::paths::ProjectContext;
    use crate::models::workflow::{
        HealthCheckMode, WorkflowOperation, WorkflowPersistenceMode, WorkflowResult, WorkflowRoute,
    };

    fn health_report(id: &str, persistent: bool, generated_at: String) -> HealthCheckReport {
        HealthCheckReport {
            report_id: id.into(),
            task_id: id.into(),
            mode: HealthCheckMode::LocalQuick,
            route: WorkflowRoute::Local {
                route_revision: "local-v1".into(),
            },
            persistent,
            issues: Vec::new(),
            finding_origins: BTreeMap::new(),
            coverage: HealthCheckCoverage {
                scanned_pages: 1,
                source_pages: 1,
                wiki_pages: 0,
                deep_covered_pages: None,
                deep_truncated: false,
                not_applicable_rules: vec!["index_drift".into()],
            },
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            findings_by_type: BTreeMap::new(),
            duration_ms: 1,
            generated_at,
        }
    }

    fn agent_health_owner(report: &HealthCheckReport) -> crate::models::workflow::WorkflowRun {
        let report_digest = LintService::health_check_report_digest(report).unwrap();
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "taskId": report.task_id,
            "projectId": "project-a",
            "canonicalIdentityKey": "identity-a",
            "identityRevision": "revision-a",
            "kind": "health_check",
            "operation": { "kind": "built_in" },
            "displayStatus": "completed",
            "scope": { "kind": "health_check", "mode": "complete" },
            "route": report.route,
            "fingerprint": "run-fingerprint",
            "baselineFingerprint": "baseline-a",
            "persistence": if report.persistent {
                "persistent"
            } else {
                "memory_only"
            },
            "stages": [],
            "currentStageId": null,
            "queuePosition": null,
            "continuationRequired": false,
            "retry": null,
            "pendingAction": null,
            "decisionReview": null,
            "result": {
                "kind": "health_check",
                "reportId": report.report_id,
                "persistent": report.persistent,
                "reportDigest": report_digest,
                "errorCount": 0,
                "warningCount": 0,
                "infoCount": 0,
                "coverage": null,
                "findingsByType": {}
            },
            "error": null,
            "startedAt": "2026-08-12T00:00:00Z",
            "updatedAt": "2026-08-12T00:01:00Z",
            "completedAt": "2026-08-12T00:01:00Z",
            "cancellable": false,
            "undoCancelUntil": null
        }))
        .unwrap()
    }

    #[test]
    fn local_lint_report_is_persisted_with_history_index() {
        let (context, root) = tmp_context("history-local");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let service = LintService::default();
        let report = LintReport {
            issues: Vec::new(),
            generated_at: "2026-07-04T00:00:00Z".into(),
            scanned_pages: 2,
        };

        let entry = service.persist_local_report(&context, &report).unwrap();
        let history = service.list_lint_history(&context).unwrap();
        let persisted = service
            .read_lint_history_report(&context, &entry.id)
            .unwrap();

        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].id, entry.id);
        assert!(persisted.local_report.is_some());
        assert!(context.app_dir.join("lint-history.json").exists());
        assert!(context
            .app_dir
            .join("lint-reports")
            .join(format!("{}.json", entry.id))
            .exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lint_history_is_limited_to_newest_fifty_entries() {
        let (context, root) = tmp_context("history-limit");
        let service = LintService::default();
        for index in 0..55 {
            let report = LintReport {
                issues: Vec::new(),
                generated_at: format!("2026-07-04T00:{index:02}:00Z"),
                scanned_pages: 1,
            };
            service.persist_local_report(&context, &report).unwrap();
        }
        let history = service.list_lint_history(&context).unwrap();
        assert_eq!(history.entries.len(), 50);
        assert!(history.entries[0].created_at > history.entries[49].created_at);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_single_lint_report_returns_a_report_error_not_a_history_crash() {
        let (context, root) = tmp_context("history-corrupt-report");
        write_file(
            &context,
            ".app/lint-history.json",
            r#"{"version":1,"entries":[{"id":"bad","kind":"local","createdAt":"2026-07-04T00:00:00Z","issueCount":1,"errorCount":1,"warningCount":0,"infoCount":0}]}"#,
        );
        write_file(&context, ".app/lint-reports/bad.json", "{ not valid json");

        let service = LintService::default();
        let history = service.list_lint_history(&context).unwrap();
        let err = service
            .read_lint_history_report(&context, "bad")
            .expect_err("bad report should fail only when opened");

        assert_eq!(history.entries.len(), 1);
        assert_eq!(err.code, "JSON_PARSE_FAILED");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn in_memory_health_check_history_is_limited_to_newest_fifty_entries() {
        let (context, root) = tmp_context("health-memory-limit");
        let service = LintService::default();

        for index in 0..55 {
            let report = health_report(
                &format!("health-{index:02}"),
                false,
                format!("2026-07-04T00:{index:02}:00Z"),
            );
            service
                .store_health_check_report(&context, &report)
                .unwrap();
        }

        let history = service.list_lint_history(&context).unwrap();
        assert_eq!(history.entries.len(), 50);
        assert_eq!(history.entries[0].id, "health-54");
        assert_eq!(history.entries[49].id, "health-05");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persistent_health_reader_rejects_mismatched_or_malformed_disk_reports() {
        let (context, root) = tmp_context("health-repair-disk-fail-closed");
        let service = LintService::default();
        let report = health_report("health-disk", true, "2026-07-04T00:00:00Z".into());
        service
            .store_health_check_report(&context, &report)
            .unwrap();

        let report_path = ".app/lint-reports/health-disk.json";
        let mut mismatched = serde_json::to_value(
            service
                .read_lint_history_report(&context, "health-disk")
                .unwrap(),
        )
        .unwrap();
        mismatched["healthCheckReport"]["reportId"] = serde_json::json!("other-report");
        mismatched["healthCheckReport"]["taskId"] = serde_json::json!("other-task");
        mismatched["entry"]["taskId"] = serde_json::json!("other-task");
        write_file(
            &context,
            report_path,
            &serde_json::to_string(&mismatched).unwrap(),
        );
        let restarted = LintService::default();
        let error = restarted
            .read_current_health_report(&context, "health-disk")
            .expect_err("mismatched persisted report identity must fail closed");
        assert_eq!(error.code, "LINT_MEMORY_HEALTH_REPORT_REQUIRED");

        write_file(&context, report_path, "{ malformed report");
        let error = restarted
            .read_current_health_report(&context, "health-disk")
            .expect_err("malformed persisted report must fail closed");
        assert_eq!(error.code, "LINT_MEMORY_HEALTH_REPORT_REQUIRED");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_health_report_rejects_unsafe_ids() {
        let (context, root) = tmp_context("health-repair-report-id-boundary");
        let service = LintService::default();
        for id in ["", "../health", "nested/health", r"nested\health"] {
            let error = service
                .read_current_health_report(&context, id)
                .expect_err("unsafe report ids must fail closed before reading files");
            assert_eq!(error.code, "LINT_HISTORY_ID_INVALID");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repair_report_reader_accepts_current_memory_or_persistent_health_reports() {
        let (context, root) = tmp_context("health-repair-report-authority");
        let service = LintService::default();
        let memory = health_report("health-memory", false, "2026-07-04T00:00:00Z".into());
        service
            .store_health_check_report(&context, &memory)
            .unwrap();

        let selected = service
            .read_current_health_report(&context, "health-memory")
            .unwrap();
        assert_eq!(selected, memory);

        let mut mismatched_memory = health_report(
            "health-memory-task-mismatch",
            false,
            "2026-07-04T00:00:30Z".into(),
        );
        mismatched_memory.task_id = "different-task".into();
        service
            .store_health_check_report(&context, &mismatched_memory)
            .unwrap();
        let error = service
            .read_current_health_report(&context, "health-memory-task-mismatch")
            .expect_err("memory reports must bind report and task identity");
        assert_eq!(error.code, "LINT_MEMORY_HEALTH_REPORT_REQUIRED");

        let disk = health_report("health-disk", true, "2026-07-04T00:01:00Z".into());
        service.store_health_check_report(&context, &disk).unwrap();
        assert!(context
            .app_dir
            .join("lint-reports/health-disk.json")
            .is_file());

        // A new LintService represents the process-restart boundary: the
        // persistent report must remain readable without memory state.
        let restarted = LintService::default();
        let selected = restarted
            .read_current_health_report(&context, "health-disk")
            .unwrap();
        assert_eq!(selected, disk);

        let error = restarted
            .read_current_health_report(&context, "health-memory")
            .expect_err("memory-only reports must not cross a process boundary");
        assert_eq!(error.code, "LINT_MEMORY_HEALTH_REPORT_REQUIRED");

        // A report file copied to a different project is not enough to pass
        // repair authorization; the owner/identity check remains mandatory.
        let other_root = tempfile::tempdir().unwrap().keep();
        let other_context = ProjectContext::new("project-a", other_root.clone());
        std::fs::create_dir_all(&other_context.app_dir).unwrap();
        write_file(
            &other_context,
            ".app/lint-reports/health-disk.json",
            &serde_json::to_string(
                &restarted
                    .read_lint_history_report(&context, "health-disk")
                    .unwrap(),
            )
            .unwrap(),
        );
        let copied = restarted
            .read_current_health_report(&other_context, "health-disk")
            .unwrap();
        assert_eq!(copied, disk);
        let copied_owner = agent_health_owner(&disk);
        let other_identity = crate::services::project_identity(&other_context.root).unwrap();
        let error = LintService::validate_current_health_report_owner(
            &copied,
            &copied_owner,
            &other_context.project_id,
            &other_identity.canonical_identity_key,
            &other_identity.identity_revision,
            &disk.route,
            "baseline-a",
        )
        .expect_err("a copied report must fail its current project identity check");
        assert_eq!(error.code, "LINT_REPAIR_REPORT_STALE");
        std::fs::remove_dir_all(other_root).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repair_report_owner_must_be_current_completed_built_in_health() {
        let route = WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: None,
            route_revision: "analysis-route-v1".into(),
        };
        let mut report = health_report("health-task-1", true, "2026-08-12T00:00:00Z".into());
        report.mode = HealthCheckMode::Complete;
        report.route = route.clone();
        let run = agent_health_owner(&report);

        LintService::validate_current_health_report_owner(
            &report,
            &run,
            "project-a",
            "identity-a",
            "revision-a",
            &route,
            "baseline-a",
        )
        .unwrap();

        let mut memory_only_owner = run.clone();
        memory_only_owner.persistence = WorkflowPersistenceMode::MemoryOnly;
        assert_eq!(
            LintService::validate_current_health_report_owner(
                &report,
                &memory_only_owner,
                "project-a",
                "identity-a",
                "revision-a",
                &route,
                "baseline-a",
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_HEALTH_REPORT_REQUIRED"
        );

        let mut tampered_report = report.clone();
        tampered_report.generated_at = "2026-08-13T00:00:00Z".into();
        assert_eq!(
            LintService::validate_current_health_report_owner(
                &tampered_report,
                &run,
                "project-a",
                "identity-a",
                "revision-a",
                &route,
                "baseline-a",
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_HEALTH_REPORT_REQUIRED"
        );

        let mut old_baseline = run.clone();
        old_baseline.baseline_fingerprint = "baseline-old".into();
        assert_eq!(
            LintService::validate_current_health_report_owner(
                &report,
                &old_baseline,
                "project-a",
                "identity-a",
                "revision-a",
                &route,
                "baseline-a",
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_REPORT_STALE"
        );

        let retargeted = WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: None,
            route_revision: "analysis-route-v2".into(),
        };
        assert_eq!(
            LintService::validate_current_health_report_owner(
                &report,
                &run,
                "project-a",
                "identity-a",
                "revision-a",
                &retargeted,
                "baseline-a",
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_REPORT_STALE"
        );

        let mut repair_owner = run;
        repair_owner.operation = WorkflowOperation::AgentLintRepair {
            preparation_id: "prepare-1".into(),
            preparation_revision: "prepare-revision-1".into(),
            report_id: report.report_id.clone(),
            selection_revision: "selection-revision-1".into(),
            selected_finding_ids: vec!["contradiction:wiki/a.md".into()],
            selected_findings: vec![crate::models::lint::AgentLintRepairFinding {
                id: "contradiction:wiki/a.md".into(),
                issue_type: crate::models::lint::DeepLintIssueType::Contradiction,
                severity: crate::models::lint::LintSeverity::Warning,
                path: "wiki/a.md".into(),
                message: "Contradiction".into(),
                evidence: None,
                suggested_action: None,
            }],
            skill: crate::models::lint::WikiLintSkillRef::builtin(),
            authorized_path_hashes: [("wiki/a.md".into(), Some("a".repeat(64)))]
                .into_iter()
                .collect(),
            expected_git_head: "b".repeat(40),
        };
        repair_owner.result = Some(WorkflowResult::AgentLintRepair {
            outcome: crate::models::lint::AgentLintRepairOutcome::Succeeded,
            resolved_finding_ids: Vec::new(),
            unresolved_finding_ids: Vec::new(),
            introduced_finding_ids: Vec::new(),
            skipped_finding_ids: Vec::new(),
            rounds: Vec::new(),
            affected_paths: Vec::new(),
            affected_path_hashes: std::collections::BTreeMap::new(),
            checkpoint_hash: None,
            final_commit: None,
            diff_available: false,
            rollback_available: false,
            index_refresh_warnings: Vec::new(),
        });
        assert_eq!(
            LintService::validate_current_health_report_owner(
                &report,
                &repair_owner,
                "project-a",
                "identity-a",
                "revision-a",
                &route,
                "baseline-a",
            )
            .unwrap_err()
            .code,
            "LINT_REPAIR_HEALTH_REPORT_REQUIRED"
        );
    }

    #[test]
    fn guarded_persistent_health_report_rolls_back_body_and_history() {
        let (context, root) = tmp_context("health-persistent-rollback");
        let service = LintService::default();
        let report = health_report("health-stale", true, "2026-07-04T00:00:00Z".into());
        let validation_count = Cell::new(0);

        let error = service
            .store_health_check_report_guarded(&context, &report, || {
                validation_count.set(validation_count.get() + 1);
                if validation_count.get() == 2 {
                    return Err(BackendError::new(
                        "WORKFLOW_INPUT_CHANGED",
                        "The project changed while the report was being stored.",
                        true,
                        true,
                    ));
                }
                Ok(())
            })
            .expect_err("post-write baseline failure must roll back the report");

        assert_eq!(error.code, "WORKFLOW_INPUT_CHANGED");
        assert!(!context
            .app_dir
            .join("lint-reports")
            .join("health-stale.json")
            .exists());
        assert!(service
            .list_lint_history(&context)
            .unwrap()
            .entries
            .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn in_memory_project_key_changes_when_same_path_is_recreated() {
        let (context, root) = tmp_context("health-memory-identity");
        let service = LintService::default();
        service
            .store_health_check_report(
                &context,
                &health_report("identity-a", false, "2026-08-24T00:00:00Z".into()),
            )
            .unwrap();
        let first = service.memory_project_key(&context).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        service
            .store_health_check_report(
                &context,
                &health_report("identity-b", false, "2026-08-24T00:00:01Z".into()),
            )
            .unwrap();
        let second = service.memory_project_key(&context).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        service
            .store_health_check_report(
                &context,
                &health_report("identity-c", false, "2026-08-24T00:00:02Z".into()),
            )
            .unwrap();
        let third = service.memory_project_key(&context).unwrap();

        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);
        let anchors = service.memory_project_roots.lock().unwrap();
        assert!(anchors.contains_key(&first));
        assert!(anchors.contains_key(&second));
        assert!(anchors.contains_key(&third));
        drop(anchors);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn memory_project_key_is_stable_for_child_edits_and_canonical_aliases() {
        use std::os::unix::fs::symlink;

        let (context, root) = tmp_context("health-memory-stable-identity");
        let alias = root.with_extension("alias");
        let service = LintService::default();
        service
            .store_health_check_report(
                &context,
                &health_report("stable", false, "2026-08-24T00:00:00Z".into()),
            )
            .unwrap();
        let before = service.memory_project_key(&context).unwrap();
        std::fs::write(root.join("ordinary-child.md"), "changed").unwrap();
        let after_child_edit = service.memory_project_key(&context).unwrap();
        symlink(&root, &alias).unwrap();
        let alias_context = ProjectContext::new("project-a", alias.clone());
        let through_alias = service.memory_project_key(&alias_context).unwrap();

        assert_eq!(before, after_child_edit);
        assert_eq!(before, through_alias);
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_report_namespaces_and_unix_anchors_are_bounded_together() {
        let service = LintService::default();
        let mut roots = Vec::new();
        for index in 0..=super::LINT_MEMORY_PROJECT_LIMIT {
            let (context, root) = tmp_context(&format!("health-memory-cap-{index}"));
            service
                .store_health_check_report(
                    &context,
                    &health_report(
                        &format!("memory-{index}"),
                        false,
                        format!("2026-08-24T00:{index:02}:00Z"),
                    ),
                )
                .unwrap();
            roots.push(root);
        }

        assert_eq!(
            service.memory_reports.read().unwrap().len(),
            super::LINT_MEMORY_PROJECT_LIMIT
        );
        #[cfg(unix)]
        assert_eq!(
            service.memory_project_roots.lock().unwrap().len(),
            super::LINT_MEMORY_PROJECT_LIMIT
        );
        drop(service);
        for root in roots {
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn namespace_recreation_cannot_race_eviction_anchor_release() {
        use std::sync::{mpsc, Arc};
        use std::time::Duration;

        let service = Arc::new(LintService::default());
        let (victim_context, victim_root) = tmp_context("health-memory-linear-victim");
        service
            .store_health_check_report(
                &victim_context,
                &health_report("victim-old", false, "2026-08-24T00:00:00Z".into()),
            )
            .unwrap();
        let mut roots = vec![victim_root];
        for index in 0..(super::LINT_MEMORY_PROJECT_LIMIT - 1) {
            let (context, root) = tmp_context(&format!("health-memory-linear-fill-{index}"));
            service
                .store_health_check_report(
                    &context,
                    &health_report(
                        &format!("fill-{index}"),
                        false,
                        format!("2026-08-24T01:{index:02}:00Z"),
                    ),
                )
                .unwrap();
            roots.push(root);
        }

        let (new_context, new_root) = tmp_context("health-memory-linear-new");
        roots.push(new_root);
        let (trim_reached_tx, trim_reached_rx) = mpsc::channel();
        let (release_trim_tx, release_trim_rx) = mpsc::channel();
        let service_a = Arc::clone(&service);
        let thread_a = std::thread::spawn(move || {
            super::set_after_memory_namespace_trim(move || {
                trim_reached_tx.send(()).unwrap();
                release_trim_rx
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            });
            service_a
                .store_health_check_report(
                    &new_context,
                    &health_report("new", false, "2026-08-24T03:00:00Z".into()),
                )
                .unwrap();
        });
        trim_reached_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap();

        let (victim_done_tx, victim_done_rx) = mpsc::channel();
        let service_b = Arc::clone(&service);
        let victim_context_for_thread = victim_context.clone();
        let thread_b = std::thread::spawn(move || {
            let result = service_b.store_health_check_report(
                &victim_context_for_thread,
                &health_report("victim-new", false, "2026-08-24T04:00:00Z".into()),
            );
            victim_done_tx.send(result).unwrap();
        });

        assert!(
            victim_done_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err(),
            "namespace recreation escaped the atomic report/anchor eviction section"
        );
        release_trim_tx.send(()).unwrap();
        thread_a.join().unwrap();
        victim_done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        thread_b.join().unwrap();

        let victim_key = service.memory_project_key(&victim_context).unwrap();
        assert!(service
            .memory_reports
            .read()
            .unwrap()
            .contains_key(&victim_key));
        assert!(
            service
                .memory_project_roots
                .lock()
                .unwrap()
                .contains_key(&victim_key),
            "every live memory-report namespace must retain its root anchor"
        );

        drop(service);
        for root in roots {
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

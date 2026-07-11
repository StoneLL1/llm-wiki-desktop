use crate::errors::BackendError;
use crate::models::compile::CompileRoutePreference;
use crate::models::lint::{
    DeepLintReport, LintHistoryEntry, LintHistoryFile, LintIssue, LintReport, LintReportKind,
    LintSeverity, PersistedLintReport,
};
use crate::models::paths::ProjectContext;

use super::{LintService, LINT_REPORTS_DIR};

const LINT_HISTORY_PATH: &str = ".app/lint-history.json";
const LINT_HISTORY_LIMIT: usize = 50;

impl LintService {
    pub fn persist_local_report(
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
        };
        self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
        self.file_store.write_json_atomic(
            context,
            &format!("{LINT_REPORTS_DIR}/{id}.json"),
            &persisted,
        )?;
        self.record_history_entry(context, entry.clone())?;
        Ok(entry)
    }

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
        };
        self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
        self.file_store.write_json_atomic(
            context,
            &format!("{LINT_REPORTS_DIR}/{task_id}.json"),
            &persisted,
        )?;
        self.record_history_entry(context, entry.clone())?;
        Ok(entry)
    }

    pub fn list_lint_history(
        &self,
        context: &ProjectContext,
    ) -> Result<LintHistoryFile, BackendError> {
        Ok(self.load_history(context))
    }

    pub fn read_lint_history_report(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<PersistedLintReport, BackendError> {
        reject_report_id(id)?;
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
                    })
                    .map_err(|_| wrapper_error)
            }
        }
    }

    fn load_history(&self, context: &ProjectContext) -> LintHistoryFile {
        match self
            .file_store
            .read_json::<LintHistoryFile>(context, LINT_HISTORY_PATH)
        {
            Ok(mut file) => {
                file.version = 1;
                file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                file.entries.truncate(LINT_HISTORY_LIMIT);
                file
            }
            Err(err) if err.code == "FILE_READ_FAILED" => LintHistoryFile {
                version: 1,
                entries: Vec::new(),
            },
            Err(err) => {
                eprintln!(
                    "[lint] ignoring unreadable {LINT_HISTORY_PATH}: {}",
                    err.message
                );
                LintHistoryFile {
                    version: 1,
                    entries: Vec::new(),
                }
            }
        }
    }

    fn record_history_entry(
        &self,
        context: &ProjectContext,
        entry: LintHistoryEntry,
    ) -> Result<(), BackendError> {
        let mut file = self.load_history(context);
        file.entries.retain(|existing| existing.id != entry.id);
        file.entries.insert(0, entry);
        file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        file.entries.truncate(LINT_HISTORY_LIMIT);
        self.file_store
            .write_json_atomic(context, LINT_HISTORY_PATH, &file)
    }
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
    }
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
    use super::super::test_support::{tmp_context, write_file};
    use super::super::LintService;
    use crate::models::lint::LintReport;

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
}

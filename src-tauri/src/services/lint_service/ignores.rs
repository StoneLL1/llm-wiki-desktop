use crate::app_state::ProjectWritePermit;
use crate::errors::BackendError;
use crate::models::lint::{LintIgnoreEntry, LintIgnoreFile, LintIssueType};
use crate::models::paths::ProjectContext;
use crate::utils::time_utils::now_rfc3339;

use super::LintService;

fn valid_ignore_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".md")
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && !path.as_bytes().get(1).is_some_and(|byte| *byte == b':')
        && !path.contains(char::from(92))
}

impl LintService {
    /// Read the layout's ignore file. A missing file is the first-run default
    /// (empty). A corrupt file is surfaced explicitly because silently
    /// disabling every ignore can make a clean-looking report misleading.
    pub(super) fn load_ignores(
        &self,
        context: &ProjectContext,
    ) -> Result<LintIgnoreFile, BackendError> {
        let Some(ignore_path) = context.layout.lint_ignore_path.as_deref() else {
            return Ok(LintIgnoreFile::default());
        };
        match self
            .file_store
            .read_json::<LintIgnoreFile>(context, ignore_path)
        {
            Ok(file) => Ok(file),
            Err(err) if err.code == "FILE_READ_FAILED" => {
                if context.root.join(ignore_path).exists() {
                    Err(BackendError::new(
                        "LINT_IGNORE_READ_FAILED",
                        format!("Could not read {ignore_path}: {}", err.message),
                        true,
                        true,
                    ))
                } else {
                    Ok(LintIgnoreFile::default())
                }
            }
            Err(err) => Err(BackendError::new(
                "LINT_IGNORE_READ_FAILED",
                format!("Could not read {ignore_path}: {}", err.message),
                true,
                true,
            )),
        }
    }

    /// Persist the ignore list under the layout's app-state root.
    fn save_ignores(
        &self,
        context: &ProjectContext,
        file: &LintIgnoreFile,
    ) -> Result<(), BackendError> {
        let ignore_path = context.layout.lint_ignore_path.as_deref().ok_or_else(|| {
            BackendError::new(
                "LINT_IGNORE_UNAVAILABLE",
                "This layout has no writable ignore configuration.",
                true,
                true,
            )
        })?;
        self.file_store
            .write_json_atomic(context, ignore_path, file)
    }

    /// Record an ignored `(path, rule)`. Dedupes by key (re-adding refreshes
    /// the timestamp) and returns the resulting list.
    fn add_ignore_unchecked(
        &self,
        context: &ProjectContext,
        path: &str,
        rule: LintIssueType,
    ) -> Result<LintIgnoreFile, BackendError> {
        // This is a report-matching key, never a file write target. Accept
        // readable Source and compatible roots as well as native Wiki paths.
        if !valid_ignore_path(path) {
            return Err(BackendError::new(
                "LINT_IGNORE_PATH_OUT_OF_SCOPE",
                "Ignored paths must not escape the project folder.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path })));
        }
        let _guard = self.metadata_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_METADATA_LOCKED",
                "Lint metadata is currently being updated by another operation.",
                true,
                true,
            )
        })?;
        let mut file = self.load_ignores(context)?;
        if let Some(existing) = file
            .ignored
            .iter_mut()
            .find(|entry| entry.path == path && entry.rule == rule)
        {
            existing.created_at = now_rfc3339();
        } else {
            file.ignored.push(LintIgnoreEntry {
                path: path.to_string(),
                rule,
                created_at: now_rfc3339(),
            });
        }
        self.save_ignores(context, &file)?;
        Ok(file)
    }

    /// Remove an ignored `(path, rule)`. Returns the resulting list.
    fn remove_ignore_unchecked(
        &self,
        context: &ProjectContext,
        path: &str,
        rule: LintIssueType,
    ) -> Result<LintIgnoreFile, BackendError> {
        if !valid_ignore_path(path) {
            return Err(BackendError::new(
                "LINT_IGNORE_PATH_OUT_OF_SCOPE",
                "Ignored paths must be project-relative Markdown paths.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path })));
        }
        let _guard = self.metadata_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_METADATA_LOCKED",
                "Lint metadata is currently being updated by another operation.",
                true,
                true,
            )
        })?;
        let mut file = self.load_ignores(context)?;
        file.ignored
            .retain(|entry| !(entry.path == path && entry.rule == rule));
        self.save_ignores(context, &file)?;
        Ok(file)
    }

    pub(crate) fn add_ignore_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        path: &str,
        rule: LintIssueType,
    ) -> Result<LintIgnoreFile, BackendError> {
        self.add_ignore_unchecked(permit.context(), path, rule)
    }

    pub(crate) fn remove_ignore_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        path: &str,
        rule: LintIssueType,
    ) -> Result<LintIgnoreFile, BackendError> {
        self.remove_ignore_unchecked(permit.context(), path, rule)
    }

    #[cfg(debug_assertions)]
    pub fn add_ignore(
        &self,
        context: &ProjectContext,
        path: &str,
        rule: LintIssueType,
    ) -> Result<LintIgnoreFile, BackendError> {
        self.add_ignore_unchecked(context, path, rule)
    }

    #[cfg(debug_assertions)]
    pub fn remove_ignore(
        &self,
        context: &ProjectContext,
        path: &str,
        rule: LintIssueType,
    ) -> Result<LintIgnoreFile, BackendError> {
        self.remove_ignore_unchecked(context, path, rule)
    }

    /// Return the current ignore list (empty when none persisted).
    pub fn list_ignores(&self, context: &ProjectContext) -> Result<LintIgnoreFile, BackendError> {
        self.load_ignores(context)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{tmp_context, write_file};
    use super::super::LintService;
    use crate::models::lint::LintIssueType;
    use crate::services::SearchService;

    #[test]
    fn add_lint_ignore_rejects_traversal_path() {
        let (context, root) = tmp_context("ignore-traversal");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let service = LintService::default();
        for path in [
            "../etc/evil.md",
            "wiki/../evil.md",
            "/absolute.md",
            "C:/evil.md",
            "wiki\\evil.md",
        ] {
            let err = service
                .add_ignore(&context, path, LintIssueType::DeadLink)
                .expect_err("only project-relative keys are accepted");
            assert_eq!(err.code, "LINT_IGNORE_PATH_OUT_OF_SCOPE");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignore_source_and_compatible_findings_uses_layout_state_path() {
        for path in ["raw/extracted/来源.md", "资料/版本1..版本2.md"] {
            let (mut context, root) = tmp_context("ignore-source-compatible");
            if path.starts_with("资料/") {
                context
                    .layout
                    .markdown_roots
                    .push(crate::models::layout::ProjectMarkdownRoot {
                        path: "资料".into(),
                        role: crate::models::layout::ProjectMarkdownRootRole::Source,
                        exclude: None,
                    });
                context.layout.lint_ignore_path = Some(".app/compat/lint-ignore.json".into());
            }
            write_file(&context, path, "# 来源\n\nReadable source");
            let lint = LintService::default();
            let search = SearchService::default();
            assert!(lint
                .run_local_lint(&context, &search)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.path == path
                    && issue.issue_type == LintIssueType::MissingFrontmatter));
            lint.add_ignore(&context, path, LintIssueType::MissingFrontmatter)
                .unwrap();
            assert!(root
                .join(context.layout.lint_ignore_path.as_ref().unwrap())
                .is_file());
            assert!(!lint
                .run_local_lint(&context, &search)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.path == path
                    && issue.issue_type == LintIssueType::MissingFrontmatter));
            lint.remove_ignore(&context, path, LintIssueType::MissingFrontmatter)
                .unwrap();
            assert!(lint
                .run_local_lint(&context, &search)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.path == path
                    && issue.issue_type == LintIssueType::MissingFrontmatter));
            if path.starts_with("资料/") {
                assert!(!root.join(".app/lint-ignore.json").exists());
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn run_local_lint_excludes_ignored_issues() {
        let (context, root) = tmp_context("ignore");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]].",
        );
        write_file(&context, "wiki/concepts/bare.md", "# Bare\n\n[[agent]].");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let service = LintService::default();
        let search = SearchService::default();
        let before = service.run_local_lint(&context, &search).unwrap();
        assert!(before
            .issues
            .iter()
            .any(|i| i.issue_type == LintIssueType::DeadLink));
        assert!(before
            .issues
            .iter()
            .any(|i| i.issue_type == LintIssueType::MissingFrontmatter));

        // Ignore dead links on agent.md only — the (path, rule) granularity.
        service
            .add_ignore(&context, "wiki/concepts/agent.md", LintIssueType::DeadLink)
            .unwrap();

        let after = service.run_local_lint(&context, &search).unwrap();
        assert!(
            !after
                .issues
                .iter()
                .any(|i| i.issue_type == LintIssueType::DeadLink),
            "ignored dead link must be suppressed"
        );
        assert!(
            after
                .issues
                .iter()
                .any(|i| i.issue_type == LintIssueType::MissingFrontmatter),
            "unrelated issue must remain"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn add_then_remove_lint_ignore_round_trips() {
        let (context, root) = tmp_context("ignore-rt");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let service = LintService::default();

        assert!(service.list_ignores(&context).unwrap().ignored.is_empty());

        let after_add = service
            .add_ignore(&context, "wiki/concepts/x.md", LintIssueType::DeadLink)
            .unwrap();
        assert_eq!(after_add.ignored.len(), 1);
        // Dedupe: re-adding the same (path, rule) must not duplicate.
        service
            .add_ignore(&context, "wiki/concepts/x.md", LintIssueType::DeadLink)
            .unwrap();
        let listed = service.list_ignores(&context).unwrap();
        assert_eq!(listed.ignored.len(), 1);
        assert_eq!(listed.ignored[0].path, "wiki/concepts/x.md");
        assert_eq!(listed.ignored[0].rule, LintIssueType::DeadLink);
        assert!(context.app_dir.join("lint-ignore.json").exists());

        let after_remove = service
            .remove_ignore(&context, "wiki/concepts/x.md", LintIssueType::DeadLink)
            .unwrap();
        assert!(after_remove.ignored.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_local_lint_reports_corrupt_ignore_file() {
        let (context, root) = tmp_context("ignore-corrupt");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]].",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        write_file(
            &context,
            ".app/lint-ignore.json",
            "{ this is not valid json",
        );
        // A corrupt ignore file must not silently disable the user's ignores.
        let err = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .expect_err("corrupt ignore config must be surfaced");
        assert_eq!(err.code, "LINT_IGNORE_READ_FAILED");
        std::fs::remove_dir_all(root).unwrap();
    }
}

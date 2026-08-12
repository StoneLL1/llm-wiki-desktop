mod deep;
mod fixes;
mod ignores;
mod repair;
mod reports;
mod rules;

#[cfg(test)]
mod test_support;

use crate::models::lint::{LintIssue, PersistedLintReport};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, RwLock};

pub use deep::DeepLintSnapshot;
pub use repair::{
    AgentLintRepairCandidate, AgentLintRepairWorkspaceDescriptor, AgentLintRepairWorkspaceLease,
};
pub use rules::{health_source_paths, LocalLintPhase};

pub(crate) const LINT_REPORTS_DIR: &str = ".app/lint-reports";

/// Facade for deterministic lint rules, deep analysis, report persistence,
/// ignore persistence, and checkpoint-protected fix orchestration.
#[derive(Default)]
pub struct LintService {
    pub(super) file_store: FileStore,
    /// Serialize read-modify-write updates to lint history and ignore config
    /// inside the desktop process.
    pub(super) metadata_write_lock: Mutex<()>,
    /// Serialize Lint mutations so two UI commands cannot interleave their
    /// optimistic hash checks, writes, verification, and checkpoint cleanup.
    /// External editors are still guarded by the FileStore post-write check.
    pub(super) fix_write_lock: Mutex<()>,
    /// Serialize the check-and-create section of `start_deep_lint`. The task
    /// itself still runs asynchronously, but only one active deep run may be
    /// attached to a project at a time.
    pub(super) deep_start_lock: Mutex<()>,
    /// Read-only/restricted Health Check reports live here for the current
    /// process. The outer key combines the canonical project identity with its
    /// identity revision and the inner key is the report/task id, so a replaced
    /// folder at the same path cannot observe the previous project's result.
    pub(super) memory_reports: RwLock<HashMap<String, HashMap<String, PersistedLintReport>>>,
}

impl LintService {
    pub fn lock_deep_start(&self) -> Result<MutexGuard<'_, ()>, crate::errors::BackendError> {
        self.deep_start_lock.lock().map_err(|_| {
            crate::errors::BackendError::new(
                "LINT_DEEP_START_LOCK_FAILED",
                "Deep Lint could not reserve its project-scoped start slot.",
                true,
                false,
            )
        })
    }

    /// Attach the content version that was current when a report was built.
    /// Fix commands use this value as their optimistic-lock baseline.
    pub fn attach_scan_hashes(&self, context: &ProjectContext, issues: &mut [LintIssue]) {
        for issue in issues {
            issue.scan_hash = self.file_store.file_hash(context, &issue.path).ok();
        }
    }

    pub fn capture_page_hashes(
        &self,
        context: &ProjectContext,
        paths: &HashSet<String>,
    ) -> std::collections::HashMap<String, String> {
        paths
            .iter()
            .filter_map(|path| {
                self.file_store
                    .file_hash(context, path)
                    .ok()
                    .map(|hash| (path.clone(), hash))
            })
            .collect()
    }

    /// Hash every input that can affect the deep-lint prompt, including
    /// optional project guidance files. The pinned built-in Skill is represented
    /// by a synthetic immutable key; project-local Skill files are never inputs.
    /// `None` is retained for missing files
    /// so creation/deletion is detected as a snapshot change too.
    pub fn capture_prompt_input_hashes(
        &self,
        context: &ProjectContext,
        page_paths: &HashSet<String>,
    ) -> Result<std::collections::HashMap<String, Option<String>>, crate::errors::BackendError>
    {
        let mut paths = page_paths.clone();
        paths.extend([
            "wiki/index.md".to_string(),
            "purpose.md".to_string(),
            "schema.md".to_string(),
        ]);
        let mut hashes = paths
            .into_iter()
            .map(|path| {
                let hash = self.file_store.file_hash_if_exists(context, &path)?;
                Ok((path, hash))
            })
            .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
        hashes.insert(
            format!(
                "builtin://{}/{}",
                crate::models::lint::WIKI_LINT_SKILL_ID,
                crate::models::lint::WIKI_LINT_SKILL_VERSION
            ),
            Some(crate::models::lint::WIKI_LINT_SKILL_SHA256.into()),
        );
        Ok(hashes)
    }

    /// Apply the same persisted ignore rules to deterministic and deep
    /// findings so the Lint surface never shows an issue the user dismissed.
    pub fn filter_ignored_issues(
        &self,
        context: &ProjectContext,
        issues: &mut Vec<LintIssue>,
    ) -> Result<(), crate::errors::BackendError> {
        let ignored = self
            .load_ignores(context)?
            .ignored
            .into_iter()
            .map(|entry| (entry.path, entry.rule))
            .collect::<HashSet<_>>();
        if !ignored.is_empty() {
            issues.retain(|issue| !ignored.contains(&(issue.path.clone(), issue.issue_type)));
        }
        Ok(())
    }
}

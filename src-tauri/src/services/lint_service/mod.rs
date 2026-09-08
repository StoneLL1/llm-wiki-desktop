mod deep;
mod fixes;
mod health;
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
use std::sync::{Mutex, RwLock};

pub use deep::DeepLintSnapshot;
pub use health::{HealthLocalScan, HealthScanPhase, HealthScanProgress};
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
    /// Read-only/restricted Health Check reports live here for the current
    /// process. The outer key combines the canonical project identity with its
    /// identity revision and the inner key is the report/task id, so a replaced
    /// folder at the same path cannot observe the previous project's result.
    pub(super) memory_reports: RwLock<HashMap<String, MemoryLintReports>>,
}

/// The root handle and its reports have one lifetime and one lock. Eviction
/// cannot release an anchor independently of the namespace it protects.
pub(super) struct MemoryLintReports {
    pub(super) reports: HashMap<String, PersistedLintReport>,
    #[cfg(unix)]
    pub(super) _anchor: std::fs::File,
}

impl LintService {
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

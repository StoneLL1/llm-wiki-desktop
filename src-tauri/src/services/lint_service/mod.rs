mod deep;
mod fixes;
mod ignores;
mod reports;
mod rules;

#[cfg(test)]
mod test_support;

use crate::services::file_store::FileStore;

pub(crate) const LINT_REPORTS_DIR: &str = ".app/lint-reports";

/// Facade for deterministic lint rules, deep analysis, report persistence,
/// ignore persistence, and checkpoint-protected fix orchestration.
#[derive(Default)]
pub struct LintService {
    pub(super) file_store: FileStore,
}

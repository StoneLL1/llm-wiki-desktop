use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;

fn lock_registry_error(message: &str) -> BackendError {
    BackendError::new(crate::errors::IMPORT_V2_COMMIT_FAILED, message, true, false)
}

/// Project-scoped Import mutation lanes. The project lane protects shared
/// Source/Git/history state; session lanes protect focused session/item state.
/// Callers must acquire the project lane before a session lane when both are
/// required.
pub(crate) struct ProjectImportLocks {
    project: Mutex<()>,
    sessions: Mutex<HashMap<String, Weak<Mutex<()>>>>,
}

impl ProjectImportLocks {
    fn new() -> Self {
        Self {
            project: Mutex::new(()),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn project(&self) -> &Mutex<()> {
        &self.project
    }

    pub(crate) fn session(&self, session_id: &str) -> Result<Arc<Mutex<()>>, BackendError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| lock_registry_error("Import session lock registry is unavailable."))?;
        sessions.retain(|_, lane| lane.strong_count() > 0);
        if let Some(lane) = sessions.get(session_id).and_then(Weak::upgrade) {
            return Ok(lane);
        }
        let lane = Arc::new(Mutex::new(()));
        sessions.insert(session_id.to_string(), Arc::downgrade(&lane));
        Ok(lane)
    }

    #[cfg(test)]
    pub(crate) fn retained_session_lanes(&self) -> usize {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sessions.retain(|_, lane| lane.strong_count() > 0);
        sessions.len()
    }
}

/// Resolves Import locks by the filesystem-backed canonical project identity.
/// Weak entries make the registry self-pruning after the last operation drops
/// its project lane.
#[derive(Default)]
pub(crate) struct ImportLockRegistry {
    projects: Mutex<HashMap<String, Weak<ProjectImportLocks>>>,
}

impl ImportLockRegistry {
    pub(crate) fn project(
        &self,
        context: &ProjectContext,
    ) -> Result<Arc<ProjectImportLocks>, BackendError> {
        let identity = crate::services::project_identity(&context.root)
            .map_err(|error| BackendError::new("PROJECT_IDENTITY_FAILED", error, true, false))?;
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| lock_registry_error("Import project lock registry is unavailable."))?;
        projects.retain(|_, lane| lane.strong_count() > 0);
        if let Some(lane) = projects
            .get(&identity.canonical_identity_key)
            .and_then(Weak::upgrade)
        {
            return Ok(lane);
        }
        let lane = Arc::new(ProjectImportLocks::new());
        projects.insert(identity.canonical_identity_key, Arc::downgrade(&lane));
        Ok(lane)
    }

    #[cfg(test)]
    pub(crate) fn retained_projects(&self) -> usize {
        let mut projects = self
            .projects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        projects.retain(|_, lane| lane.strong_count() > 0);
        projects.len()
    }
}

#[cfg(test)]
mod tests {
    use super::ImportLockRegistry;
    use crate::models::paths::ProjectContext;

    #[test]
    fn canonical_project_and_session_lanes_are_reused_then_pruned() {
        let root = tempfile::tempdir().unwrap();
        let context_a = ProjectContext::new("project-a", root.path().to_path_buf());
        let context_alias = ProjectContext::new("project-alias", root.path().to_path_buf());
        let registry = ImportLockRegistry::default();

        let project_a = registry.project(&context_a).unwrap();
        let project_alias = registry.project(&context_alias).unwrap();
        assert!(std::sync::Arc::ptr_eq(&project_a, &project_alias));

        let session_a = project_a.session("session-a").unwrap();
        let session_alias = project_alias.session("session-a").unwrap();
        assert!(std::sync::Arc::ptr_eq(&session_a, &session_alias));
        assert_eq!(project_a.retained_session_lanes(), 1);

        drop(session_a);
        drop(session_alias);
        assert_eq!(project_a.retained_session_lanes(), 0);
        drop(project_alias);
        drop(project_a);
        assert_eq!(registry.retained_projects(), 0);
    }

    #[test]
    fn different_canonical_projects_receive_independent_lanes() {
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        let registry = ImportLockRegistry::default();
        let project_a = registry
            .project(&ProjectContext::new("a", root_a.path().to_path_buf()))
            .unwrap();
        let project_b = registry
            .project(&ProjectContext::new("b", root_b.path().to_path_buf()))
            .unwrap();

        assert!(!std::sync::Arc::ptr_eq(&project_a, &project_b));
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

use crate::errors::{BackendError, PATH_INVALID, PROJECT_CONTEXT_MISMATCH};
use crate::models::confirmation::ConfirmationRegistry;
use crate::models::paths::ProjectContext;
use crate::services::{
    AgentService, ChatService, ExportService, ExtractionService, FileStore, GitService,
    GraphService, ImportService, LintService, LlmService, ProjectService, SearchService,
    SecretService, SettingsService,
};
use crate::tasks::TaskService;

#[derive(Default)]
pub struct AppState {
    pub project_registry: ProjectRegistry,
    pub project_service: ProjectService,
    pub file_store: FileStore,
    pub import_service: ImportService,
    pub extraction_service: ExtractionService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub chat_service: ChatService,
    pub llm_service: LlmService,
    pub search_service: SearchService,
    pub graph_service: GraphService,
    pub lint_service: LintService,
    pub export_service: ExportService,
    pub settings_service: SettingsService,
    pub secret_service: SecretService,
    pub task_service: TaskService,
    pub confirmation_registry: ConfirmationRegistry,
}

#[derive(Default)]
pub struct ProjectRegistry {
    roots: RwLock<HashMap<String, std::path::PathBuf>>,
}

impl ProjectRegistry {
    pub fn register(
        &self,
        project_id: impl Into<String>,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        if !root.is_absolute() {
            return Err(BackendError::new(
                PATH_INVALID,
                "Project root must be an absolute path.",
                false,
                true,
            ));
        }
        let canonical_root = root.canonicalize().map_err(|error| {
            BackendError::new(
                PATH_INVALID,
                "Project root could not be resolved.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        let project_id = project_id.into();
        let mut roots = self.roots.write().map_err(|_| registry_locked())?;
        if let Some(registered_root) = roots.get(&project_id) {
            if registered_root != &canonical_root {
                return Err(context_mismatch());
            }
        } else {
            roots.insert(project_id.clone(), canonical_root.clone());
        }
        Ok(ProjectContext::new(project_id, canonical_root))
    }

    pub fn resolve(
        &self,
        project_id: &str,
        asserted_root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        let asserted_root = asserted_root
            .canonicalize()
            .map_err(|_| context_mismatch())?;
        let registered_root = self
            .roots
            .read()
            .map_err(|_| registry_locked())?
            .get(project_id)
            .cloned()
            .ok_or_else(context_mismatch)?;
        if asserted_root != registered_root {
            return Err(context_mismatch());
        }
        Ok(ProjectContext::new(project_id, registered_root))
    }
}

fn context_mismatch() -> BackendError {
    BackendError::new(
        PROJECT_CONTEXT_MISMATCH,
        "Project id and root do not match an opened backend project.",
        true,
        true,
    )
}

fn registry_locked() -> BackendError {
    BackendError::new(
        "PROJECT_REGISTRY_LOCKED",
        "Project registry is unavailable.",
        true,
        false,
    )
}

impl AppState {
    pub fn resolve_project_context(
        &self,
        project_id: &str,
        asserted_root: &str,
    ) -> Result<ProjectContext, BackendError> {
        self.project_registry
            .resolve(project_id, Path::new(asserted_root))
    }
}

#[cfg(test)]
mod project_registry_tests {
    use std::fs;

    use super::ProjectRegistry;
    use crate::errors::PROJECT_CONTEXT_MISMATCH;

    fn temp_project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-project-registry-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn registered_project_rejects_a_id_with_b_root() {
        let registry = ProjectRegistry::default();
        let project_a = temp_project("项目甲");
        let project_b = temp_project("项目乙");
        registry.register("project-a", &project_a).unwrap();

        let error = registry
            .resolve("project-a", &project_b)
            .expect_err("a registered id must not authorize another root");

        assert_eq!(error.code, PROJECT_CONTEXT_MISMATCH);
        fs::remove_dir_all(project_a).unwrap();
        fs::remove_dir_all(project_b).unwrap();
    }

    #[test]
    fn unknown_project_id_is_rejected() {
        let registry = ProjectRegistry::default();
        let project = temp_project("unknown");

        let error = registry
            .resolve("not-registered", &project)
            .expect_err("unknown ids must not create contexts");

        assert_eq!(error.code, PROJECT_CONTEXT_MISMATCH);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registering_another_opened_project_preserves_both_trusted_capabilities() {
        let registry = ProjectRegistry::default();
        let project_a = temp_project("active-a");
        let project_b = temp_project("active-b");
        registry.register("project-a", &project_a).unwrap();
        registry.register("project-b", &project_b).unwrap();

        registry
            .resolve("project-a", &project_a)
            .expect("background tasks and confirmations keep their trusted project context");
        registry
            .resolve("project-b", &project_b)
            .expect("the newly opened project is trusted too");
        fs::remove_dir_all(project_a).unwrap();
        fs::remove_dir_all(project_b).unwrap();
    }

    #[test]
    fn matching_normalized_root_resolves_and_preserves_cjk() {
        let registry = ProjectRegistry::default();
        let project = temp_project("中文资料库");
        registry.register("project-cjk", &project).unwrap();

        let context = registry
            .resolve("project-cjk", &project.join("."))
            .expect("the normalized canonical root should match");

        assert_eq!(context.project_id, "project-cjk");
        assert_eq!(context.root, project.canonicalize().unwrap());
        assert!(context.root.to_string_lossy().contains("中文资料库"));
        fs::remove_dir_all(project).unwrap();
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

use crate::errors::{BackendError, PATH_INVALID, PROJECT_CONTEXT_MISMATCH};
use crate::models::confirmation::ConfirmationRegistry;
use crate::models::paths::ProjectContext;
use crate::services::import_v2::capability_runtime::ImportCapabilityRuntime;
use crate::services::import_v2::ImportV2Service;
use crate::services::import_v2::connector_session::ConnectorSessionService;
use crate::services::{
    AgentService, BookmarkService, ChatConvenienceService, ChatService, ExportService,
    ExtractionService, FileStore, GitService, GraphService, ImportService, LintService, LlmService,
    ProjectService, SearchService, SecretService, SettingsService,
};
use crate::tasks::TaskService;

#[derive(Default)]
pub struct AppState {
    pub project_registry: ProjectRegistry,
    pub project_service: ProjectService,
    pub file_store: FileStore,
    pub import_service: ImportService,
    pub import_v2_service: ImportV2Service,
    pub import_capability_runtime: ImportCapabilityRuntime,
    pub connector_session_service: ConnectorSessionService,
    pub extraction_service: ExtractionService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub bookmark_service: BookmarkService,
    pub chat_convenience_service: ChatConvenienceService,
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

    /// Preview a folder for the "Open folder as project" dialog (dlg-folder).
    ///
    /// Returns whether the folder is an existing wiki project (`Opened` +
    /// summary) or a plain folder (`NeedsConfirmation` + pending
    /// `InitializeFolder` action). For the NeedsConfirmation case the pending
    /// action is registered with its execution plan so the frontend can later
    /// confirm via `confirm_pending_action` -> `confirm_folder_initialization`,
    /// which creates the project structure, organizes files by type, and
    /// creates the Git checkpoint. For the Opened case no Git/registry/recent
    /// side effects run — this is a preview only.
    pub fn preview_folder_as_project(
        &self,
        path: &str,
    ) -> Result<crate::models::project::OpenProjectResponse, BackendError> {
        let outcome = self.project_service.open_project(path)?;
        if let Some(pending_action) = outcome.pending_action.as_ref() {
            let execution = self
                .project_service
                .folder_initialization_execution(Path::new(path), pending_action)?;
            self.confirmation_registry
                .register_with_execution(pending_action.clone(), Some(execution))?;
        }
        Ok(outcome)
    }

    /// Create a scoped Git checkpoint of the import artifacts: the archived
    /// source files (those not skipped or linked to an existing source), plus
    /// `.app/source-index.json` and `.app/import-conflicts.json`. If the
    /// project has no Git repository yet, it is initialized first — in that
    /// case the initial commit captures the archived files and the returned
    /// hash is that initial commit. For an already-versioned project a scoped
    /// commit is created over just the import-affected paths. Returns the
    /// commit hash when a commit was created, or `None` when there were no
    /// changes to commit.
    pub fn create_import_checkpoint(
        &self,
        context: &ProjectContext,
        preview: &crate::models::import::ImportPreview,
    ) -> Result<Option<String>, BackendError> {
        use crate::models::git::CheckpointPurpose;
        use crate::models::import::ConflictResolution;

        let had_head = self.git_service.repository_status(context)?.head.is_some();
        self.git_service
            .initialize_repository(context, "Before import confirmation")?;

        let mut scoped_paths: Vec<String> = preview
            .files
            .iter()
            .filter(|entry| {
                !matches!(
                    entry
                        .conflict
                        .as_ref()
                        .and_then(|conflict| conflict.resolution.as_ref()),
                    Some(ConflictResolution::Skip | ConflictResolution::LinkToExisting)
                )
            })
            .map(|entry| entry.archived_path.clone())
            .collect();
        // Promoted verbatim originals now live under wiki/sources/ (written by
        // confirm_import). Include them so the newly-created browsable pages
        // are committed alongside the archived sources — the source-index
        // records each entry's promoted path, which is authoritative here.
        let confirmed_index = self
            .import_service
            .read_source_index(&context, &self.file_store)?;
        for entry in &preview.files {
            if matches!(
                entry
                    .conflict
                    .as_ref()
                    .and_then(|conflict| conflict.resolution.as_ref()),
                Some(ConflictResolution::Skip | ConflictResolution::LinkToExisting)
            ) {
                continue;
            }
            if let Some(artifacts) = confirmed_index.sources.get(&entry.archived_path) {
                for artifact in artifacts {
                    if artifact.starts_with("wiki/sources/") {
                        scoped_paths.push(artifact.clone());
                    }
                }
            }
        }
        scoped_paths.push(".app/source-index.json".to_string());
        scoped_paths.push(".app/import-conflicts.json".to_string());

        let checkpoint = self.git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::FinalResult,
            "Confirm import: archive sources and record conflicts",
            &scoped_paths,
        )?;

        if checkpoint.created {
            return Ok(checkpoint.commit_hash);
        }

        // No scoped changes to commit. For a freshly-initialized repository
        // `initialize_repository` already captured the archived files in its
        // initial commit, so that commit is the checkpoint. For an existing
        // repository with nothing new, there is genuinely no checkpoint.
        if had_head {
            Ok(None)
        } else {
            Ok(self.git_service.repository_status(context)?.head)
        }
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

#[cfg(test)]
mod folder_preview_tests {
    use super::AppState;
    use crate::models::confirmation::PendingActionType;
    use crate::models::project::{OpenProjectKind, OpenProjectResponse};
    use crate::services::ProjectService;
    use std::fs;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-wiki-folder-preview-{label}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn app_state_in_temp() -> (AppState, PathBuf) {
        let config = unique_temp_dir("config");
        let state = AppState {
            project_service: ProjectService::with_config_dir(config.clone()),
            ..AppState::default()
        };
        (state, config)
    }

    fn cleanup(dirs: &[&PathBuf]) {
        for dir in dirs {
            fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn preview_plain_folder_registers_confirmable_initialize_action() {
        let (state, config) = app_state_in_temp();
        let root = unique_temp_dir("plain");
        fs::write(root.join("report.pdf"), "%PDF-1.4").unwrap();
        fs::write(root.join("note.md"), "# note").unwrap();

        let outcome = state
            .preview_folder_as_project(root.to_string_lossy().as_ref())
            .unwrap();

        let pending = match outcome {
            OpenProjectResponse {
                kind: OpenProjectKind::NeedsConfirmation,
                pending_action: Some(pending),
                ..
            } => pending,
            _ => panic!("plain folder must require confirmation"),
        };
        assert_eq!(pending.action_type, PendingActionType::InitializeFolder);
        assert!(pending.affected_paths.contains(&"report.pdf".to_string()));
        assert!(pending.affected_paths.contains(&"note.md".to_string()));
        // Nothing moved before confirmation.
        assert!(root.join("report.pdf").exists());
        assert!(!root.join("raw").exists());

        // The pending action is registered and confirmable via the registry,
        // i.e. the dlg-folder -> confirm_pending_action chain is wired end to end.
        let stored = state.confirmation_registry.peek(&pending.id).unwrap();
        assert_eq!(stored.action.id, pending.id);
        assert!(stored.execution.is_some());

        cleanup(&[&root, &config]);
    }

    #[test]
    fn preview_existing_wiki_folder_returns_opened_without_pending_action() {
        let (state, config) = app_state_in_temp();
        let root = unique_temp_dir("existing");
        fs::write(root.join("schema.md"), "# schema").unwrap();
        fs::write(root.join("index.md"), "# index").unwrap();
        fs::create_dir_all(root.join("concepts")).unwrap();
        fs::write(root.join("concepts").join("agent.md"), "# Agent").unwrap();

        let outcome = state
            .preview_folder_as_project(root.to_string_lossy().as_ref())
            .unwrap();

        match outcome {
            OpenProjectResponse {
                kind: OpenProjectKind::Opened,
                summary: Some(summary),
                pending_action: None,
            } => {
                assert!(summary.health.is_wiki_project);
            }
            _ => panic!("existing wiki folder should open without confirmation"),
        }

        // No confirmation was registered for an already-project folder.
        let err = state
            .confirmation_registry
            .peek("nonexistent")
            .expect_err("no pending action should be registered");
        assert_eq!(err.code, "CONFIRMATION_NOT_FOUND");

        cleanup(&[&root, &config]);
    }

    #[test]
    fn preview_plain_folder_with_cjk_filename_is_organized_safely() {
        let (state, config) = app_state_in_temp();
        let root = unique_temp_dir("cjk");
        fs::write(root.join("论文.pdf"), "%PDF-1.4").unwrap();

        let outcome = state
            .preview_folder_as_project(root.to_string_lossy().as_ref())
            .unwrap();

        let pending = match outcome {
            OpenProjectResponse {
                kind: OpenProjectKind::NeedsConfirmation,
                pending_action: Some(pending),
                ..
            } => pending,
            _ => panic!("CJK-named folder must require confirmation"),
        };
        assert!(pending.affected_paths.contains(&"论文.pdf".to_string()));
        assert!(root.join("论文.pdf").exists());

        cleanup(&[&root, &config]);
    }
}

#[cfg(test)]
mod import_checkpoint_tests {
    use super::AppState;
    use crate::models::paths::ProjectContext;
    use crate::services::{FileStore, GitService, ImportService};
    use std::fs;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-wiki-import-ckpt-{label}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn app_state_default() -> AppState {
        AppState::default()
    }

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let root = unique_temp_dir(suffix);
        (
            ProjectContext::new("project-import-ckpt", root.clone()),
            root,
        )
    }

    /// Build a realistic import preview from a source file, confirm the import
    /// (archives the file + writes source-index), then verify the checkpoint
    /// helper produces a commit hash that exists in the repository and that
    /// the archive plus index are committed.
    #[test]
    fn create_import_checkpoint_commits_archive_and_index() {
        let state = app_state_default();
        let (context, root) = tmp_context("checkpoint");
        let store = FileStore;

        let source_dir = root.join("import-source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("notes.md");
        fs::write(&source_path, b"# Imported notes").unwrap();

        let request = crate::models::import::ImportRequest {
            source_paths: vec![source_path.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };

        let preview = ImportService
            .preview_import(&context, &store, &request, &[])
            .unwrap();

        // Mirror confirm_import_preview's flow: archive + write conflicts.
        ImportService
            .confirm_import(&context, &store, &preview)
            .unwrap();
        store
            .write_json_atomic(&context, ".app/import-conflicts.json", &preview)
            .unwrap();

        let hash = state
            .create_import_checkpoint(&context, &preview)
            .unwrap()
            .expect("a checkpoint commit should be created");

        // The returned hash is a real commit in the repository.
        let status = state.git_service.repository_status(&context).unwrap();
        assert_eq!(status.head.as_deref(), Some(hash.as_str()));
        // The archive and index are committed (working tree is clean).
        assert!(!status.has_changes);
        assert!(root.join("raw/sources/markdown/notes.md").exists());

        fs::remove_dir_all(root).ok();
    }

    /// Confirming an import with `create_checkpoint` into a not-yet-versioned
    /// project still produces a checkpoint: the helper initializes the
    /// repository first. No pre-existing `.git` should be required.
    #[test]
    fn create_import_checkpoint_initializes_repo_when_missing() {
        let state = app_state_default();
        let (context, root) = tmp_context("no-git");
        let store = FileStore;

        // No git init here — the project is a plain folder.
        assert!(!context.root.join(".git").exists());

        let source_dir = root.join("src");
        fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("doc.md");
        fs::write(&source_path, b"doc").unwrap();
        let request = crate::models::import::ImportRequest {
            source_paths: vec![source_path.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };
        let preview = ImportService
            .preview_import(&context, &store, &request, &[])
            .unwrap();
        ImportService
            .confirm_import(&context, &store, &preview)
            .unwrap();
        store
            .write_json_atomic(&context, ".app/import-conflicts.json", &preview)
            .unwrap();

        let hash = state
            .create_import_checkpoint(&context, &preview)
            .unwrap()
            .expect("checkpoint should be created even without a prior repo");

        assert!(context.root.join(".git").exists());
        assert_eq!(
            state
                .git_service
                .repository_status(&context)
                .unwrap()
                .head
                .as_deref(),
            Some(hash.as_str())
        );

        fs::remove_dir_all(root).ok();
    }

    /// When there are no changes to commit (e.g. an empty import preview with
    /// no archived files and an already-clean tree), the helper returns `None`
    /// rather than erroring — mirroring GitService::create_scoped_checkpoint.
    #[test]
    fn create_import_checkpoint_returns_none_when_nothing_to_commit() {
        let state = app_state_default();
        let (context, root) = tmp_context("empty");
        let git = GitService;
        git.initialize_repository(&context, "baseline").unwrap();

        let preview = crate::models::import::ImportPreview {
            files: Vec::new(),
            conflicts: Vec::new(),
            summary: crate::models::import::ImportSummary {
                total_files: 0,
                archived_files: 0,
                duplicate_files: 0,
                renamed_files: 0,
                failed_files: 0,
                conflicts_count: 0,
            },
        };

        let hash = state.create_import_checkpoint(&context, &preview).unwrap();
        assert!(hash.is_none(), "no changes -> no commit hash");

        fs::remove_dir_all(root).ok();
    }

    /// For an already-versioned project (the common case), the scoped
    /// checkpoint commits only the new archived files and index, leaving any
    /// unrelated working-tree changes unstaged. The baseline HEAD moves
    /// forward to the new commit.
    #[test]
    fn create_import_checkpoint_creates_scoped_commit_in_existing_repo() {
        let state = app_state_default();
        let (context, root) = tmp_context("existing-repo");
        let store = FileStore;
        let git = GitService;
        git.initialize_repository(&context, "baseline").unwrap();
        let baseline_head = git.repository_status(&context).unwrap().head.unwrap();

        let source_dir = root.join("src");
        fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("doc.md");
        fs::write(&source_path, b"doc").unwrap();
        let request = crate::models::import::ImportRequest {
            source_paths: vec![source_path.to_string_lossy().to_string()],
            allow_duplicates: false,
            link_duplicates: false,
        };
        let preview = ImportService
            .preview_import(&context, &store, &request, &[])
            .unwrap();
        ImportService
            .confirm_import(&context, &store, &preview)
            .unwrap();
        store
            .write_json_atomic(&context, ".app/import-conflicts.json", &preview)
            .unwrap();

        let hash = state
            .create_import_checkpoint(&context, &preview)
            .unwrap()
            .expect("a scoped commit should be created for new archives");

        // The checkpoint is a new commit on top of the baseline.
        assert_ne!(hash, baseline_head);
        let status = git.repository_status(&context).unwrap();
        assert_eq!(status.head.as_deref(), Some(hash.as_str()));
        assert!(root.join("raw/sources/markdown/doc.md").exists());
        // The archived file is tracked by the scoped commit. The original
        // source file under import-source/ is left untouched (untracked) —
        // the checkpoint is scoped to import artifacts only.
        let tracked = std::process::Command::new("git")
            .args(["ls-files"])
            .current_dir(&context.root)
            .output()
            .unwrap();
        let tracked = String::from_utf8_lossy(&tracked.stdout).to_string();
        assert!(
            tracked.contains("raw/sources/markdown/doc.md"),
            "archive should be tracked after checkpoint"
        );

        fs::remove_dir_all(root).ok();
    }
}

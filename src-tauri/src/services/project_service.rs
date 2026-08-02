use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::paths::ProjectContext;
use crate::models::project::{
    AgentRoute, GraphState, IndexState, OpenProjectResponse, ProjectFilesystemAccess,
    ProjectHealthReport, ProjectSummary, ProjectTemplate, ProjectTrustKind, RecentProject,
};
use crate::services::file_store::FileStore;
use crate::services::git_service::GitService;
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_root,
};

mod trust_store;

use trust_store::ProjectTrustStore;

const GENERAL_PURPOSE: &str = include_str!("../../templates/projects/general/purpose.md");
const GENERAL_SCHEMA: &str = include_str!("../../templates/projects/general/schema.md");
const RESEARCH_PURPOSE: &str = include_str!("../../templates/projects/research/purpose.md");
const RESEARCH_SCHEMA: &str = include_str!("../../templates/projects/research/schema.md");
const READING_PURPOSE: &str = include_str!("../../templates/projects/reading/purpose.md");
const READING_SCHEMA: &str = include_str!("../../templates/projects/reading/schema.md");
const GROWTH_PURPOSE: &str = include_str!("../../templates/projects/personal-growth/purpose.md");
const GROWTH_SCHEMA: &str = include_str!("../../templates/projects/personal-growth/schema.md");
const BUSINESS_PURPOSE: &str = include_str!("../../templates/projects/business/purpose.md");
const BUSINESS_SCHEMA: &str = include_str!("../../templates/projects/business/schema.md");

const RECENT_PROJECT_FILE: &str = "recent-projects.json";
const MAX_RECENT_PROJECTS: usize = 20;

pub struct ProjectService {
    config_dir: PathBuf,
    trust_store: ProjectTrustStore,
}

impl Default for ProjectService {
    fn default() -> Self {
        let config_dir = default_config_dir();
        Self {
            trust_store: ProjectTrustStore::new(&config_dir),
            config_dir,
        }
    }
}

impl ProjectService {
    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self {
            trust_store: ProjectTrustStore::new(&config_dir),
            config_dir,
        }
    }

    pub(crate) fn grant_project_trust(
        &self,
        root: &Path,
        trust_kind: ProjectTrustKind,
        expected_identity_key: &str,
        expected_identity_revision: &str,
    ) -> Result<(), BackendError> {
        self.trust_store
            .grant(
                root,
                trust_kind,
                expected_identity_key,
                expected_identity_revision,
            )
            .map(|_| ())
    }

    pub(crate) fn restore_project_trust(
        &self,
        root: &Path,
    ) -> Result<Option<ProjectTrustKind>, BackendError> {
        self.trust_store
            .restore(root)
            .map(|trust| trust.map(|trust| trust.trust_kind))
    }

    pub(crate) fn revoke_project_trust(&self, root: &Path) -> Result<(), BackendError> {
        self.trust_store.revoke(root)
    }

    pub(crate) fn filesystem_access(
        &self,
        context: &ProjectContext,
        trusted: bool,
    ) -> ProjectFilesystemAccess {
        if !trusted {
            return ProjectFilesystemAccess::ReadOnly;
        }
        let Ok(safe_root) = validate_existing_project_root(&context.root) else {
            return ProjectFilesystemAccess::ReadOnly;
        };
        if probe_writable_directory(&safe_root) {
            ProjectFilesystemAccess::Writable
        } else {
            ProjectFilesystemAccess::ReadOnly
        }
    }

    pub(crate) fn has_writable_task_state_root(&self, context: &ProjectContext) -> bool {
        let Some(relative) = context.layout.task_state_root.as_deref() else {
            return false;
        };
        let Ok(task_state_root) = context.resolve_project_path(relative) else {
            return false;
        };
        validate_existing_project_directory(&context.root, &task_state_root).is_ok_and(
            |safe_task_state_root| {
                probe_writable_project_directory(&context.root, &safe_task_state_root)
            },
        )
    }

    pub fn create_project(
        &self,
        root_path: &str,
        name: &str,
        template: ProjectTemplate,
    ) -> Result<ProjectSummary, BackendError> {
        let root = validate_root_for_creation(root_path)?;
        let project_id = uuid::Uuid::new_v4().to_string();
        let context = ProjectContext::new(project_id.clone(), root.clone());
        let store = FileStore;

        self.ensure_skeleton(&context, &store)?;

        store.write_markdown(&context, "purpose.md", template_purpose(template))?;
        store.write_markdown(&context, "schema.md", template_schema(template))?;
        store.write_markdown(&context, "wiki/index.md", &starter_index(name))?;
        store.write_markdown(&context, "wiki/log.md", &starter_log(name))?;
        store.write_markdown(&context, "wiki/overview.md", &starter_overview(name))?;

        let project_settings = ProjectSettings { template };
        store.write_json_atomic(&context, ".app/settings.json", &project_settings)?;
        store.write_json_atomic(&context, ".app/agent-config.json", &serde_json::json!({}))?;
        store.write_json_atomic(&context, ".app/bookmarks.json", &serde_json::json!([]))?;
        store.write_json_atomic(
            &context,
            ".app/graph-cache.json",
            &serde_json::json!({ "nodes": [], "edges": [] }),
        )?;
        store.write_json_atomic(
            &context,
            ".app/import-conflicts.json",
            &serde_json::json!({ "conflicts": [] }),
        )?;
        GitService.initialize_repository(&context, "Initial wiki project")?;

        let mut summary = self.scan_project(&context, Some(name));
        summary.template = template;
        summary.health.is_wiki_project = true;
        Ok(summary)
    }

    fn ensure_skeleton(
        &self,
        context: &ProjectContext,
        store: &FileStore,
    ) -> Result<(), BackendError> {
        for dir in [
            "raw/sources/pdfs",
            "raw/sources/docs",
            "raw/sources/slides",
            "raw/sources/sheets",
            "raw/sources/markdown",
            "raw/sources/links",
            "raw/sources/other",
            "raw/extracted",
            "raw/assets",
            "wiki/entities",
            "wiki/concepts",
            "wiki/sources",
            "wiki/queries",
            "wiki/synthesis",
            "wiki/comparisons",
            "exports/html",
            "skills",
            ".app/chats",
            ".app/tasks",
        ] {
            store.ensure_dir(context, dir)?;
        }
        Ok(())
    }

    pub fn open_project(&self, path: &str) -> Result<OpenProjectResponse, BackendError> {
        let root = canonicalize_root(path)?;
        let health = self.health_report(&root);

        if health.is_wiki_project {
            let context = ProjectContext::new(uuid::Uuid::new_v4().to_string(), root.clone());
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Project".to_string());
            let summary = self.scan_project(&context, Some(&name));
            return Ok(OpenProjectResponse::opened(summary));
        }

        let pending = self.plan_folder_initialization(&root)?;
        Ok(OpenProjectResponse::needs_confirmation(pending))
    }

    pub fn scan_project(
        &self,
        context: &ProjectContext,
        name_override: Option<&str>,
    ) -> ProjectSummary {
        let store = FileStore;
        let wiki_base = if context.wiki_dir.exists() {
            context.wiki_dir.clone()
        } else {
            context.root.clone()
        };

        let wiki_page_count = store
            .list_markdown_files(&wiki_base)
            .map(|files| files.len())
            .unwrap_or(0);
        let source_count = count_files_recursive(&context.raw_dir.join("sources"));
        let task_count = count_files(&context.app_dir.join("tasks"));
        let index_state = if context.wiki_dir.join("index.md").exists()
            || context.root.join("index.md").exists()
        {
            IndexState::Indexed
        } else {
            IndexState::Missing
        };
        let graph_state = if graph_cache_has_content(&context.app_dir.join("graph-cache.json")) {
            GraphState::Cached
        } else {
            GraphState::Missing
        };

        let name = name_override
            .map(str::to_string)
            .or_else(|| {
                context
                    .root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Untitled".to_string());

        let template = self.read_project_template(context).unwrap_or_default();

        ProjectSummary {
            project_id: context.project_id.clone(),
            name,
            root_path: context.root.to_string_lossy().replace('\\', "/"),
            template,
            wiki_page_count,
            source_count,
            task_count,
            index_state,
            graph_state,
            agent_route: AgentRoute::Unconfigured,
            health: self.health_report(&context.root),
        }
    }

    pub fn list_recent_projects(&self) -> Result<Vec<RecentProject>, BackendError> {
        let path = self.recent_projects_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let store = FileStore;
        match store.read_json_file::<RecentProjectsFile>(&path) {
            Ok(file) => Ok(file
                .projects
                .into_iter()
                .map(|entry| self.enrich_recent_project(entry))
                .collect()),
            Err(_) => Ok(Vec::new()),
        }
    }

    fn enrich_recent_project(&self, mut entry: RecentProject) -> RecentProject {
        let root = PathBuf::from(&entry.root_path);
        if !root.exists() {
            entry.missing = true;
            entry.wiki_page_count = 0;
            entry.source_count = 0;
            entry.task_count = 0;
            entry.index_state = IndexState::Missing;
            entry.graph_state = GraphState::Missing;
            return entry;
        }
        let context = ProjectContext::new(entry.project_id.clone(), root);
        let summary = self.scan_project(&context, Some(&entry.name));
        entry.name = summary.name;
        entry.root_path = summary.root_path;
        entry.template = summary.template;
        entry.wiki_page_count = summary.wiki_page_count;
        entry.source_count = summary.source_count;
        entry.task_count = summary.task_count;
        entry.index_state = summary.index_state;
        entry.graph_state = summary.graph_state;
        entry.missing = false;
        entry
    }

    pub fn remember_recent_project(
        &self,
        project: RecentProject,
    ) -> Result<Vec<RecentProject>, BackendError> {
        fs::create_dir_all(&self.config_dir).map_err(|err| {
            BackendError::new("PROJECT_CONFIG_DIR_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": self.config_dir.to_string_lossy() }))
        })?;

        let mut projects = self.list_recent_projects().unwrap_or_default();
        let normalized_root = normalize_root_key(&project.root_path);
        projects.retain(|entry| normalize_root_key(&entry.root_path) != normalized_root);
        projects.insert(0, project);
        projects.truncate(MAX_RECENT_PROJECTS);

        let store = FileStore;
        store.write_json_atomic_absolute(
            &self.recent_projects_path(),
            &RecentProjectsFile {
                projects: projects.clone(),
            },
        )?;
        Ok(projects)
    }

    pub fn plan_folder_initialization(&self, root: &Path) -> Result<PendingAction, BackendError> {
        let loose_files = loose_top_level_files(root);
        let affected_paths: Vec<String> =
            loose_files.iter().map(|(name, _)| name.clone()).collect();

        let summary = if loose_files.is_empty() {
            "Folder is empty; only the project structure will be created.".to_string()
        } else {
            format!(
                "{} file(s) will be organized into raw/ by type. No files will be moved until you confirm.",
                loose_files.len()
            )
        };

        let preview_detail = loose_files
            .iter()
            .map(|(name, target)| format!("- {name} -> {target}"))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(PendingAction {
            id: uuid::Uuid::new_v4().to_string(),
            action_type: PendingActionType::InitializeFolder,
            title: "Initialize folder as project".to_string(),
            message: "This folder is not a wiki project yet. Confirming will create the project structure (purpose.md, schema.md, raw/, wiki/, .app/, exports/) and organize existing files by type. Raw sources stay immutable after import.".to_string(),
            risk_level: RiskLevel::Medium,
            affected_paths,
            preview: Some(ActionPreview {
                summary,
                before: None,
                after: Some(preview_detail),
                diff: None,
            }),
            expires_at: None,
            checkpoint_hash: None,
        })
    }

    pub fn confirm_folder_initialization(
        &self,
        root: &Path,
        pending_action: &PendingAction,
        expected_hashes: &[(String, String)],
    ) -> Result<(ProjectSummary, bool), BackendError> {
        let current_files = loose_top_level_files(root);
        let current_paths: Vec<String> =
            current_files.iter().map(|(path, _)| path.clone()).collect();
        let mut expected_paths = pending_action.affected_paths.clone();
        let mut sorted_current = current_paths.clone();
        expected_paths.sort();
        sorted_current.sort();
        if expected_paths != sorted_current {
            return Err(BackendError::new(
                "CONFIRMATION_STATE_MISMATCH",
                "The folder changed after confirmation was requested. Review it again before continuing.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "expectedPaths": expected_paths,
                "currentPaths": sorted_current,
            })));
        }

        let context = ProjectContext::new(uuid::Uuid::new_v4().to_string(), root.to_path_buf());
        self.verify_initialization_hashes(&context, expected_hashes)?;

        let git = GitService;
        let pre_state = git.initialize_repository(&context, "Before folder initialization")?;
        if pre_state.has_changes {
            let _ = git.create_checkpoint(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "Before folder initialization",
            )?;
        }
        self.ensure_skeleton(&context, &FileStore)?;
        self.write_missing_project_files(&context)?;
        self.archive_loose_files(&context, &current_files)?;
        let _ = git.create_checkpoint(
            &context,
            CheckpointPurpose::FinalResult,
            "Initialize wiki project structure",
        )?;

        let mut summary = self.scan_project(&context, None);
        summary.health.is_wiki_project = true;
        let checkpoint_exists = git.repository_status(&context)?.head.is_some();
        Ok((summary, checkpoint_exists))
    }

    pub fn folder_initialization_execution(
        &self,
        root: &Path,
        pending_action: &PendingAction,
    ) -> Result<ConfirmationExecution, BackendError> {
        let context = ProjectContext::new(uuid::Uuid::new_v4().to_string(), root.to_path_buf());
        let store = FileStore;
        let mut file_hashes = Vec::new();
        for relative_path in &pending_action.affected_paths {
            file_hashes.push((
                relative_path.clone(),
                store.file_hash(&context, relative_path)?,
            ));
        }
        Ok(ConfirmationExecution::InitializeFolder {
            root_path: root.to_string_lossy().to_string(),
            file_hashes,
        })
    }

    fn verify_initialization_hashes(
        &self,
        context: &ProjectContext,
        expected_hashes: &[(String, String)],
    ) -> Result<(), BackendError> {
        let store = FileStore;
        for (relative_path, expected_hash) in expected_hashes {
            let current_hash = store.file_hash(context, relative_path)?;
            if &current_hash != expected_hash {
                return Err(BackendError::new(
                    "CONFIRMATION_STATE_MISMATCH",
                    "A file changed after confirmation was requested. Review the action again.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "path": relative_path,
                    "expectedHash": expected_hash,
                    "currentHash": current_hash,
                })));
            }
        }
        Ok(())
    }

    fn write_missing_project_files(&self, context: &ProjectContext) -> Result<(), BackendError> {
        let store = FileStore;
        if !context.root.join("purpose.md").exists() {
            store.write_markdown(
                context,
                "purpose.md",
                template_purpose(ProjectTemplate::General),
            )?;
        }
        if !context.root.join("schema.md").exists() {
            store.write_markdown(
                context,
                "schema.md",
                template_schema(ProjectTemplate::General),
            )?;
        }
        for (path, contents) in [
            ("wiki/index.md", starter_index("Wiki Project")),
            ("wiki/log.md", starter_log("Wiki Project")),
            ("wiki/overview.md", starter_overview("Wiki Project")),
        ] {
            if !context.resolve_project_path(path)?.exists() {
                store.write_markdown(context, path, &contents)?;
            }
        }
        store.write_json_atomic(
            context,
            ".app/settings.json",
            &ProjectSettings {
                template: ProjectTemplate::General,
            },
        )?;
        store.write_json_atomic(context, ".app/agent-config.json", &serde_json::json!({}))?;
        store.write_json_atomic(context, ".app/bookmarks.json", &serde_json::json!([]))?;
        store.write_json_atomic(
            context,
            ".app/graph-cache.json",
            &serde_json::json!({ "nodes": [], "edges": [] }),
        )?;
        store.write_json_atomic(
            context,
            ".app/import-conflicts.json",
            &serde_json::json!({ "conflicts": [] }),
        )?;
        Ok(())
    }

    fn archive_loose_files(
        &self,
        context: &ProjectContext,
        files: &[(String, String)],
    ) -> Result<(), BackendError> {
        for (relative_source, target_dir) in files {
            let source = context.resolve_project_path(relative_source)?;
            if !source.exists() {
                return Err(BackendError::new(
                    "CONFIRMATION_STATE_MISMATCH",
                    "A file listed in the pending action no longer exists.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": relative_source })));
            }
            let file_name = source
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .ok_or_else(|| {
                    BackendError::new(
                        "FILE_NAME_INVALID",
                        "Cannot archive a path without a file name.",
                        true,
                        true,
                    )
                })?;
            let target_relative = unique_archive_target(context, target_dir, &file_name)?;
            let target = context.resolve_project_path(&target_relative)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    BackendError::new("FILE_DIR_CREATE_FAILED", err.to_string(), true, false)
                        .with_details(serde_json::json!({ "path": parent.to_string_lossy() }))
                })?;
            }
            fs::rename(&source, &target).map_err(|err| {
                BackendError::new("FILE_MOVE_FAILED", err.to_string(), true, false).with_details(
                    serde_json::json!({
                        "from": source.to_string_lossy(),
                        "to": target.to_string_lossy(),
                    }),
                )
            })?;
        }
        Ok(())
    }

    fn recent_projects_path(&self) -> PathBuf {
        self.config_dir.join(RECENT_PROJECT_FILE)
    }

    fn read_project_template(&self, context: &ProjectContext) -> Option<ProjectTemplate> {
        let store = FileStore;
        let settings: ProjectSettings = store.read_json(context, ".app/settings.json").ok()?;
        Some(settings.template)
    }

    fn health_report(&self, root: &Path) -> ProjectHealthReport {
        let has_purpose = has_child_named(root, "purpose.md");
        let has_schema = has_child_named(root, "schema.md");
        let has_app_state = root.join(".app").exists();
        let has_obsidian = root.join(".obsidian").exists();
        let has_wiki_dir = root.join("wiki").exists();

        let required = [
            ("purpose.md", has_child_named(root, "purpose.md")),
            ("schema.md", has_child_named(root, "schema.md")),
            ("raw/sources", root.join("raw").join("sources").exists()),
            ("wiki", has_wiki_dir || root.join("index.md").exists()),
            (".app", has_app_state),
            ("exports", root.join("exports").exists()),
        ];
        let missing_paths: Vec<String> = required
            .iter()
            .filter(|(_, present)| !present)
            .map(|(name, _)| (*name).to_string())
            .collect();

        let is_wiki_project = has_purpose
            || has_schema
            || has_app_state
            || has_wiki_dir
            || has_obsidian
            || root.join("index.md").exists();

        ProjectHealthReport {
            is_wiki_project,
            has_purpose,
            has_schema,
            has_app_state,
            has_obsidian,
            missing_paths,
        }
    }
}

fn probe_writable_directory(directory: &Path) -> bool {
    let Ok(canonical_directory) = directory.canonicalize() else {
        return false;
    };
    if !canonical_directory.is_dir() {
        return false;
    }
    let probe_path = canonical_directory.join(format!(
        ".llm-wiki-writability-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    probe_writable_path(&probe_path)
}

fn probe_writable_project_directory(project_root: &Path, directory: &Path) -> bool {
    let Ok(canonical_root) = validate_existing_project_root(project_root) else {
        return false;
    };
    let Ok(safe_directory) = validate_existing_project_directory(project_root, directory) else {
        return false;
    };
    let Ok(canonical_directory) = safe_directory.canonicalize() else {
        return false;
    };
    if !canonical_directory.starts_with(&canonical_root) {
        return false;
    }
    let Ok(revalidated) = validate_existing_project_directory(project_root, &safe_directory) else {
        return false;
    };
    let Ok(current_directory) = revalidated.canonicalize() else {
        return false;
    };
    if current_directory != canonical_directory || !current_directory.starts_with(&canonical_root) {
        return false;
    }
    let probe_path = revalidated.join(format!(
        ".llm-wiki-writability-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    probe_writable_path(&probe_path)
}

fn probe_writable_path(probe_path: &Path) -> bool {
    let mut probe = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)
    {
        Ok(probe) => probe,
        Err(_) => return false,
    };
    // Arm cleanup only after create_new proves this process created the file.
    let cleanup = ProbeCleanup(probe_path.to_path_buf());
    let write_succeeded = probe
        .write_all(b"llm-wiki-writability-probe")
        .and_then(|_| probe.sync_all())
        .is_ok();
    drop(probe);
    let cleanup_succeeded = fs::remove_file(&probe_path).is_ok();
    drop(cleanup);
    write_succeeded && cleanup_succeeded
}

struct ProbeCleanup(PathBuf);

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ProjectSettings {
    #[serde(default)]
    template: ProjectTemplate,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RecentProjectsFile {
    #[serde(default)]
    projects: Vec<RecentProject>,
}

fn template_purpose(template: ProjectTemplate) -> &'static str {
    match template {
        ProjectTemplate::General => GENERAL_PURPOSE,
        ProjectTemplate::Research => RESEARCH_PURPOSE,
        ProjectTemplate::Reading => READING_PURPOSE,
        ProjectTemplate::PersonalGrowth => GROWTH_PURPOSE,
        ProjectTemplate::Business => BUSINESS_PURPOSE,
    }
}

fn template_schema(template: ProjectTemplate) -> &'static str {
    match template {
        ProjectTemplate::General => GENERAL_SCHEMA,
        ProjectTemplate::Research => RESEARCH_SCHEMA,
        ProjectTemplate::Reading => READING_SCHEMA,
        ProjectTemplate::PersonalGrowth => GROWTH_SCHEMA,
        ProjectTemplate::Business => BUSINESS_SCHEMA,
    }
}

fn starter_index(name: &str) -> String {
    format!(
        "# {name}\n\n> Wiki index. This is the navigation entry point the compiler keeps up to date.\n\nNo pages yet. Import sources and run a compile to populate the wiki.\n"
    )
}

fn starter_log(name: &str) -> String {
    format!("# Log\n\n> Append-only operation history for {name}.\n\n- Project initialized.\n")
}

fn starter_overview(name: &str) -> String {
    format!(
        "# Overview\n\n> Global summary of the {name} knowledge base. The compiler refreshes this as the wiki grows.\n\nThis project has no compiled content yet.\n"
    )
}

fn validate_root_for_creation(root_path: &str) -> Result<PathBuf, BackendError> {
    if root_path.trim().is_empty() {
        return Err(BackendError::new(
            "PROJECT_PATH_INVALID",
            "Project path cannot be empty.",
            true,
            true,
        ));
    }
    let root = PathBuf::from(root_path);
    let parent = root
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        if !parent.exists() {
            return Err(BackendError::new(
                "PROJECT_PARENT_MISSING",
                "The parent directory for the new project does not exist.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": parent.to_string_lossy() })));
        }
    }

    if root.exists() {
        let is_non_empty = fs::read_dir(&root)
            .map(|mut iter| iter.next().is_some())
            .unwrap_or(false);
        if is_non_empty {
            return Err(BackendError::new(
                "PROJECT_DIR_NOT_EMPTY",
                "The selected directory is not empty. Create a project in an empty folder or open it as an existing project.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": root.to_string_lossy() })));
        }
    }
    Ok(root)
}

fn canonicalize_root(path: &str) -> Result<PathBuf, BackendError> {
    if path.trim().is_empty() {
        return Err(BackendError::new(
            "PROJECT_PATH_INVALID",
            "Project path cannot be empty.",
            true,
            true,
        ));
    }
    let raw = PathBuf::from(path);
    if !raw.exists() {
        return Err(BackendError::new(
            "PROJECT_NOT_FOUND",
            "The selected project folder does not exist.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": path })));
    }
    raw.canonicalize()
        .or(Ok(raw))
        .map_err(|err: std::io::Error| {
            BackendError::new("PROJECT_PATH_INVALID", err.to_string(), true, true)
        })
}

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(file_type) if file_type.is_file() => count += 1,
                Ok(file_type) if file_type.is_dir() => stack.push(entry.path()),
                _ => {}
            }
        }
    }
    count
}

fn graph_cache_has_content(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };
    let nodes = value
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let edges = value
        .get("edges")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    nodes > 0 || edges > 0
}

fn has_child_named(root: &Path, expected: &str) -> bool {
    let expected_lower = expected.to_ascii_lowercase();
    fs::read_dir(root)
        .map(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry.file_name().to_string_lossy().to_ascii_lowercase() == expected_lower
            })
        })
        .unwrap_or(false)
}

fn loose_top_level_files(root: &Path) -> Vec<(String, String)> {
    // Recursively enumerate loose files so the PendingAction preview reports the
    // full set of files a later "organize" step would relocate — never just the
    // top level. Skips dotfiles/dotdirs (e.g. .git, .obsidian, .app) everywhere.
    let mut mapped = Vec::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel_prefix)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let rel = if rel_prefix.is_empty() {
                file_name.clone()
            } else {
                format!("{rel_prefix}/{file_name}")
            };
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                stack.push((path, rel));
                continue;
            }
            if !path.is_file() {
                continue;
            }
            mapped.push((rel, archive_target_for(&path)));
        }
    }
    mapped.sort();
    mapped
}

fn archive_target_for(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "raw/sources/pdfs/".to_string(),
        "doc" | "docx" | "rtf" | "odt" => "raw/sources/docs/".to_string(),
        "ppt" | "pptx" => "raw/sources/slides/".to_string(),
        "xls" | "xlsx" | "csv" | "tsv" => "raw/sources/sheets/".to_string(),
        "md" | "markdown" | "txt" => "raw/sources/markdown/".to_string(),
        "url" | "webloc" => "raw/sources/links/".to_string(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "raw/assets/".to_string(),
        _ => "raw/sources/other/".to_string(),
    }
}

fn unique_archive_target(
    context: &ProjectContext,
    target_dir: &str,
    file_name: &str,
) -> Result<String, BackendError> {
    let clean_dir = target_dir.trim_end_matches('/');
    let candidate = format!("{clean_dir}/{file_name}");
    if !context.resolve_project_path(&candidate)?.exists() {
        return Ok(candidate);
    }

    let source_name = Path::new(file_name);
    let stem = source_name
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let extension = source_name
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();

    for index in 1..1000 {
        let candidate = format!("{clean_dir}/{stem}-{index}{extension}");
        if !context.resolve_project_path(&candidate)?.exists() {
            return Ok(candidate);
        }
    }

    Err(BackendError::new(
        "FILE_ARCHIVE_TARGET_UNAVAILABLE",
        "Could not find a safe archive target for the file.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "fileName": file_name })))
}

fn normalize_root_key(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn default_config_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("llm-wiki-desktop");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("llm-wiki-desktop");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("llm-wiki-desktop");
    }
    std::env::temp_dir().join("llm-wiki-desktop")
}

#[cfg(test)]
mod tests {
    use super::ProjectService;
    use crate::models::confirmation::ConfirmationExecution;
    use crate::models::confirmation::{PendingActionType, RiskLevel};
    use crate::models::paths::ProjectContext;
    use crate::models::project::{
        GraphState, IndexState, ProjectFilesystemAccess, ProjectTemplate,
    };
    use crate::services::GitService;
    use std::fs;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-wiki-project-{label}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn service_in_temp() -> (ProjectService, PathBuf) {
        let config = unique_temp_dir("config");
        (ProjectService::with_config_dir(config.clone()), config)
    }

    fn expected_dirs() -> &'static [&'static str] {
        &[
            "raw/sources/pdfs",
            "raw/sources/docs",
            "raw/sources/slides",
            "raw/sources/sheets",
            "raw/sources/markdown",
            "raw/sources/links",
            "raw/sources/other",
            "raw/extracted",
            "raw/assets",
            "wiki/entities",
            "wiki/concepts",
            "wiki/sources",
            "wiki/queries",
            "wiki/synthesis",
            "wiki/comparisons",
            "exports/html",
            "skills",
            ".app/chats",
            ".app/tasks",
        ]
    }

    fn expected_files() -> &'static [&'static str] {
        &[
            "purpose.md",
            "schema.md",
            "wiki/index.md",
            "wiki/log.md",
            "wiki/overview.md",
            ".app/settings.json",
            ".app/agent-config.json",
            ".app/bookmarks.json",
            ".app/graph-cache.json",
            ".app/import-conflicts.json",
        ]
    }

    #[test]
    fn untrusted_filesystem_access_is_fail_closed_without_a_probe_write() {
        let root = unique_temp_dir("untrusted-access");
        fs::write(root.join("现有.md"), "# Existing").unwrap();
        let context = ProjectContext::new("untrusted", root.clone());
        let (service, config) = service_in_temp();
        let before = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        let access = service.filesystem_access(&context, false);

        let after = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(access, ProjectFilesystemAccess::ReadOnly);
        assert_eq!(after, before);
        assert!(!root.join(".app").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn trusted_writable_probe_cleans_up_and_does_not_invent_task_state() {
        let root = unique_temp_dir("trusted-access");
        let context = ProjectContext::new("trusted", root.clone());
        let (service, config) = service_in_temp();

        let access = service.filesystem_access(&context, true);

        assert_eq!(access, ProjectFilesystemAccess::Writable);
        assert!(fs::read_dir(&root).unwrap().next().is_none());
        assert!(!service.has_writable_task_state_root(&context));
        assert!(!root.join(".app").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn task_state_root_must_already_exist_and_be_path_safe() {
        let root = unique_temp_dir("task-state-root");
        let context = ProjectContext::new("trusted", root.clone());
        let (service, config) = service_in_temp();

        assert!(!service.has_writable_task_state_root(&context));
        fs::create_dir_all(root.join(".app/tasks")).unwrap();
        assert!(service.has_writable_task_state_root(&context));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn failed_create_new_probe_never_deletes_a_preexisting_path() {
        let root = unique_temp_dir("occupied-probe");
        let occupied = root.join("occupied.tmp");
        fs::write(&occupied, "owned by someone else").unwrap();

        assert!(!super::probe_writable_path(&occupied));
        assert_eq!(
            fs::read_to_string(&occupied).unwrap(),
            "owned by someone else"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn read_only_task_state_root_forces_memory_only_without_probe_residue() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let root = unique_temp_dir("read-only-task-state");
        let task_root = root.join(".app/tasks");
        fs::create_dir_all(&task_root).unwrap();
        let context = ProjectContext::new("trusted", root.clone());
        let (service, config) = service_in_temp();
        fs::set_permissions(&task_root, fs::Permissions::from_mode(0o555)).unwrap();

        assert_eq!(
            service.filesystem_access(&context, true),
            ProjectFilesystemAccess::Writable
        );
        assert!(!service.has_writable_task_state_root(&context));
        assert!(fs::read_dir(&task_root).unwrap().next().is_none());

        fs::set_permissions(&task_root, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(config).ok();
    }

    #[cfg(unix)]
    #[test]
    fn trusted_read_only_directory_is_reported_read_only_without_probe_residue() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp_dir("read-only-access");
        let context = ProjectContext::new("trusted", root.clone());
        let (service, config) = service_in_temp();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();

        let access = service.filesystem_access(&context, true);

        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        if unsafe { libc::geteuid() } == 0 {
            fs::remove_dir_all(root).unwrap();
            fs::remove_dir_all(config).ok();
            return;
        }
        assert_eq!(access, ProjectFilesystemAccess::ReadOnly);
        assert!(fs::read_dir(&root).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn create_project_builds_full_skeleton_and_templates() {
        let (service, config) = service_in_temp();
        let root = unique_temp_dir("create-root");
        let target = root.join("llm-wiki-create-target");

        let summary = service
            .create_project(
                target.to_string_lossy().as_ref(),
                "Agent Wiki",
                ProjectTemplate::Research,
            )
            .expect("create_project should succeed");

        for dir in expected_dirs() {
            assert!(target.join(dir).exists(), "missing dir: {dir}");
        }
        for file in expected_files() {
            assert!(target.join(file).exists(), "missing file: {file}");
        }

        let purpose = fs::read_to_string(target.join("purpose.md")).unwrap();
        assert!(purpose.contains("Research knowledge base"));
        let schema = fs::read_to_string(target.join("schema.md")).unwrap();
        assert!(schema.contains("research"));

        assert_eq!(summary.template, ProjectTemplate::Research);
        assert_eq!(summary.wiki_page_count, 3); // index.md, log.md, overview.md
        assert!(summary.health.is_wiki_project);
        assert!(
            target.join(".git").exists(),
            "new projects must initialize Git"
        );

        let recents = service.list_recent_projects().unwrap_or_default();
        let _ = recents;
        fs::remove_dir_all(config).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn create_project_rejects_non_empty_directory() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("nonempty");
        fs::write(root.join("leftover.txt"), "x").unwrap();

        let err = service
            .create_project(
                root.to_string_lossy().as_ref(),
                "X",
                ProjectTemplate::General,
            )
            .expect_err("non-empty dir must be rejected");
        assert_eq!(err.code, "PROJECT_DIR_NOT_EMPTY");
        assert!(!root.join("purpose.md").exists());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn open_project_opens_existing_wiki_like_folder() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("open-existing");
        fs::write(root.join("schema.md"), "# schema").unwrap();
        fs::write(root.join("index.md"), "# index").unwrap();
        fs::create_dir_all(root.join("concepts")).unwrap();
        fs::write(root.join("concepts").join("agent.md"), "# Agent").unwrap();

        let outcome = service
            .open_project(root.to_string_lossy().as_ref())
            .unwrap();
        match outcome {
            crate::models::project::OpenProjectResponse {
                kind: crate::models::project::OpenProjectKind::Opened,
                summary: Some(summary),
                ..
            } => {
                assert_eq!(summary.index_state, IndexState::Indexed);
                assert!(summary.wiki_page_count >= 2);
                assert!(summary.health.is_wiki_project);
            }
            _ => panic!("existing wiki folder should open without confirmation"),
        }

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn open_project_returns_pending_action_for_ordinary_folder_without_moving() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("ordinary");
        fs::write(root.join("report.pdf"), "%PDF-1.4").unwrap();
        fs::write(root.join("note.md"), "# note").unwrap();
        fs::write(root.join("photo.png"), "PNG").unwrap();
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes").join("deep.docx"), "doc").unwrap();

        let outcome = service
            .open_project(root.to_string_lossy().as_ref())
            .unwrap();
        let pending = match outcome {
            crate::models::project::OpenProjectResponse {
                kind: crate::models::project::OpenProjectKind::NeedsConfirmation,
                pending_action: Some(pending_action),
                ..
            } => pending_action,
            _ => panic!("ordinary folder should require confirmation"),
        };

        assert_eq!(pending.action_type, PendingActionType::InitializeFolder);
        assert_eq!(pending.risk_level, RiskLevel::Medium);
        assert!(pending.affected_paths.contains(&"report.pdf".to_string()));
        assert!(pending.affected_paths.contains(&"note.md".to_string()));
        // Nested files must appear so the preview never understates what a later
        // organize step would relocate.
        assert!(pending
            .affected_paths
            .contains(&"notes/deep.docx".to_string()));
        // Critical: nothing moved before confirmation.
        assert!(root.join("report.pdf").exists());
        assert!(root.join("note.md").exists());
        assert!(!root.join("raw").exists());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn confirm_folder_initialization_revalidates_state_and_initializes_git_project() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("confirm-ordinary");
        fs::write(root.join("report.pdf"), "%PDF-1.4").unwrap();
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes").join("deep.docx"), "doc").unwrap();

        let pending = service.plan_folder_initialization(&root).unwrap();
        let execution = service
            .folder_initialization_execution(&root, &pending)
            .unwrap();
        let ConfirmationExecution::InitializeFolder { file_hashes, .. } = execution else {
            unreachable!()
        };
        let (summary, checkpoint_exists) = service
            .confirm_folder_initialization(&root, &pending, &file_hashes)
            .expect("confirmation should initialize the folder");

        assert!(checkpoint_exists);
        assert!(root.join(".git").exists());
        assert!(root.join("purpose.md").exists());
        assert!(root.join("raw/sources/pdfs/report.pdf").exists());
        assert!(root.join("raw/sources/docs/deep.docx").exists());
        assert!(!root.join("report.pdf").exists());
        assert!(summary.health.is_wiki_project);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn confirm_folder_initialization_rejects_state_mismatch() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("confirm-mismatch");
        fs::write(root.join("report.pdf"), "%PDF-1.4").unwrap();
        let pending = service.plan_folder_initialization(&root).unwrap();
        let execution = service
            .folder_initialization_execution(&root, &pending)
            .unwrap();
        let ConfirmationExecution::InitializeFolder { file_hashes, .. } = execution else {
            unreachable!()
        };
        fs::write(root.join("new.md"), "# new").unwrap();

        let err = service
            .confirm_folder_initialization(&root, &pending, &file_hashes)
            .expect_err("changed folder state must fail safely");
        assert_eq!(err.code, "CONFIRMATION_STATE_MISMATCH");
        assert!(root.join("report.pdf").exists());
        assert!(root.join("new.md").exists());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn confirm_folder_initialization_rejects_same_path_content_change() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("confirm-content-mismatch");
        fs::write(root.join("report.pdf"), "first").unwrap();
        let pending = service.plan_folder_initialization(&root).unwrap();
        let execution = service
            .folder_initialization_execution(&root, &pending)
            .unwrap();
        let ConfirmationExecution::InitializeFolder { file_hashes, .. } = execution else {
            unreachable!()
        };
        fs::write(root.join("report.pdf"), "changed").unwrap();

        let err = service
            .confirm_folder_initialization(&root, &pending, &file_hashes)
            .expect_err("same-path content changes must fail safely");
        assert_eq!(err.code, "CONFIRMATION_STATE_MISMATCH");
        assert!(root.join("report.pdf").exists());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn confirm_folder_initialization_checkpoints_existing_dirty_repo_before_changes() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("confirm-dirty-repo");
        fs::write(root.join("existing.md"), "# existing").unwrap();
        let context = ProjectContext::new("project-1", root.clone());
        GitService
            .initialize_repository(&context, "Initial external repo")
            .unwrap();
        fs::write(root.join("report.pdf"), "%PDF-1.4").unwrap();

        let pending = service.plan_folder_initialization(&root).unwrap();
        let execution = service
            .folder_initialization_execution(&root, &pending)
            .unwrap();
        let ConfirmationExecution::InitializeFolder { file_hashes, .. } = execution else {
            unreachable!()
        };
        service
            .confirm_folder_initialization(&root, &pending, &file_hashes)
            .unwrap();

        let log = std::process::Command::new("git")
            .args(["log", "--oneline", "--format=%s"])
            .current_dir(&root)
            .output()
            .unwrap();
        let subjects = String::from_utf8_lossy(&log.stdout);
        assert!(subjects.contains("Before folder initialization"));
        assert!(subjects.contains("Initialize wiki project structure"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn remember_and_list_recent_projects_round_trip() {
        let (service, config) = service_in_temp();
        let root_a = unique_temp_dir("recent-a");
        let root_b = unique_temp_dir("recent-b");

        service
            .remember_recent_project(crate::models::project::RecentProject {
                project_id: "a".into(),
                name: "A".into(),
                root_path: root_a.to_string_lossy().to_string(),
                template: ProjectTemplate::General,
                opened_at: "2026-06-19T00:00:00Z".into(),
                wiki_page_count: 0,
                source_count: 0,
                task_count: 0,
                index_state: IndexState::Missing,
                graph_state: GraphState::Missing,
                missing: false,
            })
            .unwrap();
        let after = service
            .remember_recent_project(crate::models::project::RecentProject {
                project_id: "b".into(),
                name: "B".into(),
                root_path: root_b.to_string_lossy().to_string(),
                template: ProjectTemplate::Business,
                opened_at: "2026-06-19T00:00:01Z".into(),
                wiki_page_count: 0,
                source_count: 0,
                task_count: 0,
                index_state: IndexState::Missing,
                graph_state: GraphState::Missing,
                missing: false,
            })
            .unwrap();

        assert_eq!(after.len(), 2);
        assert_eq!(after[0].project_id, "b");

        // Re-remembering A dedupes by root path and moves it to the top.
        let merged = service
            .remember_recent_project(crate::models::project::RecentProject {
                project_id: "a".into(),
                name: "A".into(),
                root_path: root_a.to_string_lossy().to_string(),
                template: ProjectTemplate::General,
                opened_at: "2026-06-19T00:00:02Z".into(),
                wiki_page_count: 0,
                source_count: 0,
                task_count: 0,
                index_state: IndexState::Missing,
                graph_state: GraphState::Missing,
                missing: false,
            })
            .unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].project_id, "a");

        let raw = fs::read_to_string(config.join("recent-projects.json")).unwrap();
        assert!(raw.contains("projectId"));

        fs::remove_dir_all(config).ok();
        fs::remove_dir_all(root_a).ok();
        fs::remove_dir_all(root_b).ok();
    }

    #[test]
    fn scan_project_counts_nested_sources_and_ignores_empty_graph_cache() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("metadata");
        fs::create_dir_all(root.join("raw/sources/pdfs")).unwrap();
        fs::create_dir_all(root.join("raw/sources/docs")).unwrap();
        fs::create_dir_all(root.join(".app/tasks")).unwrap();
        fs::write(root.join("raw/sources/pdfs/report.pdf"), "pdf").unwrap();
        fs::write(root.join("raw/sources/docs/brief.docx"), "doc").unwrap();
        fs::write(
            root.join(".app/graph-cache.json"),
            "{\n  \"nodes\": [],\n  \"edges\": []\n}",
        )
        .unwrap();

        let context = ProjectContext::new("metadata", root.clone());
        let summary = service.scan_project(&context, Some("Metadata"));

        assert_eq!(summary.source_count, 2);
        assert_eq!(summary.graph_state, GraphState::Missing);

        fs::write(
            root.join(".app/graph-cache.json"),
            "{\n  \"nodes\": [{\"id\":\"a\"}],\n  \"edges\": []\n}",
        )
        .unwrap();
        let summary = service.scan_project(&context, Some("Metadata"));
        assert_eq!(summary.graph_state, GraphState::Cached);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn list_recent_projects_marks_missing_paths_without_deleting_them() {
        let (service, config) = service_in_temp();
        let missing = config.join("missing-project");
        service
            .remember_recent_project(crate::models::project::RecentProject {
                project_id: "missing".into(),
                name: "Missing".into(),
                root_path: missing.to_string_lossy().to_string(),
                template: ProjectTemplate::General,
                opened_at: "2026-07-04T00:00:00Z".into(),
                wiki_page_count: 0,
                source_count: 0,
                task_count: 0,
                index_state: IndexState::Missing,
                graph_state: GraphState::Missing,
                missing: false,
            })
            .unwrap();

        let listed = service.list_recent_projects().unwrap();
        assert_eq!(listed[0].project_id, "missing");
        assert!(listed[0].missing);
        assert!(service.recent_projects_path().exists());
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn open_project_opens_sample_wiki_vault_as_compatible() {
        let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let sample = manifest_root.join("..").join("wiki").join("wiki");
        if !sample.exists() {
            // Skip gracefully in environments without the sample repo.
            return;
        }
        let (service, _config) = service_in_temp();
        let outcome = service
            .open_project(sample.to_string_lossy().as_ref())
            .unwrap();
        match outcome {
            crate::models::project::OpenProjectResponse {
                kind: crate::models::project::OpenProjectKind::Opened,
                summary: Some(summary),
                ..
            } => {
                assert!(summary.health.is_wiki_project);
                assert!(summary.health.has_obsidian);
                assert!(
                    summary.wiki_page_count > 100,
                    "sample page count {}",
                    summary.wiki_page_count
                );
            }
            _ => panic!("sample wiki should open as a compatible project"),
        }
    }
}

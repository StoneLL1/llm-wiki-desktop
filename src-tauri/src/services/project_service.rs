use std::collections::HashSet;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::graph::GraphData;
use crate::models::layout::{
    canonical_internal_read_path, inspect_native_layout, is_link_or_reparse,
    native_repair_directory_allowed, project_descendant_path_enters_link, CompatibleLayoutMapping,
    NativeLayoutState, ProjectMarkdownRootRole, COMPATIBLE_LAYOUT_MAPPING_PATH,
};
use crate::models::paths::ProjectContext;
use crate::models::project::{
    AgentRoute, GraphState, IndexState, OpenProjectResponse, ProjectFilesystemAccess,
    ProjectHealthReport, ProjectInventoryState, ProjectRepairOperation, ProjectRepairOperationType,
    ProjectRepairPlan, ProjectSummary, ProjectTemplate, ProjectTrustKind, RecentProject,
};
use crate::services::file_store::{FileStore, WriteMode};
use crate::services::git_service::GitService;
use crate::utils::path_safety::{
    ensure_project_directory_with_created, validate_existing_project_directory,
    validate_existing_project_file, validate_existing_project_root,
};

pub(crate) mod assessment;
mod decision_store;
mod trust_store;

pub use assessment::{assess_project_folder, ProjectAssessmentService};
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
const NATIVE_PROJECT_ID_FILE: &str = ".app/project.json";
const MAX_RECENT_PROJECTS: usize = 20;
const PROJECT_CREATION_STAGING_PREFIX: &str = ".llm-wiki-create-";
const PROJECT_CREATION_BACKUP_PREFIX: &str = ".llm-wiki-create-backup-";
const MAX_REPAIR_GRAPH_CACHE_BYTES: u64 = 16 * 1024 * 1024;

/// Compatibility enablement is a mutation into a folder that may not yet
/// have app-owned state. Serializing it prevents two commands from our own
/// process from racing on the same `.app/compat` paths. This is deliberately
/// not presented as a cross-process file lock: every path-based write still
/// revalidates immediately before use, and an external actor can race any
/// path-based filesystem API.
static COMPATIBILITY_GUIDANCE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static PROJECT_REPAIR_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
/// Recent-project history is a global app file. This mutex avoids redundant
/// contention in one process; every mutation also takes the configuration
/// directory's OS-backed mutation lock so separate LLM Wiki processes cannot
/// overwrite each other's read-modify-write cycles.
static RECENT_PROJECT_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub struct ProjectService {
    config_dir: PathBuf,
    trust_store: ProjectTrustStore,
    #[cfg(test)]
    forced_read_only_roots: Mutex<HashSet<PathBuf>>,
}

impl Default for ProjectService {
    fn default() -> Self {
        let config_dir = default_config_dir();
        Self {
            trust_store: ProjectTrustStore::new(&config_dir),
            config_dir,
            #[cfg(test)]
            forced_read_only_roots: Mutex::new(HashSet::new()),
        }
    }
}

impl ProjectService {
    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self {
            trust_store: ProjectTrustStore::new(&config_dir),
            config_dir,
            #[cfg(test)]
            forced_read_only_roots: Mutex::new(HashSet::new()),
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

    /// Prepares only the app-owned container used by the first-run project
    /// dialog. It deliberately never creates a project or changes a folder
    /// supplied by the user through the directory picker.
    pub(crate) fn prepare_default_project_parent(&self) -> Result<String, BackendError> {
        let documents = dirs::document_dir().ok_or_else(|| {
            BackendError::new(
                "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
                "The system Documents directory is unavailable. Choose a parent folder instead.",
                true,
                true,
            )
        })?;
        prepare_default_project_parent_at(&documents)
            .map(|path| path.to_string_lossy().into_owned())
    }

    pub(crate) fn filesystem_access(
        &self,
        context: &ProjectContext,
        trusted: bool,
    ) -> ProjectFilesystemAccess {
        if !trusted {
            return ProjectFilesystemAccess::ReadOnly;
        }
        #[cfg(test)]
        if self
            .forced_read_only_roots
            .lock()
            .is_ok_and(|roots| roots.contains(&context.root))
        {
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

    #[cfg(test)]
    pub(crate) fn force_read_only_for_test(&self, root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        self.forced_read_only_roots
            .lock()
            .expect("forced read-only roots lock")
            .insert(root);
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

    pub fn enable_compatible_guidance(
        &self,
        context: &ProjectContext,
        template: ProjectTemplate,
    ) -> Result<Vec<String>, BackendError> {
        let _write_guard = compatibility_guidance_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _initial_root = validate_existing_project_root(&context.root).map_err(|message| {
            compatibility_path_unsafe_error(
                "Compatibility guidance cannot be written to an unsafe project path.",
                message,
            )
        })?;
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        let root = validate_existing_project_root(&context.root).map_err(|message| {
            compatibility_path_unsafe_error(
                "Compatibility guidance project path changed while waiting for the write lock.",
                message,
            )
        })?;
        let compat_dir = root.join(".app").join("compat");
        let (compat_dir, mut created_dirs) =
            ensure_project_directory_with_created(&root, &compat_dir).map_err(|message| {
                compatibility_path_unsafe_error(
                    "Compatibility guidance cannot use a linked or unsafe directory.",
                    message,
                )
            })?;
        // Persistence is a separate capability from content mutation.  These
        // two empty app-owned roots are the minimum durable state needed for
        // project-scoped tasks and workflow history; all user content roots
        // remain absent for a state-only compatible vault.
        for state_root in ["tasks", "workflows"] {
            let (_, mut created) =
                ensure_project_directory_with_created(&root, &compat_dir.join(state_root))
                    .map_err(|message| {
                        compatibility_path_unsafe_error(
                            "Compatibility state cannot use a linked or unsafe directory.",
                            message,
                        )
                    })?;
            created_dirs.append(&mut created);
        }
        let mut created_files = Vec::new();

        let result = (|| {
            for (name, contents) in [
                ("purpose.md", template_purpose(template)),
                ("schema.md", template_schema(template)),
            ] {
                // Path checks are intentionally repeated close to each file
                // operation. `ensure_project_directory_with_created` also
                // re-walked these components after creation, but neither
                // check can turn a path-based filesystem API into a
                // cross-process atomic no-follow operation.
                validate_existing_project_directory(&root, &compat_dir).map_err(|message| {
                    compatibility_path_unsafe_error(
                        "Compatibility guidance cannot use a linked or unsafe directory.",
                        message,
                    )
                })?;
                let target = compat_dir.join(name);
                match fs::symlink_metadata(&target) {
                    Ok(_) => {
                        let safe_target =
                            validate_existing_project_file(&root, &target).map_err(|message| {
                                compatibility_path_unsafe_error(
                                    "Compatibility guidance cannot use a linked or unsafe file.",
                                    message,
                                )
                            })?;
                        let existing = fs::read(&safe_target).map_err(|error| {
                            BackendError::new(
                                "PROJECT_COMPAT_GUIDANCE_READ_FAILED",
                                "Existing compatibility guidance could not be verified.",
                                true,
                                true,
                            )
                            .with_details(serde_json::json!({ "error": error.to_string() }))
                        })?;
                        validate_existing_project_file(&root, &target).map_err(|message| {
                            compatibility_path_unsafe_error(
                                "Compatibility guidance changed while it was being verified.",
                                message,
                            )
                        })?;
                        if existing == contents.as_bytes() {
                            continue;
                        }
                        return Err(BackendError::new(
                            "PROJECT_COMPAT_GUIDANCE_EXISTS",
                            "Existing compatibility guidance will not be overwritten.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({
                            "path": format!(".app/compat/{name}"),
                        })));
                    }
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(BackendError::new(
                            "PROJECT_COMPAT_GUIDANCE_READ_FAILED",
                            "Existing compatibility guidance could not be verified.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "error": error.to_string() })));
                    }
                }

                validate_existing_project_directory(&root, &compat_dir).map_err(|message| {
                    compatibility_path_unsafe_error(
                        "Compatibility guidance cannot use a linked or unsafe directory.",
                        message,
                    )
                })?;
                let temporary = compat_dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
                let write_result = (|| {
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&temporary)
                        .map_err(|error| {
                            BackendError::new(
                                "PROJECT_COMPAT_GUIDANCE_WRITE_FAILED",
                                "Compatibility guidance could not be written.",
                                true,
                                true,
                            )
                            .with_details(serde_json::json!({ "error": error.to_string() }))
                        })?;
                    validate_existing_project_file(&root, &temporary).map_err(|message| {
                        compatibility_path_unsafe_error(
                            "Compatibility guidance temporary file became unsafe before writing.",
                            message,
                        )
                    })?;
                    file.write_all(contents.as_bytes()).map_err(|error| {
                        BackendError::new(
                            "PROJECT_COMPAT_GUIDANCE_WRITE_FAILED",
                            "Compatibility guidance could not be written.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "error": error.to_string() }))
                    })?;
                    file.sync_all().map_err(|error| {
                        BackendError::new(
                            "PROJECT_COMPAT_GUIDANCE_WRITE_FAILED",
                            "Compatibility guidance could not be synchronized.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "error": error.to_string() }))
                    })?;
                    validate_existing_project_directory(&root, &compat_dir).map_err(|message| {
                        compatibility_path_unsafe_error(
                            "Compatibility guidance directory changed before commit.",
                            message,
                        )
                    })?;
                    fs::rename(&temporary, &target).map_err(|error| {
                        BackendError::new(
                            "PROJECT_COMPAT_GUIDANCE_COMMIT_FAILED",
                            "Compatibility guidance could not be committed.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "error": error.to_string() }))
                    })?;
                    validate_existing_project_file(&root, &target).map_err(|message| {
                        compatibility_path_unsafe_error(
                            "Compatibility guidance target became unsafe after commit.",
                            message,
                        )
                    })?;
                    Ok::<(), BackendError>(())
                })();
                if write_result.is_err() {
                    remove_compatible_file_if_safe(&root, &temporary);
                }
                write_result?;
                created_files.push(target);
            }
            Ok::<(), BackendError>(())
        })();

        if let Err(error) = result {
            for file in created_files.iter().rev() {
                remove_compatible_file_if_safe(&root, file);
            }
            for directory in created_dirs.iter().rev() {
                remove_compatible_directory_if_safe(&root, directory);
            }
            return Err(error);
        }

        Ok(vec![
            ".app/compat/purpose.md".into(),
            ".app/compat/schema.md".into(),
            ".app/compat/tasks".into(),
            ".app/compat/workflows".into(),
        ])
    }

    /// Persists a confirmed association with existing compatible-vault
    /// directories.  This never creates or rewrites Markdown, raw sources, or
    /// functional roots.  The caller supplies the preview hash so an external
    /// mapping edit between preview and confirmation fails rather than being
    /// overwritten.
    pub(crate) fn write_compatible_layout_mapping(
        &self,
        context: &ProjectContext,
        mapping: &CompatibleLayoutMapping,
        expected_hash: Option<&str>,
    ) -> Result<(), BackendError> {
        let _write_guard = compatibility_guidance_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = validate_existing_project_root(&context.root).map_err(|message| {
            compatibility_path_unsafe_error(
                "Compatible layout mapping cannot use an unsafe project path.",
                message,
            )
        })?;
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        let purpose = root.join(".app/compat/purpose.md");
        let schema = root.join(".app/compat/schema.md");
        validate_existing_project_file(&root, &purpose).map_err(|message| {
            compatibility_path_unsafe_error(
                "Compatible layout mapping requires verified app-owned guidance.",
                message,
            )
        })?;
        validate_existing_project_file(&root, &schema).map_err(|message| {
            compatibility_path_unsafe_error(
                "Compatible layout mapping requires verified app-owned guidance.",
                message,
            )
        })?;
        mapping.validate_existing_roots(&root)?;
        let mapping_path = root.join(COMPATIBLE_LAYOUT_MAPPING_PATH);
        let mode = match expected_hash {
            Some(hash) => WriteMode::OverwriteIfHashMatches(hash.to_string()),
            None => WriteMode::CreateNew,
        };
        FileStore
            .write_json_atomic_checked(context, COMPATIBLE_LAYOUT_MAPPING_PATH, mapping, mode)
            .map_err(|error| {
                if matches!(
                    error.code.as_str(),
                    "FILE_HASH_MISMATCH" | "FILE_ALREADY_EXISTS" | "FILE_NOT_FOUND"
                ) {
                    BackendError::new(
                        "PROJECT_COMPAT_LAYOUT_CHANGED",
                        "The compatible layout mapping changed after preview. Prepare it again.",
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({ "path": COMPATIBLE_LAYOUT_MAPPING_PATH }))
                } else {
                    error
                }
            })?;
        validate_existing_project_file(&root, &mapping_path).map_err(|message| {
            compatibility_path_unsafe_error(
                "Compatible layout mapping became unsafe after writing.",
                message,
            )
        })?;
        Ok(())
    }

    /// Prepares the only currently supported automatic recovery: replacing an
    /// invalid graph cache with a known empty cache after preserving the exact
    /// invalid bytes. The preparation phase is read-only and intentionally
    /// refuses every other corrupt `.app` JSON file because those files may
    /// contain user preferences, bookmarks, or workflow state that cannot be
    /// reconstructed without guessing.
    pub(crate) fn prepare_graph_cache_repair_plan(
        &self,
        context: &ProjectContext,
        canonical_identity_key: String,
        identity_revision: String,
        expected_git_head: Option<String>,
        expected_git_paths: Vec<String>,
    ) -> Result<ProjectRepairPlan, BackendError> {
        let root = repair_safe_root(context)?;
        let target = root.join(".app").join("graph-cache.json");
        let bytes = read_invalid_graph_cache(&root, &target)?;
        let repair_plan_id = uuid::Uuid::new_v4().to_string();
        let backup_path =
            format!(".app/recovery-backups/graph-cache.{repair_plan_id}.invalid.json");

        Ok(ProjectRepairPlan {
            repair_plan_id,
            canonical_identity_key,
            identity_revision,
            expected_git_head,
            expected_git_paths,
            operations: vec![ProjectRepairOperation {
                operation_type: ProjectRepairOperationType::RegenerateGraphCache,
                target_path: ".app/graph-cache.json".into(),
                backup_path: Some(backup_path),
                expected_hash: Some(sha256_hex(&bytes)),
                allowlist_descriptor: None,
            }],
            protected_paths: vec![
                "raw/".into(),
                "wiki/".into(),
                "purpose.md".into(),
                "schema.md".into(),
            ],
            external_links_remain_blocked: true,
        })
    }

    /// Executes an already-confirmed recovery plan. The caller must revalidate
    /// assessment identity and Git state immediately before this method; this
    /// layer then revalidates the exact file hash and every project-owned path
    /// it touches. A failed replacement leaves the original cache and, once
    /// written, its standalone backup intact for manual recovery.
    pub(crate) fn apply_graph_cache_repair_plan(
        &self,
        context: &ProjectContext,
        plan: &ProjectRepairPlan,
    ) -> Result<Vec<String>, BackendError> {
        let _write_guard = project_repair_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _initial_root = repair_safe_root(context)?;
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        let root = repair_safe_root(context)?;
        let [operation] = plan.operations.as_slice() else {
            return Err(BackendError::new(
                "PROJECT_REPAIR_PLAN_UNSUPPORTED",
                "The repair plan contains unsupported operations.",
                true,
                true,
            ));
        };
        if operation.operation_type != ProjectRepairOperationType::RegenerateGraphCache
            || operation.target_path != ".app/graph-cache.json"
            || !operation
                .backup_path
                .as_deref()
                .is_some_and(|path| path.starts_with(".app/recovery-backups/graph-cache."))
            || !operation
                .backup_path
                .as_deref()
                .is_some_and(|path| path.ends_with(".invalid.json"))
        {
            return Err(BackendError::new(
                "PROJECT_REPAIR_PLAN_UNSUPPORTED",
                "The repair plan contains an unsafe operation.",
                true,
                true,
            ));
        }

        let target = root.join(".app").join("graph-cache.json");
        let bytes = read_invalid_graph_cache(&root, &target)?;
        if operation.expected_hash.as_deref() != Some(sha256_hex(&bytes).as_str()) {
            return Err(BackendError::new(
                "PROJECT_REPAIR_TARGET_CHANGED",
                "The corrupt cache changed after the repair preview. Prepare repair again.",
                true,
                true,
            ));
        }

        let backup_path = operation
            .backup_path
            .as_deref()
            .expect("validated repair backup path");
        let backup = root.join(backup_path);
        let backup_directory = backup.parent().ok_or_else(|| {
            BackendError::new(
                "PROJECT_REPAIR_PLAN_UNSUPPORTED",
                "The repair backup path is invalid.",
                false,
                true,
            )
        })?;
        ensure_project_directory_with_created(&root, backup_directory).map_err(|message| {
            repair_path_unsafe_error(
                "Repair backup cannot be created in an unsafe project path.",
                message,
            )
        })?;
        validate_existing_project_directory(&root, backup_directory).map_err(|message| {
            repair_path_unsafe_error("Repair backup directory became unsafe.", message)
        })?;
        match fs::symlink_metadata(&backup) {
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(BackendError::new(
                    "PROJECT_REPAIR_BACKUP_EXISTS",
                    "The prepared recovery backup path is no longer empty. Prepare repair again.",
                    true,
                    true,
                ));
            }
            Err(error) => {
                return Err(BackendError::new(
                    "PROJECT_REPAIR_BACKUP_UNAVAILABLE",
                    "The recovery backup path could not be verified.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "error": error.to_string() })));
            }
        }
        write_repair_backup(&root, &backup, &bytes)?;

        let replacement =
            serde_json::to_vec_pretty(&GraphData::empty(String::new())).map_err(|error| {
                BackendError::new(
                    "PROJECT_REPAIR_WRITE_FAILED",
                    "The regenerated graph cache could not be serialized.",
                    false,
                    false,
                )
                .with_details(serde_json::json!({ "error": error.to_string() }))
            })?;
        replace_graph_cache_atomically(&root, &target, &replacement)?;

        Ok(vec![backup_path.to_string(), operation.target_path.clone()])
    }

    /// Prepares a directory-only repair for a recognized legacy native
    /// layout.  This is intentionally independent from graph-cache recovery:
    /// empty-directory creation neither overwrites content nor creates a Git
    /// tree entry, so it must not pretend to have a cache-repair backup or
    /// checkpoint contract.
    pub(crate) fn prepare_native_layout_repair_plan(
        &self,
        context: &ProjectContext,
        canonical_identity_key: String,
        identity_revision: String,
    ) -> Result<ProjectRepairPlan, BackendError> {
        let root = repair_safe_root(context)?;
        let NativeLayoutState::RepairableLegacy { missing } = inspect_native_layout(&root).state
        else {
            return Err(BackendError::new(
                "PROJECT_NATIVE_REPAIR_UNAVAILABLE",
                "This project does not have a safe legacy native layout repair.",
                true,
                true,
            ));
        };
        if missing.is_empty() {
            return Err(BackendError::new(
                "PROJECT_NATIVE_REPAIR_UNAVAILABLE",
                "No missing native directories can be repaired safely.",
                true,
                true,
            ));
        }
        let mut operations = Vec::with_capacity(missing.len());
        for requirement in missing {
            let target_path = requirement.relative_path();
            let target = root.join(target_path);
            match fs::symlink_metadata(&target) {
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Ok(_) => {
                    return Err(BackendError::new(
                        "PROJECT_NATIVE_REPAIR_UNAVAILABLE",
                        "A required native path is no longer safely missing.",
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({ "path": target_path })));
                }
                Err(error) => {
                    return Err(BackendError::new(
                        "PROJECT_NATIVE_REPAIR_UNAVAILABLE",
                        "A required native path could not be inspected safely.",
                        true,
                        true,
                    )
                    .with_details(
                        serde_json::json!({ "path": target_path, "error": error.to_string() }),
                    ));
                }
            }
            operations.push(ProjectRepairOperation {
                operation_type: ProjectRepairOperationType::CreateDirectory,
                target_path: target_path.into(),
                backup_path: None,
                expected_hash: None,
                allowlist_descriptor: Some(format!(
                    "native-layout-v{}",
                    crate::models::layout::CURRENT_NATIVE_LAYOUT_VERSION
                )),
            });
        }

        Ok(ProjectRepairPlan {
            repair_plan_id: uuid::Uuid::new_v4().to_string(),
            canonical_identity_key,
            identity_revision,
            expected_git_head: None,
            expected_git_paths: Vec::new(),
            operations,
            protected_paths: vec![
                "raw/".into(),
                "wiki/".into(),
                "purpose.md".into(),
                "schema.md".into(),
            ],
            external_links_remain_blocked: true,
        })
    }

    /// Applies only an already-previewed set of allowlisted missing
    /// directories.  Every target is rechecked immediately before creation;
    /// a failure removes only directories created by this invocation and only
    /// when they remain empty.
    pub(crate) fn apply_native_layout_repair_plan(
        &self,
        context: &ProjectContext,
        plan: &ProjectRepairPlan,
    ) -> Result<Vec<String>, BackendError> {
        let _write_guard = project_repair_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _initial_root = repair_safe_root(context)?;
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        let root = repair_safe_root(context)?;

        let NativeLayoutState::RepairableLegacy { missing } = inspect_native_layout(&root).state
        else {
            return Err(BackendError::new(
                "PROJECT_NATIVE_REPAIR_STALE",
                "The native layout changed after the repair preview. Prepare repair again.",
                true,
                true,
            ));
        };
        let expected = missing
            .iter()
            .map(|requirement| requirement.relative_path())
            .collect::<HashSet<_>>();
        let planned = plan
            .operations
            .iter()
            .map(|operation| operation.target_path.as_str())
            .collect::<HashSet<_>>();
        if expected != planned || planned.is_empty() || planned.len() != plan.operations.len() {
            return Err(BackendError::new(
                "PROJECT_NATIVE_REPAIR_STALE",
                "The planned native directories no longer match the project layout.",
                true,
                true,
            ));
        }

        let mut created = Vec::new();
        let descriptor = format!(
            "native-layout-v{}",
            crate::models::layout::CURRENT_NATIVE_LAYOUT_VERSION
        );
        let result = (|| {
            for operation in &plan.operations {
                if operation.operation_type != ProjectRepairOperationType::CreateDirectory
                    || !native_repair_directory_allowed(&operation.target_path)
                    || operation.backup_path.is_some()
                    || operation.expected_hash.is_some()
                    || operation.allowlist_descriptor.as_deref() != Some(descriptor.as_str())
                {
                    return Err(BackendError::new(
                        "PROJECT_REPAIR_PLAN_UNSUPPORTED",
                        "The repair plan contains an unsafe native directory operation.",
                        true,
                        true,
                    ));
                }
                let target = root.join(&operation.target_path);
                match fs::symlink_metadata(&target) {
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Ok(_) => {
                        return Err(BackendError::new(
                            "PROJECT_NATIVE_REPAIR_STALE",
                            "A native repair target appeared after preview. Prepare repair again.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "path": operation.target_path })));
                    }
                    Err(error) => {
                        return Err(BackendError::new(
                            "PROJECT_REPAIR_PATH_UNSAFE",
                            "A native repair target could not be inspected safely.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "path": operation.target_path, "error": error.to_string() })));
                    }
                }
                let created_paths =
                    create_native_repair_directory(&root, &target).map_err(|message| {
                        repair_path_unsafe_error(
                            "Native repair cannot create an unsafe directory.",
                            message,
                        )
                    })?;
                if !created_paths.iter().any(|path| path == &target) {
                    return Err(BackendError::new(
                        "PROJECT_NATIVE_REPAIR_STALE",
                        "A native repair target appeared while the repair was running. Prepare repair again.",
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({ "path": operation.target_path })));
                }
                for path in created_paths {
                    let relative = path.strip_prefix(&root).map_err(|_| {
                        BackendError::new(
                            "PROJECT_REPAIR_PATH_UNSAFE",
                            "Native repair created a path outside the project.",
                            false,
                            true,
                        )
                    })?;
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    if !native_repair_directory_allowed(&relative) {
                        return Err(BackendError::new(
                            "PROJECT_REPAIR_PLAN_UNSUPPORTED",
                            "Native repair attempted to create a directory outside its allowlist.",
                            false,
                            true,
                        ));
                    }
                    created.push(path);
                }
            }
            Ok(())
        })();

        if let Err(error) = result {
            for directory in created.iter().rev() {
                remove_compatible_directory_if_safe(&root, directory);
            }
            return Err(error);
        }

        Ok(created
            .iter()
            .filter_map(|path| path.strip_prefix(&root).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect())
    }

    pub fn create_project(
        &self,
        root_path: &str,
        name: &str,
        template: ProjectTemplate,
    ) -> Result<ProjectSummary, BackendError> {
        let root = validate_root_for_creation(root_path)?;
        validate_project_name(name)?;
        let root_existed = root.exists();
        // Build every project privately first, including when the user chose
        // an empty existing directory. This prevents a concurrent write from
        // turning an initially empty target into a partially initialized one.
        let build_root = create_project_staging_root(&root)?;
        let project_id = uuid::Uuid::new_v4().to_string();
        let context = ProjectContext::new(project_id.clone(), build_root.clone());
        let store = FileStore;

        let build_result = self.populate_new_project(&context, &store, name, template);
        if let Err(error) = build_result {
            return Err(project_creation_failure(
                error,
                &root,
                &build_root,
                root_existed,
            ));
        }

        if let Err(error) = install_staged_project(&root, &build_root, root_existed) {
            return Err(project_creation_failure(
                error,
                &root,
                &build_root,
                root_existed,
            ));
        }

        let final_context = ProjectContext::new(project_id, root.clone());
        let mut summary = self.scan_project(&final_context, Some(name));
        summary.template = template;
        summary.health.is_wiki_project = true;
        Ok(summary)
    }

    fn populate_new_project(
        &self,
        context: &ProjectContext,
        store: &FileStore,
        name: &str,
        template: ProjectTemplate,
    ) -> Result<(), BackendError> {
        self.ensure_skeleton(context, store)?;

        store.write_markdown(context, "purpose.md", template_purpose(template))?;
        store.write_markdown(context, "schema.md", template_schema(template))?;
        store.write_markdown(context, "wiki/index.md", &starter_index(name))?;
        store.write_markdown(context, "wiki/log.md", &starter_log(name))?;
        store.write_markdown(context, "wiki/overview.md", &starter_overview(name))?;

        store.write_json_atomic(
            context,
            NATIVE_PROJECT_ID_FILE,
            &NativeProjectIdentityFile {
                project_id: context.project_id.clone(),
            },
        )?;
        let project_settings = ProjectSettings { template };
        store.write_json_atomic(context, ".app/settings.json", &project_settings)?;
        store.write_json_atomic(context, ".app/agent-config.json", &serde_json::json!({}))?;
        store.write_json_atomic(context, ".app/bookmarks.json", &serde_json::json!([]))?;
        store.write_json_atomic(
            context,
            ".app/graph-cache.json",
            &GraphData::empty(String::new()),
        )?;
        store.write_json_atomic(
            context,
            ".app/import-conflicts.json",
            &serde_json::json!({ "conflicts": [] }),
        )?;
        GitService.initialize_repository(context, "Initial wiki project")?;
        Ok(())
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
            let project_id = self
                .stable_native_project_id(&root)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let context = ProjectContext::new(project_id, root.clone()).with_resolved_layout()?;
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Project".to_string());
            let summary = self.scan_project(&context, Some(&name));
            return Ok(OpenProjectResponse::opened(summary));
        }

        Err(BackendError::new(
            "PROJECT_OPEN_REQUIRES_ASSESSMENT",
            "This folder is not a directly openable knowledge base. Assess it before choosing whether to open it as a knowledge base or create a separate knowledge base for its materials.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "path": root.to_string_lossy(),
        })))
    }

    pub fn scan_project(
        &self,
        context: &ProjectContext,
        name_override: Option<&str>,
    ) -> ProjectSummary {
        let wiki_page_count = layout_markdown_count(
            context,
            &[
                ProjectMarkdownRootRole::Wiki,
                ProjectMarkdownRootRole::Mixed,
            ],
        );
        let source_count = layout_source_count(context);
        let task_count = layout_task_count(context);
        self.project_summary(
            context,
            name_override,
            wiki_page_count,
            source_count,
            task_count,
            ProjectInventoryState::Ready,
        )
    }

    /// Returns only metadata that is cheap to obtain when a folder has just
    /// been opened. The complete file inventory is deliberately performed by
    /// a cancellable background task so large vaults can enter the workbench
    /// immediately.
    pub fn quick_project_summary(
        &self,
        context: &ProjectContext,
        name_override: Option<&str>,
    ) -> ProjectSummary {
        self.project_summary(
            context,
            name_override,
            0,
            0,
            0,
            ProjectInventoryState::Scanning,
        )
    }

    /// Performs a read-only, cancellation-aware inventory. Markdown page
    /// counts may traverse only contained descendant links using the same
    /// read-only policy as the index; source and task state never do.
    pub fn scan_project_inventory<C, P>(
        &self,
        context: &ProjectContext,
        name_override: Option<&str>,
        cancelled: C,
        mut on_progress: P,
    ) -> ProjectSummary
    where
        C: Fn() -> bool,
        P: FnMut(u64, String),
    {
        let wiki_pages = count_inventory_markdown_roles(
            context,
            &[
                ProjectMarkdownRootRole::Wiki,
                ProjectMarkdownRootRole::Mixed,
            ],
            &cancelled,
            &mut on_progress,
        );
        if wiki_pages.cancelled {
            return self.project_summary(
                context,
                name_override,
                wiki_pages.count,
                0,
                0,
                ProjectInventoryState::Partial,
            );
        }

        let sources = count_inventory_sources(context, &cancelled, &mut on_progress);
        if sources.cancelled {
            return self.project_summary(
                context,
                name_override,
                wiki_pages.count,
                sources.count,
                0,
                ProjectInventoryState::Partial,
            );
        }

        let tasks = count_inventory_tasks(context, &cancelled, &mut on_progress);
        self.project_summary(
            context,
            name_override,
            wiki_pages.count,
            sources.count,
            tasks.count,
            if tasks.cancelled {
                ProjectInventoryState::Partial
            } else {
                ProjectInventoryState::Ready
            },
        )
    }

    fn project_summary(
        &self,
        context: &ProjectContext,
        name_override: Option<&str>,
        wiki_page_count: usize,
        source_count: usize,
        task_count: usize,
        inventory_state: ProjectInventoryState,
    ) -> ProjectSummary {
        let index_state = if context
            .layout
            .wiki_index_path
            .as_deref()
            .and_then(|path| context.resolve_project_path(path).ok())
            .is_some_and(|path| path.exists())
        {
            IndexState::Indexed
        } else {
            IndexState::Missing
        };
        let graph_state = if context
            .layout
            .graph_cache_path
            .as_deref()
            .and_then(|path| context.resolve_project_path(path).ok())
            .is_some_and(|path| graph_cache_has_content(&path))
        {
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
            inventory_state,
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
        let Ok(context) =
            ProjectContext::new(entry.project_id.clone(), root).with_resolved_layout()
        else {
            entry.wiki_page_count = 0;
            entry.source_count = 0;
            entry.task_count = 0;
            entry.index_state = IndexState::Missing;
            entry.graph_state = GraphState::Missing;
            return entry;
        };
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
        let _write_guard = recent_project_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        fs::create_dir_all(&self.config_dir).map_err(|err| {
            BackendError::new("PROJECT_CONFIG_DIR_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": self.config_dir.to_string_lossy() }))
        })?;

        let store = FileStore;
        let mut projects =
            match store.read_json_file::<RecentProjectsFile>(&self.recent_projects_path()) {
                Ok(file) => file.projects,
                Err(_) => Vec::new(),
            };
        let normalized_root = normalize_root_key(&project.root_path);
        projects.retain(|entry| normalize_root_key(&entry.root_path) != normalized_root);
        projects.insert(0, project);
        projects.truncate(MAX_RECENT_PROJECTS);

        store.write_json_atomic_absolute(
            &self.recent_projects_path(),
            &RecentProjectsFile {
                projects: projects.clone(),
            },
        )?;
        Ok(projects)
    }

    /// Forgetting a recent entry only updates the global application list. The
    /// root path is part of the selector so a stale project ID cannot remove a
    /// different folder's entry.
    pub fn remove_recent_project(
        &self,
        project_id: &str,
        root_path: &str,
    ) -> Result<Vec<RecentProject>, BackendError> {
        let _write_guard = recent_project_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        let path = self.recent_projects_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let store = FileStore;
        let mut file = store.read_json_file::<RecentProjectsFile>(&path)?;
        let normalized_root = normalize_root_key(root_path);
        let entry_count = file.projects.len();
        file.projects.retain(|entry| {
            entry.project_id != project_id
                || normalize_root_key(&entry.root_path) != normalized_root
        });
        if file.projects.len() != entry_count {
            store.write_json_atomic_absolute(&path, &file)?;
        }
        Ok(file.projects)
    }

    /// Returns the project ID generated during native knowledge-base creation.
    /// Legacy and compatible folders intentionally return `None`: their
    /// existing layout cannot safely prove that a later folder is a move of
    /// the same knowledge base.
    pub fn stable_native_project_id(&self, root: &Path) -> Option<String> {
        let identity_path = root.join(NATIVE_PROJECT_ID_FILE);
        let safe_path = validate_existing_project_file(root, &identity_path).ok()?;
        let record = FileStore
            .read_json_file::<NativeProjectIdentityFile>(&safe_path)
            .ok()?;
        uuid::Uuid::parse_str(&record.project_id)
            .ok()
            .map(|id| id.to_string())
    }

    /// Reads a newly-created native project's durable identity for a
    /// user-requested relocation. Unlike the normal open fallback, missing,
    /// malformed, or linked identity data fails closed here so picker success
    /// cannot be mistaken for proof of the same project.
    pub fn require_stable_native_project_id(&self, root: &Path) -> Result<String, BackendError> {
        let identity_path = root.join(NATIVE_PROJECT_ID_FILE);
        let safe_path = validate_existing_project_file(root, &identity_path).map_err(|error| {
            BackendError::new(
                "PROJECT_RELOCATION_ID_UNAVAILABLE",
                "The selected native knowledge base has no safe durable identity for relocation.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error }))
        })?;
        let record = FileStore
            .read_json_file::<NativeProjectIdentityFile>(&safe_path)
            .map_err(|error| {
                BackendError::new(
                    "PROJECT_RELOCATION_ID_INVALID",
                    "The selected native knowledge base has an invalid durable identity.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "error": error }))
            })?;
        uuid::Uuid::parse_str(&record.project_id)
            .map(|id| id.to_string())
            .map_err(|_| {
                BackendError::new(
                    "PROJECT_RELOCATION_ID_INVALID",
                    "The selected native knowledge base has an invalid durable identity.",
                    true,
                    true,
                )
            })
    }

    /// Atomically replaces one exact recent entry after the command layer has
    /// proven that the selected native root owns the same durable project ID.
    /// It only updates global app configuration and never scans or writes a
    /// project directory.
    pub fn relocate_recent_project(
        &self,
        previous_project_id: &str,
        previous_root_path: &str,
        relocated: RecentProject,
    ) -> Result<Vec<RecentProject>, BackendError> {
        if relocated.project_id != previous_project_id {
            return Err(BackendError::new(
                "PROJECT_RELOCATION_ID_MISMATCH",
                "The selected knowledge base does not match the recent project identity.",
                true,
                true,
            ));
        }
        let _write_guard = recent_project_write_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _process_guard = self.trust_store.acquire_project_mutation_lock()?;
        match fs::symlink_metadata(previous_root_path) {
            Ok(_) => {
                return Err(BackendError::new(
                    "PROJECT_RELOCATION_SOURCE_AVAILABLE",
                    "The original recent knowledge-base path is still available and cannot be relocated.",
                    true,
                    true,
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(BackendError::new(
                    "PROJECT_RELOCATION_SOURCE_UNVERIFIABLE",
                    "The original recent knowledge-base path cannot be verified as missing.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "error": error.to_string() })));
            }
        }
        let verified_project_id =
            self.require_stable_native_project_id(Path::new(&relocated.root_path))?;
        if verified_project_id != previous_project_id {
            return Err(BackendError::new(
                "PROJECT_RELOCATION_ID_MISMATCH",
                "The selected knowledge base does not match the recent project identity.",
                true,
                true,
            ));
        }
        let path = self.recent_projects_path();
        let store = FileStore;
        let mut file = store
            .read_json_file::<RecentProjectsFile>(&path)
            .map_err(|_| {
                BackendError::new(
                    "PROJECT_RECENT_RELOCATION_NOT_FOUND",
                    "The original recent knowledge-base entry is no longer available.",
                    true,
                    true,
                )
            })?;
        let previous_root = normalize_root_key(previous_root_path);
        let previous_index = file.projects.iter().position(|entry| {
            entry.project_id == previous_project_id
                && normalize_root_key(&entry.root_path) == previous_root
        });
        let Some(previous_index) = previous_index else {
            return Err(BackendError::new(
                "PROJECT_RECENT_RELOCATION_NOT_FOUND",
                "The original recent knowledge-base entry is no longer available.",
                true,
                true,
            ));
        };
        let relocated_root = normalize_root_key(&relocated.root_path);
        if file.projects.iter().enumerate().any(|(index, entry)| {
            index != previous_index && normalize_root_key(&entry.root_path) == relocated_root
        }) {
            return Err(BackendError::new(
                "PROJECT_RECENT_RELOCATION_TARGET_CONFLICT",
                "Another recent knowledge-base entry already uses the selected folder.",
                true,
                true,
            ));
        }
        file.projects.remove(previous_index);
        file.projects.insert(0, relocated);
        store.write_json_atomic_absolute(&path, &file)?;
        Ok(file.projects)
    }

    fn recent_projects_path(&self) -> PathBuf {
        self.config_dir.join(RECENT_PROJECT_FILE)
    }

    fn read_project_template(&self, context: &ProjectContext) -> Option<ProjectTemplate> {
        let settings_path = context.layout.settings_path.as_deref()?;
        let store = FileStore;
        let settings: ProjectSettings = store.read_json(context, settings_path).ok()?;
        Some(settings.template)
    }

    fn health_report(&self, root: &Path) -> ProjectHealthReport {
        let native_inspection = inspect_native_layout(root);
        let has_purpose = has_child_named(root, "purpose.md");
        let has_schema = has_child_named(root, "schema.md");
        let has_app_state = root.join(".app").exists();
        let has_obsidian = root.join(".obsidian").exists();
        let has_wiki_dir = root.join("wiki").exists();

        let missing_paths = match native_inspection.state {
            NativeLayoutState::RepairableLegacy { missing } => missing
                .iter()
                .map(|requirement| requirement.relative_path().to_string())
                .collect(),
            NativeLayoutState::IncompleteLegacy { reasons } => reasons
                .iter()
                .map(|reason| match reason {
                    crate::models::layout::NativeLayoutGap::MissingSemanticFile(path) => {
                        (*path).to_string()
                    }
                })
                .collect(),
            _ => Vec::new(),
        };

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

fn compatibility_guidance_write_lock() -> &'static Mutex<()> {
    COMPATIBILITY_GUIDANCE_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn recent_project_write_lock() -> &'static Mutex<()> {
    RECENT_PROJECT_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn compatibility_path_unsafe_error(message: &str, error: String) -> BackendError {
    BackendError::new("PROJECT_COMPAT_PATH_UNSAFE", message, true, true)
        .with_details(serde_json::json!({ "error": error }))
}

/// Roll back only a file whose current path still passes the same descendant
/// no-link checks required for normal project writes. If the path changed
/// underneath us, leaving the artifact is safer than deleting somebody else's
/// file through a redirected path.
fn remove_compatible_file_if_safe(root: &Path, file: &Path) {
    if validate_existing_project_file(root, file).is_ok() {
        let _ = fs::remove_file(file);
    }
}

/// See [`remove_compatible_file_if_safe`]. `remove_dir` is intentionally
/// non-recursive, so it cannot remove unplanned content if another process
/// populated the directory while compatibility enablement was failing.
fn remove_compatible_directory_if_safe(root: &Path, directory: &Path) {
    if validate_existing_project_directory(root, directory).is_ok() {
        let _ = fs::remove_dir(directory);
    }
}

fn project_repair_write_lock() -> &'static Mutex<()> {
    PROJECT_REPAIR_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn repair_safe_root(context: &ProjectContext) -> Result<PathBuf, BackendError> {
    validate_existing_project_root(&context.root).map_err(|message| {
        repair_path_unsafe_error(
            "Recovery repair cannot use an unsafe project path.",
            message,
        )
    })
}

/// Creates one directory path without ever resolving a descendant through a
/// link or reparse point. This is separate from the generic app-state helper:
/// a native repair promises that it creates only the previewed empty paths,
/// even while another process changes the project tree.
#[cfg(unix)]
fn create_native_repair_directory(root: &Path, target: &Path) -> Result<Vec<PathBuf>, String> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    let relative = target
        .strip_prefix(root)
        .map_err(|_| "Native repair target is outside the project root".to_string())?;
    let root_name = CString::new(root.as_os_str().as_bytes())
        .map_err(|_| "Project root contains an unsupported NUL byte".to_string())?;
    let root_fd = unsafe {
        libc::open(
            root_name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(format!(
            "Project root could not be opened without following links: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut handles = vec![unsafe { fs::File::from_raw_fd(root_fd) }];
    let mut current = root.to_path_buf();
    let mut created = Vec::new();

    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err("Native repair target contains an unsafe path component".into());
        };
        let name = CString::new(segment.as_bytes())
            .map_err(|_| "Native repair target contains an unsupported NUL byte".to_string())?;
        let parent_fd = handles
            .last()
            .expect("the project root handle is retained during repair")
            .as_raw_fd();
        let mut made = false;
        let mut child_fd = unsafe {
            libc::openat(
                parent_fd,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if child_fd < 0 && std::io::Error::last_os_error().kind() == ErrorKind::NotFound {
            let result = unsafe { libc::mkdirat(parent_fd, name.as_ptr(), 0o755) };
            if result == 0 {
                made = true;
            } else if std::io::Error::last_os_error().kind() != ErrorKind::AlreadyExists {
                return Err(format!(
                    "Native repair could not create a directory: {}",
                    std::io::Error::last_os_error()
                ));
            }
            child_fd = unsafe {
                libc::openat(
                    parent_fd,
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
        }
        if child_fd < 0 {
            return Err(format!(
                "Native repair encountered a linked or invalid directory: {}",
                std::io::Error::last_os_error()
            ));
        }
        current.push(segment);
        if made {
            created.push(current.clone());
        }
        handles.push(unsafe { fs::File::from_raw_fd(child_fd) });
    }
    Ok(created)
}

#[cfg(windows)]
fn create_native_repair_directory(root: &Path, target: &Path) -> Result<Vec<PathBuf>, String> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    fn open_locked_directory(path: &Path) -> Result<fs::File, String> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|error| {
                format!(
                    "Native repair cannot open directory {}: {error}",
                    path.display()
                )
            })?;
        let metadata = file.metadata().map_err(|error| {
            format!(
                "Native repair cannot inspect directory {}: {error}",
                path.display()
            )
        })?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!(
                "Native repair encountered a linked or invalid directory: {}",
                path.display()
            ));
        }
        Ok(file)
    }

    let relative = target
        .strip_prefix(root)
        .map_err(|_| "Native repair target is outside the project root".to_string())?;
    let mut handles = vec![open_locked_directory(root)?];
    let mut current = root.to_path_buf();
    let mut created = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err("Native repair target contains an unsafe path component".into());
        };
        current.push(segment);
        let made = match fs::create_dir(&current) {
            Ok(()) => true,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => false,
            Err(error) => {
                return Err(format!(
                    "Native repair could not create directory {}: {error}",
                    current.display()
                ));
            }
        };
        let handle = open_locked_directory(&current)?;
        if made {
            created.push(current.clone());
        }
        handles.push(handle);
    }
    Ok(created)
}

#[cfg(not(any(unix, windows)))]
fn create_native_repair_directory(root: &Path, target: &Path) -> Result<Vec<PathBuf>, String> {
    ensure_project_directory_with_created(root, target).map(|(_, created)| created)
}

fn repair_path_unsafe_error(message: &str, error: String) -> BackendError {
    BackendError::new("PROJECT_REPAIR_PATH_UNSAFE", message, true, true)
        .with_details(serde_json::json!({ "error": error }))
}

fn read_invalid_graph_cache(root: &Path, target: &Path) -> Result<Vec<u8>, BackendError> {
    let safe_target = validate_existing_project_file(root, target).map_err(|message| {
        repair_path_unsafe_error(
            "The graph cache is not a safe regular file inside this project.",
            message,
        )
    })?;
    let metadata = fs::metadata(&safe_target).map_err(|error| {
        BackendError::new(
            "PROJECT_REPAIR_TARGET_UNAVAILABLE",
            "The graph cache could not be inspected for recovery.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "error": error.to_string() }))
    })?;
    if metadata.len() > MAX_REPAIR_GRAPH_CACHE_BYTES {
        return Err(BackendError::new(
            "PROJECT_REPAIR_TARGET_TOO_LARGE",
            "The corrupt graph cache is too large for safe automatic recovery.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "path": ".app/graph-cache.json",
            "maxBytes": MAX_REPAIR_GRAPH_CACHE_BYTES,
        })));
    }
    let bytes = fs::read(&safe_target).map_err(|error| {
        BackendError::new(
            "PROJECT_REPAIR_TARGET_UNAVAILABLE",
            "The graph cache could not be read for recovery.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "error": error.to_string() }))
    })?;
    if serde_json::from_slice::<GraphData>(&bytes).is_ok() {
        return Err(BackendError::new(
            "PROJECT_REPAIR_NOT_NEEDED",
            "The graph cache has a valid schema and does not need this recovery action.",
            true,
            true,
        ));
    }
    Ok(bytes)
}

fn write_repair_backup(root: &Path, backup: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let parent = backup.parent().ok_or_else(|| {
        BackendError::new(
            "PROJECT_REPAIR_PLAN_UNSUPPORTED",
            "The repair backup path is invalid.",
            false,
            true,
        )
    })?;
    validate_existing_project_directory(root, parent).map_err(|message| {
        repair_path_unsafe_error("Repair backup directory became unsafe.", message)
    })?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(backup)
        .map_err(|error| {
            BackendError::new(
                "PROJECT_REPAIR_BACKUP_WRITE_FAILED",
                "The corrupt graph cache could not be backed up.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
    validate_existing_project_file(root, backup).map_err(|message| {
        repair_path_unsafe_error("Repair backup path became unsafe before writing.", message)
    })?;
    file.write_all(bytes).map_err(|error| {
        BackendError::new(
            "PROJECT_REPAIR_BACKUP_WRITE_FAILED",
            "The corrupt graph cache backup could not be written.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "error": error.to_string() }))
    })?;
    file.sync_all().map_err(|error| {
        BackendError::new(
            "PROJECT_REPAIR_BACKUP_WRITE_FAILED",
            "The corrupt graph cache backup could not be synchronized.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "error": error.to_string() }))
    })
}

fn replace_graph_cache_atomically(
    root: &Path,
    target: &Path,
    replacement: &[u8],
) -> Result<(), BackendError> {
    let parent = target.parent().ok_or_else(|| {
        BackendError::new(
            "PROJECT_REPAIR_PLAN_UNSUPPORTED",
            "The graph cache path is invalid.",
            false,
            true,
        )
    })?;
    validate_existing_project_directory(root, parent).map_err(|message| {
        repair_path_unsafe_error(
            "Graph cache directory became unsafe before repair.",
            message,
        )
    })?;
    let temporary = parent.join(format!(".graph-cache.{}.repair.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                BackendError::new(
                    "PROJECT_REPAIR_WRITE_FAILED",
                    "The regenerated graph cache could not be prepared.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "error": error.to_string() }))
            })?;
        validate_existing_project_file(root, &temporary).map_err(|message| {
            repair_path_unsafe_error("Graph cache temporary path became unsafe.", message)
        })?;
        file.write_all(replacement).map_err(|error| {
            BackendError::new(
                "PROJECT_REPAIR_WRITE_FAILED",
                "The regenerated graph cache could not be written.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        file.sync_all().map_err(|error| {
            BackendError::new(
                "PROJECT_REPAIR_WRITE_FAILED",
                "The regenerated graph cache could not be synchronized.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        validate_existing_project_directory(root, parent).map_err(|message| {
            repair_path_unsafe_error(
                "Graph cache directory changed before repair commit.",
                message,
            )
        })?;
        fs::rename(&temporary, target).map_err(|error| {
            BackendError::new(
                "PROJECT_REPAIR_COMMIT_FAILED",
                "The regenerated graph cache could not replace the corrupt cache.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        validate_existing_project_file(root, target).map_err(|message| {
            repair_path_unsafe_error("Graph cache target became unsafe after repair.", message)
        })?;
        Ok::<(), BackendError>(())
    })();
    if write_result.is_err() {
        remove_compatible_file_if_safe(root, &temporary);
    }
    write_result
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
#[serde(rename_all = "camelCase")]
struct NativeProjectIdentityFile {
    project_id: String,
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

fn create_project_staging_root(root: &Path) -> Result<PathBuf, BackendError> {
    let parent = root.parent().ok_or_else(|| {
        BackendError::new(
            "PROJECT_PATH_INVALID",
            "A new knowledge base must have a parent directory.",
            true,
            true,
        )
    })?;
    for _ in 0..8 {
        let staging = parent.join(format!(
            "{PROJECT_CREATION_STAGING_PREFIX}{}",
            uuid::Uuid::new_v4()
        ));
        match fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(BackendError::new(
                    "PROJECT_CREATION_STAGE_FAILED",
                    "A private staging directory could not be created for the new knowledge base.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "parentPath": parent.to_string_lossy(),
                    "error": error.to_string(),
                })));
            }
        }
    }
    Err(BackendError::new(
        "PROJECT_CREATION_STAGE_FAILED",
        "A unique private staging directory could not be created for the new knowledge base.",
        true,
        true,
    ))
}

fn create_project_backup_path(root: &Path) -> Result<PathBuf, BackendError> {
    let parent = root.parent().ok_or_else(|| {
        BackendError::new(
            "PROJECT_PATH_INVALID",
            "A new knowledge base must have a parent directory.",
            true,
            true,
        )
    })?;
    for _ in 0..8 {
        let backup = parent.join(format!(
            "{PROJECT_CREATION_BACKUP_PREFIX}{}",
            uuid::Uuid::new_v4()
        ));
        if !backup.exists() {
            return Ok(backup);
        }
    }
    Err(BackendError::new(
        "PROJECT_CREATION_STAGE_FAILED",
        "A unique private backup path could not be reserved for the new knowledge base.",
        true,
        true,
    ))
}

fn install_staged_project(
    root: &Path,
    staging: &Path,
    root_existed: bool,
) -> Result<(), BackendError> {
    if !root_existed {
        if root.exists() {
            return Err(BackendError::new(
                "PROJECT_DIR_APPEARED",
                "The selected project directory appeared while the project was being created.",
                true,
                true,
            ));
        }
        return fs::rename(staging, root).map_err(|error| {
            BackendError::new(
                "PROJECT_CREATION_INSTALL_FAILED",
                "The prepared knowledge base could not be installed at the selected location.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "rootPath": root.to_string_lossy(),
                "stagingPath": staging.to_string_lossy(),
                "error": error.to_string(),
            }))
        });
    }

    let backup = create_project_backup_path(root)?;
    fs::rename(root, &backup).map_err(|error| {
        BackendError::new(
            "PROJECT_CREATION_TARGET_CLAIM_FAILED",
            "The selected empty directory could not be reserved for project creation.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "rootPath": root.to_string_lossy(),
            "backupPath": backup.to_string_lossy(),
            "error": error.to_string(),
        }))
    })?;

    let still_empty = normal_empty_directory(&backup).unwrap_or(false);
    if !still_empty {
        let restored = fs::rename(&backup, root).is_ok();
        return Err(BackendError::new(
            "PROJECT_DIR_CHANGED_DURING_CREATION",
            "The selected directory changed while the knowledge base was being prepared.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "rootPath": root.to_string_lossy(),
            "backupPath": backup.to_string_lossy(),
            "targetRestored": restored,
            "nextAction": "Review the selected directory and retry project creation.",
        })));
    }

    if let Err(error) = fs::rename(staging, root) {
        let restored = fs::rename(&backup, root).is_ok();
        return Err(BackendError::new(
            "PROJECT_CREATION_INSTALL_FAILED",
            "The prepared knowledge base could not be installed at the selected location.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "rootPath": root.to_string_lossy(),
            "stagingPath": staging.to_string_lossy(),
            "backupPath": backup.to_string_lossy(),
            "targetRestored": restored,
            "error": error.to_string(),
        })));
    }

    // The original target was verified empty immediately after its atomic
    // move. Use non-recursive removal only; if another process wrote there,
    // preserve that data rather than deleting it.
    let _ = fs::remove_dir(&backup);
    Ok(())
}

fn project_creation_failure(
    mut error: BackendError,
    root: &Path,
    build_root: &Path,
    root_existed: bool,
) -> BackendError {
    let mut remaining_paths = Vec::new();
    let rollback = match remove_owned_staging_root(build_root) {
        Ok(()) => "staging_removed",
        Err(reason) => {
            remaining_paths.push(build_root.to_string_lossy().into_owned());
            let existing = error.details.take();
            error.details = Some(serde_json::json!({
                "original": existing,
                "recovery": {
                    "rootPath": root.to_string_lossy(),
                    "stagingPath": build_root.to_string_lossy(),
                    "targetExistedBeforeCreation": root_existed,
                    "rollback": "staging_retained",
                    "remainingPaths": remaining_paths,
                    "reason": reason,
                },
            }));
            return error;
        }
    };

    let existing = error.details.take();
    error.details = Some(serde_json::json!({
        "original": existing,
        "recovery": {
            "rootPath": root.to_string_lossy(),
            "stagingPath": build_root.to_string_lossy(),
            "targetExistedBeforeCreation": root_existed,
            "rollback": rollback,
            "remainingPaths": remaining_paths,
        },
    }));
    error
}

fn remove_owned_staging_root(staging: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(staging).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("The staging path is no longer a normal directory.".into());
    }
    let file_name = staging
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !file_name.starts_with(PROJECT_CREATION_STAGING_PREFIX) {
        return Err(
            "The staging directory name no longer matches the private creation prefix.".into(),
        );
    }
    fs::remove_dir_all(staging).map_err(|error| error.to_string())
}

fn validate_project_name(name: &str) -> Result<(), BackendError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(BackendError::new(
            "PROJECT_NAME_REQUIRED",
            "Knowledge base name is required.",
            true,
            true,
        ));
    }
    if name.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
    }) {
        return Err(BackendError::new(
            "PROJECT_NAME_INVALID",
            "Knowledge base name contains characters that are not valid in a folder name.",
            true,
            true,
        ));
    }
    if name.ends_with(['.', ' ']) {
        return Err(BackendError::new(
            "PROJECT_NAME_INVALID",
            "Knowledge base name cannot end with a dot or space.",
            true,
            true,
        ));
    }
    let normalized = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(BackendError::new(
            "PROJECT_NAME_RESERVED",
            "Knowledge base name is reserved by Windows and cannot be used.",
            true,
            true,
        ));
    }
    Ok(())
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
    if root.to_string_lossy().chars().count() > 240 {
        return Err(BackendError::new(
            "PROJECT_PATH_TOO_LONG",
            "The final knowledge base path is too long.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": root.to_string_lossy() })));
    }
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
        if !parent.is_dir() {
            return Err(BackendError::new(
                "PROJECT_PARENT_INVALID",
                "The selected project parent is not a directory.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": parent.to_string_lossy() })));
        }
    }

    if root.exists() {
        if !normal_empty_directory(&root).unwrap_or(false) {
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

fn prepare_default_project_parent_at(documents: &Path) -> Result<PathBuf, BackendError> {
    let documents = documents.canonicalize().map_err(|error| {
        BackendError::new(
            "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
            "The system Documents directory is unavailable. Choose a parent folder instead.",
            true,
            true,
        )
        .with_details(
            serde_json::json!({ "path": documents.to_string_lossy(), "error": error.to_string() }),
        )
    })?;
    if !documents.is_dir() {
        return Err(BackendError::new(
            "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
            "The system Documents directory is unavailable. Choose a parent folder instead.",
            true,
            true,
        ));
    }

    let parent = documents.join("LLM Wiki");
    match fs::symlink_metadata(&parent) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(BackendError::new(
                "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
                "Documents/LLM Wiki is not a normal directory. Choose a parent folder instead.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": parent.to_string_lossy() })));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir(&parent).map_err(|error| {
                BackendError::new(
                    "PROJECT_DEFAULT_PARENT_CREATE_FAILED",
                    "The default Documents/LLM Wiki folder could not be created. Choose a parent folder instead.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": parent.to_string_lossy(), "error": error.to_string() }))
            })?;
            let metadata = fs::symlink_metadata(&parent).map_err(|error| {
                BackendError::new(
                    "PROJECT_DEFAULT_PARENT_CREATE_FAILED",
                    "The default Documents/LLM Wiki folder could not be verified. Choose a parent folder instead.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": parent.to_string_lossy(), "error": error.to_string() }))
            })?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(BackendError::new(
                    "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
                    "Documents/LLM Wiki is not a normal directory. Choose a parent folder instead.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": parent.to_string_lossy() })));
            }
        }
        Err(error) => {
            return Err(BackendError::new(
                "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
                "The default Documents/LLM Wiki folder could not be inspected. Choose a parent folder instead.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": parent.to_string_lossy(), "error": error.to_string() })));
        }
    }
    parent.canonicalize().map_err(|error| {
        BackendError::new(
            "PROJECT_DEFAULT_PARENT_UNAVAILABLE",
            "The default Documents/LLM Wiki folder could not be resolved. Choose a parent folder instead.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": parent.to_string_lossy(), "error": error.to_string() }))
    })
}

fn normal_empty_directory(path: &Path) -> Result<bool, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    Ok(fs::read_dir(path)?.next().is_none())
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

fn layout_markdown_count(context: &ProjectContext, roles: &[ProjectMarkdownRootRole]) -> usize {
    context
        .list_markdown_files_for_roles(roles)
        .map(|files| files.len())
        .unwrap_or(0)
}

fn layout_source_count(context: &ProjectContext) -> usize {
    let never_cancelled = || false;
    let mut no_progress = |_, _| {};
    count_inventory_sources(context, &never_cancelled, &mut no_progress).count
}

fn layout_task_count(context: &ProjectContext) -> usize {
    let never_cancelled = || false;
    let mut no_progress = |_, _| {};
    count_inventory_tasks(context, &never_cancelled, &mut no_progress).count
}

#[derive(Default)]
struct InventoryFileCount {
    count: usize,
    cancelled: bool,
}

fn layout_root_path(context: &ProjectContext, relative: &str) -> Option<PathBuf> {
    if relative == "." {
        Some(context.root.clone())
    } else {
        context.resolve_project_path(relative).ok()
    }
}

fn count_inventory_markdown_roles<C, P>(
    context: &ProjectContext,
    roles: &[ProjectMarkdownRootRole],
    cancelled: &C,
    on_progress: &mut P,
) -> InventoryFileCount
where
    C: Fn() -> bool,
    P: FnMut(u64, String),
{
    let mut combined = InventoryFileCount::default();
    for markdown_root in &context.layout.markdown_roots {
        if !roles.contains(&markdown_root.role) {
            continue;
        }
        let Some(root) = layout_root_path(context, &markdown_root.path) else {
            continue;
        };
        let result = count_inventory_files(
            &context.root,
            &root,
            true,
            true,
            markdown_root.path != ".",
            cancelled,
            on_progress,
        );
        combined.count += result.count;
        if result.cancelled {
            combined.cancelled = true;
            return combined;
        }
    }
    combined
}

fn count_inventory_sources<C, P>(
    context: &ProjectContext,
    cancelled: &C,
    on_progress: &mut P,
) -> InventoryFileCount
where
    C: Fn() -> bool,
    P: FnMut(u64, String),
{
    // Native libraries retain their source-artifact contract: `raw/sources`
    // contains originals of any type, not only Markdown. Compatible vaults
    // have no app-owned evidence root, so their discovered Source roots are
    // counted with the same Markdown policy used by the index.
    let Some(evidence_root) = context.layout.evidence_root.as_deref() else {
        return count_inventory_markdown_roles(
            context,
            &[ProjectMarkdownRootRole::Source],
            cancelled,
            on_progress,
        );
    };
    let Some(root) = layout_root_path(context, &format!("{evidence_root}/sources")) else {
        return InventoryFileCount::default();
    };
    count_inventory_files(
        &context.root,
        &root,
        false,
        false,
        true,
        cancelled,
        on_progress,
    )
}

fn count_inventory_tasks<C, P>(
    context: &ProjectContext,
    cancelled: &C,
    on_progress: &mut P,
) -> InventoryFileCount
where
    C: Fn() -> bool,
    P: FnMut(u64, String),
{
    let Some(task_root) = context.layout.task_state_root.as_deref() else {
        return InventoryFileCount::default();
    };
    let Some(root) = layout_root_path(context, task_root) else {
        return InventoryFileCount::default();
    };
    count_inventory_files(
        &context.root,
        &root,
        false,
        false,
        true,
        cancelled,
        on_progress,
    )
}

fn count_inventory_files<C, P>(
    project_root: &Path,
    dir: &Path,
    markdown_only: bool,
    follow_contained_links: bool,
    recursive: bool,
    cancelled: &C,
    on_progress: &mut P,
) -> InventoryFileCount
where
    C: Fn() -> bool,
    P: FnMut(u64, String),
{
    let mut result = InventoryFileCount::default();
    let mut visited = 0_u64;
    let canonical_root = if follow_contained_links {
        match project_root.canonicalize() {
            Ok(root) => Some(root),
            Err(_) => return result,
        }
    } else {
        None
    };
    let initial_entered_via_link = match canonical_root.as_ref() {
        Some(_) => match project_descendant_path_enters_link(project_root, dir) {
            Ok(entered_via_link) => entered_via_link,
            Err(_) => return result,
        },
        None => match fs::symlink_metadata(dir) {
            Ok(metadata) if inventory_metadata_is_link_or_reparse(&metadata) => return result,
            Ok(_) => false,
            Err(_) => return result,
        },
    };
    let initial = match canonical_root.as_ref() {
        Some(root) => match canonical_internal_read_path(root, dir, initial_entered_via_link) {
            Ok(Some(path)) => path,
            Ok(None) | Err(_) => return result,
        },
        None => dir.to_path_buf(),
    };
    let mut seen_directories = HashSet::new();
    let mut seen_files = HashSet::new();
    let mut stack = vec![(initial, initial_entered_via_link)];
    while let Some((current, entered_via_link)) = stack.pop() {
        if cancelled() {
            result.cancelled = true;
            return result;
        }
        if !seen_directories.insert(current.clone()) {
            continue;
        }
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if cancelled() {
                result.cancelled = true;
                return result;
            }
            visited += 1;
            if visited % 64 == 0 {
                on_progress(visited, "Inventorying project files".into());
            }
            let path = entry.path();
            let Ok(link_metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let entered_via_link = entered_via_link || is_link_or_reparse(&link_metadata);
            let path = if let Some(root) = canonical_root.as_ref() {
                let Ok(Some(path)) = canonical_internal_read_path(root, &path, entered_via_link)
                else {
                    continue;
                };
                path
            } else {
                // Source and task-state inventory are not content-index
                // readers, so descendants stay strict no-follow.
                if inventory_metadata_is_link_or_reparse(&link_metadata) {
                    continue;
                }
                path
            };
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if metadata.is_dir() && recursive {
                stack.push((path, entered_via_link));
            } else if metadata.is_file()
                && (!markdown_only
                    || path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("md")))
                && seen_files.insert(path)
            {
                result.count += 1;
            }
        }
    }
    on_progress(visited, "Inventory complete".into());
    result
}

fn inventory_metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn graph_cache_has_content(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(graph) = serde_json::from_str::<GraphData>(&contents) else {
        return false;
    };
    !graph.nodes.is_empty() || !graph.edges.is_empty()
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

fn normalize_root_key(path: &str) -> String {
    let mut normalized = path.trim_end_matches(['/', '\\']).replace('\\', "/");
    if cfg!(windows) {
        if let Some(without_device_prefix) = normalized.strip_prefix("//?/") {
            normalized = without_device_prefix.to_string();
            if let Some(unc_path) = normalized.strip_prefix("unc/") {
                normalized = format!("//{unc_path}");
            }
        }
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

pub(crate) fn default_config_dir() -> PathBuf {
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
    use super::{ProjectRepairOperationType, ProjectService};
    use crate::models::graph::GraphData;
    use crate::models::paths::ProjectContext;
    use crate::models::project::{
        GraphState, IndexState, ProjectFilesystemAccess, ProjectInventoryState, ProjectTemplate,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

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

    #[test]
    fn default_project_parent_creates_only_the_app_owned_documents_container() {
        let documents = unique_temp_dir("documents");
        let existing = documents.join("unrelated-notes.md");
        fs::write(&existing, "leave me alone").unwrap();

        let parent = super::prepare_default_project_parent_at(&documents).unwrap();

        assert_eq!(parent, documents.join("LLM Wiki").canonicalize().unwrap());
        assert!(parent.is_dir());
        assert_eq!(fs::read_to_string(existing).unwrap(), "leave me alone");
        assert_eq!(
            fs::read_dir(&documents)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>()
                .len(),
            2
        );

        fs::remove_dir_all(documents).unwrap();
    }

    #[test]
    fn default_project_parent_never_reuses_a_file_or_link_like_container() {
        let documents = unique_temp_dir("documents-invalid");
        fs::write(documents.join("LLM Wiki"), "not a directory").unwrap();

        let error = super::prepare_default_project_parent_at(&documents).unwrap_err();

        assert_eq!(error.code, "PROJECT_DEFAULT_PARENT_UNAVAILABLE");
        assert_eq!(
            fs::read_to_string(documents.join("LLM Wiki")).unwrap(),
            "not a directory"
        );
        fs::remove_dir_all(documents).unwrap();
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
            ".app/project.json",
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
        let graph_cache: GraphData =
            serde_json::from_slice(&fs::read(target.join(".app/graph-cache.json")).unwrap())
                .expect("new projects must write the current empty graph cache schema");
        assert!(graph_cache.nodes.is_empty());
        assert!(graph_cache.edges.is_empty());

        assert_eq!(summary.template, ProjectTemplate::Research);
        assert_eq!(
            service.stable_native_project_id(&target),
            Some(summary.project_id.clone()),
            "new native knowledge bases persist the ID used for future relocation"
        );
        assert_eq!(summary.wiki_page_count, 3); // index.md, log.md, overview.md
        assert!(summary.health.is_wiki_project);
        assert!(
            target.join(".git").exists(),
            "new projects must initialize Git"
        );
        assert!(
            fs::read_dir(&root).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(super::PROJECT_CREATION_STAGING_PREFIX)),
            "successful creation must not leave a staging sibling"
        );

        let recents = service.list_recent_projects().unwrap_or_default();
        let _ = recents;
        fs::remove_dir_all(config).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn project_creation_failure_removes_an_owned_staging_directory_and_reports_it() {
        let parent = unique_temp_dir("create-rollback");
        let target = parent.join("target");
        let staging = parent.join(format!("{}test", super::PROJECT_CREATION_STAGING_PREFIX));
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("partial.md"), "partial").unwrap();

        let error = super::project_creation_failure(
            crate::errors::BackendError::new("PROJECT_TEST_FAILURE", "test", true, true),
            &target,
            &staging,
            false,
        );

        assert!(!staging.exists());
        assert_eq!(error.code, "PROJECT_TEST_FAILURE");
        assert_eq!(
            error.details.unwrap()["recovery"]["rollback"],
            serde_json::json!("staging_removed")
        );
        fs::remove_dir_all(parent).ok();
    }

    #[test]
    fn project_creation_failure_never_removes_a_preexisting_empty_target() {
        let parent = unique_temp_dir("create-preexisting-recovery");
        let target = parent.join("target");
        let staging = parent.join(format!("{}test", super::PROJECT_CREATION_STAGING_PREFIX));
        fs::create_dir(&target).unwrap();
        fs::create_dir(&staging).unwrap();
        let error = super::project_creation_failure(
            crate::errors::BackendError::new("PROJECT_TEST_FAILURE", "test", true, true),
            &target,
            &staging,
            true,
        );

        assert!(target.exists());
        assert!(!staging.exists());
        assert_eq!(
            error.details.unwrap()["recovery"]["rollback"],
            serde_json::json!("staging_removed")
        );
        fs::remove_dir_all(parent).ok();
    }

    #[test]
    fn create_project_uses_staging_for_a_preexisting_empty_target() {
        let (service, config) = service_in_temp();
        let parent = unique_temp_dir("create-existing-empty");
        let target = parent.join("target");
        fs::create_dir(&target).unwrap();

        service
            .create_project(
                target.to_string_lossy().as_ref(),
                "Existing empty",
                ProjectTemplate::General,
            )
            .expect("an empty explicit target should be created transactionally");

        assert!(target.join(".git").exists());
        assert!(target.join("wiki/index.md").exists());
        assert!(fs::read_dir(&parent).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.starts_with(super::PROJECT_CREATION_STAGING_PREFIX)
                && !name.starts_with(super::PROJECT_CREATION_BACKUP_PREFIX)
        }));

        fs::remove_dir_all(parent).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn staging_install_restores_a_target_that_changed_after_initial_validation() {
        let parent = unique_temp_dir("create-race-recovery");
        let target = parent.join("target");
        fs::create_dir(&target).unwrap();
        let staging = parent.join(format!("{}test", super::PROJECT_CREATION_STAGING_PREFIX));
        fs::create_dir(&staging).unwrap();
        fs::write(target.join("concurrent.txt"), "external write").unwrap();

        let error = super::install_staged_project(&target, &staging, true)
            .expect_err("a changed target must not be replaced");

        assert_eq!(error.code, "PROJECT_DIR_CHANGED_DURING_CREATION");
        assert_eq!(
            fs::read_to_string(target.join("concurrent.txt")).unwrap(),
            "external write"
        );
        assert!(
            staging.exists(),
            "caller retains staging until recovery reporting runs"
        );
        super::remove_owned_staging_root(&staging).unwrap();
        fs::remove_dir_all(parent).ok();
    }

    #[test]
    fn create_project_rejects_invalid_or_windows_reserved_names_before_writing() {
        let (service, config) = service_in_temp();
        let root = unique_temp_dir("invalid-project-name");
        let invalid_target = root.join("clean-target");
        let invalid = service
            .create_project(
                invalid_target.to_string_lossy().as_ref(),
                "invalid*name",
                ProjectTemplate::General,
            )
            .expect_err("invalid names must be rejected before creating files");
        assert_eq!(invalid.code, "PROJECT_NAME_INVALID");
        assert!(!invalid_target.exists());

        let reserved_target = root.join("reserved-target");
        let reserved = service
            .create_project(
                reserved_target.to_string_lossy().as_ref(),
                "CON",
                ProjectTemplate::General,
            )
            .expect_err("Windows-reserved names must be rejected on every platform");
        assert_eq!(reserved.code, "PROJECT_NAME_RESERVED");
        assert!(!reserved_target.exists());

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
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
    fn open_project_rejects_ordinary_folder_without_creating_or_moving_files() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("ordinary");
        fs::write(root.join("report.pdf"), "%PDF-1.4").unwrap();
        fs::write(root.join("note.md"), "# note").unwrap();
        fs::write(root.join("photo.png"), "PNG").unwrap();
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes").join("deep.docx"), "doc").unwrap();

        let err = service
            .open_project(root.to_string_lossy().as_ref())
            .expect_err("ordinary folders must be routed through read-only assessment");

        assert_eq!(err.code, "PROJECT_OPEN_REQUIRES_ASSESSMENT");
        assert!(root.join("report.pdf").exists());
        assert!(root.join("note.md").exists());
        assert!(root.join("photo.png").exists());
        assert!(root.join("notes/deep.docx").exists());
        assert!(!root.join("raw").exists());
        assert!(!root.join(".app").exists());
        assert!(!root.join(".git").exists());

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
            serde_json::to_string(&GraphData::empty(String::new())).unwrap(),
        )
        .unwrap();

        let context = ProjectContext::new("metadata", root.clone());
        let summary = service.scan_project(&context, Some("Metadata"));

        assert_eq!(summary.source_count, 2);
        assert_eq!(summary.graph_state, GraphState::Missing);

        fs::write(
            root.join(".app/graph-cache.json"),
            r#"{
  "nodes": [{
    "id": "a",
    "path": "wiki/a.md",
    "label": "A",
    "type": "concept",
    "tags": [],
    "starred": false,
    "degree": 0
  }],
  "edges": [],
  "contentHash": "hash",
  "builtAt": "2026-08-03T00:00:00Z"
}"#,
        )
        .unwrap();
        let summary = service.scan_project(&context, Some("Metadata"));
        assert_eq!(summary.graph_state, GraphState::Cached);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn post_open_inventory_is_read_only_and_reports_ready_counts() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("background-inventory");
        fs::create_dir_all(root.join("wiki/nested")).unwrap();
        fs::create_dir_all(root.join("raw/sources/pdfs")).unwrap();
        fs::create_dir_all(root.join(".app/tasks")).unwrap();
        fs::write(root.join("wiki/index.md"), "# Index").unwrap();
        fs::write(root.join("wiki/nested/notes.MD"), "# Notes").unwrap();
        fs::write(root.join("wiki/nested/ignore.txt"), "not markdown").unwrap();
        fs::write(root.join("raw/sources/pdfs/report.pdf"), "pdf").unwrap();
        fs::write(root.join(".app/tasks/previous.json"), "{}").unwrap();
        let context = ProjectContext::new("inventory", root.clone());

        let opening = service.quick_project_summary(&context, Some("Inventory"));
        assert_eq!(opening.inventory_state, ProjectInventoryState::Scanning);
        assert_eq!(opening.wiki_page_count, 0);

        let summary =
            service.scan_project_inventory(&context, Some("Inventory"), || false, |_, _| {});
        assert_eq!(summary.inventory_state, ProjectInventoryState::Ready);
        assert_eq!(summary.wiki_page_count, 2);
        assert_eq!(summary.source_count, 1);
        assert_eq!(summary.task_count, 1);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compatible_layout_summary_and_inventory_follow_resolved_roles() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("compatible-layout-inventory");
        fs::create_dir_all(root.join("pages")).unwrap();
        fs::create_dir_all(root.join("sources")).unwrap();
        fs::create_dir_all(root.join(".app/compat")).unwrap();
        fs::write(root.join("index.md"), "# Root index").unwrap();
        fs::write(root.join("pages/page.md"), "# Page").unwrap();
        fs::write(root.join("sources/source.md"), "# Source").unwrap();
        fs::write(root.join(".app/compat/purpose.md"), "compat purpose").unwrap();
        fs::write(root.join(".app/compat/schema.md"), "compat schema").unwrap();
        let context = ProjectContext::new("compatible-inventory", root.clone())
            .with_resolved_layout()
            .expect("compatible layout should resolve");

        let summary = service.scan_project(&context, Some("Compatible"));
        assert_eq!(summary.wiki_page_count, 2, "root + pages Markdown only");
        assert_eq!(summary.source_count, 1, "Source role is counted separately");
        assert_eq!(summary.task_count, 0, "compatible layout has no task root");

        let inventory =
            service.scan_project_inventory(&context, Some("Compatible"), || false, |_, _| {});
        assert_eq!(inventory.inventory_state, ProjectInventoryState::Ready);
        assert_eq!(inventory.wiki_page_count, 2);
        assert_eq!(inventory.source_count, 1);
        assert_eq!(inventory.task_count, 0);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn post_open_inventory_returns_partial_counts_when_cancelled() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("cancelled-inventory");
        fs::create_dir_all(root.join("wiki")).unwrap();
        for index in 0..96 {
            fs::write(root.join("wiki").join(format!("page-{index}.md")), "# Page").unwrap();
        }
        let context = ProjectContext::new("inventory", root.clone());
        let cancelled = AtomicBool::new(false);
        let summary = service.scan_project_inventory(
            &context,
            None,
            || cancelled.load(Ordering::SeqCst),
            |current, _| {
                if current >= 64 {
                    cancelled.store(true, Ordering::SeqCst);
                }
            },
        );

        assert_eq!(summary.inventory_state, ProjectInventoryState::Partial);
        assert!(summary.wiki_page_count < 96);

        fs::remove_dir_all(root).ok();
    }

    #[cfg(windows)]
    #[test]
    fn post_open_inventory_does_not_follow_windows_junctions() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("inventory-junction-root");
        let external = unique_temp_dir("inventory-junction-external");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/index.md"), "# Index").unwrap();
        fs::write(external.join("outside.md"), "# Outside").unwrap();
        let junction = root.join("wiki").join("linked");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                junction.to_string_lossy().as_ref(),
                external.to_string_lossy().as_ref(),
            ])
            .status()
            .unwrap();
        assert!(status.success(), "junction setup failed");

        let context = ProjectContext::new("inventory", root.clone());
        let summary = service.scan_project_inventory(&context, None, || false, |_, _| {});
        assert_eq!(summary.wiki_page_count, 1);

        fs::remove_dir(junction).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(external).ok();
    }

    #[cfg(windows)]
    #[test]
    fn post_open_inventory_counts_markdown_through_contained_windows_junctions_once() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("inventory-contained-junction-root");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::create_dir_all(root.join("shared")).unwrap();
        fs::write(root.join("wiki/index.md"), "# Index").unwrap();
        fs::write(root.join("shared/linked.md"), "# Linked").unwrap();
        let junction = root.join("wiki").join("linked");
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&junction)
            .arg(root.join("shared"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let context = ProjectContext::new("inventory", root.clone());
        let summary = service.scan_project_inventory(&context, None, || false, |_, _| {});
        assert_eq!(summary.wiki_page_count, 2);

        fs::remove_dir(junction).ok();
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
    fn removing_a_recent_project_only_updates_the_matching_global_entry() {
        let (service, config) = service_in_temp();
        let root_a = unique_temp_dir("recent-remove-a");
        let root_b = unique_temp_dir("recent-remove-b");
        let entry_a = crate::models::project::RecentProject {
            project_id: "project-a".into(),
            name: "A".into(),
            root_path: root_a.to_string_lossy().into_owned(),
            template: ProjectTemplate::General,
            opened_at: "2026-08-03T00:00:00Z".into(),
            wiki_page_count: 0,
            source_count: 0,
            task_count: 0,
            index_state: IndexState::Missing,
            graph_state: GraphState::Missing,
            missing: false,
        };
        let entry_b = crate::models::project::RecentProject {
            project_id: "project-b".into(),
            name: "B".into(),
            root_path: root_b.to_string_lossy().into_owned(),
            ..entry_a.clone()
        };
        service.remember_recent_project(entry_a.clone()).unwrap();
        service.remember_recent_project(entry_b.clone()).unwrap();
        fs::remove_dir(&root_b).unwrap();

        let remaining = service
            .remove_recent_project(&entry_a.project_id, &entry_a.root_path)
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].project_id, entry_b.project_id);
        assert!(
            !remaining[0].missing,
            "removal must not re-assess another project"
        );
        assert!(root_a.exists());
        assert!(!root_b.exists());

        let wrong_root = service
            .remove_recent_project(&entry_b.project_id, root_a.to_string_lossy().as_ref())
            .unwrap();
        assert_eq!(wrong_root.len(), 1);
        assert_eq!(wrong_root[0].project_id, entry_b.project_id);
        fs::remove_dir_all(root_a).ok();
        fs::remove_dir_all(root_b).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn relocating_a_recent_project_replaces_only_the_verified_exact_entry() {
        let (service, config) = service_in_temp();
        let old_root = config.join("missing-native");
        let new_root = unique_temp_dir("recent-relocated-native");
        let other_root = unique_temp_dir("recent-relocated-other");
        let old_entry = crate::models::project::RecentProject {
            project_id: uuid::Uuid::new_v4().to_string(),
            name: "Moved native".into(),
            root_path: old_root.to_string_lossy().into_owned(),
            template: ProjectTemplate::General,
            opened_at: "2026-08-03T00:00:00Z".into(),
            wiki_page_count: 0,
            source_count: 0,
            task_count: 0,
            index_state: IndexState::Missing,
            graph_state: GraphState::Missing,
            missing: true,
        };
        let other_entry = crate::models::project::RecentProject {
            project_id: uuid::Uuid::new_v4().to_string(),
            name: "Other".into(),
            root_path: other_root.to_string_lossy().into_owned(),
            ..old_entry.clone()
        };
        service
            .remember_recent_project(other_entry.clone())
            .unwrap();
        service.remember_recent_project(old_entry.clone()).unwrap();
        let relocated = crate::models::project::RecentProject {
            root_path: new_root.to_string_lossy().into_owned(),
            opened_at: "2026-08-03T00:00:01Z".into(),
            missing: false,
            ..old_entry.clone()
        };
        fs::create_dir_all(new_root.join(".app")).unwrap();
        fs::write(
            new_root.join(super::NATIVE_PROJECT_ID_FILE),
            format!(r#"{{"projectId":"{}"}}"#, old_entry.project_id),
        )
        .unwrap();
        fs::create_dir_all(other_root.join(".app")).unwrap();
        fs::write(
            other_root.join(super::NATIVE_PROJECT_ID_FILE),
            format!(r#"{{"projectId":"{}"}}"#, old_entry.project_id),
        )
        .unwrap();

        fs::create_dir_all(&old_root).unwrap();
        let source_available = service
            .relocate_recent_project(
                &old_entry.project_id,
                &old_entry.root_path,
                relocated.clone(),
            )
            .expect_err("an available old root must not be treated as missing");
        assert_eq!(source_available.code, "PROJECT_RELOCATION_SOURCE_AVAILABLE");
        fs::remove_dir_all(&old_root).unwrap();

        fs::write(
            new_root.join(super::NATIVE_PROJECT_ID_FILE),
            format!(r#"{{"projectId":"{}"}}"#, uuid::Uuid::new_v4()),
        )
        .unwrap();
        let id_changed = service
            .relocate_recent_project(
                &old_entry.project_id,
                &old_entry.root_path,
                relocated.clone(),
            )
            .expect_err("the durable ID must be checked again immediately before write");
        assert_eq!(id_changed.code, "PROJECT_RELOCATION_ID_MISMATCH");
        assert_eq!(service.list_recent_projects().unwrap().len(), 2);
        fs::write(
            new_root.join(super::NATIVE_PROJECT_ID_FILE),
            format!(r#"{{"projectId":"{}"}}"#, old_entry.project_id),
        )
        .unwrap();

        let projects = service
            .relocate_recent_project(&old_entry.project_id, &old_entry.root_path, relocated)
            .unwrap();

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].project_id, old_entry.project_id);
        assert_eq!(projects[0].root_path, new_root.to_string_lossy());
        assert_eq!(projects[1].project_id, other_entry.project_id);
        assert!(!old_root.exists());
        assert!(new_root.exists());
        assert!(other_root.exists());

        let conflict = crate::models::project::RecentProject {
            root_path: other_root.to_string_lossy().into_owned(),
            ..old_entry.clone()
        };
        fs::remove_dir_all(&new_root).unwrap();
        let error = service
            .relocate_recent_project(&old_entry.project_id, &new_root.to_string_lossy(), conflict)
            .expect_err("a different recent entry must not be replaced");
        assert_eq!(error.code, "PROJECT_RECENT_RELOCATION_TARGET_CONFLICT");

        fs::remove_dir_all(new_root).ok();
        fs::remove_dir_all(other_root).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn recent_root_normalization_only_folds_case_on_windows() {
        let upper = super::normalize_root_key("D:/Knowledge/Foo");
        let lower = super::normalize_root_key("D:/Knowledge/foo");
        if cfg!(windows) {
            assert_eq!(upper, lower);
        } else {
            assert_ne!(upper, lower);
        }
    }

    #[test]
    fn native_relocation_identity_fails_closed_without_a_valid_uuid_file() {
        let (service, config) = service_in_temp();
        let root = unique_temp_dir("recent-relocation-identity");
        fs::create_dir_all(root.join(".app")).unwrap();

        let missing = service
            .require_stable_native_project_id(&root)
            .expect_err("missing identity must not prove a move");
        assert_eq!(missing.code, "PROJECT_RELOCATION_ID_UNAVAILABLE");

        fs::write(
            root.join(super::NATIVE_PROJECT_ID_FILE),
            r#"{"projectId":"not-a-uuid"}"#,
        )
        .unwrap();
        let invalid = service
            .require_stable_native_project_id(&root)
            .expect_err("malformed identity must not prove a move");
        assert_eq!(invalid.code, "PROJECT_RELOCATION_ID_INVALID");

        let expected = uuid::Uuid::new_v4().to_string();
        fs::write(
            root.join(super::NATIVE_PROJECT_ID_FILE),
            format!(r#"{{"projectId":"{expected}"}}"#),
        )
        .unwrap();
        assert_eq!(
            service.require_stable_native_project_id(&root).unwrap(),
            expected
        );

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn concurrent_recent_remember_and_remove_keep_both_updates() {
        let (service, config) = service_in_temp();
        let service = Arc::new(service);
        let root_a = unique_temp_dir("recent-concurrent-a");
        let root_b = unique_temp_dir("recent-concurrent-b");
        let entry_a = crate::models::project::RecentProject {
            project_id: "project-a".into(),
            name: "A".into(),
            root_path: root_a.to_string_lossy().into_owned(),
            template: ProjectTemplate::General,
            opened_at: "2026-08-03T00:00:00Z".into(),
            wiki_page_count: 0,
            source_count: 0,
            task_count: 0,
            index_state: IndexState::Missing,
            graph_state: GraphState::Missing,
            missing: false,
        };
        let entry_b = crate::models::project::RecentProject {
            project_id: "project-b".into(),
            name: "B".into(),
            root_path: root_b.to_string_lossy().into_owned(),
            opened_at: "2026-08-03T00:00:01Z".into(),
            ..entry_a.clone()
        };
        service.remember_recent_project(entry_a.clone()).unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let remover_service = Arc::clone(&service);
        let remover_barrier = Arc::clone(&barrier);
        let remover_entry = entry_a.clone();
        let remover = std::thread::spawn(move || {
            remover_barrier.wait();
            remover_service
                .remove_recent_project(&remover_entry.project_id, &remover_entry.root_path)
        });
        let remember_service = Arc::clone(&service);
        let remember_barrier = Arc::clone(&barrier);
        let rememberer = std::thread::spawn(move || {
            remember_barrier.wait();
            remember_service.remember_recent_project(entry_b)
        });
        barrier.wait();

        remover.join().unwrap().unwrap();
        rememberer.join().unwrap().unwrap();
        let projects = service.list_recent_projects().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project_id, "project-b");

        fs::remove_dir_all(root_a).ok();
        fs::remove_dir_all(root_b).ok();
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

    #[test]
    fn compatible_guidance_adds_only_scoped_state_and_does_not_initialize_git() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("compatible-guidance");
        fs::create_dir(root.join(".obsidian")).unwrap();
        fs::write(root.join("首页.md"), "# 首页\n原文").unwrap();
        fs::write(root.join(".obsidian/app.json"), "{\"legacy\":true}").unwrap();
        let markdown_before = fs::read(root.join("首页.md")).unwrap();
        let obsidian_before = fs::read(root.join(".obsidian/app.json")).unwrap();
        let context = ProjectContext::new("compatible", root.clone());

        let changed = service
            .enable_compatible_guidance(&context, ProjectTemplate::General)
            .unwrap();

        assert_eq!(
            changed,
            vec![
                ".app/compat/purpose.md",
                ".app/compat/schema.md",
                ".app/compat/tasks",
                ".app/compat/workflows",
            ]
        );
        assert!(root.join(".app/compat/purpose.md").is_file());
        assert!(root.join(".app/compat/schema.md").is_file());
        assert!(root.join(".app/compat/tasks").is_dir());
        assert!(root.join(".app/compat/workflows").is_dir());
        assert!(!root.join("purpose.md").exists());
        assert!(!root.join("schema.md").exists());
        assert!(!root.join(".git").exists());
        assert_eq!(fs::read(root.join("首页.md")).unwrap(), markdown_before);
        assert_eq!(
            fs::read(root.join(".obsidian/app.json")).unwrap(),
            obsidian_before
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compatible_guidance_never_overwrites_existing_app_owned_guidance() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("compatible-existing");
        fs::create_dir_all(root.join(".app/compat")).unwrap();
        fs::write(root.join(".app/compat/purpose.md"), "custom").unwrap();
        let context = ProjectContext::new("compatible", root.clone());

        let error = service
            .enable_compatible_guidance(&context, ProjectTemplate::Research)
            .unwrap_err();

        assert_eq!(error.code, "PROJECT_COMPAT_GUIDANCE_EXISTS");
        assert_eq!(
            fs::read_to_string(root.join(".app/compat/purpose.md")).unwrap(),
            "custom"
        );
        assert!(!root.join(".app/compat/schema.md").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compatible_guidance_retry_accepts_only_the_exact_generated_templates() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("compatible-idempotent");
        let context = ProjectContext::new("compatible", root.clone());

        service
            .enable_compatible_guidance(&context, ProjectTemplate::Research)
            .unwrap();
        let purpose = fs::read(root.join(".app/compat/purpose.md")).unwrap();
        let schema = fs::read(root.join(".app/compat/schema.md")).unwrap();

        let changed = service
            .enable_compatible_guidance(&context, ProjectTemplate::Research)
            .unwrap();

        assert_eq!(
            changed,
            vec![
                ".app/compat/purpose.md",
                ".app/compat/schema.md",
                ".app/compat/tasks",
                ".app/compat/workflows",
            ]
        );
        assert_eq!(
            fs::read(root.join(".app/compat/purpose.md")).unwrap(),
            purpose
        );
        assert_eq!(
            fs::read(root.join(".app/compat/schema.md")).unwrap(),
            schema
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compatible_guidance_serializes_same_process_enablement() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("compatible-concurrent");
        let context = ProjectContext::new("compatible", root.clone());

        std::thread::scope(|scope| {
            let first = scope
                .spawn(|| service.enable_compatible_guidance(&context, ProjectTemplate::Business));
            let second = scope
                .spawn(|| service.enable_compatible_guidance(&context, ProjectTemplate::Business));

            assert!(first.join().unwrap().is_ok());
            assert!(second.join().unwrap().is_ok());
        });

        assert_eq!(
            fs::read_to_string(root.join(".app/compat/purpose.md")).unwrap(),
            super::template_purpose(ProjectTemplate::Business)
        );
        assert_eq!(
            fs::read_to_string(root.join(".app/compat/schema.md")).unwrap(),
            super::template_schema(ProjectTemplate::Business)
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn graph_cache_repair_preserves_invalid_bytes_and_regenerates_only_cache() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("graph-cache-repair");
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::create_dir_all(root.join("wiki")).unwrap();
        let invalid = b"{ graph cache is corrupt";
        fs::write(root.join(".app/graph-cache.json"), invalid).unwrap();
        fs::write(root.join("wiki/notes.md"), "# Preserve me\n").unwrap();
        let context = ProjectContext::new("recovery", root.clone());

        let plan = service
            .prepare_graph_cache_repair_plan(
                &context,
                "identity".into(),
                "revision".into(),
                Some("head".into()),
                Vec::new(),
            )
            .unwrap();
        let changed = service
            .apply_graph_cache_repair_plan(&context, &plan)
            .unwrap();

        assert_eq!(changed.len(), 2);
        let operation = plan.operations.first().unwrap();
        assert_eq!(
            fs::read(root.join(operation.backup_path.as_deref().unwrap())).unwrap(),
            invalid
        );
        let repaired: crate::models::graph::GraphData =
            serde_json::from_slice(&fs::read(root.join(".app/graph-cache.json")).unwrap()).unwrap();
        assert!(repaired.nodes.is_empty());
        assert!(repaired.edges.is_empty());
        assert_eq!(
            fs::read_to_string(root.join("wiki/notes.md")).unwrap(),
            "# Preserve me\n"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn graph_cache_repair_can_follow_a_required_git_checkpoint() {
        let (service, config) = service_in_temp();
        let root = unique_temp_dir("graph-cache-repair-checkpoint");
        let target = root.join("knowledge-base");
        let summary = service
            .create_project(
                target.to_string_lossy().as_ref(),
                "Knowledge Base",
                ProjectTemplate::General,
            )
            .unwrap();
        let context = ProjectContext::new(summary.project_id, target.clone());
        let invalid = b"{ invalid graph cache";
        fs::write(target.join(".app/graph-cache.json"), invalid).unwrap();

        let git = crate::services::git_service::GitService;
        let status = git.repository_status(&context).unwrap();
        let expected_paths = git.changed_paths(&context).unwrap();
        let plan = service
            .prepare_graph_cache_repair_plan(
                &context,
                "identity".into(),
                "revision".into(),
                status.head.clone(),
                expected_paths.clone(),
            )
            .unwrap();
        git.verify_checkpoint_state(&context, status.head.as_deref(), &expected_paths)
            .unwrap();
        let checkpoint = git
            .create_checkpoint(
                &context,
                crate::models::git::CheckpointPurpose::HighRiskOperation,
                "Checkpoint before project recovery repair",
            )
            .unwrap();
        assert!(checkpoint.created);
        assert!(checkpoint
            .affected_paths
            .contains(&".app/graph-cache.json".to_string()));

        service
            .apply_graph_cache_repair_plan(&context, &plan)
            .unwrap();
        let operation = plan.operations.first().unwrap();
        assert_eq!(
            fs::read(target.join(operation.backup_path.as_deref().unwrap())).unwrap(),
            invalid
        );
        let repaired: GraphData =
            serde_json::from_slice(&fs::read(target.join(".app/graph-cache.json")).unwrap())
                .unwrap();
        assert!(repaired.nodes.is_empty());
        assert!(repaired.edges.is_empty());

        fs::remove_dir_all(config).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn graph_cache_repair_refuses_a_target_changed_after_preview() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("graph-cache-repair-stale");
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join(".app/graph-cache.json"), "{ invalid").unwrap();
        let context = ProjectContext::new("recovery", root.clone());
        let plan = service
            .prepare_graph_cache_repair_plan(
                &context,
                "identity".into(),
                "revision".into(),
                Some("head".into()),
                Vec::new(),
            )
            .unwrap();
        fs::write(root.join(".app/graph-cache.json"), "{ changed").unwrap();

        let error = service
            .apply_graph_cache_repair_plan(&context, &plan)
            .unwrap_err();

        assert_eq!(error.code, "PROJECT_REPAIR_TARGET_CHANGED");
        assert!(!root.join(".app/recovery-backups").exists());
        assert_eq!(
            fs::read_to_string(root.join(".app/graph-cache.json")).unwrap(),
            "{ changed"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn native_layout_repair_creates_only_previewed_empty_directories() {
        let (service, _config) = service_in_temp();
        let root = unique_temp_dir("native-layout-repair");
        for directory in [
            ".app/chats",
            ".app/tasks",
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
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();
        fs::write(root.join("schema.md"), "# Schema\n").unwrap();
        fs::write(root.join("wiki/keep.md"), "# Keep\n").unwrap();
        fs::remove_dir(root.join(".app/tasks")).unwrap();
        let context = ProjectContext::new("legacy", root.clone());

        let plan = service
            .prepare_native_layout_repair_plan(&context, "identity".into(), "revision".into())
            .unwrap();
        assert!(plan.expected_git_head.is_none());
        assert!(plan.operations.iter().all(|operation| {
            operation.operation_type == ProjectRepairOperationType::CreateDirectory
                && operation.backup_path.is_none()
                && operation.expected_hash.is_none()
        }));

        let changed = service
            .apply_native_layout_repair_plan(&context, &plan)
            .unwrap();
        assert_eq!(changed, vec![".app/tasks"]);
        assert!(root.join(".app/tasks").is_dir());
        assert_eq!(
            fs::read_to_string(root.join("wiki/keep.md")).unwrap(),
            "# Keep\n"
        );

        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn native_repair_directory_creation_never_follows_a_linked_ancestor() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("native-layout-repair-linked-ancestor");
        let outside = unique_temp_dir("native-layout-repair-outside");
        symlink(&outside, root.join("raw")).unwrap();

        let error = create_native_repair_directory(&root, &root.join("raw/sources"))
            .expect_err("native repair must reject a linked ancestor");
        assert!(error.contains("linked or invalid"));
        assert!(!outside.join("sources").exists());

        fs::remove_file(root.join("raw")).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }
}

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    errors::BackendError,
    models::{
        import_v2::{ImportArtifact, ImportInputKind, ImportItem, ImportSession},
        import_v2_agent::{AgentAssistanceTrigger, AgentToolGrant},
        paths::ProjectContext,
    },
    utils::safe_project_dir::{remove_project_file, BoundProjectMutationRoot},
};

const WORKSPACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskBundle {
    pub schema_version: u32,
    pub session_id: String,
    pub item_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub public_source: String,
    pub input_hashes: Vec<String>,
    pub allowed_tools: Vec<AgentToolGrant>,
    pub required_outputs: Vec<String>,
    pub untrusted_source_material: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentWorkspace {
    pub project_root: PathBuf,
    pub workspace_id: String,
    pub root: PathBuf,
    pub task_path: PathBuf,
    pub source_dir: PathBuf,
    pub deterministic_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub output_dir: PathBuf,
    pub lease_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentWorkspaceLease {
    session_id: String,
    item_id: String,
    workspace_id: String,
    task_id: Option<String>,
    process_instance_id: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentWorkspaceBuilder;

impl AgentWorkspaceBuilder {
    pub fn build(
        &self,
        context: &ProjectContext,
        session: &ImportSession,
        item: &ImportItem,
        trigger: AgentAssistanceTrigger,
    ) -> Result<AgentWorkspace, BackendError> {
        self.build_with_owner(context, session, item, trigger, None)
    }

    pub fn build_for_task(
        &self,
        context: &ProjectContext,
        session: &ImportSession,
        item: &ImportItem,
        trigger: AgentAssistanceTrigger,
        task_id: &str,
    ) -> Result<AgentWorkspace, BackendError> {
        if !is_safe_component(task_id) {
            return Err(workspace_error("Agent workspace task identity is invalid."));
        }
        self.build_with_owner(context, session, item, trigger, Some(task_id))
    }

    fn build_with_owner(
        &self,
        context: &ProjectContext,
        session: &ImportSession,
        item: &ImportItem,
        trigger: AgentAssistanceTrigger,
        task_id: Option<&str>,
    ) -> Result<AgentWorkspace, BackendError> {
        validate_identity(context, session, item)?;
        let workspace_id = uuid::Uuid::new_v4().to_string();
        let import_paths = context.layout.import_paths()?;
        let relative_root = import_paths.item_staging_child(
            &session.session_id,
            &item.item_id,
            &["agent", &workspace_id],
        )?;
        let root = context.resolve_project_path(&relative_root)?;
        let lease_file = format!("{workspace_id}.json");
        let lease_relative = import_paths.item_staging_child(
            &session.session_id,
            &item.item_id,
            &["agent-leases", &lease_file],
        )?;
        let lease_path = context.resolve_project_path(&lease_relative)?;
        write_json_atomic_path(
            &context.root,
            &lease_path,
            &AgentWorkspaceLease {
                session_id: session.session_id.clone(),
                item_id: item.item_id.clone(),
                workspace_id: workspace_id.clone(),
                task_id: task_id.map(str::to_owned),
                process_instance_id: process_instance_id().clone(),
                expires_at: Utc::now() + Duration::minutes(10),
            },
        )?;
        let source_dir = root.join("source");
        let deterministic_dir = root.join("deterministic");
        let logs_dir = root.join("logs");
        let output_dir = root.join("output");
        for dir in [&source_dir, &deterministic_dir, &logs_dir, &output_dir] {
            if let Err(error) = BoundProjectMutationRoot::ensure_and_bind(
                &context.root,
                &dir.join(".wiki-directory-binding-probe"),
            )
            .map_err(workspace_io_error)
            {
                let _ = remove_workspace_tree(&context.root, &root);
                let _ = remove_project_file(&context.root, &lease_path);
                return Err(error);
            }
        }
        if let Err(error) = reject_links_between(&context.root, &root) {
            let _ = remove_workspace_tree(&context.root, &root);
            let _ = remove_project_file(&context.root, &lease_path);
            return Err(error);
        }

        let result = (|| {
            let (source_name, mut input_hashes) = if let Some(preview) = &item.preview {
                let source_name =
                    stable_copy_name("source", &preview.source_snapshot.relative_path);
                copy_verified_item_artifact(
                    context,
                    session,
                    item,
                    &preview.source_snapshot,
                    &source_dir.join(&source_name),
                )?;
                let deterministic_copy = deterministic_dir.join("candidate.md");
                copy_verified_item_artifact(
                    context,
                    session,
                    item,
                    &preview.markdown,
                    &deterministic_copy,
                )?;
                let mut preview_hashes = vec![
                    preview.source_snapshot.sha256.clone(),
                    preview.markdown.sha256.clone(),
                ];
                for (index, asset) in preview.assets.iter().enumerate() {
                    let asset_dir = deterministic_dir.join("assets");
                    BoundProjectMutationRoot::ensure_and_bind(
                        &context.root,
                        &asset_dir.join(".wiki-directory-binding-probe"),
                    )
                    .map_err(workspace_io_error)?;
                    let name = stable_copy_name(&format!("asset-{index}"), &asset.relative_path);
                    copy_verified_item_artifact(
                        context,
                        session,
                        item,
                        asset,
                        &asset_dir.join(name),
                    )?;
                    preview_hashes.push(asset.sha256.clone());
                }
                (source_name, preview_hashes)
            } else {
                copy_hard_failure_source(context, session, item, &source_dir)?
            };
            input_hashes.sort();

            let task = AgentTaskBundle {
                schema_version: WORKSPACE_SCHEMA_VERSION,
                session_id: session.session_id.clone(),
                item_id: item.item_id.clone(),
                trigger,
                public_source: public_source(item),
                input_hashes,
                allowed_tools: vec![
                    AgentToolGrant::InspectSource,
                    AgentToolGrant::RunDeterministicRoute,
                    AgentToolGrant::ValidateCandidate,
                ],
                required_outputs: vec!["output/manifest.json".into(), "output/candidate.md".into()],
                untrusted_source_material: vec![format!("source/{source_name}")],
            };
            let task_path = root.join("task.json");
            write_json(&context.root, &task_path, &task)?;
            let attempts_path = logs_dir.join("attempts.json");
            write_json(
                &context.root,
                &attempts_path,
                &Vec::<serde_json::Value>::new(),
            )?;

            set_tree_readonly(&source_dir)?;
            set_tree_readonly(&deterministic_dir)?;
            set_readonly(&task_path, true)?;
            set_readonly(&attempts_path, true)?;
            set_readonly(&output_dir, false)?;

            Ok(AgentWorkspace {
                project_root: context.root.clone(),
                workspace_id,
                root: root.clone(),
                task_path,
                source_dir,
                deterministic_dir,
                logs_dir,
                output_dir,
                lease_path: lease_path.clone(),
            })
        })();

        if result.is_err() {
            let _ = remove_workspace_tree(&context.root, &root);
            let _ = remove_project_file(&context.root, &lease_path);
        }
        result
    }

    pub fn cleanup_terminal(workspace: &AgentWorkspace) -> Result<Vec<String>, BackendError> {
        let mut hashes = Vec::new();
        if workspace.output_dir.exists() {
            collect_file_hashes(&workspace.output_dir, &mut hashes)?;
        }
        remove_workspace_tree(&workspace.project_root, &workspace.root)?;
        if workspace.lease_path.exists() {
            remove_project_file(&workspace.project_root, &workspace.lease_path)
                .map_err(workspace_io_error)?;
        }
        hashes.sort();
        Ok(hashes)
    }

    pub fn validate_output_target(workspace: &AgentWorkspace) -> Result<(), BackendError> {
        reject_links_between(&workspace.root, &workspace.output_dir)?;
        let root = workspace.root.canonicalize().map_err(workspace_io_error)?;
        let output = workspace
            .output_dir
            .canonicalize()
            .map_err(workspace_io_error)?;
        if !output.starts_with(&root) || output == root {
            return Err(workspace_error(
                "Agent output directory escaped the isolated workspace.",
            ));
        }
        Ok(())
    }

    pub fn cleanup_recorded_workspace(
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        workspace_relative_path: &str,
    ) -> Result<(), BackendError> {
        if !is_safe_component(session_id) || !is_safe_component(item_id) {
            return Err(workspace_error("Agent workspace identity is invalid."));
        }
        let expected_prefix = format!(
            "{}/",
            context
                .layout
                .import_paths()?
                .item_staging_child(session_id, item_id, &["agent"])?
        );
        let normalized = workspace_relative_path.replace('\\', "/");
        let workspace_id = normalized.strip_prefix(&expected_prefix).ok_or_else(|| {
            workspace_error("Recorded Agent workspace is outside the current import item.")
        })?;
        if !is_safe_component(workspace_id) || workspace_id.contains('/') {
            return Err(workspace_error(
                "Recorded Agent workspace identity is invalid.",
            ));
        }
        let root = context.resolve_project_path(&normalized)?;
        if root.exists() {
            reject_links_between(&context.root, &root)?;
            let metadata = fs::symlink_metadata(&root).map_err(workspace_io_error)?;
            if !metadata.is_dir() {
                return Err(workspace_error(
                    "Agent workspace storage is not a directory.",
                ));
            }
            remove_workspace_tree(&context.root, &root)?;
        }
        let lease_file = format!("{workspace_id}.json");
        let lease =
            context.resolve_project_path(&context.layout.import_paths()?.item_staging_child(
                session_id,
                item_id,
                &["agent-leases", &lease_file],
            )?)?;
        if lease.exists() {
            reject_links_between(&context.root, &lease)?;
            remove_project_file(&context.root, &lease).map_err(workspace_io_error)?;
        }
        Ok(())
    }

    pub fn cleanup_abandoned_leases(
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        preserve_task: impl Fn(&str) -> bool,
    ) -> Result<(), BackendError> {
        if !is_safe_component(session_id) || !is_safe_component(item_id) {
            return Err(workspace_error("Agent workspace identity is invalid."));
        }
        let leases =
            context.resolve_project_path(&context.layout.import_paths()?.item_staging_child(
                session_id,
                item_id,
                &["agent-leases"],
            )?)?;
        if !leases.exists() {
            return Ok(());
        }
        validate_isolated_directory(&context.root, &leases)?;
        for entry in fs::read_dir(&leases).map_err(workspace_io_error)? {
            let path = entry.map_err(workspace_io_error)?.path();
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| workspace_error("Agent workspace lease filename is invalid."))?;
            if name.starts_with('.') && name.ends_with(".tmp") {
                reject_links_between(&context.root, &path)?;
                let metadata = fs::symlink_metadata(&path).map_err(workspace_io_error)?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || is_windows_reparse(&metadata)
                {
                    return Err(workspace_error(
                        "Agent workspace lease temporary is invalid.",
                    ));
                }
                remove_project_file(&context.root, &path).map_err(workspace_io_error)?;
                continue;
            }
            if !name.ends_with(".json") {
                return Err(workspace_error(
                    "Agent workspace lease filename is invalid.",
                ));
            }
            let bytes = read_isolated_regular_file(&context.root, &path, 64 * 1024)?;
            let lease: AgentWorkspaceLease = serde_json::from_slice(&bytes)
                .map_err(|_| workspace_error("Agent workspace lease is invalid."))?;
            if lease.session_id != session_id
                || lease.item_id != item_id
                || !is_safe_component(&lease.workspace_id)
            {
                return Err(workspace_error(
                    "Agent workspace lease identity is invalid.",
                ));
            }
            let preserve = match lease.task_id.as_deref() {
                Some(task_id) => preserve_task(task_id),
                None => {
                    lease.process_instance_id == *process_instance_id()
                        && lease.expires_at > Utc::now()
                }
            };
            if !preserve {
                let relative = context.layout.import_paths()?.item_staging_child(
                    session_id,
                    item_id,
                    &["agent", &lease.workspace_id],
                )?;
                Self::cleanup_recorded_workspace(context, session_id, item_id, &relative)?;
            }
        }
        Ok(())
    }
}

fn process_instance_id() -> &'static String {
    static INSTANCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    INSTANCE.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

pub(crate) fn validate_isolated_directory(
    root: &Path,
    directory: &Path,
) -> Result<(), BackendError> {
    reject_links_between(root, directory)?;
    let canonical_root = root.canonicalize().map_err(workspace_io_error)?;
    let canonical = directory.canonicalize().map_err(workspace_io_error)?;
    if !canonical.starts_with(&canonical_root)
        || !fs::symlink_metadata(directory)
            .map_err(workspace_io_error)?
            .is_dir()
    {
        return Err(workspace_error(
            "Agent input directory escaped its isolated workspace.",
        ));
    }
    Ok(())
}

pub(crate) fn read_isolated_regular_file(
    root: &Path,
    path: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>, BackendError> {
    reject_links_between(root, path)?;
    let canonical_root = root.canonicalize().map_err(workspace_io_error)?;
    let before = path.canonicalize().map_err(workspace_io_error)?;
    if !before.starts_with(&canonical_root) {
        return Err(workspace_error(
            "Agent input escaped its isolated workspace.",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let mut file = options.open(path).map_err(workspace_io_error)?;
    let opened = file.metadata().map_err(workspace_io_error)?;
    let path_metadata = fs::symlink_metadata(path).map_err(workspace_io_error)?;
    if !opened.is_file()
        || path_metadata.file_type().is_symlink()
        || is_windows_reparse(&path_metadata)
        || !same_path_metadata(&opened, &path_metadata)
    {
        return Err(workspace_error(
            "Agent input changed while it was being opened.",
        ));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((max_bytes.saturating_add(1)) as u64)
        .read_to_end(&mut bytes)
        .map_err(workspace_io_error)?;
    if bytes.len() > max_bytes {
        return Err(workspace_error(
            "Agent input exceeds the isolated read limit.",
        ));
    }
    let after = path.canonicalize().map_err(workspace_io_error)?;
    let after_metadata = fs::symlink_metadata(path).map_err(workspace_io_error)?;
    let verification = options.open(path).map_err(workspace_io_error)?;
    if before != after
        || !after.starts_with(&canonical_root)
        || after_metadata.file_type().is_symlink()
        || is_windows_reparse(&after_metadata)
        || !same_path_metadata(&opened, &after_metadata)
        || !same_open_file(&file, &verification)
    {
        return Err(workspace_error(
            "Agent input changed while it was being read.",
        ));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn same_path_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_path_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    left.file_attributes() == right.file_attributes()
        && left.file_size() == right.file_size()
        && left.last_write_time() == right.last_write_time()
}

#[cfg(unix)]
fn same_open_file(left: &fs::File, right: &fs::File) -> bool {
    match (left.metadata(), right.metadata()) {
        (Ok(left), Ok(right)) => same_path_metadata(&left, &right),
        _ => false,
    }
}

#[cfg(windows)]
fn same_open_file(left: &fs::File, right: &fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
    };
    fn identity(file: &fs::File) -> Option<(u32, u32, u32)> {
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        let ok =
            unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information) }
                != 0;
        ok.then_some((
            information.dwVolumeSerialNumber,
            information.nFileIndexHigh,
            information.nFileIndexLow,
        ))
    }
    identity(left).is_some_and(|left| Some(left) == identity(right))
}

fn validate_identity(
    context: &ProjectContext,
    session: &ImportSession,
    item: &ImportItem,
) -> Result<(), BackendError> {
    if session.project_id != context.project_id
        || !session
            .items
            .iter()
            .any(|candidate| candidate.item_id == item.item_id)
        || !is_safe_component(&session.session_id)
        || !is_safe_component(&item.item_id)
    {
        return Err(workspace_error(
            "Agent workspace identity is not bound to this project item.",
        ));
    }
    Ok(())
}

fn copy_hard_failure_source(
    context: &ProjectContext,
    session: &ImportSession,
    item: &ImportItem,
    destination_dir: &Path,
) -> Result<(String, Vec<String>), BackendError> {
    let staging = context.resolve_project_path(
        &context
            .layout
            .import_paths()?
            .item_staging(&session.session_id, &item.item_id)?,
    )?;
    let mut candidates = vec![staging.join("source.bin")];
    let authorized = staging.join("authorized");
    if authorized.exists() {
        validate_isolated_directory(&context.root, &authorized)?;
        for entry in fs::read_dir(&authorized).map_err(workspace_io_error)? {
            let path = entry.map_err(workspace_io_error)?.path();
            if fs::symlink_metadata(&path)
                .map_err(workspace_io_error)?
                .is_file()
            {
                candidates.push(path);
            }
        }
    }
    let staged = candidates.into_iter().find(|path| {
        fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    });
    let (source, bytes) = if let Some(path) = staged {
        let bytes = read_isolated_regular_file(&context.root, &path, 128 * 1024 * 1024)?;
        (path, bytes)
    } else if let Some(identity) = &item.input.source_identity {
        let asserted = PathBuf::from(&identity.canonical_path);
        let parent = asserted.parent().ok_or_else(|| {
            workspace_error("The authorized source has no isolated parent directory.")
        })?;
        let bytes = read_isolated_regular_file(parent, &asserted, 128 * 1024 * 1024)?;
        let canonical = asserted.canonicalize().map_err(workspace_io_error)?;
        if canonical != asserted {
            return Err(workspace_error(
                "The authorized source changed before Agent assistance.",
            ));
        }
        (canonical, bytes)
    } else {
        return Err(workspace_error(
            "No sanitized source snapshot is available for this hard failure.",
        ));
    };
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if let Some(identity) = &item.input.source_identity {
        if identity.size_bytes != bytes.len() as u64 || identity.sha256 != hash {
            return Err(workspace_error(
                "The authorized source changed before Agent assistance.",
            ));
        }
    }
    let source_name = stable_copy_name("source", &source.to_string_lossy());
    let destination = destination_dir.join(&source_name);
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(&context.root, &destination)
        .map_err(workspace_io_error)?;
    binding
        .write_atomic_replace(&destination, &bytes)
        .map_err(workspace_io_error)?;
    Ok((source_name, vec![hash]))
}

fn copy_verified_item_artifact(
    context: &ProjectContext,
    session: &ImportSession,
    item: &ImportItem,
    artifact: &ImportArtifact,
    destination: &Path,
) -> Result<(), BackendError> {
    let expected_prefix = format!(
        "{}/",
        context
            .layout
            .import_paths()?
            .item_root(&session.session_id, &item.item_id)?
    );
    let normalized = artifact.relative_path.replace('\\', "/");
    if !normalized.starts_with(&expected_prefix) {
        return Err(workspace_error(
            "Artifact is outside the current import item staging area.",
        ));
    }
    let source = context.resolve_project_path(&normalized).map_err(|_| {
        workspace_error("Artifact path is not contained by the current project item.")
    })?;
    let bytes = read_isolated_regular_file(&context.root, &source, 128 * 1024 * 1024)?;
    if bytes.len() as u64 != artifact.size_bytes
        || format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256
    {
        return Err(workspace_error(
            "Artifact changed after deterministic staging.",
        ));
    }
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(&context.root, destination)
        .map_err(workspace_io_error)?;
    binding
        .write_atomic_replace(destination, &bytes)
        .map_err(workspace_io_error)
}

fn reject_links_between(root: &Path, target: &Path) -> Result<(), BackendError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| workspace_error("Artifact is outside the project."))?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component);
        let metadata = fs::symlink_metadata(&cursor).map_err(workspace_io_error)?;
        if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) {
            return Err(workspace_error(
                "Links and reparse points are not accepted as Agent inputs.",
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_windows_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

fn public_source(item: &ImportItem) -> String {
    if item.input.kind == ImportInputKind::Url {
        return item
            .input
            .normalized_locator
            .clone()
            .unwrap_or_else(|| "redacted-url".into());
    }
    item.input.display_name.clone()
}

fn stable_copy_name(stem: &str, original: &str) -> String {
    let extension = Path::new(original)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.len() <= 12 && value.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default();
    format!("{stem}{extension}")
}

fn write_json(
    project_root: &Path,
    path: &Path,
    value: &impl Serialize,
) -> Result<(), BackendError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        BackendError::new(
            "IMPORT_AGENT_WORKSPACE_INVALID",
            error.to_string(),
            false,
            false,
        )
    })?;
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(project_root, path)
        .map_err(workspace_io_error)?;
    binding
        .write_atomic_replace(path, &bytes)
        .map_err(workspace_io_error)
}

fn write_json_atomic_path(
    project_root: &Path,
    path: &Path,
    value: &impl Serialize,
) -> Result<(), BackendError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| workspace_error("Agent workspace lease is invalid."))?;
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(project_root, path)
        .map_err(workspace_io_error)?;
    binding
        .write_atomic_replace(path, &bytes)
        .map_err(workspace_io_error)
}

fn set_tree_readonly(root: &Path) -> Result<(), BackendError> {
    for entry in fs::read_dir(root).map_err(workspace_io_error)? {
        let path = entry.map_err(workspace_io_error)?.path();
        if path.is_dir() {
            set_tree_readonly(&path)?;
        } else {
            set_readonly(&path, true)?;
        }
    }
    Ok(())
}

fn set_readonly(path: &Path, readonly: bool) -> Result<(), BackendError> {
    let mut permissions = fs::metadata(path)
        .map_err(workspace_io_error)?
        .permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions).map_err(workspace_io_error)
}

fn remove_workspace_tree(project_root: &Path, root: &Path) -> Result<(), BackendError> {
    let binding = match BoundProjectMutationRoot::bind(project_root, root) {
        Ok(binding) => binding,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(workspace_io_error(error)),
    };
    binding
        .remove_directory_tree(root)
        .map_err(workspace_io_error)
}

fn collect_file_hashes(root: &Path, hashes: &mut Vec<String>) -> Result<(), BackendError> {
    for entry in fs::read_dir(root).map_err(workspace_io_error)? {
        let path = entry.map_err(workspace_io_error)?.path();
        reject_links_between(root, &path)?;
        if path.is_dir() {
            collect_file_hashes(&path, hashes)?;
        } else {
            hashes.push(format!(
                "{:x}",
                Sha256::digest(fs::read(path).map_err(workspace_io_error)?)
            ));
        }
    }
    Ok(())
}

fn is_safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn workspace_error(message: impl Into<String>) -> BackendError {
    BackendError::new("IMPORT_AGENT_WORKSPACE_PATH_REJECTED", message, false, true)
}

fn workspace_io_error(error: std::io::Error) -> BackendError {
    BackendError::new(
        "IMPORT_AGENT_WORKSPACE_IO_FAILED",
        error.to_string(),
        true,
        false,
    )
}

#[cfg(test)]
mod sealed_read_tests {
    use super::*;

    #[test]
    fn isolated_read_accepts_regular_file_and_rejects_linked_directory() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(root.path().join("safe.txt"), "safe").unwrap();
        fs::write(outside.path().join("secret.txt"), "outside-secret").unwrap();
        assert_eq!(
            read_isolated_regular_file(root.path(), &root.path().join("safe.txt"), 32).unwrap(),
            b"safe"
        );

        let link = root.path().join("linked");
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "junction setup failed");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();

        assert!(read_isolated_regular_file(root.path(), &link.join("secret.txt"), 32).is_err());
    }
}

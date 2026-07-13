use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    errors::BackendError,
    models::{
        import_v2::{ImportArtifact, ImportInputKind, ImportItem, ImportSession},
        import_v2_agent::{AgentAssistanceTrigger, AgentToolGrant},
        paths::ProjectContext,
    },
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
    pub workspace_id: String,
    pub root: PathBuf,
    pub task_path: PathBuf,
    pub source_dir: PathBuf,
    pub deterministic_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub output_dir: PathBuf,
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
        validate_identity(context, session, item)?;
        let workspace_id = uuid::Uuid::new_v4().to_string();
        let relative_root = format!(
            ".app/import-sessions/{}/items/{}/staging/agent/{workspace_id}",
            session.session_id, item.item_id
        );
        let root = context.resolve_project_path(&relative_root)?;
        let source_dir = root.join("source");
        let deterministic_dir = root.join("deterministic");
        let logs_dir = root.join("logs");
        let output_dir = root.join("output");
        for dir in [&source_dir, &deterministic_dir, &logs_dir, &output_dir] {
            fs::create_dir_all(dir).map_err(workspace_io_error)?;
        }
        if let Err(error) = reject_links_between(&context.root, &root) {
            make_tree_writable(&root);
            let _ = fs::remove_dir_all(&root);
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
                for (index, asset) in preview.assets.iter().enumerate() {
                    let asset_dir = deterministic_dir.join("assets");
                    fs::create_dir_all(&asset_dir).map_err(workspace_io_error)?;
                    let name = stable_copy_name(&format!("asset-{index}"), &asset.relative_path);
                    copy_verified_item_artifact(
                        context,
                        session,
                        item,
                        asset,
                        &asset_dir.join(name),
                    )?;
                }
                (
                    source_name,
                    vec![
                        preview.source_snapshot.sha256.clone(),
                        preview.markdown.sha256.clone(),
                    ],
                )
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
            write_json(&task_path, &task)?;
            let attempts_path = logs_dir.join("attempts.json");
            write_json(&attempts_path, &Vec::<serde_json::Value>::new())?;

            set_tree_readonly(&source_dir)?;
            set_tree_readonly(&deterministic_dir)?;
            set_readonly(&task_path, true)?;
            set_readonly(&attempts_path, true)?;
            set_readonly(&output_dir, false)?;

            Ok(AgentWorkspace {
                workspace_id,
                root: root.clone(),
                task_path,
                source_dir,
                deterministic_dir,
                logs_dir,
                output_dir,
            })
        })();

        if result.is_err() {
            make_tree_writable(&root);
            let _ = fs::remove_dir_all(&root);
        }
        result
    }

    pub fn cleanup_terminal(workspace: &AgentWorkspace) -> Result<Vec<String>, BackendError> {
        let mut hashes = Vec::new();
        if workspace.output_dir.exists() {
            collect_file_hashes(&workspace.output_dir, &mut hashes)?;
        }
        make_tree_writable(&workspace.root);
        fs::remove_dir_all(&workspace.root).map_err(workspace_io_error)?;
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
    let staging = context.resolve_project_path(&format!(
        ".app/import-sessions/{}/items/{}/staging",
        session.session_id, item.item_id
    ))?;
    let mut candidates = vec![staging.join("source.bin")];
    let authorized = staging.join("authorized");
    if authorized.is_dir() {
        for entry in fs::read_dir(&authorized).map_err(workspace_io_error)? {
            let path = entry.map_err(workspace_io_error)?.path();
            if path.is_file() {
                candidates.push(path);
            }
        }
    }
    let staged = candidates.into_iter().find(|path| path.is_file());
    let source = if let Some(path) = staged {
        reject_links_between(&context.root, &path)?;
        path
    } else if let Some(identity) = &item.input.source_identity {
        let asserted = PathBuf::from(&identity.canonical_path);
        let canonical = asserted.canonicalize().map_err(workspace_io_error)?;
        if canonical != asserted
            || fs::symlink_metadata(&canonical)
                .map_err(workspace_io_error)?
                .file_type()
                .is_symlink()
        {
            return Err(workspace_error(
                "The authorized source changed before Agent assistance.",
            ));
        }
        canonical
    } else {
        return Err(workspace_error(
            "No sanitized source snapshot is available for this hard failure.",
        ));
    };
    let metadata = fs::metadata(&source).map_err(workspace_io_error)?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err(workspace_error(
            "The hard-failure source is not an allowed regular file.",
        ));
    }
    let bytes = fs::read(&source).map_err(workspace_io_error)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if let Some(identity) = &item.input.source_identity {
        if source == PathBuf::from(&identity.canonical_path)
            && (identity.size_bytes != bytes.len() as u64 || identity.sha256 != hash)
        {
            return Err(workspace_error(
                "The authorized source changed before Agent assistance.",
            ));
        }
    }
    let source_name = stable_copy_name("source", &source.to_string_lossy());
    fs::write(destination_dir.join(&source_name), bytes).map_err(workspace_io_error)?;
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
        ".app/import-sessions/{}/items/{}/",
        session.session_id, item.item_id
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
    reject_links_between(&context.root, &source)?;
    let bytes = fs::read(&source).map_err(workspace_io_error)?;
    if bytes.len() as u64 != artifact.size_bytes
        || format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256
    {
        return Err(workspace_error(
            "Artifact changed after deterministic staging.",
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(workspace_io_error)?;
    }
    fs::write(destination, bytes).map_err(workspace_io_error)
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

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), BackendError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        BackendError::new(
            "IMPORT_AGENT_WORKSPACE_INVALID",
            error.to_string(),
            false,
            false,
        )
    })?;
    fs::write(path, bytes).map_err(workspace_io_error)
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

fn make_tree_writable(root: &Path) {
    let Ok(metadata) = fs::symlink_metadata(root) else {
        return;
    };
    if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) {
        return;
    }
    {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = fs::set_permissions(root, permissions);
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            make_tree_writable(&entry.path());
        }
    }
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

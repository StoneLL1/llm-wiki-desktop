use std::path::{Component, Path};
#[cfg(not(any(unix, windows)))]
use std::time::UNIX_EPOCH;

use chrono::Utc;

use crate::models::task::{BackendTask, TaskStatus};
use crate::models::workflow::{
    WorkflowCandidateReference, WorkflowErrorSummary, WorkflowExecutionState, WorkflowKind,
    WorkflowPrerequisiteAction, WorkflowProjectMutationState,
};

use super::fingerprint::hex_sha256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkflowIdentity {
    pub canonical_root: std::path::PathBuf,
    pub canonical_identity_key: String,
    pub identity_revision: String,
}

pub fn project_identity(root: &Path) -> Result<ProjectWorkflowIdentity, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Project root could not be canonicalized: {error}"))?;
    let mut normalized = canonical_root.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized = normalized.to_lowercase();
    }
    let canonical_identity_key = hex_sha256(format!("workflow-root-v1\n{normalized}").as_bytes());
    let revision_material = platform_revision_material(&canonical_root)?;
    let identity_revision = hex_sha256(
        format!("workflow-identity-v1\n{canonical_identity_key}\n{revision_material}").as_bytes(),
    );
    Ok(ProjectWorkflowIdentity {
        canonical_root,
        canonical_identity_key,
        identity_revision,
    })
}

#[cfg(unix)]
fn platform_revision_material(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Project root metadata is unavailable: {error}"))?;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn platform_revision_material(path: &Path) -> Result<String, String> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    // Omitting FILE_SHARE_DELETE pins the lexical root while its identity is
    // measured. OPEN_REPARSE_POINT then prevents a raced leaf junction from
    // being followed after canonicalization.
    let directory = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| format!("Project root could not be opened safely: {error}"))?;
    let metadata = directory
        .metadata()
        .map_err(|error| format!("Project root metadata is unavailable: {error}"))?;
    let attributes = metadata.file_attributes();
    if attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err("Project root is not a directory.".into());
    }
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Project root resolved to a reparse point.".into());
    }

    let mut information = FILE_ID_INFO::default();
    // SAFETY: `directory` owns a live directory handle for the duration of the
    // call and `information` is writable storage with the exact Win32 layout.
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            directory.as_raw_handle().cast(),
            FileIdInfo,
            (&mut information as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if succeeded == 0 {
        return Err(format!(
            "Project root file identity is unavailable: {}",
            std::io::Error::last_os_error()
        ));
    }
    if information.FileId.Identifier.iter().all(|byte| *byte == 0) {
        return Err(
            "Project root filesystem does not expose a stable 128-bit file identity.".into(),
        );
    }

    let file_id = information
        .FileId
        .Identifier
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!(
        "windows:{:016x}:{file_id}",
        information.VolumeSerialNumber
    ))
}

#[cfg(not(any(unix, windows)))]
fn platform_revision_material(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Project root metadata is unavailable: {error}"))?;
    let created = metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(format!("portable:{created}:{}", metadata.len()))
}

pub fn recover_workflow(
    task: &mut BackendTask,
    workflow: &mut WorkflowExecutionState,
    project_root: &Path,
) {
    if let Some(result) =
        super::runners::update_wiki::committed_update_wiki_result(&task.id, project_root)
    {
        let now = Utc::now().to_rfc3339();
        for stage in &mut workflow.stages {
            if matches!(
                stage.status,
                crate::models::workflow::WorkflowStageStatus::Running
                    | crate::models::workflow::WorkflowStageStatus::Pending
            ) {
                stage.status = crate::models::workflow::WorkflowStageStatus::Completed;
                stage.completed_at = Some(now.clone());
            }
        }
        workflow.current_stage_id = None;
        workflow.pending_action = None;
        workflow.result = Some(result);
        workflow.error = None;
        workflow.continuation_required = false;
        task.status = TaskStatus::Succeeded;
        task.cancellable = false;
        task.completed_at = Some(now.clone());
        task.updated_at = now;
        return;
    }
    match task.status {
        TaskStatus::Queued => {
            workflow.continuation_required = true;
        }
        TaskStatus::Running | TaskStatus::Cancelling => interrupt(task, workflow),
        TaskStatus::WaitingForConfirmation
            if !pending_action_is_valid(&task.id, workflow, project_root) =>
        {
            interrupt(task, workflow)
        }
        TaskStatus::WaitingForConfirmation
        | TaskStatus::Succeeded
        | TaskStatus::Failed
        | TaskStatus::Cancelled
        | TaskStatus::Interrupted => {}
    }
}

fn interrupt(task: &mut BackendTask, workflow: &mut WorkflowExecutionState) {
    let now = Utc::now().to_rfc3339();
    task.status = TaskStatus::Interrupted;
    task.error = Some(crate::errors::BackendError::new(
        "WORKFLOW_INTERRUPTED",
        "Workflow was interrupted by application restart",
        true,
        true,
    ));
    task.updated_at = now.clone();
    task.completed_at = Some(now);
    workflow.continuation_required = false;
    workflow.queue_position = None;
    workflow.error = Some(WorkflowErrorSummary {
        code: "WORKFLOW_INTERRUPTED".into(),
        message_key: "workflows.error.interrupted".into(),
        recoverable: true,
        user_action_required: true,
        suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
        project_mutation_state: WorkflowProjectMutationState::Unknown,
    });
}

fn pending_action_is_valid(
    task_id: &str,
    workflow: &WorkflowExecutionState,
    project_root: &Path,
) -> bool {
    let Some(pending) = workflow.pending_action.as_ref() else {
        return false;
    };
    if pending.id.trim().is_empty()
        || pending
            .affected_paths
            .iter()
            .any(|path| !safe_relative(path))
    {
        return false;
    }
    if let Some(expires_at) = pending.expires_at.as_deref() {
        let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expires_at) else {
            return false;
        };
        if expires < Utc::now() {
            return false;
        }
    }
    if let Some(checkpoint_hash) = pending.checkpoint_hash.as_deref() {
        if !crate::services::GitService::checkpoint_exists(project_root, checkpoint_hash) {
            return false;
        }
    }
    if let Some(candidate) = pending.candidate.as_ref() {
        match candidate {
            WorkflowCandidateReference::TaskOwned { candidate_id } => {
                let valid = match workflow.kind {
                    WorkflowKind::UpdateWiki => super::runners::update_wiki::update_wiki_candidate_is_valid_for_workflow(
                        task_id,
                        project_root,
                        workflow,
                    ),
                    WorkflowKind::GenerateContent => super::runners::generate_content::generate_content_candidate_is_valid_for_workflow(
                        task_id,
                        candidate_id,
                        project_root,
                        workflow,
                    ),
                    WorkflowKind::HealthCheck => match &workflow.execution_options.operation {
                        crate::models::workflow::WorkflowOperation::AgentLintRepair { .. } => {
                            super::runners::agent_lint_repair::agent_lint_repair_candidate_is_valid_for_workflow(
                                task_id,
                                candidate_id,
                                project_root,
                                workflow,
                            )
                        }
                        // Deterministic Health remains read-only and has no
                        // task-owned mutation candidate to restore.
                        crate::models::workflow::WorkflowOperation::BuiltIn => false,
                    },
                };
                if !valid {
                    return false;
                }
            }
            WorkflowCandidateReference::ProjectRelative { path } => {
                if !safe_relative(path) || !existing_path_is_inside(project_root, path) {
                    return false;
                }
            }
        }
    }
    let Some(current_stage_id) = workflow.current_stage_id.as_deref() else {
        return false;
    };
    workflow.stages.iter().any(|stage| {
        stage.id == current_stage_id
            && stage.status == crate::models::workflow::WorkflowStageStatus::Waiting
            && stage.decision.as_ref() == Some(pending)
    })
}

fn existing_path_is_inside(project_root: &Path, relative: &str) -> bool {
    let Ok(root) = project_root.canonicalize() else {
        return false;
    };
    project_root
        .join(relative.replace('\\', "/"))
        .canonicalize()
        .is_ok_and(|candidate| candidate.starts_with(root))
}

fn safe_relative(value: &str) -> bool {
    let value = value.trim().replace('\\', "/");
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains(':')
        && Path::new(&value)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

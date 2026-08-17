use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::git::{CheckpointPurpose, GitCheckpoint, GitDiff, GitRepositoryStatus};
use crate::models::project::AssessmentId;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCheckpointRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub purpose: CheckpointPurpose,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessedGitRequest {
    pub assessment_id: AssessmentId,
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn git_status(
    state: State<'_, AppState>,
    request: GitProjectRequest,
) -> Result<GitRepositoryStatus, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    // Git status is always a live repository read. Accepting force_refresh
    // keeps the typed IPC contract explicit if a cache is introduced later.
    let _force_refresh = request.force_refresh;
    state.git_service.repository_status(&context)
}

#[tauri::command]
pub fn initialize_git_repository(
    state: State<'_, AppState>,
    request: AssessedGitRequest,
) -> Result<crate::models::confirmation::PendingAction, BackendError> {
    let context = revalidate_assessed_git_request(&state, &request)?;
    let status = state.git_service.repository_status(&context)?;
    if status.head.is_some() {
        return Err(BackendError::new(
            "GIT_REPOSITORY_EXISTS",
            "The project already has local Git history.",
            true,
            true,
        ));
    }
    let expected_paths = state.git_service.initial_commit_paths(&context)?;
    let mut affected_paths = vec![".git".to_string()];
    affected_paths.extend(expected_paths.iter().cloned());
    affected_paths.sort();
    affected_paths.dedup();
    let action = crate::models::confirmation::PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: crate::models::confirmation::PendingActionType::InitializeGitRepository,
        title: "Initialize local Git history".into(),
        message: "Create a local Git repository and initial commit. No remote will be added."
            .into(),
        risk_level: crate::models::confirmation::RiskLevel::High,
        affected_paths,
        preview: None,
        expires_at: None,
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(
            crate::models::confirmation::ConfirmationExecution::InitializeAssessedGit {
                assessment_id: request.assessment_id,
                project_id: request.project_id,
                root_path: request.project_root_path,
                expected_head: status.head,
                expected_paths,
            },
        ),
    )?;
    Ok(action)
}

#[tauri::command]
pub fn request_assessed_git_checkpoint(
    state: State<'_, AppState>,
    request: AssessedGitRequest,
) -> Result<crate::models::confirmation::PendingAction, BackendError> {
    let context = revalidate_assessed_git_request(&state, &request)?;
    let status = state.git_service.repository_status(&context)?;
    if !status.is_repository {
        return Err(BackendError::new(
            "GIT_REPOSITORY_MISSING",
            "Initialize local Git history before creating a checkpoint.",
            true,
            true,
        ));
    }
    if status.head.is_none() {
        return Err(BackendError::new(
            "GIT_HEAD_MISSING",
            "Complete local Git initialization before creating a checkpoint.",
            true,
            true,
        ));
    }
    let affected_paths = state.git_service.changed_paths(&context)?;
    if affected_paths.is_empty() {
        return Err(BackendError::new(
            "GIT_WORKTREE_CLEAN",
            "There are no project changes to checkpoint.",
            true,
            true,
        ));
    }
    let action = crate::models::confirmation::PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: crate::models::confirmation::PendingActionType::CreateGitCheckpoint,
        title: "Checkpoint current project changes".into(),
        message: "Commit all current project changes as an explicit local checkpoint.".into(),
        risk_level: crate::models::confirmation::RiskLevel::High,
        affected_paths: affected_paths.clone(),
        preview: None,
        expires_at: None,
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(
            crate::models::confirmation::ConfirmationExecution::CheckpointAssessedGit {
                assessment_id: request.assessment_id,
                project_id: request.project_id,
                root_path: request.project_root_path,
                expected_head: status.head,
                expected_paths: affected_paths.clone(),
            },
        ),
    )?;
    Ok(action)
}

fn revalidate_assessed_git_request(
    state: &AppState,
    request: &AssessedGitRequest,
) -> Result<crate::models::paths::ProjectContext, BackendError> {
    let assessment = crate::commands::project_commands::revalidate_project_assessment(
        state,
        &request.assessment_id,
    )?;
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let assessed_root = std::path::PathBuf::from(&assessment.canonical_root_path)
        .canonicalize()
        .map_err(|_| assessed_git_context_mismatch())?;
    if assessed_root != context.root {
        return Err(assessed_git_context_mismatch());
    }
    let access = state.resolve_workflow_access(&context)?;
    if access.trust != crate::models::workflow::WorkflowProjectTrust::Trusted {
        return Err(BackendError::new(
            "WORKFLOW_PROJECT_UNTRUSTED",
            "Git remediation requires a trusted project.",
            true,
            true,
        ));
    }
    if access.filesystem_access != crate::models::workflow::WorkflowFilesystemAccess::Writable {
        return Err(BackendError::new(
            "WORKFLOW_PROJECT_READ_ONLY",
            "Git remediation requires writable project access.",
            true,
            true,
        ));
    }
    Ok(context)
}

fn assessed_git_context_mismatch() -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_CONTEXT_MISMATCH",
        "The assessment does not belong to the active project.",
        true,
        true,
    )
}

#[tauri::command]
pub fn create_git_checkpoint(
    state: State<'_, AppState>,
    request: CreateCheckpointRequest,
) -> Result<GitCheckpoint, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            state
                .git_service
                .create_checkpoint(context, request.purpose, &request.message)
        },
    )
}

#[tauri::command]
pub fn git_diff_markdown(
    state: State<'_, AppState>,
    request: GitProjectRequest,
) -> Result<GitDiff, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.git_service.diff_markdown(&context)
}

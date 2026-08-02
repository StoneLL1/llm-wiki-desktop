use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{
    ConfirmationExecution, ConfirmationStatus, ConfirmedAction, StoredPendingAction,
};
use crate::services::import_v2::source_lifecycle::{
    reject_generic_source_create, reject_generic_source_path,
};
use crate::services::{
    cancel_generate_content_confirmation, confirm_generate_content_overwrite,
    GenerateContentExecutionServices, WriteMode,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteMarkdownRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
    pub contents: String,
    pub mode: WriteMode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteJsonRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
    pub value: serde_json::Value,
    pub mode: WriteMode,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHashResponse {
    pub relative_path: String,
    pub hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPendingActionRequest {
    pub action_id: String,
    pub status: ConfirmationStatus,
}

#[tauri::command]
pub fn read_markdown_file(
    state: State<'_, AppState>,
    request: ProjectFileRequest,
) -> Result<String, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .file_store
        .read_markdown(&context, &request.relative_path)
}

#[tauri::command]
pub fn write_markdown_file(
    state: State<'_, AppState>,
    request: WriteMarkdownRequest,
) -> Result<FileHashResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    reject_generic_source_path(&context, &state.file_store, &request.relative_path)?;
    reject_generic_source_create(&request.relative_path, None, Some(&request.contents))?;
    state.file_store.write_markdown_checked(
        &context,
        &request.relative_path,
        &request.contents,
        request.mode,
    )?;
    let hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    Ok(FileHashResponse {
        relative_path: request.relative_path,
        hash,
    })
}

#[tauri::command]
pub fn write_json_file(
    state: State<'_, AppState>,
    request: WriteJsonRequest,
) -> Result<FileHashResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.file_store.write_json_atomic_checked(
        &context,
        &request.relative_path,
        &request.value,
        request.mode,
    )?;
    let hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    Ok(FileHashResponse {
        relative_path: request.relative_path,
        hash,
    })
}

#[tauri::command]
pub fn get_file_hash(
    state: State<'_, AppState>,
    request: ProjectFileRequest,
) -> Result<FileHashResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    Ok(FileHashResponse {
        relative_path: request.relative_path,
        hash,
    })
}

#[tauri::command]
pub fn confirm_pending_action(
    state: State<'_, AppState>,
    request: ConfirmPendingActionRequest,
) -> Result<ConfirmedAction, BackendError> {
    if state
        .confirmation_registry
        .peek(&request.action_id)?
        .execution
        .is_some_and(|execution| {
            matches!(execution, ConfirmationExecution::UpdateWikiReview { .. })
        })
    {
        return Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Update Wiki review must be handled by confirm_workflow_action or discard_workflow_result.",
            true,
            true,
        ));
    }
    if request.status == ConfirmationStatus::Confirmed {
        let pending = state.confirmation_registry.peek(&request.action_id)?;
        if let Some(ConfirmationExecution::GenerateContentOverwrite {
            project_id,
            root_path,
            ..
        }) = pending.execution.as_ref()
        {
            let context = state.resolve_project_context(&project_id, &root_path)?;
            let access = crate::services::WorkflowAccessSnapshot::legacy_fail_closed(
                &context,
                &state.git_service,
            )?;
            if access.trust != crate::models::workflow::WorkflowProjectTrust::Trusted {
                return Err(BackendError::new(
                    "WORKFLOW_PROJECT_UNTRUSTED",
                    "Generate Content confirmation requires a trusted project.",
                    true,
                    true,
                ));
            }
            if access.filesystem_access
                != crate::models::workflow::WorkflowFilesystemAccess::Writable
            {
                return Err(BackendError::new(
                    "WORKFLOW_PROJECT_READ_ONLY",
                    "Generate Content confirmation requires writable project access.",
                    true,
                    true,
                ));
            }
        }

        if matches!(
            pending.execution.as_ref(),
            Some(
                ConfirmationExecution::EnableCompatibleProject { .. }
                    | ConfirmationExecution::TrustCompatibleProject { .. }
                    | ConfirmationExecution::InitializeAssessedGit { .. }
                    | ConfirmationExecution::CheckpointAssessedGit { .. }
            )
        ) {
            let stored = state.confirmation_registry.claim(&request.action_id)?;
            let result = execute_claimed_project_authority_action(&state, stored);
            match result {
                Ok(confirmed) => {
                    state
                        .confirmation_registry
                        .finish_claim(&request.action_id, true)?;
                    return Ok(confirmed);
                }
                Err(error) => {
                    state
                        .confirmation_registry
                        .finish_claim(&request.action_id, false)?;
                    return Err(error);
                }
            }
        }
    }
    let stored = state
        .confirmation_registry
        .confirm(&request.action_id, request.status.clone())?;

    if request.status == ConfirmationStatus::Cancelled {
        if let Some(ConfirmationExecution::GenerateContentOverwrite { task_id, .. }) =
            stored.execution.as_ref()
        {
            let next = cancel_generate_content_confirmation(
                task_id,
                &GenerateContentExecutionServices {
                    export_service: &state.export_service,
                    search_service: &state.search_service,
                    settings_service: &state.settings_service,
                    secret_service: &state.secret_service,
                    agent_service: &state.agent_service,
                    llm_service: &state.llm_service,
                    git_service: &state.git_service,
                    confirmation_registry: &state.confirmation_registry,
                    task_service: &state.task_service,
                    coordinator: &state.workflow_service.coordinator,
                },
            )?;
            if let Some(next) = next {
                state.workflow_service.dispatch_claimed_run(&next)?;
            }
        }
        return Ok(ConfirmedAction {
            action: stored.action,
            status: ConfirmationStatus::Cancelled,
            checkpoint_exists: false,
            project_summary: None,
        });
    }

    match stored.execution {
        Some(ConfirmationExecution::InitializeFolder {
            root_path,
            file_hashes,
        }) => {
            let (project_summary, checkpoint_exists) =
                state.project_service.confirm_folder_initialization(
                    &PathBuf::from(root_path),
                    &stored.action,
                    &file_hashes,
                )?;
            state.project_registry.register(
                project_summary.project_id.clone(),
                &PathBuf::from(&project_summary.root_path),
            )?;
            Ok(ConfirmedAction {
                action: stored.action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists,
                project_summary: Some(project_summary),
            })
        }
        Some(
            ConfirmationExecution::EnableCompatibleProject { .. }
            | ConfirmationExecution::TrustCompatibleProject { .. }
            | ConfirmationExecution::InitializeAssessedGit { .. }
            | ConfirmationExecution::CheckpointAssessedGit { .. },
        ) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Project authority confirmations must execute through the retryable claim path.",
            true,
            true,
        )),
        Some(ConfirmationExecution::CompileMerge { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Compile conflicts must be handled by confirm_compile_action.",
            true,
            true,
        )),
        Some(ConfirmationExecution::LintFix { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Lint fixes must be handled by apply_lint_fix.",
            true,
            true,
        )),
        Some(ConfirmationExecution::ChatOverwrite { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Chat overwrites must be handled by save_answer_to_wiki.",
            true,
            true,
        )),
        Some(ConfirmationExecution::GenerateContentOverwrite {
            project_id,
            root_path,
            task_id,
        }) => {
            let context = state.resolve_project_context(&project_id, &root_path)?;
            match confirm_generate_content_overwrite(
                &context,
                &task_id,
                &GenerateContentExecutionServices {
                    export_service: &state.export_service,
                    search_service: &state.search_service,
                    settings_service: &state.settings_service,
                    secret_service: &state.secret_service,
                    agent_service: &state.agent_service,
                    llm_service: &state.llm_service,
                    git_service: &state.git_service,
                    confirmation_registry: &state.confirmation_registry,
                    task_service: &state.task_service,
                    coordinator: &state.workflow_service.coordinator,
                },
            ) {
                Ok((_, next)) => {
                    if let Some(next) = next {
                        state.workflow_service.dispatch_claimed_run(&next)?;
                    }
                }
                Err(failure) => {
                    if let Some(next) = failure.next {
                        state.workflow_service.dispatch_claimed_run(&next)?;
                    }
                    return Err(failure.error);
                }
            }
            Ok(ConfirmedAction {
                action: stored.action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists: true,
                project_summary: None,
            })
        }
        Some(ConfirmationExecution::UpdateWikiReview { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Update Wiki review must be handled by confirm_workflow_action.",
            true,
            true,
        )),
        Some(ConfirmationExecution::DeleteWikiPage {
            project_id,
            root_path,
            target_path,
            target_hash,
        }) => execute_wiki_page_delete(
            &state,
            stored.action,
            &project_id,
            &root_path,
            &target_path,
            &target_hash,
        ),
        None => Err(BackendError::new(
            "CONFIRMATION_EXECUTION_MISSING",
            "The pending action has no backend execution plan.",
            false,
            true,
        )
        .with_details(serde_json::json!({ "actionId": request.action_id }))),
    }
}

fn execute_claimed_project_authority_action(
    state: &AppState,
    stored: StoredPendingAction,
) -> Result<ConfirmedAction, BackendError> {
    let StoredPendingAction { action, execution } = stored;
    match execution {
        Some(ConfirmationExecution::EnableCompatibleProject {
            assessment_id,
            project_id,
            root_path,
            template,
            initialize_git,
        }) => {
            let assessment = crate::commands::project_commands::revalidate_project_assessment(
                state,
                &assessment_id,
            )?;
            crate::commands::project_commands::ensure_compatible_trust_candidate(&assessment)?;
            let context = state.resolve_project_context(&project_id, &root_path)?;
            let assessed_root = PathBuf::from(&assessment.canonical_root_path)
                .canonicalize()
                .map_err(|_| assessed_project_context_mismatch())?;
            if context.root != assessed_root {
                return Err(assessed_project_context_mismatch());
            }
            if state.project_service.filesystem_access(&context, true)
                != crate::models::project::ProjectFilesystemAccess::Writable
            {
                return Err(BackendError::new(
                    "WORKFLOW_PROJECT_READ_ONLY",
                    "Compatible features require writable project access.",
                    true,
                    true,
                ));
            }

            let mut checkpoint_exists = assessment.git.head.is_some();

            // The explicit confirmation authorizes the compatibility write and
            // writability probe, but trust is not published until every requested
            // filesystem/Git side effect has completed successfully.
            state
                .project_service
                .enable_compatible_guidance(&context, template)?;
            if initialize_git {
                let status = state
                    .git_service
                    .initialize_repository(&context, "Initialize compatible knowledge base")?;
                checkpoint_exists = status.head.is_some();
            }
            state.grant_compatible_project_trust(&project_id, &context.root)?;
            state
                .project_assessment_service
                .invalidate(&assessment_id)?;
            Ok(ConfirmedAction {
                action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists,
                project_summary: None,
            })
        }
        Some(ConfirmationExecution::TrustCompatibleProject {
            assessment_id,
            project_id,
            root_path,
        }) => {
            let assessment = crate::commands::project_commands::revalidate_project_assessment(
                state,
                &assessment_id,
            )?;
            crate::commands::project_commands::ensure_compatible_trust_candidate(&assessment)?;
            let context = state.resolve_project_context(&project_id, &root_path)?;
            let assessed_root = PathBuf::from(&assessment.canonical_root_path)
                .canonicalize()
                .map_err(|_| assessed_project_context_mismatch())?;
            if context.root != assessed_root {
                return Err(assessed_project_context_mismatch());
            }
            let checkpoint_exists = assessment.git.head.is_some();
            state.grant_compatible_project_trust(&project_id, &context.root)?;
            state
                .project_assessment_service
                .invalidate(&assessment_id)?;
            Ok(ConfirmedAction {
                action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists,
                project_summary: None,
            })
        }
        Some(ConfirmationExecution::InitializeAssessedGit {
            assessment_id,
            project_id,
            root_path,
            expected_head,
            expected_paths,
        }) => {
            let context =
                revalidate_assessed_context(state, &assessment_id, &project_id, &root_path)?;
            state.git_service.verify_initialization_state(
                &context,
                expected_head.as_deref(),
                &expected_paths,
            )?;
            let status = state
                .git_service
                .initialize_repository_from_snapshot(
                    &context,
                    "Initialize local knowledge base history",
                    &expected_paths,
                )?;
            state
                .project_assessment_service
                .invalidate(&assessment_id)?;
            Ok(ConfirmedAction {
                action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists: status.is_repository,
                project_summary: None,
            })
        }
        Some(ConfirmationExecution::CheckpointAssessedGit {
            assessment_id,
            project_id,
            root_path,
            expected_head,
            expected_paths,
        }) => {
            let context =
                revalidate_assessed_context(state, &assessment_id, &project_id, &root_path)?;
            state.git_service.verify_checkpoint_state(
                &context,
                expected_head.as_deref(),
                &expected_paths,
            )?;
            let checkpoint = state.git_service.create_checkpoint(
                &context,
                crate::models::git::CheckpointPurpose::HighRiskOperation,
                "Checkpoint existing project changes",
            )?;
            state
                .project_assessment_service
                .invalidate(&assessment_id)?;
            Ok(ConfirmedAction {
                action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists: checkpoint.commit_hash.is_some(),
                project_summary: None,
            })
        }
        _ => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "This confirmation is not a project-authority action.",
            true,
            true,
        )),
    }
}

fn revalidate_assessed_context(
    state: &AppState,
    assessment_id: &crate::models::project::AssessmentId,
    project_id: &str,
    root_path: &str,
) -> Result<crate::models::paths::ProjectContext, BackendError> {
    let assessment =
        crate::commands::project_commands::revalidate_project_assessment(state, assessment_id)?;
    let context = state.resolve_project_context(project_id, root_path)?;
    let assessed_root = PathBuf::from(&assessment.canonical_root_path)
        .canonicalize()
        .map_err(|_| assessed_project_context_mismatch())?;
    if context.root != assessed_root {
        return Err(assessed_project_context_mismatch());
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

fn assessed_project_context_mismatch() -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_CONTEXT_MISMATCH",
        "The assessment does not belong to the active project.",
        true,
        true,
    )
}

/// Execute a confirmed wiki page deletion: resolve the project context and
/// delegate to `SearchService::apply_page_delete`, which re-verifies the hash,
/// creates a scoped Git checkpoint, removes the file, and invalidates the graph
/// cache. The destructive logic lives in the (lib-available) service so it can
/// be unit-tested without the GUI feature; this wrapper only adapts the stored
/// confirmation execution to the service signature and assembles the
/// `ConfirmedAction`.
fn execute_wiki_page_delete(
    state: &AppState,
    action: crate::models::confirmation::PendingAction,
    project_id: &str,
    root_path: &str,
    target_path: &str,
    target_hash: &str,
) -> Result<ConfirmedAction, BackendError> {
    let context = state.resolve_project_context(project_id, root_path)?;
    let checkpoint_exists = state.search_service.apply_page_delete(
        &context,
        &state.git_service,
        target_path,
        target_hash,
    )?;
    Ok(ConfirmedAction {
        action,
        status: ConfirmationStatus::Confirmed,
        checkpoint_exists,
        project_summary: Some(state.project_service.scan_project(&context, None)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::confirmation::{
        PendingAction, PendingActionType, RiskLevel, StoredPendingAction,
    };
    use crate::models::project::{AssessmentOperationStatus, ProjectTemplate};
    use crate::services::{ProjectAssessmentService, ProjectService};
    use std::fs;
    use std::time::Duration;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-file-command-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn compatible_enablement_can_leave_git_initialization_disabled() {
        let root = temp_root("no-git");
        let config = temp_root("no-git-config");
        fs::create_dir(root.join(".obsidian")).unwrap();
        fs::write(root.join("note.md"), "# Note\n").unwrap();
        let state = AppState {
            project_service: ProjectService::with_config_dir(config.clone()),
            project_assessment_service: ProjectAssessmentService::new(config.clone()),
            ..AppState::default()
        };
        state.project_registry.register("project-a", &root).unwrap();
        let started = state
            .project_assessment_service
            .start(root.to_string_lossy().into_owned())
            .unwrap();
        let mut completed = None;
        for _ in 0..5_000 {
            let operation = state
                .project_assessment_service
                .get_operation(&started.assessment_operation_id)
                .unwrap();
            if operation.status == AssessmentOperationStatus::Completed {
                completed = operation.assessment;
                break;
            }
            if operation.status == AssessmentOperationStatus::Failed {
                panic!("assessment failed: {:?}", operation.error);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let assessment = completed.expect("assessment should complete");
        let action = PendingAction {
            id: "enable-compatible".into(),
            action_type: PendingActionType::EnableCompatibleProject,
            title: "Enable".into(),
            message: "Enable".into(),
            risk_level: RiskLevel::High,
            affected_paths: vec![
                ".app/compat/purpose.md".into(),
                ".app/compat/schema.md".into(),
            ],
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        };

        let confirmed = execute_claimed_project_authority_action(
            &state,
            StoredPendingAction {
                action,
                execution: Some(ConfirmationExecution::EnableCompatibleProject {
                    assessment_id: assessment.assessment_id,
                    project_id: "project-a".into(),
                    root_path: root.to_string_lossy().into_owned(),
                    template: ProjectTemplate::General,
                    initialize_git: false,
                }),
            },
        )
        .unwrap();

        assert_eq!(confirmed.status, ConfirmationStatus::Confirmed);
        assert!(!root.join(".git").exists());
        assert!(root.join(".app/compat/purpose.md").is_file());
        assert!(root.join(".app/compat/schema.md").is_file());
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
    }
}

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
    let pending = state.confirmation_registry.peek(&request.action_id)?;
    reject_workflow_owned_generic_confirmation(&pending)?;
    if request.status == ConfirmationStatus::Confirmed {
        if matches!(
            pending.execution.as_ref(),
            Some(
                ConfirmationExecution::RepairProject { .. }
                    | ConfirmationExecution::EnableCompatibleProject { .. }
                    | ConfirmationExecution::ConfigureCompatibleLayout { .. }
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
                state.workflow_service.dispatch_claimed_run_with_settings(
                    &state.task_service,
                    &state.settings_service,
                    &next,
                )?;
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
        Some(
            ConfirmationExecution::RepairProject { .. }
            | ConfirmationExecution::EnableCompatibleProject { .. }
            | ConfirmationExecution::ConfigureCompatibleLayout { .. }
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
        Some(ConfirmationExecution::AgentLintRepairStart { .. }) => Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            "Agent lint repair must be handled by confirm_agent_lint_repair_start.",
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
            ..
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
                        state.workflow_service.dispatch_claimed_run_with_settings(
                            &state.task_service,
                            &state.settings_service,
                            &next,
                        )?;
                    }
                }
                Err(failure) => {
                    if let Some(next) = failure.next {
                        state.workflow_service.dispatch_claimed_run_with_settings(
                            &state.task_service,
                            &state.settings_service,
                            &next,
                        )?;
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

fn reject_workflow_owned_generic_confirmation(
    pending: &StoredPendingAction,
) -> Result<(), BackendError> {
    let message = match pending.execution.as_ref() {
        Some(ConfirmationExecution::GenerateContentOverwrite { .. }) => Some(
            "Generate Content review must be handled by confirm_workflow_action or discard_workflow_result.",
        ),
        Some(ConfirmationExecution::UpdateWikiReview { .. }) => Some(
            "Update Wiki review must be handled by confirm_workflow_action or discard_workflow_result.",
        ),
        Some(ConfirmationExecution::AgentLintRepairStart { .. }) => Some(
            "Agent lint repair must be handled by confirm_agent_lint_repair_start or cancel_agent_lint_repair_preparation.",
        ),
        _ => None,
    };
    if let Some(message) = message {
        return Err(BackendError::new(
            "CONFIRMATION_COMMAND_INVALID",
            message,
            true,
            true,
        ));
    }
    Ok(())
}

fn execute_claimed_project_authority_action(
    state: &AppState,
    stored: StoredPendingAction,
) -> Result<ConfirmedAction, BackendError> {
    let StoredPendingAction { action, execution } = stored;
    match execution {
        Some(ConfirmationExecution::RepairProject {
            assessment_id,
            project_id,
            root_path,
            plan,
        }) => {
            let assessment = crate::commands::project_commands::revalidate_project_assessment(
                state,
                &assessment_id,
            )?;
            if !matches!(
                assessment.health,
                crate::models::project::ProjectHealth::Recovery
                    | crate::models::project::ProjectHealth::Repairable
            ) || assessment.canonical_identity_key != plan.canonical_identity_key
                || assessment.identity_revision != plan.identity_revision
            {
                return Err(BackendError::new(
                    "PROJECT_REPAIR_PLAN_STALE",
                    "The project recovery state changed after the repair preview. Prepare repair again.",
                    true,
                    true,
                ));
            }
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
                    "PROJECT_REPAIR_READ_ONLY",
                    "Recovery repair requires writable project access.",
                    true,
                    true,
                ));
            }
            let directory_only = plan.operations.iter().all(|operation| {
                operation.operation_type
                    == crate::models::project::ProjectRepairOperationType::CreateDirectory
            });
            let checkpoint_exists = if directory_only {
                state
                    .project_service
                    .apply_native_layout_repair_plan(&context, &plan)?;
                false
            } else {
                state.git_service.verify_checkpoint_state(
                    &context,
                    plan.expected_git_head.as_deref(),
                    &plan.expected_git_paths,
                )?;
                let checkpoint = state.git_service.create_checkpoint(
                    &context,
                    crate::models::git::CheckpointPurpose::HighRiskOperation,
                    "Checkpoint before project recovery repair",
                )?;
                state
                    .project_service
                    .apply_graph_cache_repair_plan(&context, &plan)?;
                checkpoint.commit_hash.is_some()
            };
            state
                .project_assessment_service
                .invalidate(&assessment_id)?;
            let repaired = state
                .project_assessment_service
                .inspect_current(context.root.to_string_lossy().as_ref())?;
            if directory_only {
                if repaired.format != crate::models::project::ProjectFormat::NativeCurrent
                    || repaired.health != crate::models::project::ProjectHealth::Healthy
                {
                    return Err(BackendError::new(
                        "PROJECT_NATIVE_REPAIR_STALE",
                        "The repair did not produce a healthy current native layout.",
                        true,
                        true,
                    ));
                }
                state.refresh_native_authority_after_repair(&project_id, &context.root)?;
            }
            Ok(ConfirmedAction {
                action,
                status: ConfirmationStatus::Confirmed,
                checkpoint_exists,
                project_summary: None,
            })
        }
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
        Some(ConfirmationExecution::ConfigureCompatibleLayout {
            assessment_id,
            project_id,
            root_path,
            mapping,
            expected_hash,
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
            state.require_project_write_access(&context)?;
            let checkpoint_exists = if expected_hash.is_some() {
                let status = state.git_service.repository_status(&context)?;
                if !status.is_repository {
                    return Err(BackendError::new(
                        "PROJECT_COMPAT_LAYOUT_GIT_REQUIRED",
                        "Changing an existing compatible layout mapping requires a local Git checkpoint.",
                        true,
                        true,
                    ));
                }
                if !state.git_service.is_path_tracked(
                    &context,
                    crate::models::layout::COMPATIBLE_LAYOUT_MAPPING_PATH,
                )? {
                    return Err(BackendError::new(
                        "PROJECT_COMPAT_LAYOUT_GIT_TRACKING_REQUIRED",
                        "Changing a compatible layout mapping requires its existing app-owned mapping file to be tracked by Git.",
                        true,
                        true,
                    ));
                }
                if status.has_changes {
                    return Err(BackendError::new(
                        "PROJECT_COMPAT_LAYOUT_DIRTY_WORKTREE",
                        "Changing a compatible layout mapping requires a clean Git worktree so the checkpoint can remain scoped to the mapping.",
                        true,
                        true,
                    ));
                }
                state
                    .git_service
                    .create_scoped_checkpoint(
                        &context,
                        crate::models::git::CheckpointPurpose::HighRiskOperation,
                        "Checkpoint before compatible layout mapping update",
                        &[crate::models::layout::COMPATIBLE_LAYOUT_MAPPING_PATH.to_string()],
                    )?
                    .commit_hash
                    .is_some()
            } else {
                false
            };
            state.project_service.write_compatible_layout_mapping(
                &context,
                &mapping,
                expected_hash.as_deref(),
            )?;
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
            let status = state.git_service.initialize_repository_from_snapshot(
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
    use crate::models::project::{AssessmentOperationStatus, ProjectHealth, ProjectTemplate};
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
                ".app/compat/tasks".into(),
                ".app/compat/workflows".into(),
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
        assert!(root.join(".app/compat/tasks").is_dir());
        assert!(root.join(".app/compat/workflows").is_dir());
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn confirmed_recovery_repair_checkpoints_then_preserves_and_regenerates_graph_cache() {
        let parent = temp_root("recovery-repair");
        let root = parent.join("recovery-project");
        let config = temp_root("recovery-repair-config");
        let project_service = ProjectService::with_config_dir(config.clone());
        let summary = project_service
            .create_project(
                root.to_string_lossy().as_ref(),
                "Recovery project",
                ProjectTemplate::General,
            )
            .unwrap();
        let invalid = b"{ invalid graph cache";
        fs::write(root.join(".app/graph-cache.json"), invalid).unwrap();
        let state = AppState {
            project_service,
            project_assessment_service: ProjectAssessmentService::new(config.clone()),
            ..AppState::default()
        };
        state
            .project_registry
            .register_trusted_native(summary.project_id.clone(), &root)
            .unwrap();
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
        assert_eq!(assessment.health, ProjectHealth::Recovery);
        let context = state
            .resolve_project_context(&summary.project_id, root.to_string_lossy().as_ref())
            .unwrap();
        let git = state.git_service.repository_status(&context).unwrap();
        let plan = state
            .project_service
            .prepare_graph_cache_repair_plan(
                &context,
                assessment.canonical_identity_key.clone(),
                assessment.identity_revision.clone(),
                git.head,
                state.git_service.changed_paths(&context).unwrap(),
            )
            .unwrap();
        let backup_path = plan.operations[0]
            .backup_path
            .clone()
            .expect("graph-cache repair plans always declare a backup path");
        let action = PendingAction {
            id: plan.repair_plan_id.clone(),
            action_type: PendingActionType::RepairProject,
            title: "Repair".into(),
            message: "Repair".into(),
            risk_level: RiskLevel::High,
            affected_paths: vec![".app/graph-cache.json".into(), backup_path.clone()],
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        };

        let confirmed = execute_claimed_project_authority_action(
            &state,
            StoredPendingAction {
                action,
                execution: Some(ConfirmationExecution::RepairProject {
                    assessment_id: assessment.assessment_id,
                    project_id: summary.project_id,
                    root_path: root.to_string_lossy().into_owned(),
                    plan,
                }),
            },
        )
        .unwrap();

        assert_eq!(confirmed.status, ConfirmationStatus::Confirmed);
        assert!(confirmed.checkpoint_exists);
        assert_eq!(fs::read(root.join(backup_path)).unwrap(), invalid);
        let repaired: crate::models::graph::GraphData =
            serde_json::from_slice(&fs::read(root.join(".app/graph-cache.json")).unwrap()).unwrap();
        assert!(repaired.nodes.is_empty());
        assert!(repaired.edges.is_empty());
        let log = std::process::Command::new("git")
            .args(["log", "-1", "--format=%s"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&log.stdout)
            .contains("Checkpoint before project recovery repair"));

        fs::remove_dir_all(parent).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn generic_confirmation_rejects_workflow_owned_generate_content_actions() {
        let pending = StoredPendingAction {
            action: PendingAction {
                id: "generate-review".into(),
                action_type: PendingActionType::OverwriteFile,
                title: "Review".into(),
                message: "Review".into(),
                risk_level: RiskLevel::High,
                affected_paths: vec!["exports/report.html".into()],
                preview: None,
                expires_at: None,
                checkpoint_hash: Some("checkpoint".into()),
            },
            execution: Some(ConfirmationExecution::GenerateContentOverwrite {
                project_id: "project-a".into(),
                root_path: "D:/project-a".into(),
                canonical_identity_key: "identity-a".into(),
                identity_revision: "revision-a".into(),
                task_id: "task-a".into(),
                action_id: "generate-review".into(),
                candidate: crate::models::workflow::WorkflowCandidateReference::TaskOwned {
                    candidate_id: "task-a:candidate".into(),
                },
            }),
        };

        let error = reject_workflow_owned_generic_confirmation(&pending).unwrap_err();

        assert_eq!(error.code, "CONFIRMATION_COMMAND_INVALID");
        assert!(error.message.contains("confirm_workflow_action"));
    }

    #[test]
    fn generic_confirmation_rejects_agent_lint_start_before_consuming_it() {
        let pending = StoredPendingAction {
            action: PendingAction {
                id: "agent-lint-start".into(),
                action_type: PendingActionType::AgentAutoFix,
                title: "Repair".into(),
                message: "Repair selected findings".into(),
                risk_level: RiskLevel::High,
                affected_paths: vec!["wiki/page.md".into()],
                preview: None,
                expires_at: Some("2099-01-01T00:15:00Z".into()),
                checkpoint_hash: None,
            },
            execution: Some(ConfirmationExecution::AgentLintRepairStart {
                project_id: "project-a".into(),
                root_path: "D:/project-a".into(),
                canonical_identity_key: "identity-a".into(),
                identity_revision: "revision-a".into(),
                preparation_id: "preparation-a".into(),
                preparation_revision: "preparation-revision-a".into(),
                report_id: "report-a".into(),
                selection_revision: "selection-a".into(),
                selected_finding_ids: vec!["finding-a".into()],
                route: crate::models::workflow::WorkflowRoute::Agent {
                    agent: crate::models::agent::AgentKind::Codex,
                    model: None,
                    route_revision: "route-a".into(),
                },
                skill: crate::models::lint::WikiLintSkillRef::builtin(),
                authorized_path_hashes: [("wiki/page.md".into(), Some("a".repeat(64)))]
                    .into_iter()
                    .collect(),
                baseline_fingerprint: "baseline-a".into(),
                expected_git_head: "b".repeat(40),
            }),
        };

        for status in [ConfirmationStatus::Confirmed, ConfirmationStatus::Cancelled] {
            let registry = crate::models::confirmation::ConfirmationRegistry::default();
            registry
                .register_with_execution(pending.action.clone(), pending.execution.clone())
                .unwrap();
            let stored = registry.peek(&pending.action.id).unwrap();
            let error = reject_workflow_owned_generic_confirmation(&stored).unwrap_err();

            assert_eq!(error.code, "CONFIRMATION_COMMAND_INVALID");
            assert!(error.message.contains("confirm_agent_lint_repair_start"));
            assert_eq!(registry.peek(&pending.action.id).unwrap(), pending);
            assert_eq!(
                registry.confirm(&pending.action.id, status).unwrap().action,
                pending.action.clone()
            );
        }
    }
}

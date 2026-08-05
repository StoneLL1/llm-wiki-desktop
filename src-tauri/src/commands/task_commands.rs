use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::task::{BackendTask, TaskActivity, TaskStatus, TaskType};
use crate::models::workflow::{
    WorkflowFilesystemAccess, WorkflowPersistenceMode, WorkflowProjectTrust, WorkflowRunPage,
};
use crate::tasks::task_model::LogLine;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub task_type: TaskType,
    pub project_id: Option<String>,
    pub title: String,
    pub cancellable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskByIdRequest {
    pub task_id: String,
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub status_filter: Option<TaskStatus>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetActiveProjectRequest {
    pub project_id: Option<String>,
    pub root_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskProjectPersistenceReason {
    NoProject,
    ProjectUntrusted,
    ProjectReadOnly,
    TaskStateRootUnavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetActiveProjectResult {
    pub tasks: Vec<BackendTask>,
    pub persistence: WorkflowPersistenceMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistence_reason: Option<TaskProjectPersistenceReason>,
}

#[tauri::command]
pub fn create_task(
    state: State<'_, AppState>,
    request: CreateTaskRequest,
) -> Result<BackendTask, BackendError> {
    let task = state.task_service.create_task(
        request.task_type,
        request.project_id,
        request.title,
        request.cancellable,
    );
    Ok(task)
}

#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    request: ListTasksRequest,
) -> Result<Vec<BackendTask>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(state
        .task_service
        .list_tasks_for_root(&context.root, request.status_filter))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn get_task(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Option<BackendTask>, BackendError> {
    require_task_project(&state, &request)?;
    Ok(state.task_service.get_task(&request.task_id))
}

#[tauri::command]
pub fn cancel_task(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<BackendTask, BackendError> {
    require_task_project(&state, &request)?;
    if let Some(run) = state.task_service.get_workflow_run(&request.task_id) {
        let was_waiting = run.display_status
            == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation;
        state
            .workflow_service
            .coordinator
            .cancel(&state.task_service, &request.task_id)
            .map_err(|msg| BackendError::new("TASK_CANCEL_FAILED", &msg, true, false))?;
        if was_waiting {
            if let Some(action) = run.pending_action {
                let _ = state.confirmation_registry.confirm(
                    &action.id,
                    crate::models::confirmation::ConfirmationStatus::Cancelled,
                );
            }
            if let Err(error) = crate::services::discard_update_wiki_candidate(&request.task_id) {
                let _ = state.task_service.append_log(
                    &request.task_id,
                    crate::tasks::task_model::LogLevel::Warn,
                    format!(
                        "Workflow was cancelled, but candidate cleanup needs attention: {}",
                        error.message
                    ),
                );
            }
            if let Err(error) =
                crate::services::discard_generate_content_candidate(&request.task_id)
            {
                let _ = state.task_service.append_log(
                    &request.task_id,
                    crate::tasks::task_model::LogLevel::Warn,
                    format!(
                        "Workflow was cancelled, but generated artifact cleanup needs attention: {}",
                        error.message
                    ),
                );
            }
            let (_, next) = state
                .workflow_service
                .coordinator
                .finish_cancelled_and_claim_next(&state.task_service, &request.task_id)
                .map_err(|msg| BackendError::new("TASK_CANCEL_FAILED", &msg, true, false))?;
            if let Some(next) = next {
                state.workflow_service.dispatch_claimed_run(&next)?;
            }
        }
        return state
            .task_service
            .get_task(&request.task_id)
            .ok_or_else(|| BackendError::new("TASK_NOT_FOUND", "Task not found.", false, false));
    }
    let result = if state
        .task_service
        .get_task(&request.task_id)
        .is_some_and(|task| {
            matches!(
                task.task_type,
                TaskType::LlmRequest | TaskType::SourceAiOrganize
            )
        }) {
        state.task_service.request_cancel(&request.task_id)
    } else {
        state.task_service.cancel_task(&request.task_id)
    };
    result.map_err(|msg| BackendError::new("TASK_CANCEL_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn get_task_logs(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Vec<LogLine>, BackendError> {
    require_task_project(&state, &request)?;
    state
        .task_service
        .get_logs(&request.task_id)
        .map_err(|msg| BackendError::new("TASK_LOGS_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn get_task_activities(
    state: State<'_, AppState>,
    request: TaskByIdRequest,
) -> Result<Vec<TaskActivity>, BackendError> {
    require_task_project(&state, &request)?;
    state
        .task_service
        .get_activities(&request.task_id)
        .map_err(|msg| BackendError::new("TASK_ACTIVITIES_FAILED", &msg, true, false))
}

#[tauri::command]
pub fn remove_completed_tasks(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<usize, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(state.task_service.remove_completed_for_root(&context.root))
}

fn require_task_project(state: &AppState, request: &TaskByIdRequest) -> Result<(), BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if !state
        .task_service
        .task_belongs_to_root(&request.task_id, &context.root)
    {
        return Err(BackendError::new(
            "TASK_PROJECT_MISMATCH",
            "Task does not belong to the asserted project.",
            true,
            true,
        ));
    }
    Ok(())
}

/// Bind (or clear) the active project authority for task persistence. Persistent
/// projects recover from the backend-derived layout root; memory-only projects
/// neither bind nor create project app state.
#[tauri::command]
pub fn set_active_project(
    state: State<'_, AppState>,
    request: SetActiveProjectRequest,
) -> Result<SetActiveProjectResult, BackendError> {
    set_active_project_for_state(&state, request)
}

fn set_active_project_for_state(
    state: &AppState,
    request: SetActiveProjectRequest,
) -> Result<SetActiveProjectResult, BackendError> {
    let project_context = match (request.project_id.as_deref(), request.root_path.as_deref()) {
        (Some(project_id), Some(root_path)) => {
            Some(state.resolve_project_context(project_id, root_path)?)
        }
        (None, None) => None,
        _ => {
            return Err(BackendError::new(
                "PROJECT_CONTEXT_MISMATCH",
                "Project id and root must be supplied together.",
                true,
                true,
            ))
        }
    };
    match project_context {
        Some(context) => state.with_workflow_access(&context, |access| {
            activate_project_with_access(state, &context, access)
        }),
        None => {
            state
                .task_service
                .set_project_root(None)
                .map_err(|msg| BackendError::new("TASK_RECOVERY_FAILED", &msg, true, false))?;
            Ok(SetActiveProjectResult {
                tasks: Vec::new(),
                persistence: WorkflowPersistenceMode::MemoryOnly,
                persistence_reason: Some(TaskProjectPersistenceReason::NoProject),
            })
        }
    }
}

fn activate_project_with_access(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    access: crate::services::WorkflowAccessSnapshot,
) -> Result<SetActiveProjectResult, BackendError> {
    let binding =
        crate::services::resolve_workflow_persistence_binding(context, access.persistence.clone())?;
    let persistence_reason = persistence_reason(&access, &binding);
    let Some(task_state_root) = binding.task_state_root else {
        state
            .task_service
            .set_project_root(None)
            .map_err(|msg| BackendError::new("TASK_RECOVERY_FAILED", &msg, true, false))?;
        state
            .task_service
            .rebind_workflows_for_root(&context.root, None)
            .map_err(|msg| {
                BackendError::new("WORKFLOW_PERSISTENCE_REBIND_FAILED", &msg, true, true)
            })?;
        return Ok(SetActiveProjectResult {
            // Read-only projects still need to expose in-memory operations
            // such as the cancellable post-open inventory. Do not persist or
            // create `.app` state merely to make those task cards visible.
            tasks: state.task_service.list_tasks_for_root(&context.root, None),
            persistence: WorkflowPersistenceMode::MemoryOnly,
            persistence_reason,
        });
    };
    let identity = crate::services::project_identity(&context.root)
        .map_err(|message| BackendError::new("WORKFLOW_IDENTITY_FAILED", &message, true, false))?;
    state
        .task_service
        .set_project_context(
            context.project_id.clone(),
            context.root.clone(),
            task_state_root.clone(),
        )
        .map_err(|msg| BackendError::new("TASK_RECOVERY_FAILED", &msg, true, false))?;
    state
        .task_service
        .rebind_workflows_for_root(&context.root, Some(task_state_root))
        .map_err(|msg| BackendError::new("WORKFLOW_PERSISTENCE_REBIND_FAILED", &msg, true, true))?;
    let tasks = state.task_service.list_tasks_for_root(&context.root, None);
    for task in &tasks {
        let Some(run) = state.task_service.get_workflow_run(&task.id) else {
            continue;
        };
        if run.canonical_identity_key != identity.canonical_identity_key
            || run.identity_revision != identity.identity_revision
        {
            continue;
        }
        if run.kind == crate::models::workflow::WorkflowKind::GenerateContent
            && run.display_status
                == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
        {
            crate::services::restore_generate_content_confirmation(
                context,
                &run,
                &state.task_service,
                &state.confirmation_registry,
            )?;
        }
        if run.kind == crate::models::workflow::WorkflowKind::UpdateWiki
            && run.display_status
                == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
        {
            crate::services::restore_update_wiki_confirmation(
                context,
                &run,
                &state.task_service,
                &state.confirmation_registry,
            )?;
        }
    }
    Ok(SetActiveProjectResult {
        tasks,
        persistence: WorkflowPersistenceMode::Persistent,
        persistence_reason: None,
    })
}

fn persistence_reason(
    access: &crate::services::WorkflowAccessSnapshot,
    binding: &crate::services::WorkflowPersistenceBinding,
) -> Option<TaskProjectPersistenceReason> {
    if binding.mode == WorkflowPersistenceMode::Persistent {
        return None;
    }
    if access.trust == WorkflowProjectTrust::Untrusted {
        return Some(TaskProjectPersistenceReason::ProjectUntrusted);
    }
    if access.filesystem_access == WorkflowFilesystemAccess::ReadOnly {
        return Some(TaskProjectPersistenceReason::ProjectReadOnly);
    }
    Some(TaskProjectPersistenceReason::TaskStateRootUnavailable)
}

#[tauri::command]
pub fn continue_queued_workflows(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<WorkflowRunPage, BackendError> {
    continue_queued_workflows_for_state(&state, request)
}

fn continue_queued_workflows_for_state(
    state: &AppState,
    request: WorkflowProjectRequest,
) -> Result<WorkflowRunPage, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let (runs, claimed) = state.with_workflow_access(&context, |access| {
        let identity = crate::services::project_identity(&context.root).map_err(|message| {
            BackendError::new("WORKFLOW_IDENTITY_FAILED", &message, true, false)
        })?;
        let mut queued = state
            .task_service
            .list_workflow_runs()
            .into_iter()
            .filter(|run| {
                run.canonical_identity_key == identity.canonical_identity_key
                    && run.identity_revision == identity.identity_revision
                    && run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
            })
            .collect::<Vec<_>>();
        queued.sort_by_key(|run| {
            (
                run.queue_position.unwrap_or(u32::MAX),
                run.started_at.clone(),
            )
        });
        let mut persistence_bindings = Vec::with_capacity(queued.len());
        let mut eligibility_error = None;
        for queued_run in &queued {
            let replay = super::workflow_commands::revalidate_workflow_replay_with_access(
                state,
                &context,
                queued_run,
                access.clone(),
            )?;
            if let Err(error) = replay.eligibility {
                if eligibility_error.is_none() {
                    eligibility_error = Some(error);
                }
            }
            persistence_bindings.push((
                queued_run.task_id.clone(),
                replay.persistence.task_state_root,
            ));
        }
        let (runs, claimed) = state
            .workflow_service
            .coordinator
            .apply_persistence_and_continue_queued(
                &state.task_service,
                &identity.canonical_identity_key,
                &identity.identity_revision,
                &persistence_bindings,
                eligibility_error.is_none(),
            )
            .map_err(|message| {
                BackendError::new("WORKFLOW_CONTINUE_FAILED", &message, true, false)
            })?;
        if let Some(error) = eligibility_error {
            return Err(error);
        }
        Ok((runs, claimed))
    })?;
    if let Some(run) = claimed {
        state.workflow_service.dispatch_claimed_run(&run)?;
    }
    Ok(WorkflowRunPage {
        runs,
        next_cursor: None,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;
    use crate::models::project::ProjectTrustKind;
    use crate::models::workflow::{
        HealthCheckMode, WorkflowExecutionOptions, WorkflowFilesystemAccess, WorkflowGitState,
        WorkflowKind, WorkflowPersistenceTransition, WorkflowProjectTrust, WorkflowRoute,
        WorkflowScope, WorkflowStartOutcome,
    };
    use crate::services::{
        workflow_baseline_for_scope, workflow_stages, EnqueueWorkflow, ProjectService,
        WorkflowAccessSnapshot, WorkflowPersistenceBinding,
    };
    use crate::tasks::TaskService;

    fn native_project() -> TempDir {
        let project = tempfile::tempdir().expect("project tempdir");
        fs::write(project.path().join("purpose.md"), "# Purpose\n").unwrap();
        fs::write(project.path().join("schema.md"), "# Schema\n").unwrap();
        for relative in ["raw/sources", "wiki", ".app/tasks", "exports", "skills"] {
            fs::create_dir_all(project.path().join(relative)).unwrap();
        }
        project
    }

    fn enqueue_health_check(
        state: &AppState,
        root: &Path,
        preparation_revision: &str,
    ) -> crate::models::workflow::WorkflowRun {
        enqueue_health_check_with_persistence(
            state,
            root,
            preparation_revision,
            Some(root.join(".app/tasks")),
        )
    }

    fn enqueue_health_check_with_persistence(
        state: &AppState,
        root: &Path,
        preparation_revision: &str,
        task_state_root: Option<std::path::PathBuf>,
    ) -> crate::models::workflow::WorkflowRun {
        let scope = WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick,
        };
        let baseline = workflow_baseline_for_scope(
            &state
                .resolve_project_context("project-a", root.to_string_lossy().as_ref())
                .unwrap(),
            &scope,
        )
        .unwrap();
        let outcome = state
            .workflow_service
            .coordinator
            .enqueue(
                &state.task_service,
                EnqueueWorkflow {
                    project_id: "project-a".into(),
                    project_root: root.to_path_buf(),
                    task_state_root,
                    title: "Health Check".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope,
                    route: Some(WorkflowRoute::Local {
                        route_revision: "local-v1".into(),
                    }),
                    baseline_fingerprint: baseline.fingerprint,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: preparation_revision.into(),
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: workflow_stages(&WorkflowKind::HealthCheck),
                    retry: None,
                },
            )
            .unwrap();
        match outcome {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("workflow should be unique"),
        }
    }

    #[test]
    fn set_active_project_keeps_untrusted_compatible_folder_memory_only_without_app_writes() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("notes")).unwrap();
        fs::write(project.path().join("notes/index.md"), "# Notes\n").unwrap();
        let state = AppState::default();
        state
            .project_registry
            .register("project-a", project.path())
            .unwrap();

        let result = set_active_project_for_state(
            &state,
            SetActiveProjectRequest {
                project_id: Some("project-a".into()),
                root_path: Some(project.path().to_string_lossy().into_owned()),
            },
        )
        .unwrap();

        assert_eq!(result.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            result.persistence_reason,
            Some(TaskProjectPersistenceReason::ProjectUntrusted)
        );
        assert!(result.tasks.is_empty());
        assert!(!project.path().join(".app").exists());
    }

    #[test]
    fn trusted_read_only_activation_returns_memory_only_without_app_writes() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("notes")).unwrap();
        fs::write(project.path().join("notes/index.md"), "# Notes\n").unwrap();
        let state = AppState::default();
        let context = state
            .project_registry
            .register("project-a", project.path())
            .unwrap()
            .with_resolved_layout()
            .unwrap();

        let result = activate_project_with_access(
            &state,
            &context,
            WorkflowAccessSnapshot {
                trust: WorkflowProjectTrust::Trusted,
                trust_kind: Some(ProjectTrustKind::Compatible),
                filesystem_access: WorkflowFilesystemAccess::ReadOnly,
                persistence: WorkflowPersistenceMode::MemoryOnly,
                git_state: WorkflowGitState::Unavailable,
                authority_revision: "read-only".into(),
            },
        )
        .unwrap();

        assert_eq!(result.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            result.persistence_reason,
            Some(TaskProjectPersistenceReason::ProjectReadOnly)
        );
        assert!(result.tasks.is_empty());
        assert!(!project.path().join(".app").exists());
    }

    #[test]
    fn trusted_compatible_without_task_state_root_stays_memory_only() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join(".obsidian")).unwrap();
        fs::write(project.path().join("index.md"), "# Vault\n").unwrap();
        let config = tempfile::tempdir().unwrap();
        let state = AppState {
            project_service: ProjectService::with_config_dir(config.path().to_path_buf()),
            ..AppState::default()
        };
        state
            .project_registry
            .register("project-a", project.path())
            .unwrap();
        state
            .grant_compatible_project_trust("project-a", project.path())
            .unwrap();

        let result = set_active_project_for_state(
            &state,
            SetActiveProjectRequest {
                project_id: Some("project-a".into()),
                root_path: Some(project.path().to_string_lossy().into_owned()),
            },
        )
        .unwrap();

        assert_eq!(result.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            result.persistence_reason,
            Some(TaskProjectPersistenceReason::TaskStateRootUnavailable)
        );
        assert!(result.tasks.is_empty());
        assert!(!project.path().join(".app").exists());
    }

    #[test]
    fn set_active_project_recovers_native_tasks_from_derived_task_root() {
        let project = native_project();
        let seed = TaskService::default();
        let expected = seed
            .create_project_task(
                TaskType::Export,
                "project-a".into(),
                project.path().to_path_buf(),
                "Existing task".into(),
                true,
            )
            .unwrap();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("project-a", project.path())
            .unwrap();

        let result = set_active_project_for_state(
            &state,
            SetActiveProjectRequest {
                project_id: Some("project-a".into()),
                root_path: Some(project.path().to_string_lossy().into_owned()),
            },
        )
        .unwrap();

        assert_eq!(result.persistence, WorkflowPersistenceMode::Persistent);
        assert_eq!(result.persistence_reason, None);
        assert!(result.tasks.iter().any(|task| task.id == expected.id));
    }

    #[test]
    fn continue_rebinds_queued_workflow_before_returning_reprepare_error() {
        let project = native_project();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("project-a", project.path())
            .unwrap();
        let _active = enqueue_health_check(&state, project.path(), "prep-a");
        let queued = enqueue_health_check(&state, project.path(), "prep-b");
        state
            .task_service
            .set_workflow_queue_state(&queued.task_id, queued.queue_position, true)
            .unwrap();
        let snapshot_path = project
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", queued.task_id));
        let before = fs::read(&snapshot_path).unwrap();

        state
            .project_registry
            .revoke_trust("project-a", project.path())
            .unwrap();
        fs::write(project.path().join("wiki/changed.md"), "# Changed\n").unwrap();

        let error = continue_queued_workflows_for_state(
            &state,
            WorkflowProjectRequest {
                project_id: "project-a".into(),
                project_root_path: project.path().to_string_lossy().into_owned(),
            },
        )
        .expect_err("baseline drift must require preparation");
        assert_eq!(error.code, "WORKFLOW_REPREPARATION_REQUIRED");
        let rebound = state
            .task_service
            .get_workflow_run(&queued.task_id)
            .unwrap();
        assert_eq!(rebound.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            rebound.persistence_transition,
            Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
        );

        state
            .workflow_service
            .coordinator
            .cancel(&state.task_service, &queued.task_id)
            .unwrap();
        assert_eq!(fs::read(&snapshot_path).unwrap(), before);
    }

    #[test]
    fn retry_rebinds_original_before_returning_reprepare_error() {
        let project = native_project();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("project-a", project.path())
            .unwrap();
        let original = enqueue_health_check(&state, project.path(), "prep-a");
        state
            .task_service
            .transition_workflow_status(&original.task_id, TaskStatus::Failed)
            .unwrap();
        let snapshot_path = project
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", original.task_id));
        let before = fs::read(&snapshot_path).unwrap();

        state
            .project_registry
            .revoke_trust("project-a", project.path())
            .unwrap();
        fs::write(project.path().join("wiki/changed.md"), "# Changed\n").unwrap();

        let error = crate::commands::workflow_commands::retry_workflow_for_state(
            &state,
            crate::commands::workflow_commands::WorkflowRunRequest {
                project_id: "project-a".into(),
                project_root_path: project.path().to_string_lossy().into_owned(),
                task_id: original.task_id.clone(),
            },
        )
        .expect_err("baseline drift must require preparation");
        assert_eq!(error.code, "WORKFLOW_REPREPARATION_REQUIRED");
        let rebound = state
            .task_service
            .get_workflow_run(&original.task_id)
            .unwrap();
        assert_eq!(rebound.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            rebound.persistence_transition,
            Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
        );
        state
            .task_service
            .append_log(
                &original.task_id,
                crate::tasks::task_model::LogLevel::Info,
                "after downgrade".into(),
            )
            .unwrap();
        assert_eq!(fs::read(&snapshot_path).unwrap(), before);
    }

    #[test]
    fn retry_after_trust_revocation_creates_memory_only_attempt_without_old_writes() {
        let project = native_project();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("project-a", project.path())
            .unwrap();
        let original = enqueue_health_check(&state, project.path(), "prep-a");
        state
            .task_service
            .transition_workflow_status(&original.task_id, TaskStatus::Failed)
            .unwrap();
        let snapshot_path = project
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", original.task_id));

        state
            .revoke_project_trust("project-a", project.path())
            .unwrap();
        let before_retry = fs::read(&snapshot_path).unwrap();
        let outcome = crate::commands::workflow_commands::retry_workflow_for_state(
            &state,
            crate::commands::workflow_commands::WorkflowRunRequest {
                project_id: "project-a".into(),
                project_root_path: project.path().to_string_lossy().into_owned(),
                task_id: original.task_id.clone(),
            },
        )
        .unwrap();
        let retry = match outcome {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("retry must create a new attempt"),
        };
        assert_eq!(retry.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            retry.persistence_transition,
            Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
        );
        state
            .task_service
            .append_log(
                &original.task_id,
                crate::tasks::task_model::LogLevel::Info,
                "after memory-only retry".into(),
            )
            .unwrap();
        assert_eq!(fs::read(&snapshot_path).unwrap(), before_retry);
    }

    #[test]
    fn retry_after_native_trust_grant_creates_attempt_in_derived_root() {
        let project = native_project();
        let state = AppState::default();
        state
            .project_registry
            .register("project-a", project.path())
            .unwrap();
        let original =
            enqueue_health_check_with_persistence(&state, project.path(), "prep-a", None);
        state
            .task_service
            .transition_workflow_status(&original.task_id, TaskStatus::Failed)
            .unwrap();

        state
            .project_registry
            .register_trusted_native("project-a", project.path())
            .unwrap();
        let outcome = crate::commands::workflow_commands::retry_workflow_for_state(
            &state,
            crate::commands::workflow_commands::WorkflowRunRequest {
                project_id: "project-a".into(),
                project_root_path: project.path().to_string_lossy().into_owned(),
                task_id: original.task_id,
            },
        )
        .unwrap();
        let retry = match outcome {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("retry must create a new attempt"),
        };
        assert_eq!(retry.persistence, WorkflowPersistenceMode::Persistent);
        assert_eq!(
            retry.persistence_transition,
            Some(WorkflowPersistenceTransition::UpgradedToPersistent)
        );
        assert!(project
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", retry.task_id))
            .exists());
    }

    #[test]
    fn continue_after_trust_revocation_claims_with_memory_only_binding() {
        let project = native_project();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("project-a", project.path())
            .unwrap();
        let active = enqueue_health_check(&state, project.path(), "prep-a");
        let queued = enqueue_health_check(&state, project.path(), "prep-b");
        state
            .task_service
            .set_workflow_queue_state(&queued.task_id, queued.queue_position, true)
            .unwrap();
        state
            .task_service
            .transition_workflow_status(&active.task_id, TaskStatus::Failed)
            .unwrap();
        state
            .revoke_project_trust("project-a", project.path())
            .unwrap();

        let page = continue_queued_workflows_for_state(
            &state,
            WorkflowProjectRequest {
                project_id: "project-a".into(),
                project_root_path: project.path().to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let continued = page
            .runs
            .iter()
            .find(|run| run.task_id == queued.task_id)
            .unwrap();
        assert_eq!(continued.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert!(!continued.continuation_required);
        assert_eq!(
            continued.persistence_transition,
            Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
        );
    }

    #[test]
    fn persistence_reason_distinguishes_read_only_from_missing_task_root() {
        let access = WorkflowAccessSnapshot {
            trust: WorkflowProjectTrust::Trusted,
            trust_kind: Some(ProjectTrustKind::Native),
            filesystem_access: WorkflowFilesystemAccess::ReadOnly,
            persistence: WorkflowPersistenceMode::MemoryOnly,
            git_state: WorkflowGitState::Unavailable,
            authority_revision: "authority".into(),
        };
        let binding = WorkflowPersistenceBinding {
            mode: WorkflowPersistenceMode::MemoryOnly,
            task_state_root: None,
        };

        assert_eq!(
            persistence_reason(&access, &binding),
            Some(TaskProjectPersistenceReason::ProjectReadOnly)
        );
    }
}

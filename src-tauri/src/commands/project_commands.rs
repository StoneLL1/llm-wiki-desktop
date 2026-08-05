use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::project::{
    AppSummary, AssessmentId, AssessmentOperationId, CreateProjectRequest, OpenProjectRequest,
    OpenProjectResponse, OpenedProject, ProjectAssessmentOperation, ProjectCapability,
    ProjectFormat, ProjectHealth, ProjectInventoryState, ProjectOpenAssessment, ProjectOpenIntent,
    ProjectSessionAuthority, ProjectSummary, ProjectTemplate, ProjectTrustState, RecentProject,
    RelocateRecentProjectRequest, RememberRecentProjectRequest, RemoveRecentProjectRequest,
    StartProjectOpenAssessmentResult,
};
use crate::models::task::{TaskResult, TaskStatus, TaskType};
use crate::tasks::task_model::LogLevel;
use crate::utils::time_utils::now_rfc3339;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn get_app_summary(_state: State<'_, AppState>) -> AppSummary {
    AppSummary {
        name: "LLM Wiki Desktop".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn prepare_default_project_parent(state: State<'_, AppState>) -> Result<String, BackendError> {
    state.project_service.prepare_default_project_parent()
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    request: CreateProjectRequest,
) -> Result<OpenedProject, BackendError> {
    let summary = state.project_service.create_project(
        &request.root_path,
        &request.name,
        request.template,
    )?;
    let context = state
        .project_registry
        .register_trusted_native(
            summary.project_id.clone(),
            &PathBuf::from(&summary.root_path),
        )
        .map_err(|error| created_project_follow_up_failure(error, &summary, "register"))?;
    let assessment = state
        .project_assessment_service
        .inspect_current(&summary.root_path)
        .map_err(|error| created_project_follow_up_failure(error, &summary, "assess"))?;
    let authority = project_session_authority(&state, &context, &assessment)
        .map_err(|error| created_project_follow_up_failure(error, &summary, "authorize"))?;

    state
        .project_service
        .remember_recent_project(RecentProject {
            project_id: summary.project_id.clone(),
            name: summary.name.clone(),
            root_path: summary.root_path.clone(),
            template: summary.template,
            opened_at: now_rfc3339(),
            wiki_page_count: summary.wiki_page_count,
            source_count: summary.source_count,
            task_count: summary.task_count,
            index_state: summary.index_state.clone(),
            graph_state: summary.graph_state.clone(),
            missing: false,
        })
        .map_err(|error| created_project_follow_up_failure(error, &summary, "remember_recent"))?;
    Ok(OpenedProject { summary, authority })
}

#[tauri::command]
pub fn open_project(
    state: State<'_, AppState>,
    request: OpenProjectRequest,
) -> Result<OpenProjectResponse, BackendError> {
    let mut outcome = state.project_service.open_project(&request.path)?;
    if let Some(summary) = outcome.summary.as_ref() {
        let context = register_opened_project(&state, summary)?;
        let assessment = state
            .project_assessment_service
            .inspect_current(&summary.root_path)?;
        outcome.authority = Some(project_session_authority(&state, &context, &assessment)?);
        state
            .project_service
            .remember_recent_project(RecentProject {
                project_id: summary.project_id.clone(),
                name: summary.name.clone(),
                root_path: summary.root_path.clone(),
                template: summary.template,
                opened_at: now_rfc3339(),
                wiki_page_count: summary.wiki_page_count,
                source_count: summary.source_count,
                task_count: summary.task_count,
                index_state: summary.index_state.clone(),
                graph_state: summary.graph_state.clone(),
                missing: false,
            })?;
    }
    Ok(outcome)
}

#[tauri::command]
pub fn start_project_open_assessment(
    state: State<'_, AppState>,
    request: StartProjectOpenAssessmentRequest,
) -> Result<StartProjectOpenAssessmentResult, BackendError> {
    state.project_assessment_service.start(request.path)
}

#[tauri::command]
pub fn get_project_open_assessment(
    state: State<'_, AppState>,
    request: AssessmentOperationRequest,
) -> Result<ProjectAssessmentOperation, BackendError> {
    state
        .project_assessment_service
        .get_operation(&request.assessment_operation_id)
}

#[tauri::command]
pub fn cancel_project_open_assessment(
    state: State<'_, AppState>,
    request: AssessmentOperationRequest,
) -> Result<(), BackendError> {
    state
        .project_assessment_service
        .cancel(&request.assessment_operation_id)
}

#[tauri::command]
pub fn open_assessed_project(
    state: State<'_, AppState>,
    request: AssessedProjectRequest,
) -> Result<OpenedProject, BackendError> {
    let assessment = revalidate_project_assessment(&state, &request.assessment_id)?;
    if matches!(
        assessment.format,
        ProjectFormat::AmbiguousMarkdown
            | ProjectFormat::OrdinaryMaterials
            | ProjectFormat::Unknown
    ) || assessment.health == ProjectHealth::Unreadable
    {
        return Err(BackendError::new(
            "PROJECT_ASSESSMENT_OPEN_UNAVAILABLE",
            "The assessed folder requires another user decision before it can be opened as a knowledge base.",
            true,
            true,
        ));
    }
    open_revalidated_assessment(&state, assessment)
}

/// Opens a native knowledge base at a new path and replaces one missing recent
/// entry only after comparing the app-owned stable ID stored in that native
/// project's `.app/project.json`. Compatible and legacy folders do not have a
/// move-proof identity, so they must remain manual open/remove flows.
#[tauri::command]
pub fn relocate_recent_project(
    state: State<'_, AppState>,
    request: RelocateRecentProjectRequest,
) -> Result<OpenedProject, BackendError> {
    let assessment = revalidate_project_assessment(&state, &request.assessment_id)?;
    if assessment.format != ProjectFormat::NativeCurrent {
        return Err(BackendError::new(
            "PROJECT_RELOCATION_NATIVE_ONLY",
            "Only a native knowledge base with a durable app-owned identity can be relocated automatically.",
            true,
            true,
        ));
    }
    if assessment.health == ProjectHealth::Unreadable {
        return Err(BackendError::new(
            "PROJECT_RELOCATION_UNREADABLE",
            "The selected knowledge base cannot be read safely enough to verify relocation.",
            true,
            true,
        ));
    }
    let project_id = state
        .project_service
        .require_stable_native_project_id(Path::new(&assessment.canonical_root_path))?;
    if project_id != request.previous_project_id {
        return Err(BackendError::new(
            "PROJECT_RELOCATION_ID_MISMATCH",
            "The selected knowledge base is not the same project as the missing recent entry.",
            true,
            true,
        ));
    }
    open_relocated_revalidated_assessment(
        &state,
        assessment,
        project_id,
        &request.previous_root_path,
    )
}

/// Starts the post-open file inventory only after the workbench has accepted
/// the opening summary. Keeping this separate prevents a fast scan from
/// publishing its completion event before the UI has subscribed to the newly
/// active project.
#[tauri::command]
pub fn start_project_inventory(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScanProjectRequest,
) -> Result<crate::models::task::BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    queue_project_inventory(app, &state, context).map_err(|error| {
        BackendError::new(
            "PROJECT_INVENTORY_START_FAILED",
            "Project inventory could not be started.",
            true,
            false,
        )
        .with_details(serde_json::json!({ "error": error }))
    })
}

/// Persists an explicit, identity-bound route for a low-confidence Markdown
/// folder and opens it in restricted-compatible mode. No marker is written to
/// the selected folder.
#[tauri::command]
pub fn resolve_ambiguous_assessed_project(
    state: State<'_, AppState>,
    request: AssessedProjectIntentRequest,
) -> Result<OpenedProject, BackendError> {
    if request.intent != ProjectOpenIntent::OpenAsMarkdownVault {
        return Err(BackendError::new(
            "PROJECT_OPEN_INTENT_REQUIRES_CREATION",
            "Creating a new knowledge base from materials must use the new-project flow.",
            true,
            true,
        ));
    }
    let assessment = state
        .project_assessment_service
        .remember_ambiguous_intent(&request.assessment_id, request.intent)?;
    open_revalidated_assessment(&state, assessment)
}

/// Records the non-opening branch of an ambiguous folder decision. The caller
/// can then open the normal new-project dialog and seed Import after creation.
#[tauri::command]
pub fn remember_ambiguous_project_intent(
    state: State<'_, AppState>,
    request: AssessedProjectIntentRequest,
) -> Result<ProjectOpenAssessment, BackendError> {
    state
        .project_assessment_service
        .remember_ambiguous_intent(&request.assessment_id, request.intent)
}

/// Clears a previously remembered ambiguous-folder decision from the global
/// application settings. The selected folder is only re-assessed and is never
/// modified by this command.
#[tauri::command]
pub fn clear_ambiguous_project_intent(
    state: State<'_, AppState>,
    request: AssessedProjectRequest,
) -> Result<ProjectOpenAssessment, BackendError> {
    state
        .project_assessment_service
        .clear_ambiguous_intent(&request.assessment_id)
}

/// Recomputes the backend-owned access snapshot for an already registered
/// project. UI callers use this after an explicit trust, Git, or filesystem
/// change; all mutation commands still revalidate independently.
#[tauri::command]
pub fn get_project_session_authority(
    state: State<'_, AppState>,
    request: ScanProjectRequest,
) -> Result<ProjectSessionAuthority, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let assessment = state
        .project_assessment_service
        .inspect_current(&request.project_root_path)?;
    let assessed_root = PathBuf::from(&assessment.canonical_root_path)
        .canonicalize()
        .map_err(|_| project_assessment_context_mismatch())?;
    if context.root != assessed_root {
        return Err(project_assessment_context_mismatch());
    }
    project_session_authority(&state, &context, &assessment)
}

#[tauri::command]
pub fn trust_project(
    state: State<'_, AppState>,
    request: AssessedCurrentProjectRequest,
) -> Result<crate::models::confirmation::PendingAction, BackendError> {
    let assessment = revalidate_current_project_assessment(&state, &request)?;
    ensure_compatible_trust_candidate(&assessment)?;
    let action = crate::models::confirmation::PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: crate::models::confirmation::PendingActionType::TrustCompatibleProject,
        title: "Trust knowledge base".into(),
        message: "Allow external AI, Agent, and Skill execution for this folder identity.".into(),
        risk_level: crate::models::confirmation::RiskLevel::High,
        affected_paths: Vec::new(),
        preview: Some(crate::models::confirmation::ActionPreview {
            summary:
                "Trust is stored in global application settings; no project file will be written."
                    .into(),
            before: Some("Restricted local reading only".into()),
            after: Some("External execution may be enabled when a route is configured".into()),
            diff: None,
        }),
        expires_at: None,
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(
            crate::models::confirmation::ConfirmationExecution::TrustCompatibleProject {
                assessment_id: request.assessment_id,
                project_id: request.project_id,
                root_path: request.project_root_path,
            },
        ),
    )?;
    Ok(action)
}

#[tauri::command]
pub fn revoke_project_trust(
    state: State<'_, AppState>,
    request: AssessedCurrentProjectRequest,
) -> Result<ProjectOpenAssessment, BackendError> {
    let assessment = revalidate_current_project_assessment(&state, &request)?;
    if assessment.format == ProjectFormat::NativeCurrent {
        return Err(BackendError::new(
            "PROJECT_NATIVE_TRUST_REQUIRED",
            "Native knowledge bases keep backend-verified native authority while open.",
            true,
            true,
        ));
    }
    state.revoke_project_trust(&request.project_id, Path::new(&request.project_root_path))?;
    state
        .project_assessment_service
        .resolve_current(&assessment.assessment_id)
}

#[tauri::command]
pub fn enable_compatible_full_features(
    state: State<'_, AppState>,
    request: EnableCompatibleFullFeaturesRequest,
) -> Result<crate::models::confirmation::PendingAction, BackendError> {
    let current = AssessedCurrentProjectRequest {
        assessment_id: request.assessment_id.clone(),
        project_id: request.project_id.clone(),
        project_root_path: request.project_root_path.clone(),
    };
    let assessment = revalidate_current_project_assessment(&state, &current)?;
    ensure_compatible_trust_candidate(&assessment)?;
    let initialize_git = request.initialize_git && !assessment.git.is_repository;
    let mut affected_paths = vec![
        ".app/compat/purpose.md".to_string(),
        ".app/compat/schema.md".to_string(),
    ];
    if initialize_git {
        affected_paths.push(".git".to_string());
    }
    let action = crate::models::confirmation::PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: crate::models::confirmation::PendingActionType::EnableCompatibleProject,
        title: "Trust and enable full features".into(),
        message:
            "Create app-owned compatibility guidance and optionally initialize local Git history."
                .into(),
        risk_level: crate::models::confirmation::RiskLevel::High,
        affected_paths,
        preview: Some(crate::models::confirmation::ActionPreview {
            summary: "Existing Markdown and .obsidian content will remain in place.".into(),
            before: None,
            after: Some(
                "Only .app/compat guidance and the optional local .git history will be created."
                    .into(),
            ),
            diff: None,
        }),
        expires_at: None,
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(
            crate::models::confirmation::ConfirmationExecution::EnableCompatibleProject {
                assessment_id: request.assessment_id,
                project_id: request.project_id,
                root_path: request.project_root_path,
                template: request.template,
                initialize_git,
            },
        ),
    )?;
    Ok(action)
}

/// Prepares a bounded Recovery repair as a pending confirmation. Preparation
/// is read-only: it snapshots identity, Git state, and the exact corrupt cache
/// hash; all writes (checkpoint, backup, replacement) occur only after the
/// user confirms through the claimed confirmation path.
#[tauri::command]
pub fn prepare_assessed_project_repair(
    state: State<'_, AppState>,
    request: AssessedCurrentProjectRequest,
) -> Result<crate::models::confirmation::PendingAction, BackendError> {
    let assessment = revalidate_current_project_assessment(&state, &request)?;
    if !matches!(assessment.health, ProjectHealth::Recovery | ProjectHealth::Repairable) {
        return Err(BackendError::new(
            "PROJECT_REPAIR_UNAVAILABLE",
            "This knowledge base has no safe repairable state.",
            true,
            true,
        ));
    }
    if assessment.filesystem_access != crate::models::project::ProjectFilesystemAccess::Writable {
        return Err(BackendError::new(
            "PROJECT_REPAIR_READ_ONLY",
            "Recovery repair requires a writable knowledge base.",
            true,
            true,
        ));
    }
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let plan = if assessment.health == ProjectHealth::Recovery {
        let git = state.git_service.repository_status(&context)?;
        if !git.is_repository || git.head.is_none() {
            return Err(BackendError::new(
                "PROJECT_REPAIR_CHECKPOINT_REQUIRED",
                "Recovery repair requires a local Git repository with an initial checkpoint.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "isRepository": git.is_repository,
                "head": git.head,
            })));
        }
        state.project_service.prepare_graph_cache_repair_plan(
            &context,
            assessment.canonical_identity_key.clone(),
            assessment.identity_revision.clone(),
            git.head.clone(),
            state.git_service.changed_paths(&context)?,
        )?
    } else {
        state.project_service.prepare_native_layout_repair_plan(
            &context,
            assessment.canonical_identity_key.clone(),
            assessment.identity_revision.clone(),
        )?
    };
    if plan.operations.is_empty() {
        return Err(BackendError::new(
            "PROJECT_REPAIR_PLAN_INVALID",
            "The repair plan did not contain a safe operation.",
            true,
            true,
        ));
    }
    let directory_only = plan.operations.iter().all(|operation| {
        operation.operation_type == crate::models::project::ProjectRepairOperationType::CreateDirectory
    });
    let affected_paths = plan.operations.iter().flat_map(|operation| {
        std::iter::once(operation.target_path.clone()).chain(operation.backup_path.clone())
    }).collect::<Vec<_>>();
    let action = crate::models::confirmation::PendingAction {
        id: plan.repair_plan_id.clone(),
        action_type: crate::models::confirmation::PendingActionType::RepairProject,
        title: "Repair project application state".into(),
        message: if directory_only {
            "Create only the listed empty native directories. Markdown and raw sources will not be moved or overwritten.".into()
        } else {
            "Back up the corrupt graph cache, create a Git checkpoint, and regenerate only that derived cache.".into()
        },
        risk_level: crate::models::confirmation::RiskLevel::High,
        affected_paths,
        preview: Some(crate::models::confirmation::ActionPreview {
            summary: "Markdown, raw sources, purpose.md, and schema.md are protected and will not be changed.".into(),
            before: Some(if directory_only { "Repairable legacy layout: required empty directories are missing.".into() } else { "Recovery mode: corrupt graph cache blocks app-state writes.".into() }),
            after: Some(if directory_only { "The native layout will be complete; no existing file is overwritten.".into() } else { "A fresh empty graph cache will be available; the invalid bytes remain in the listed backup.".into() }),
            diff: Some(if directory_only {
                format!(
                    "Operation: create empty directories\nTargets: {}\nGit checkpoint: not applicable (empty directories have no Git tree entry)\nProtected: {}\nExternal links: remain blocked",
                    plan.operations.iter().map(|operation| operation.target_path.as_str()).collect::<Vec<_>>().join(", "),
                    plan.protected_paths.join(", "),
                )
            } else {
                let operation = &plan.operations[0];
                format!(
                    "Operation: regenerate derived cache\nTarget: {}\nBackup: {}\nGit checkpoint: required before write\nProtected: {}\nExternal links: remain blocked",
                    operation.target_path,
                    operation.backup_path.as_deref().unwrap_or("(missing)"),
                    plan.protected_paths.join(", "),
                )
            }),
        }),
        expires_at: Some((Utc::now() + Duration::minutes(10)).to_rfc3339()),
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(
            crate::models::confirmation::ConfirmationExecution::RepairProject {
                assessment_id: request.assessment_id,
                project_id: request.project_id,
                root_path: request.project_root_path,
                plan,
            },
        ),
    )?;
    Ok(action)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartProjectOpenAssessmentRequest {
    pub path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentOperationRequest {
    pub assessment_operation_id: AssessmentOperationId,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessedProjectRequest {
    pub assessment_id: AssessmentId,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessedProjectIntentRequest {
    pub assessment_id: AssessmentId,
    pub intent: ProjectOpenIntent,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessedCurrentProjectRequest {
    pub assessment_id: AssessmentId,
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableCompatibleFullFeaturesRequest {
    pub assessment_id: AssessmentId,
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default)]
    pub template: ProjectTemplate,
    #[serde(default = "default_true")]
    pub initialize_git: bool,
}

fn default_true() -> bool {
    true
}

fn register_opened_project(
    state: &AppState,
    summary: &ProjectSummary,
) -> Result<crate::models::paths::ProjectContext, BackendError> {
    let root = PathBuf::from(&summary.root_path);
    state.register_opened_project_authority(summary.project_id.clone(), &root)
}

/// Creation already produced a valid, Git-initialized project before a
/// registry, assessment, or recent-project follow-up failed. Never report it
/// as a failed creation: preserve the exact root and give the UI a safe,
/// explicit recovery route.
fn created_project_follow_up_failure(
    error: BackendError,
    summary: &ProjectSummary,
    step: &'static str,
) -> BackendError {
    BackendError::new(
        "PROJECT_CREATED_OPEN_FAILED",
        "The new knowledge base was created, but it could not be opened automatically.",
        true,
        true,
    )
    .with_details(serde_json::json!({
        "rootPath": summary.root_path,
        "projectId": summary.project_id,
        "step": step,
        "nextAction": "Open existing knowledge base and select rootPath.",
        "original": error,
    }))
}

fn open_revalidated_assessment(
    state: &AppState,
    assessment: ProjectOpenAssessment,
) -> Result<OpenedProject, BackendError> {
    let project_id = if assessment.format == ProjectFormat::NativeCurrent {
        state
            .project_service
            .stable_native_project_id(Path::new(&assessment.canonical_root_path))
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
    } else {
        uuid::Uuid::new_v4().to_string()
    };
    let context = state.register_opened_project_authority(
        project_id,
        Path::new(&assessment.canonical_root_path),
    )?;
    let summary = state.project_service.quick_project_summary(&context, None);
    let authority = project_session_authority(state, &context, &assessment)?;
    remember_summary(state, &summary)?;
    state
        .project_assessment_service
        .invalidate(&assessment.assessment_id)?;
    Ok(OpenedProject { summary, authority })
}

fn open_relocated_revalidated_assessment(
    state: &AppState,
    assessment: ProjectOpenAssessment,
    project_id: String,
    previous_root_path: &str,
) -> Result<OpenedProject, BackendError> {
    let candidate_context = crate::models::paths::ProjectContext::new(
        project_id.clone(),
        PathBuf::from(&assessment.canonical_root_path),
    );
    let relocated_recent = recent_project_from_summary(
        &state
            .project_service
            .quick_project_summary(&candidate_context, None),
    );
    let context = state.project_registry.relocate_trusted_native(
        &project_id,
        Path::new(previous_root_path),
        Path::new(&assessment.canonical_root_path),
        || {
            state.project_service.relocate_recent_project(
                &project_id,
                previous_root_path,
                relocated_recent,
            )?;
            Ok(())
        },
    )?;
    let summary = state.project_service.quick_project_summary(&context, None);
    let authority = project_session_authority(state, &context, &assessment)?;
    state
        .project_assessment_service
        .invalidate(&assessment.assessment_id)?;
    Ok(OpenedProject { summary, authority })
}

fn queue_project_inventory(
    app: AppHandle,
    state: &AppState,
    context: crate::models::paths::ProjectContext,
) -> Result<crate::models::task::BackendTask, String> {
    let task = state.task_service.create_memory_project_task(
        TaskType::ProjectInventory,
        context.project_id.clone(),
        context.root.clone(),
        "Inventory knowledge base".into(),
        true,
    )?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _ = state
            .task_service
            .transition_status(&task_id, TaskStatus::Running);
        let _ = state.task_service.append_log(
            &task_id,
            LogLevel::Info,
            "Inventorying project files without following linked descendants".into(),
        );
        let summary = state.project_service.scan_project_inventory(
            &context,
            None,
            || state.task_service.is_cancelled(&task_id),
            |current, label| {
                let _ = state
                    .task_service
                    .update_progress(&task_id, current, None, Some(label));
            },
        );
        let cancelled = summary.inventory_state == ProjectInventoryState::Partial
            || state.task_service.is_cancelled(&task_id);
        let result = TaskResult {
            summary: if cancelled {
                "Inventory cancelled; discovered counts remain partial.".into()
            } else {
                "Project inventory is ready.".into()
            },
            affected_paths: Vec::new(),
            reference: None,
            pending_action: None,
        };
        let _ = state.task_service.set_result(&task_id, result);
        let _ = state.task_service.append_log(
            &task_id,
            LogLevel::Info,
            if cancelled {
                "Inventory stopped by user request.".into()
            } else {
                "Inventory complete.".into()
            },
        );
        let _ = state.task_service.transition_status(
            &task_id,
            if cancelled {
                TaskStatus::Cancelled
            } else {
                TaskStatus::Succeeded
            },
        );
        state
            .task_service
            .emit_project_refreshed(context.project_id.clone(), summary);
    });
    Ok(task)
}

fn project_session_authority(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    assessment: &ProjectOpenAssessment,
) -> Result<ProjectSessionAuthority, BackendError> {
    // This also revokes a compatible runtime grant when its durable trust
    // record disappeared, keeping the UI snapshot aligned with executable
    // workflow access.
    let _ = state.resolve_workflow_access(context)?;
    let registered = state
        .project_registry
        .resolve_authority(&context.project_id, &context.root)?;
    let trust = match registered.trust {
        crate::app_state::ProjectTrustAuthority::Untrusted => ProjectTrustState::Untrusted,
        crate::app_state::ProjectTrustAuthority::TrustedNative
        | crate::app_state::ProjectTrustAuthority::TrustedCompatible => ProjectTrustState::Trusted,
    };
    let filesystem_access = match trust {
        ProjectTrustState::Trusted => state.project_service.filesystem_access(context, true),
        // Assessment already performs the no-write metadata probe used for an
        // untrusted folder. Do not turn this snapshot refresh into a write probe.
        ProjectTrustState::Untrusted => assessment.filesystem_access,
    };
    let markdown_readable = assessment
        .capabilities
        .contains(&ProjectCapability::ReadMarkdown);
    let capabilities = crate::services::project_service::assessment::derive_capabilities(
        assessment.format,
        trust,
        filesystem_access,
        assessment.health,
        markdown_readable,
        assessment.git.head.is_some(),
        &context.layout,
    );

    Ok(ProjectSessionAuthority {
        project_id: context.project_id.clone(),
        canonical_root_path: context.root.to_string_lossy().replace('\\', "/"),
        canonical_identity_key: assessment.canonical_identity_key.clone(),
        identity_revision: assessment.identity_revision.clone(),
        authority_revision: registered.authority_revision,
        format: assessment.format,
        trust,
        filesystem_access,
        health: assessment.health,
        layout: context.layout.clone(),
        confidence: context.layout_confidence,
        capabilities,
        warnings: assessment.warnings.clone(),
        layout_warnings: context.layout_warnings.clone(),
        git: assessment.git.clone(),
    })
}

pub(crate) fn revalidate_project_assessment(
    state: &AppState,
    assessment_id: &AssessmentId,
) -> Result<ProjectOpenAssessment, BackendError> {
    state
        .project_assessment_service
        .resolve_current(assessment_id)
}

fn revalidate_current_project_assessment(
    state: &AppState,
    request: &AssessedCurrentProjectRequest,
) -> Result<ProjectOpenAssessment, BackendError> {
    let assessment = revalidate_project_assessment(state, &request.assessment_id)?;
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let assessed_root = Path::new(&assessment.canonical_root_path)
        .canonicalize()
        .map_err(|_| project_assessment_context_mismatch())?;
    if context.root != assessed_root {
        return Err(project_assessment_context_mismatch());
    }
    Ok(assessment)
}

pub(crate) fn ensure_compatible_trust_candidate(
    assessment: &ProjectOpenAssessment,
) -> Result<(), BackendError> {
    if assessment.confidence == crate::models::layout::ProjectLayoutConfidence::Low
        || assessment.layout.markdown_roots.is_empty()
        || assessment.health != ProjectHealth::Healthy
        || assessment
            .warnings
            .iter()
            .any(|warning| warning.code == "PROJECT_PATH_NAME_COLLISION")
        || !matches!(
            assessment.format,
            ProjectFormat::NativeLegacy
                | ProjectFormat::NashsuLlmWiki
                | ProjectFormat::ObsidianVault
                | ProjectFormat::MarkdownVault
        )
    {
        return Err(BackendError::new(
            "PROJECT_TRUST_AUTHORITY_INVALID",
            "Only a verified compatible Markdown knowledge base can be trusted.",
            true,
            true,
        ));
    }
    Ok(())
}

fn project_assessment_context_mismatch() -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_CONTEXT_MISMATCH",
        "The assessment does not belong to the active project.",
        true,
        true,
    )
}

fn remember_summary(state: &AppState, summary: &ProjectSummary) -> Result<(), BackendError> {
    state
        .project_service
        .remember_recent_project(recent_project_from_summary(summary))?;
    Ok(())
}

fn recent_project_from_summary(summary: &ProjectSummary) -> RecentProject {
    RecentProject {
        project_id: summary.project_id.clone(),
        name: summary.name.clone(),
        root_path: summary.root_path.clone(),
        template: summary.template,
        opened_at: now_rfc3339(),
        wiki_page_count: summary.wiki_page_count,
        source_count: summary.source_count,
        task_count: summary.task_count,
        index_state: summary.index_state.clone(),
        graph_state: summary.graph_state.clone(),
        missing: false,
    }
}

#[tauri::command]
pub fn scan_project(
    state: State<'_, AppState>,
    request: ScanProjectRequest,
) -> Result<ProjectSummary, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    Ok(state.project_service.scan_project(&context, None))
}

#[tauri::command]
pub fn list_recent_projects(
    state: State<'_, AppState>,
) -> Result<Vec<RecentProject>, BackendError> {
    state.project_service.list_recent_projects()
}

#[tauri::command]
pub fn remember_recent_project(
    state: State<'_, AppState>,
    request: RememberRecentProjectRequest,
) -> Result<Vec<RecentProject>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.root_path)?;
    let summary = state
        .project_service
        .scan_project(&context, Some(&request.name));
    state
        .project_service
        .remember_recent_project(RecentProject {
            project_id: summary.project_id,
            name: summary.name,
            root_path: summary.root_path,
            template: request.template,
            opened_at: now_rfc3339(),
            wiki_page_count: summary.wiki_page_count,
            source_count: summary.source_count,
            task_count: summary.task_count,
            index_state: summary.index_state,
            graph_state: summary.graph_state,
            missing: false,
        })
}

/// Removes a project only from the global recent-project list. It deliberately
/// does not inspect, modify, or delete the recorded project folder.
#[tauri::command]
pub fn remove_recent_project(
    state: State<'_, AppState>,
    request: RemoveRecentProjectRequest,
) -> Result<Vec<RecentProject>, BackendError> {
    state
        .project_service
        .remove_recent_project(&request.project_id, &request.root_path)
}

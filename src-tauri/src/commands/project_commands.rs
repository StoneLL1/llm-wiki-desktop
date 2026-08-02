use std::path::{Path, PathBuf};

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::project::{
    AppSummary, AssessmentId, AssessmentOperationId, CreateProjectRequest, OpenProjectKind,
    OpenProjectRequest, OpenProjectResponse, ProjectAssessmentOperation, ProjectFormat,
    ProjectHealth, ProjectOpenAssessment, ProjectSummary, ProjectTemplate, RecentProject,
    RememberRecentProjectRequest, StartProjectOpenAssessmentResult,
};
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
pub fn create_project(
    state: State<'_, AppState>,
    request: CreateProjectRequest,
) -> Result<ProjectSummary, BackendError> {
    let summary = state.project_service.create_project(
        &request.root_path,
        &request.name,
        request.template,
    )?;
    state.project_registry.register_trusted_native(
        summary.project_id.clone(),
        &PathBuf::from(&summary.root_path),
    )?;

    let recents = state
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
    let _ = recents;
    Ok(summary)
}

#[tauri::command]
pub fn open_project(
    state: State<'_, AppState>,
    request: OpenProjectRequest,
) -> Result<OpenProjectResponse, BackendError> {
    let outcome = state.project_service.open_project(&request.path)?;
    if let Some(summary) = outcome.summary.as_ref() {
        if outcome.kind == OpenProjectKind::Opened {
            register_opened_project(&state, summary)?;
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
    }
    if let Some(pending_action) = outcome.pending_action.as_ref() {
        let execution = state
            .project_service
            .folder_initialization_execution(&PathBuf::from(&request.path), pending_action)?;
        state
            .confirmation_registry
            .register_with_execution(pending_action.clone(), Some(execution))?;
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
) -> Result<ProjectSummary, BackendError> {
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
    let project_id = uuid::Uuid::new_v4().to_string();
    let context = state.register_opened_project_authority(
        project_id,
        Path::new(&assessment.canonical_root_path),
    )?;
    let summary = state.project_service.scan_project(&context, None);
    remember_summary(&state, &summary)?;
    state
        .project_assessment_service
        .invalidate(&request.assessment_id)?;
    Ok(summary)
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

fn register_opened_project(state: &AppState, summary: &ProjectSummary) -> Result<(), BackendError> {
    let root = PathBuf::from(&summary.root_path);
    state.register_opened_project_authority(summary.project_id.clone(), &root)?;
    Ok(())
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
    Ok(())
}

/// Preview entry point for the "Open folder as project" dialog (dlg-folder).
///
/// Unlike `open_project`, this is a pure preview: it returns whether the picked
/// folder is an existing wiki project (`Opened` + summary) or a plain folder
/// (`NeedsConfirmation` + pending `InitializeFolder` action). For the
/// NeedsConfirmation case the pending action is registered (with its execution
/// plan) so the frontend can later confirm via `confirm_pending_action` ->
/// `confirm_folder_initialization`, which creates the project structure, moves
/// files by type, and creates the Git checkpoint. For the Opened case no
/// Git/registry/recent side effects run — the user is only previewing, and an
/// already-project folder does not need re-initialization.
#[tauri::command]
pub fn preview_open_folder_as_project(
    state: State<'_, AppState>,
    request: OpenProjectRequest,
) -> Result<OpenProjectResponse, BackendError> {
    state.preview_folder_as_project(&request.path)
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

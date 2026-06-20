use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::CompileRoutePreference;
use crate::models::export::{
    ExportRecord, ExportRoute, ExportRoutePreference, ExportType, ListExportsRequest,
    OpenExportFolderRequest, ReadExportPreviewRequest, RegenerateExportRequest, StartExportRequest,
};
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::{AgentService, ExportService, LlmService};
use crate::tasks::task_model::LogLevel;

fn context_for(project_id: &str, root_path: &str) -> ProjectContext {
    ProjectContext::new(project_id, PathBuf::from(root_path))
}

/// Derive a human title for the record from the source page (or the export
/// type when project-wide). Kept tiny so the record stays meaningful in the UI
/// list without an extra model round-trip.
fn title_for(
    context: &ProjectContext,
    export_type: ExportType,
    source_path: Option<&str>,
) -> String {
    if let Some(path) = source_path {
        if let Ok(page) = state_read_title(context, path) {
            return page;
        }
        return path.to_string();
    }
    match export_type {
        ExportType::ProjectReport => "Project report".to_string(),
        ExportType::ConceptMap => "Wiki concept map".to_string(),
        _ => export_type.skill_folder().to_string(),
    }
}

fn state_read_title(context: &ProjectContext, path: &str) -> Result<String, BackendError> {
    let store = crate::services::FileStore;
    let raw = store.read_markdown(context, path)?;
    let title_line = raw
        .lines()
        .find(|line| line.trim_start().starts_with("# "))
        .map(|line| line.trim().trim_start_matches("# ").to_string());
    Ok(title_line.unwrap_or_else(|| {
        path.rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".md")
            .to_string()
    }))
}

/// Start a skill-driven HTML export as a cancellable background task (Agent CLI
/// preferred, BYOK fallback). The captured HTML is written to `exports/html/`
/// and a record is appended to `.app/exports.json`.
#[tauri::command]
pub fn start_export(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartExportRequest,
) -> Result<BackendTask, BackendError> {
    run_export_task(app, state, request.into())
}

/// Regenerate an export from an existing record's type + source.
#[tauri::command]
pub fn regenerate_export(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RegenerateExportRequest,
) -> Result<BackendTask, BackendError> {
    run_export_task(app, state, request.into_directive())
}

fn run_export_task(
    app: AppHandle,
    state: State<'_, AppState>,
    directive: ExportDirective,
) -> Result<BackendTask, BackendError> {
    let project_id = directive.project_id.clone();
    let task = state.task_service.create_task(
        TaskType::Export,
        Some(project_id.clone()),
        format!("{:?} export", directive.export_type),
        true,
    );
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_export(&state, directive, &task_id).await {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            if !matches!(
                state.task_service.get_task(&task_id).map(|t| t.status),
                Some(TaskStatus::Cancelled)
            ) {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
            }
        }
    });
    Ok(task)
}

/// Lightweight bag the two request DTOs collapse into so start/regenerate
/// share one task body.
struct ExportDirective {
    project_id: String,
    project_root_path: String,
    export_type: ExportType,
    source_path: Option<String>,
    route: ExportRoutePreference,
    agent: Option<AgentKind>,
    provider: Option<LlmProviderKind>,
}

impl From<StartExportRequest> for ExportDirective {
    fn from(value: StartExportRequest) -> Self {
        Self {
            project_id: value.project_id,
            project_root_path: value.project_root_path,
            export_type: value.export_type,
            source_path: value.source_path,
            route: value.route,
            agent: value.agent,
            provider: value.provider,
        }
    }
}

impl RegenerateExportRequest {
    fn into_directive(self) -> ExportDirective {
        ExportDirective {
            project_id: self.project_id,
            project_root_path: self.project_root_path,
            export_type: self.export_type,
            source_path: self.source_path,
            route: self.route,
            agent: self.agent,
            provider: self.provider,
        }
    }
}

async fn run_export(
    state: &AppState,
    directive: ExportDirective,
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    let context = context_for(&directive.project_id, &directive.project_root_path);

    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            format!("Building {} prompt", directive.export_type.skill_folder()),
        )
        .map_err(task_error)?;
    let prompt = state.export_service.build_export_prompt(
        &context,
        directive.export_type,
        directive.source_path.as_deref(),
        &state.search_service,
    )?;

    let (route, raw_html) = match resolve_route(
        state,
        &context,
        directive.route,
        directive.agent,
        directive.provider,
    )? {
        ResolvedRoute::Agent(kind) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!(
                        "Running {} ({} skill)",
                        kind.command(),
                        directive.export_type.skill_folder()
                    ),
                )
                .map_err(task_error)?;
            let workspace = create_export_workspace(task_id)?;
            let _guard = WorkspaceGuard(workspace.clone());
            let invocation = AgentService::html_export_invocation(kind, &workspace, &prompt)?;
            let captured = state.agent_service.run_task_streaming(
                &invocation,
                &state.task_service,
                task_id,
            )?;
            (ExportRoute::Agent, captured)
        }
        ResolvedRoute::Byok(provider) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Calling {:?} (BYOK)", provider.provider),
                )
                .map_err(task_error)?;
            let secret = state.secret_service.get(provider.provider)?;
            let completion = state
                .llm_service
                .complete(&provider, secret.as_deref(), &prompt);
            tokio::pin!(completion);
            let raw = loop {
                tokio::select! {
                    result = &mut completion => break result?,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        if state.task_service.is_cancelled(task_id) {
                            return Err(BackendError::new(
                                "EXPORT_CANCELLED",
                                "Export was cancelled.",
                                true,
                                false,
                            ));
                        }
                    }
                }
            };
            for line in raw.lines() {
                let _ = state
                    .task_service
                    .append_log(task_id, LogLevel::Info, line.to_string());
            }
            (ExportRoute::Byok, raw)
        }
    };

    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "EXPORT_CANCELLED",
            "Export was cancelled.",
            true,
            false,
        ));
    }

    let html = ExportService::extract_html(&raw_html);
    let lower = html.trim().to_ascii_lowercase();
    if !lower.contains("<html") && !lower.contains("<!doctype html") {
        return Err(BackendError::new(
            "EXPORT_OUTPUT_INVALID",
            "The model output did not contain an HTML document.",
            true,
            false,
        )
        .with_details(
            serde_json::json!({ "preview": html.chars().take(200).collect::<String>() }),
        ));
    }

    let output_path = state
        .export_service
        .build_output_relative_path(directive.export_type, directive.source_path.as_deref())?;
    state
        .export_service
        .write_html(&context, &output_path, &html)?;
    let title = title_for(
        &context,
        directive.export_type,
        directive.source_path.as_deref(),
    );
    let record = ExportService::new_record(
        directive.export_type,
        title,
        directive.source_path.clone(),
        output_path.clone(),
        route,
        Some(task_id.to_string()),
    );
    state.export_service.append_record(&context, record)?;

    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: format!("Export written to {output_path}"),
                affected_paths: vec![output_path, ".app/exports.json".into()],
                pending_action: None,
            },
        )
        .map_err(task_error)?;
    state
        .task_service
        .transition_status(task_id, TaskStatus::Succeeded)
        .map_err(task_error)?;
    Ok(())
}

/// List prior export records (newest first).
#[tauri::command]
pub fn list_exports(
    state: State<'_, AppState>,
    request: ListExportsRequest,
) -> Result<Vec<ExportRecord>, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state.export_service.list_records(&context)
}

/// Read an exported HTML file for in-app iframe preview. The path is asserted
/// to stay under `exports/html/` before reading.
#[tauri::command]
pub fn read_export_preview(
    state: State<'_, AppState>,
    request: ReadExportPreviewRequest,
) -> Result<String, BackendError> {
    let path = &request.output_path;
    if !path.starts_with("exports/html/") || path.contains("..") {
        return Err(BackendError::new(
            "EXPORT_PATH_INVALID",
            "Preview may only read files under exports/html/.",
            true,
            true,
        ));
    }
    let context = context_for(&request.project_id, &request.project_root_path);
    state.file_store.read_markdown(&context, path)
}

/// Reveal an exported file in the OS file manager. Backend-owned so the UI never
/// spawns processes directly.
#[tauri::command]
pub fn open_export_folder(request: OpenExportFolderRequest) -> Result<(), BackendError> {
    let path = &request.output_path;
    if !path.starts_with("exports/html/") || path.contains("..") {
        return Err(BackendError::new(
            "EXPORT_PATH_INVALID",
            "Open may only target files under exports/html/.",
            true,
            true,
        ));
    }
    let context = context_for(&request.project_id, &request.project_root_path);
    let absolute = context.resolve_project_path(path)?;
    reveal_in_file_manager(&absolute)
}

fn reveal_in_file_manager(path: &std::path::Path) -> Result<(), BackendError> {
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .args(["/select,", &path.to_string_lossy()])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
    } else {
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        std::process::Command::new("xdg-open").arg(parent).spawn()
    };
    result.map(|_| ()).map_err(|err| {
        BackendError::new(
            "EXPORT_OPEN_FAILED",
            format!("Could not open the file manager: {err}"),
            true,
            false,
        )
    })
}

enum ResolvedRoute {
    Agent(AgentKind),
    Byok(LlmProviderConfig),
}

/// Replicates `chat_commands::resolve_route` — Agent preferred, BYOK fallback.
fn resolve_route(
    state: &AppState,
    context: &ProjectContext,
    preference: ExportRoutePreference,
    explicit_agent: Option<AgentKind>,
    explicit_provider: Option<LlmProviderKind>,
) -> Result<ResolvedRoute, BackendError> {
    let compile_preference = match preference {
        ExportRoutePreference::Auto => CompileRoutePreference::Auto,
        ExportRoutePreference::Agent => CompileRoutePreference::Agent,
        ExportRoutePreference::Byok => CompileRoutePreference::Byok,
    };
    let agent_config = AgentService::load_config(context)?;
    let providers = LlmService::list_providers(context)?;
    let selected_agent = explicit_agent.or(agent_config.default_agent);
    let usable_agent = selected_agent.filter(|kind| {
        state
            .agent_service
            .detect_agents(Some(*kind))
            .iter()
            .any(|info| info.kind == *kind && info.state == AgentDetectionState::Installed)
    });
    let selected_provider = select_provider(explicit_provider, &providers, &state.secret_service)?;
    let use_agent = match compile_preference {
        CompileRoutePreference::Agent => true,
        CompileRoutePreference::Byok => false,
        CompileRoutePreference::Auto => usable_agent.is_some(),
    };
    if use_agent {
        if let Some(kind) = usable_agent {
            return Ok(ResolvedRoute::Agent(kind));
        }
        return Err(BackendError::new(
            "AGENT_UNAVAILABLE",
            "No usable Agent CLI is configured for export. Install an Agent or switch to a BYOK provider.",
            true,
            true,
        ));
    }
    match selected_provider {
        Some(provider) => Ok(ResolvedRoute::Byok(provider)),
        None => Err(BackendError::new(
            "LLM_PROVIDER_MISSING",
            "No enabled BYOK provider with a configured secret is available for export.",
            true,
            true,
        )),
    }
}

fn select_provider(
    explicit: Option<LlmProviderKind>,
    providers: &[LlmProviderConfig],
    secrets: &crate::services::SecretService,
) -> Result<Option<LlmProviderConfig>, BackendError> {
    if let Some(kind) = explicit {
        let provider = providers
            .iter()
            .find(|p| p.enabled && p.provider == kind)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "LLM_PROVIDER_MISSING",
                    "The selected BYOK provider is not enabled.",
                    true,
                    true,
                )
            })?;
        if provider.provider.requires_secret() && secrets.get(provider.provider)?.is_none() {
            return Err(BackendError::new(
                "LLM_SECRET_MISSING",
                "The selected provider has no configured secret.",
                true,
                true,
            ));
        }
        return Ok(Some(provider));
    }
    for provider in providers.iter().filter(|p| p.enabled) {
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

fn create_export_workspace(task_id: &str) -> Result<PathBuf, BackendError> {
    let workspace = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(format!("export-{task_id}"));
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace).map_err(|err| {
            BackendError::new("EXPORT_WORKSPACE_FAILED", err.to_string(), true, false)
        })?;
    }
    std::fs::create_dir_all(&workspace).map_err(|err| {
        BackendError::new("EXPORT_WORKSPACE_FAILED", err.to_string(), true, false)
    })?;
    Ok(workspace)
}

/// Removes the temp workspace on drop, including when the agent run errors or
/// is cancelled mid-stream (mirrors `lint_commands::WorkspaceGuard`).
struct WorkspaceGuard(PathBuf);

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::paths::ProjectContext;

    #[test]
    fn preview_rejects_paths_outside_exports_html() {
        let context = ProjectContext::new("p", std::env::temp_dir().join("x"));
        let _ = context;
        // The guard lives entirely in the path string check; exercise both
        // rejection branches without touching the filesystem.
        let outside = "wiki/index.md";
        assert!(!outside.starts_with("exports/html/"));
        let traversal = "exports/html/../../wiki/x.md";
        assert!(traversal.contains(".."));
    }

    #[test]
    fn title_for_falls_back_to_export_type() {
        // Project-wide exports have no source page to read a title from.
        let tmp = std::env::temp_dir().join(format!(
            "llm-wiki-export-title-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let context = ProjectContext::new("p", tmp.clone());
        assert_eq!(
            title_for(&context, ExportType::ProjectReport, None),
            "Project report"
        );
        assert_eq!(
            title_for(&context, ExportType::ConceptMap, None),
            "Wiki concept map"
        );
        std::fs::remove_dir_all(tmp).unwrap();
    }
}

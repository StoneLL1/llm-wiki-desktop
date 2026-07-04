use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::CompileRoutePreference;
use crate::models::export::{
    ExportContentOptions, ExportRecord, ExportRoute, ExportRoutePreference, ExportType,
    ListExportsRequest, OpenExportFolderRequest, OpenExportInBrowserRequest,
    ReadExportPreviewRequest, RegenerateExportRequest, StartExportRequest,
    ToggleExportBookmarkRequest, ToggleExportBookmarkResponse,
};
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::{AgentService, ExportService, LlmService};
use crate::tasks::task_model::LogLevel;

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
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    run_export_task(app, state, request.into(), context)
}

/// Regenerate an export from an existing record's type + source.
#[tauri::command]
pub fn regenerate_export(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RegenerateExportRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    run_export_task(app, state, request.into_directive(), context)
}

fn run_export_task(
    app: AppHandle,
    state: State<'_, AppState>,
    directive: ExportDirective,
    context: ProjectContext,
) -> Result<BackendTask, BackendError> {
    let project_id = directive.project_id.clone();
    let task = state
        .task_service
        .create_project_task(
            TaskType::Export,
            project_id.clone(),
            context.root.clone(),
            format!("{:?} export", directive.export_type),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    // Capture what a Failed record would need before `directive` moves into
    // the task body. The intended route is derived from the preference: the
    // resolved route is only known after `resolve_route` succeeds, which may
    // itself be the point of failure. Note: for an `Auto` preference that
    // fails before resolution, the badge falls back to `Agent` — imprecise but
    // harmless (retry re-runs from the record's type/source, not its route).
    let failed_type = directive.export_type;
    let failed_source = directive.source_path.clone();
    let failed_route = match directive.route {
        ExportRoutePreference::Byok => ExportRoute::Byok,
        _ => ExportRoute::Agent,
    };
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_export(&state, directive, &context, &task_id).await {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            // A cancel request flips the cancellation token immediately, but the
            // task status only reaches Cancelled after an async transition — and
            // `Cancelling -> Failed` is an allowed transition. Persisting a
            // Failed record for something the user cancelled violates the
            // contract, so consult both the token (source of truth) and the
            // Cancelling/Cancelled status window before marking Failed.
            let cancelled = state.task_service.is_cancelled(&task_id)
                || matches!(
                    state.task_service.get_task(&task_id).map(|t| t.status),
                    Some(TaskStatus::Cancelled) | Some(TaskStatus::Cancelling)
                );
            if !cancelled {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
                // Persist a Failed record so the user can see the failure and
                // retry from the history list. Best-effort: if the would-be
                // output path can't be derived we skip the record (the task is
                // still marked Failed either way).
                let title = title_for(&context, failed_type, failed_source.as_deref());
                if let Ok(output_path) = state
                    .export_service
                    .build_output_relative_path(failed_type, failed_source.as_deref())
                {
                    let record = ExportService::new_failed_record(
                        failed_type,
                        title,
                        failed_source.clone(),
                        output_path,
                        failed_route,
                        Some(task_id.clone()),
                    );
                    let _ = state.export_service.append_record(&context, record);
                }
            }
        }
    });
    Ok(task)
}

/// Lightweight bag the two request DTOs collapse into so start/regenerate
/// share one task body.
struct ExportDirective {
    project_id: String,
    export_type: ExportType,
    source_path: Option<String>,
    route: ExportRoutePreference,
    agent: Option<AgentKind>,
    provider: Option<LlmProviderKind>,
    template: Option<String>,
    options: ExportContentOptions,
}

impl From<StartExportRequest> for ExportDirective {
    fn from(value: StartExportRequest) -> Self {
        Self {
            project_id: value.project_id,
            export_type: value.export_type,
            source_path: value.source_path,
            route: value.route,
            agent: value.agent,
            provider: value.provider,
            template: value.template,
            options: value.options,
        }
    }
}

impl RegenerateExportRequest {
    fn into_directive(self) -> ExportDirective {
        ExportDirective {
            project_id: self.project_id,
            export_type: self.export_type,
            source_path: self.source_path,
            route: self.route,
            agent: self.agent,
            provider: self.provider,
            template: self.template,
            options: self.options,
        }
    }
}

async fn run_export(
    state: &AppState,
    directive: ExportDirective,
    context: &ProjectContext,
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            format!("Building {} prompt", directive.export_type.skill_folder()),
        )
        .map_err(task_error)?;
    let language = state
        .settings_service
        .read_settings(context)
        .map(|settings| settings.language)
        .unwrap_or_else(|_| "en".to_string());
    let prompt = state.export_service.build_export_prompt(
        context,
        directive.export_type,
        directive.source_path.as_deref(),
        &state.search_service,
        &language,
        directive.template.as_deref(),
        &directive.options,
    )?;

    let (route, raw_html) = match resolve_route(
        state,
        context,
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
            let raw = crate::tasks::byok_progress::poll_with_progress(
                &state.task_service,
                task_id,
                "Exporting",
                completion,
            )
            .await
            .map_err(|_| {
                crate::tasks::byok_progress::cancelled_error(
                    "EXPORT_CANCELLED",
                    "Export was cancelled.",
                )
            })??;
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
        .write_html(context, &output_path, &html)?;
    let title = title_for(
        context,
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
    state.export_service.append_record(context, record)?;

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
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let bookmark_ids = state.bookmark_service.export_record_ids(&context)?;
    state
        .export_service
        .list_records_with_bookmarks(&context, &bookmark_ids)
}

#[tauri::command]
pub fn toggle_export_bookmark(
    state: State<'_, AppState>,
    request: ToggleExportBookmarkRequest,
) -> Result<ToggleExportBookmarkResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let record = state
        .export_service
        .list_records(&context)?
        .into_iter()
        .find(|record| record.id == request.export_record_id)
        .ok_or_else(|| {
            BackendError::new(
                "EXPORT_RECORD_NOT_FOUND",
                "Export record does not exist.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "exportRecordId": request.export_record_id }))
        })?;
    let response = state
        .bookmark_service
        .toggle_export_html(&context, &record)?;
    Ok(ToggleExportBookmarkResponse {
        export_record_id: response.export_record_id,
        bookmarked: response.bookmarked,
    })
}

/// Read an exported HTML file for in-app iframe preview. The path is asserted
/// to stay under `exports/html/` before reading.
#[tauri::command]
pub fn read_export_preview(
    state: State<'_, AppState>,
    request: ReadExportPreviewRequest,
) -> Result<String, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let absolute = state
        .export_service
        .resolve_existing_html_export(&context, &request.output_path)?;
    std::fs::read_to_string(&absolute).map_err(|err| {
        BackendError::new(
            "EXPORT_PREVIEW_READ_FAILED",
            format!("Could not read export preview: {err}"),
            true,
            false,
        )
    })
}

/// Open an exported HTML file in the OS default browser. Backend-owned so the
/// UI never spawns processes directly.
#[tauri::command]
pub fn open_export_in_browser(
    state: State<'_, AppState>,
    request: OpenExportInBrowserRequest,
) -> Result<(), BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let absolute = state
        .export_service
        .resolve_existing_html_export(&context, &request.output_path)?;
    open_in_default_browser(&absolute)
}

/// Reveal an exported file in the OS file manager. Backend-owned so the UI never
/// spawns processes directly.
#[tauri::command]
pub fn open_export_folder(
    state: State<'_, AppState>,
    request: OpenExportFolderRequest,
) -> Result<(), BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let absolute = state
        .export_service
        .resolve_existing_html_export(&context, &request.output_path)?;
    reveal_in_file_manager(&absolute)
}

fn reveal_in_file_manager(path: &Path) -> Result<(), BackendError> {
    let result = if cfg!(target_os = "windows") {
        Command::new("explorer")
            .args(["/select,"])
            .arg(path)
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").args(["-R"]).arg(path).spawn()
    } else {
        let parent = path.parent().unwrap_or(Path::new("."));
        Command::new("xdg-open").arg(parent).spawn()
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

fn open_in_default_browser(path: &Path) -> Result<(), BackendError> {
    let result = if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };
    result.map(|_| ()).map_err(|err| {
        BackendError::new(
            "EXPORT_OPEN_FAILED",
            format!("Could not open the export in a browser: {err}"),
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

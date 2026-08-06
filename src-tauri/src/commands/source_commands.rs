use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::CompileRoutePreference;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::source::{
    ApplySourceCandidateRequest, DeleteSourcePreview, DeleteSourceRequest,
    DiscardSourceCandidateRequest, GetSourceDetailRequest, GetSourceVersionsRequest,
    MoveSourcePreview, MoveSourceRequest, PreviewDeleteSourceRequest, PreviewMoveSourceRequest,
    PreviewSourceUpdateRequest, ReprocessSourceRequest, RestoreSourceVersionRequest,
    RetrySourceAiOrganizeRequest, SourceAiOrganizeRoute, SourceCandidateKind,
    SourceCandidateSummary, SourceDetail, SourceMutationResult, SourceUpdatePreview,
    SourceVersionSummary, StartSourceAiOrganizeRequest,
};
use crate::models::task::{
    BackendTask, TaskActivity, TaskActivityStatus, TaskResult, TaskResultReference, TaskStatus,
    TaskType,
};
use crate::services::import_v2::source_ai_organize;
use crate::services::{AgentService, LlmService};
use crate::tasks::task_model::LogLevel;

#[tauri::command]
pub async fn get_source_detail(
    app: AppHandle,
    request: GetSourceDetailRequest,
) -> Result<SourceDetail, BackendError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        state
            .import_v2_service
            .get_source_detail(&context, &state.file_store, &request.source_id)
    })
    .await
    .map_err(source_io_worker_failed)?
}

fn source_io_worker_failed(error: impl std::fmt::Display) -> BackendError {
    BackendError::new(
        "SOURCE_IO_WORKER_FAILED",
        format!("The Source I/O worker stopped unexpectedly: {error}"),
        true,
        false,
    )
}

#[tauri::command]
pub fn list_source_versions(
    state: State<'_, AppState>,
    request: GetSourceVersionsRequest,
) -> Result<Vec<SourceVersionSummary>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .list_source_versions(&context, &state.file_store, &request.source_id)
}

#[tauri::command]
pub fn preview_source_update(
    state: State<'_, AppState>,
    request: PreviewSourceUpdateRequest,
) -> Result<SourceUpdatePreview, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.preview_source_update(
        &context,
        &state.file_store,
        &request.source_id,
        &request.candidate_id,
    )
}

#[tauri::command]
pub fn apply_source_candidate(
    state: State<'_, AppState>,
    request: ApplySourceCandidateRequest,
) -> Result<SourceMutationResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.apply_source_candidate(
        &context,
        &state.file_store,
        &state.git_service,
        &request,
    )
}

#[tauri::command]
pub fn discard_source_candidate(
    state: State<'_, AppState>,
    request: DiscardSourceCandidateRequest,
) -> Result<(), BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.discard_source_candidate(
        &context,
        &state.file_store,
        &request.source_id,
        &request.candidate_id,
    )
}

#[tauri::command]
pub fn restore_source_version(
    state: State<'_, AppState>,
    request: RestoreSourceVersionRequest,
) -> Result<SourceMutationResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.restore_source_version(
        &context,
        &state.file_store,
        &state.git_service,
        &request.source_id,
        &request.version_id,
        &request.expected_markdown_hash,
    )
}

#[tauri::command]
pub fn start_source_ai_organize(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartSourceAiOrganizeRequest,
) -> Result<BackendTask, BackendError> {
    start_source_ai_organize_impl(app, state, request, None)
}

fn start_source_ai_organize_impl(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartSourceAiOrganizeRequest,
    expected_resolved_identity: Option<(String, String)>,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.require_external_ai_access(&context)?;
    state.require_project_write_access(&context)?;
    let input = state.import_v2_service.prepare_source_ai_organize_input(
        &context,
        &state.file_store,
        &request.source_id,
        &request.expected_version_id,
        &request.expected_markdown_hash,
        request.custom_instructions.as_deref(),
    )?;
    let route = resolve_source_ai_route(
        &state,
        &context,
        request.route,
        request.agent,
        request.provider,
    )?;
    let resolved_identity = resolved_source_ai_identity(&route);
    if expected_resolved_identity
        .as_ref()
        .is_some_and(|expected| expected != &resolved_identity)
    {
        return Err(source_ai_recovery_engine_changed());
    }
    let recovery =
        SourceAiRecovery::from_resolved(&context, &route, request.custom_instructions.clone());
    let reservation_key = format!("{}\0{}", context.root.to_string_lossy(), request.source_id);
    state
        .import_v2_service
        .reserve_source_ai(reservation_key.clone())?;
    let task = match state.task_service.create_project_task(
        TaskType::SourceAiOrganize,
        request.project_id.clone(),
        context.root.clone(),
        format!("AI organize Source: {}", input.title),
        true,
    ) {
        Ok(task) => task,
        Err(error) => {
            state.import_v2_service.release_source_ai(&reservation_key);
            return Err(source_task_error(error));
        }
    };
    let task = match state.task_service.set_result(
        &task.id,
        TaskResult {
            summary: "Source AI organization is queued.".into(),
            affected_paths: Vec::new(),
            reference: Some(TaskResultReference::SourceAiOrganize {
                source_id: input.source_id.clone(),
                base_version_id: input.version_id.clone(),
                base_markdown_hash: input.markdown_hash.clone(),
                candidate_id: None,
                route: Some(recovery.route),
                agent: recovery.agent,
                provider: recovery.provider,
                custom_instructions: recovery.custom_instructions.clone(),
                project_root_path: Some(recovery.project_root_path.clone()),
                resolved_engine: Some(recovery.resolved_engine.clone()),
                resolved_model: Some(recovery.resolved_model.clone()),
            }),
            pending_action: None,
        },
    ) {
        Ok(task) => task,
        Err(error) => {
            state.import_v2_service.release_source_ai(&reservation_key);
            let _ = state
                .task_service
                .set_error(&task.id, source_task_error(error.clone()));
            let _ = state
                .task_service
                .transition_status(&task.id, TaskStatus::Failed);
            return Err(source_task_error(error));
        }
    };
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let outcome =
            run_source_ai_organize(&state, &context, &task_id, input, route, recovery).await;
        state.import_v2_service.release_source_ai(&reservation_key);
        if let Err(error) = outcome {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            match state
                .task_service
                .get_task(&task_id)
                .map(|task| task.status)
            {
                Some(TaskStatus::Cancelling) => {
                    let _ = state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Cancelled);
                }
                Some(TaskStatus::Cancelled) => {}
                _ => {
                    let _ = state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Failed);
                }
            }
        }
    });
    Ok(task)
}

#[tauri::command]
pub fn retry_source_ai_organize(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RetrySourceAiOrganizeRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let interrupted = state
        .task_service
        .get_task(&request.task_id)
        .ok_or_else(source_ai_recovery_unavailable)?;
    if !is_retryable_source_ai_task(&interrupted, &request.project_id) {
        return Err(source_ai_recovery_unavailable());
    }
    let (
        source_id,
        expected_version_id,
        expected_markdown_hash,
        route,
        agent,
        provider,
        custom_instructions,
        project_root_path,
        resolved_engine,
        resolved_model,
    ) = match interrupted.result.and_then(|result| result.reference) {
        Some(TaskResultReference::SourceAiOrganize {
            source_id,
            base_version_id,
            base_markdown_hash,
            candidate_id: None,
            route,
            agent,
            provider,
            custom_instructions,
            project_root_path,
            resolved_engine,
            resolved_model,
            ..
        }) => (
            source_id,
            base_version_id,
            base_markdown_hash,
            route.unwrap_or(CompileRoutePreference::Auto),
            agent,
            provider,
            custom_instructions,
            project_root_path,
            resolved_engine,
            resolved_model,
        ),
        _ => return Err(source_ai_recovery_unavailable()),
    };
    let project_root_path = project_root_path.ok_or_else(source_ai_recovery_unavailable)?;
    if std::path::PathBuf::from(&project_root_path) != context.root {
        return Err(source_ai_recovery_unavailable());
    }
    let resolved_identity = (
        resolved_engine.ok_or_else(source_ai_recovery_unavailable)?,
        resolved_model.ok_or_else(source_ai_recovery_unavailable)?,
    );
    start_source_ai_organize_impl(
        app,
        state,
        StartSourceAiOrganizeRequest {
            project_id: request.project_id,
            project_root_path: request.project_root_path,
            source_id,
            expected_version_id,
            expected_markdown_hash,
            route,
            agent,
            provider,
            custom_instructions,
        },
        Some(resolved_identity),
    )
}

fn is_retryable_source_ai_task(task: &BackendTask, project_id: &str) -> bool {
    task.project_id.as_deref() == Some(project_id)
        && task.task_type == TaskType::SourceAiOrganize
        && task.status == TaskStatus::Failed
        && task.error.as_ref().is_some_and(|error| error.recoverable)
        && matches!(
            task.result
                .as_ref()
                .and_then(|result| result.reference.as_ref()),
            Some(TaskResultReference::SourceAiOrganize {
                candidate_id: None,
                ..
            })
        )
}

#[derive(Debug, Clone)]
struct SourceAiRecovery {
    route: CompileRoutePreference,
    agent: Option<AgentKind>,
    provider: Option<LlmProviderKind>,
    custom_instructions: Option<String>,
    project_root_path: String,
    resolved_engine: String,
    resolved_model: String,
}

impl SourceAiRecovery {
    fn from_resolved(
        context: &crate::models::paths::ProjectContext,
        route: &ResolvedSourceAiRoute,
        custom_instructions: Option<String>,
    ) -> Self {
        let (preference, agent, provider) = match route {
            ResolvedSourceAiRoute::Agent { kind, .. } => {
                (CompileRoutePreference::Agent, Some(*kind), None)
            }
            ResolvedSourceAiRoute::Byok(config) => {
                (CompileRoutePreference::Byok, None, Some(config.provider))
            }
        };
        let (resolved_engine, resolved_model) = resolved_source_ai_identity(route);
        Self {
            route: preference,
            agent,
            provider,
            custom_instructions,
            project_root_path: context.root.to_string_lossy().into_owned(),
            resolved_engine,
            resolved_model,
        }
    }
}

#[derive(Debug, Clone)]
enum ResolvedSourceAiRoute {
    Agent { kind: AgentKind, version: String },
    Byok(LlmProviderConfig),
}

fn resolved_source_ai_identity(route: &ResolvedSourceAiRoute) -> (String, String) {
    match route {
        // Agent CLIs choose a model from their Source execution profile at run
        // time. Retry pins the selected CLI, but a CLI version is not a model
        // identifier and must not make a harmless update invalidate it.
        ResolvedSourceAiRoute::Agent { kind, .. } => {
            (kind.command().to_string(), "cli-default".into())
        }
        ResolvedSourceAiRoute::Byok(provider) => (
            provider_name(provider.provider).to_string(),
            provider.model.clone(),
        ),
    }
}

async fn run_source_ai_organize(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    task_id: &str,
    input: source_ai_organize::SourceAiOrganizeInput,
    route: ResolvedSourceAiRoute,
    recovery: SourceAiRecovery,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(source_task_error)?;
    if state.task_service.is_cancelled(task_id) {
        return Err(source_ai_cancelled());
    }
    state.require_external_ai_access(context)?;
    state.require_project_write_access(context)?;
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            "Prepared bounded current-Source input; raw attachments and secrets are excluded."
                .into(),
        )
        .map_err(source_task_error)?;
    let language = state
        .settings_service
        .read_settings(context)
        .map(|settings| settings.language)
        .unwrap_or_else(|_| "en".to_string());
    let (raw, candidate_route, engine, model, engine_version) = match route {
        ResolvedSourceAiRoute::Agent { kind, version } => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!(
                        "Running {} CLI {} with its Source execution profile.",
                        kind.command(),
                        version
                    ),
                )
                .map_err(source_task_error)?;
            let workspace = source_ai_organize::create_agent_workspace(task_id, &input)?;
            let outcome = (|| {
                let prompt = if matches!(kind, AgentKind::Openclaw | AgentKind::Hermes) {
                    source_ai_organize::provider_prompt(&input, &language)?
                } else {
                    source_ai_organize::agent_prompt(&language)
                };
                let invocation = AgentService::source_ai_organize_invocation(
                    kind,
                    &workspace,
                    &prompt,
                    source_ai_organize::SOURCE_AI_OUTPUT_SCHEMA,
                )?;
                let captured = state.agent_service.run_source_ai_organize(
                    kind,
                    &invocation,
                    &state.task_service,
                    task_id,
                )?;
                source_ai_organize::read_agent_result(&workspace, &captured)
            })();
            if let Err(error) = source_ai_organize::cleanup_agent_workspace(&workspace) {
                let _ = state.task_service.append_log(
                    task_id,
                    LogLevel::Warn,
                    format!(
                        "Temporary Source AI workspace cleanup failed: {}",
                        error.message
                    ),
                );
            }
            (
                outcome?,
                SourceAiOrganizeRoute::Agent,
                kind.command().to_string(),
                "cli-default".into(),
                Some(version),
            )
        }
        ResolvedSourceAiRoute::Byok(provider) => {
            let secret = state.secret_service.get(provider.provider)?;
            let prompt = source_ai_organize::provider_prompt(&input, &language)?;
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "source-ai-provider".into(),
                    status: TaskActivityStatus::Started,
                    label: Some("Waiting for the selected provider".into()),
                },
            );
            let task_service = &state.task_service;
            let completion = state
                .llm_service
                .complete_structured_long_running_streaming(
                    &provider,
                    secret.as_deref(),
                    &prompt,
                    move || task_service.is_cancelled(task_id),
                    |_| {},
                )
                .await;
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "source-ai-provider".into(),
                    status: if completion.is_ok() {
                        TaskActivityStatus::Completed
                    } else {
                        TaskActivityStatus::Failed
                    },
                    label: Some(if completion.is_ok() {
                        "Provider response received".into()
                    } else {
                        "Provider request failed".into()
                    }),
                },
            );
            let raw = completion?;
            let engine = provider_name(provider.provider).to_string();
            let model = provider.model.clone();
            (raw, SourceAiOrganizeRoute::Byok, engine, model, None)
        }
    };
    if state.task_service.is_cancelled(task_id) {
        return Err(source_ai_cancelled());
    }
    let candidate_markdown =
        source_ai_organize::build_candidate_markdown(&input.current_markdown, &input.title, &raw)?;
    if state.task_service.is_cancelled(task_id) {
        return Err(source_ai_cancelled());
    }
    let candidate = state.import_v2_service.store_source_ai_organize_candidate(
        context,
        &state.file_store,
        &input,
        task_id,
        candidate_route,
        engine,
        model,
        engine_version,
        candidate_markdown,
    )?;
    let candidate_path = format!(
        ".app/source-candidates/{}/{}.json",
        input.source_id, candidate.candidate_id
    );
    if let Err(error) = state.task_service.complete_running_with_result(
        task_id,
        TaskResult {
            summary: "Source AI candidate is ready for Diff review.".into(),
            affected_paths: vec![candidate_path],
            reference: Some(TaskResultReference::SourceAiOrganize {
                source_id: input.source_id.clone(),
                base_version_id: input.version_id.clone(),
                base_markdown_hash: input.markdown_hash.clone(),
                candidate_id: Some(candidate.candidate_id.clone()),
                route: Some(recovery.route),
                agent: recovery.agent,
                provider: recovery.provider,
                custom_instructions: recovery.custom_instructions,
                project_root_path: Some(recovery.project_root_path),
                resolved_engine: Some(recovery.resolved_engine),
                resolved_model: Some(recovery.resolved_model),
            }),
            pending_action: None,
        },
    ) {
        let _ = state
            .import_v2_service
            .discard_source_ai_organize_candidate(
                context,
                &state.file_store,
                &input.source_id,
                &candidate.candidate_id,
                task_id,
            );
        return Err(source_task_error(error));
    }
    Ok(())
}

fn resolve_source_ai_route(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    preference: CompileRoutePreference,
    explicit_agent: Option<AgentKind>,
    explicit_provider: Option<LlmProviderKind>,
) -> Result<ResolvedSourceAiRoute, BackendError> {
    let resolve_agent = || -> Result<Option<ResolvedSourceAiRoute>, BackendError> {
        let config = AgentService::load_config(context)?;
        let selected_agent = explicit_agent.or(config.default_agent);
        Ok(selected_agent
            .filter(|kind| AgentService::supports_source_ai_agent(*kind))
            .and_then(|kind| {
                let info = state
                    .agent_service
                    .detect_agent(kind, config.default_agent == Some(kind));
                (info.state == AgentDetectionState::Installed).then(|| {
                    ResolvedSourceAiRoute::Agent {
                        kind: info.kind,
                        version: info.version.unwrap_or_else(|| "default".into()),
                    }
                })
            }))
    };
    let resolve_provider = || -> Result<Option<ResolvedSourceAiRoute>, BackendError> {
        Ok(select_source_ai_provider(
            explicit_provider,
            &LlmService::list_providers(context)?,
            &state.secret_service,
        )?
        .map(ResolvedSourceAiRoute::Byok))
    };
    match preference {
        CompileRoutePreference::Agent => resolve_agent()?.ok_or_else(agent_unavailable),
        CompileRoutePreference::Byok => resolve_provider()?.ok_or_else(provider_unavailable),
        CompileRoutePreference::Auto => resolve_agent()?
            .map(Ok)
            .unwrap_or_else(|| resolve_provider()?.ok_or_else(provider_unavailable)),
    }
}

fn select_source_ai_provider(
    explicit: Option<LlmProviderKind>,
    providers: &[LlmProviderConfig],
    secrets: &crate::services::SecretService,
) -> Result<Option<LlmProviderConfig>, BackendError> {
    if let Some(kind) = explicit {
        let provider = providers
            .iter()
            .find(|provider| provider.enabled && provider.provider == kind)
            .cloned()
            .ok_or_else(provider_unavailable)?;
        if provider.provider.requires_secret() && secrets.get(provider.provider)?.is_none() {
            return Err(BackendError::new(
                "LLM_SECRET_MISSING",
                "The selected BYOK provider has no configured secret.",
                true,
                true,
            ));
        }
        return Ok(Some(provider));
    }
    for provider in providers.iter().filter(|provider| provider.enabled) {
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

fn provider_name(provider: LlmProviderKind) -> &'static str {
    match provider {
        LlmProviderKind::OpenAi => "open_ai",
        LlmProviderKind::Anthropic => "anthropic",
        LlmProviderKind::Google => "google",
        LlmProviderKind::Ollama => "ollama",
        LlmProviderKind::Custom => "custom",
    }
}

fn agent_unavailable() -> BackendError {
    BackendError::new(
        "AGENT_UNAVAILABLE",
        "No installed Agent is available for Source AI organization.",
        true,
        true,
    )
}

fn provider_unavailable() -> BackendError {
    BackendError::new(
        "LLM_PROVIDER_MISSING",
        "No enabled BYOK provider with a configured secret is available.",
        true,
        true,
    )
}

fn source_ai_recovery_unavailable() -> BackendError {
    BackendError::new(
        "SOURCE_AI_RECOVERY_UNAVAILABLE",
        "This failed Source AI run cannot be retried from its saved inputs.",
        true,
        true,
    )
}

fn source_ai_recovery_engine_changed() -> BackendError {
    BackendError::new(
        "SOURCE_AI_RECOVERY_ENGINE_CHANGED",
        "The saved Source AI route is no longer available. Start a new run after reviewing the current settings.",
        true,
        true,
    )
}

fn source_ai_cancelled() -> BackendError {
    BackendError::new(
        "SOURCE_AI_CANCELLED",
        "Source AI organization was cancelled.",
        true,
        false,
    )
}

#[tauri::command]
pub fn reprocess_source_ocr(
    state: State<'_, AppState>,
    request: ReprocessSourceRequest,
) -> Result<SourceCandidateSummary, BackendError> {
    reprocess(&state, request, SourceCandidateKind::Ocr)
}

#[tauri::command]
pub fn reprocess_source_asr(
    state: State<'_, AppState>,
    request: ReprocessSourceRequest,
) -> Result<SourceCandidateSummary, BackendError> {
    reprocess(&state, request, SourceCandidateKind::Asr)
}

#[tauri::command]
pub fn reprocess_source_subtitle(
    state: State<'_, AppState>,
    request: ReprocessSourceRequest,
) -> Result<SourceCandidateSummary, BackendError> {
    reprocess(&state, request, SourceCandidateKind::Subtitle)
}

#[tauri::command]
pub fn refresh_source(
    state: State<'_, AppState>,
    request: ReprocessSourceRequest,
) -> Result<SourceCandidateSummary, BackendError> {
    reprocess(&state, request, SourceCandidateKind::Refresh)
}

fn reprocess(
    state: &State<'_, AppState>,
    request: ReprocessSourceRequest,
    kind: SourceCandidateKind,
) -> Result<SourceCandidateSummary, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            context.root.clone(),
            format!("Reprocess Source ({kind:?})"),
            true,
        )
        .map_err(source_task_error)?;
    state
        .task_service
        .transition_status(&task.id, TaskStatus::Running)
        .map_err(source_task_error)?;
    let cancellation = state
        .task_service
        .get_cancellation_token(&task.id)
        .ok_or_else(|| source_task_error("Source processing task token is unavailable."))?;
    let result = state.import_v2_service.reprocess_source(
        &context,
        &state.file_store,
        &request,
        kind,
        &cancellation,
    );
    match result {
        Ok(candidate) => {
            state
                .task_service
                .complete_running_with_result(
                    &task.id,
                    TaskResult {
                        summary: "Source candidate is ready for Diff review.".into(),
                        affected_paths: vec![format!(
                            ".app/source-candidates/{}/{}.json",
                            request.source_id, candidate.candidate_id
                        )],
                        reference: None,
                        pending_action: None,
                    },
                )
                .map_err(source_task_error)?;
            Ok(candidate)
        }
        Err(error) => {
            if cancellation.is_cancelled() {
                let _ = state
                    .task_service
                    .transition_status(&task.id, TaskStatus::Cancelled);
            } else {
                let _ = state.task_service.set_error(&task.id, error.clone());
                let _ = state
                    .task_service
                    .transition_status(&task.id, TaskStatus::Failed);
            }
            Err(error)
        }
    }
}

fn source_task_error(error: impl ToString) -> BackendError {
    BackendError::new(
        "SOURCE_TASK_FAILED",
        "The Source processing task could not be updated.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "error": error.to_string() }))
}

#[tauri::command]
pub fn preview_move_source(
    state: State<'_, AppState>,
    request: PreviewMoveSourceRequest,
) -> Result<MoveSourcePreview, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .preview_move_source(&context, &state.file_store, &request)
}

#[tauri::command]
pub fn move_source(
    state: State<'_, AppState>,
    request: MoveSourceRequest,
) -> Result<SourceMutationResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .move_source(&context, &state.file_store, &state.git_service, &request)
}

#[tauri::command]
pub fn preview_delete_source(
    state: State<'_, AppState>,
    request: PreviewDeleteSourceRequest,
) -> Result<DeleteSourcePreview, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .preview_delete_source(&context, &state.file_store, &request)
}

#[tauri::command]
pub fn delete_source(
    state: State<'_, AppState>,
    request: DeleteSourceRequest,
) -> Result<SourceMutationResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .delete_source(&context, &state.file_store, &state.git_service, &request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_source_ai_task(
        code: &str,
        recoverable: bool,
        candidate_id: Option<String>,
    ) -> BackendTask {
        BackendTask {
            id: "task-1".into(),
            task_type: TaskType::SourceAiOrganize,
            project_id: Some("project-1".into()),
            batch_id: None,
            operation: None,
            title: "Source AI".into(),
            status: TaskStatus::Failed,
            progress: None,
            started_at: "2026-07-28T00:00:00Z".into(),
            updated_at: "2026-07-28T00:01:00Z".into(),
            completed_at: Some("2026-07-28T00:01:00Z".into()),
            cancellable: true,
            log_path: None,
            result: Some(TaskResult {
                summary: "Source AI organization failed.".into(),
                affected_paths: Vec::new(),
                reference: Some(TaskResultReference::SourceAiOrganize {
                    source_id: "source-1".into(),
                    base_version_id: "version-1".into(),
                    base_markdown_hash: "hash-1".into(),
                    candidate_id,
                    route: Some(CompileRoutePreference::Agent),
                    agent: Some(AgentKind::Claude),
                    provider: None,
                    custom_instructions: None,
                    project_root_path: Some("C:/wiki".into()),
                    resolved_engine: Some("claude".into()),
                    resolved_model: Some("cli-default".into()),
                }),
                pending_action: None,
            }),
            error: Some(BackendError::new(code, "failed", recoverable, false)),
        }
    }

    #[test]
    fn source_ai_retry_accepts_recovery_and_ordinary_recoverable_failures() {
        for code in [
            "TASK_RECOVERY",
            "SOURCE_AI_OUTPUT_INVALID",
            "SOURCE_AI_OVERVIEW_INVALID",
            "AGENT_EXIT_FAILED",
        ] {
            let task = failed_source_ai_task(code, true, None);
            assert!(
                is_retryable_source_ai_task(&task, "project-1"),
                "{code} should be explicitly retryable"
            );
        }
    }

    #[test]
    fn source_ai_retry_rejects_nonrecoverable_or_candidate_bound_failures() {
        let nonrecoverable = failed_source_ai_task("SOURCE_INVALID", false, None);
        assert!(!is_retryable_source_ai_task(&nonrecoverable, "project-1"));

        let candidate_bound =
            failed_source_ai_task("LLM_RESPONSE_INVALID", true, Some("candidate-1".into()));
        assert!(!is_retryable_source_ai_task(&candidate_bound, "project-1"));

        let wrong_project = failed_source_ai_task("LLM_RESPONSE_INVALID", true, None);
        assert!(!is_retryable_source_ai_task(&wrong_project, "project-2"));
    }

    #[test]
    fn source_ai_retry_identity_pins_agent_kind_not_cli_version() {
        let before = resolved_source_ai_identity(&ResolvedSourceAiRoute::Agent {
            kind: AgentKind::Codex,
            version: "1.2.3".into(),
        });
        let after_update = resolved_source_ai_identity(&ResolvedSourceAiRoute::Agent {
            kind: AgentKind::Codex,
            version: "1.2.4".into(),
        });
        let different_agent = resolved_source_ai_identity(&ResolvedSourceAiRoute::Agent {
            kind: AgentKind::Claude,
            version: "1.2.4".into(),
        });

        assert_eq!(before, ("codex".into(), "cli-default".into()));
        assert_eq!(before, after_update);
        assert_ne!(before, different_agent);
    }
}

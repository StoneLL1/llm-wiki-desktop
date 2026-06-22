use std::collections::HashMap;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::AgentDetectionState;
use crate::models::compile::{
    CompileManifest, CompileRequest, CompileRoute, CompileRoutePreference,
};
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::{AgentService, CompileService, LlmService};
use crate::tasks::task_model::LogLevel;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmCompileRequest {
    pub action_id: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCompileConflictRequest {
    pub action_id: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileConflictDetail {
    pub path: String,
    pub current_content: Option<String>,
    pub generated_content: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveCompileConflictRequest {
    pub action_id: String,
    pub resolution: crate::models::compile::CompileConflictResolution,
    #[serde(default)]
    pub manual_files: Vec<crate::models::compile::CompileFile>,
}

#[tauri::command]
pub fn start_wiki_compile(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CompileRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::WikiCompile,
            request.project_id.clone(),
            context.root.clone(),
            "Compile Wiki".into(),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_compile(&state, &request, &context, &task_id).await {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            if !matches!(
                state
                    .task_service
                    .get_task(&task_id)
                    .map(|task| task.status),
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

async fn run_compile(
    state: &AppState,
    request: &CompileRequest,
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
            "Validating extracted Markdown".into(),
        )
        .map_err(task_error)?;
    CompileService::extracted_markdown_files(context)?;
    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Creating Git checkpoint".into())
        .map_err(task_error)?;
    let checkpoint = state.git_service.create_checkpoint(
        context,
        CheckpointPurpose::HighRiskOperation,
        "Before wiki compile",
    )?;
    let baseline = CompileService::snapshot_wiki(context)?;
    let workspace = CompileService::create_workspace(context, task_id)?;
    let outcome = async {
        let (route, manifest) = generate_manifest(state, request, context, &workspace, task_id, &baseline).await?;
        ensure_checkpoint_head(state, context, checkpoint.commit_hash.as_deref())?;
        if state.task_service.is_cancelled(task_id) {
            return Err(BackendError::new("COMPILE_CANCELLED", "Wiki compile was cancelled.", true, false));
        }
        let backup = CompileService::backup_outputs(context, &manifest)?;
        let applied = match CompileService::apply_manifest(context, &manifest, &baseline) {
            Ok(applied) => applied,
            Err(error) => {
                let _ = CompileService::restore_outputs(context, &backup);
                return Err(error);
            }
        };
        if !applied.conflicts.is_empty() {
            let current_hashes = manifest.files.iter().map(|file| file.path.as_str()).chain(manifest.deletions.iter().map(String::as_str)).filter_map(|path| state.file_store.file_hash(context, path).ok().map(|hash| (path.to_string(), hash))).collect();
            let action = PendingAction {
                id: uuid::Uuid::new_v4().to_string(),
                action_type: PendingActionType::MergeConflict,
                title: "Review generated Wiki conflicts".into(),
                message: "Generated pages conflict with external edits or require deletion. Confirm before overwriting.".into(),
                risk_level: RiskLevel::High,
                affected_paths: applied.conflicts.clone(),
                preview: Some(ActionPreview { summary: format!("{} conflicting path(s)", applied.conflicts.len()), before: None, after: None, diff: Some(CompileService::candidate_diff(&manifest)) }),
                expires_at: None,
                // The compile checkpoint was created before the manifest was
                // generated; surface its hash so the frontend can show an
                // honest "Checkpoint: available" state for this conflict.
                checkpoint_hash: checkpoint.commit_hash.clone(),
            };
            state.confirmation_registry.register_with_execution(action.clone(), Some(ConfirmationExecution::CompileMerge {
                project_id: request.project_id.clone(), root_path: request.project_root_path.clone(), task_id: task_id.into(), route,
                manifest, current_hashes, checkpoint_hash: checkpoint.commit_hash.clone(),
            }))?;
            state.task_service.set_result(task_id, TaskResult { summary: "Compile requires conflict confirmation.".into(), affected_paths: applied.affected_paths, pending_action: Some(action) }).map_err(task_error)?;
            state.task_service.transition_status(task_id, TaskStatus::WaitingForConfirmation).map_err(task_error)?;
            return Ok(());
        }
        if let Err(error) = finish_compile(state, context, task_id, route, applied.affected_paths, checkpoint.commit_hash) {
            let _ = state
                .git_service
                .unstage_paths(context, &compile_output_paths(&manifest));
            CompileService::restore_outputs(context, &backup)?;
            return Err(error);
        }
        Ok(())
    }.await;
    let _ = std::fs::remove_dir_all(&workspace);
    outcome
}

async fn generate_manifest(
    state: &AppState,
    request: &CompileRequest,
    context: &ProjectContext,
    workspace: &std::path::Path,
    task_id: &str,
    baseline: &HashMap<String, String>,
) -> Result<(CompileRoute, CompileManifest), BackendError> {
    let agent_config = AgentService::load_config(context)?;
    let providers = LlmService::list_providers(context)?;
    let selected_agent = request.agent.or(agent_config.default_agent);
    let usable_agent = selected_agent.filter(|agent| {
        state
            .agent_service
            .detect_agents(Some(*agent))
            .iter()
            .any(|info| info.kind == *agent && info.state == AgentDetectionState::Installed)
    });
    let selected_provider = select_provider(request.provider, &providers, &state.secret_service)?;
    let language = state
        .settings_service
        .read_settings(context)
        .map(|settings| settings.language)
        .unwrap_or_else(|_| "en".to_string());
    let route = match request.route {
        CompileRoutePreference::Agent => CompileRoute::Agent,
        CompileRoutePreference::Byok => CompileRoute::Byok,
        CompileRoutePreference::Auto if usable_agent.is_some() => CompileRoute::Agent,
        CompileRoutePreference::Auto => CompileRoute::Byok,
    };
    match route {
        CompileRoute::Agent => {
            let agent = usable_agent.ok_or_else(|| {
                BackendError::new(
                    "AGENT_UNAVAILABLE",
                    "Selected Agent is not available.",
                    true,
                    true,
                )
            })?;
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Running {}", agent.command()),
                )
                .map_err(task_error)?;
            let invocation = AgentService::invocation(
                agent,
                workspace,
                &CompileService::compile_prompt(workspace, &language),
            )?;
            state
                .agent_service
                .run_task_streaming(&invocation, &state.task_service, task_id)?;
            Ok((
                route,
                CompileService::manifest_from_workspace(workspace, baseline.keys().cloned())?,
            ))
        }
        CompileRoute::Byok => {
            let provider = selected_provider.ok_or_else(|| {
                BackendError::new(
                    "LLM_PROVIDER_MISSING",
                    "No enabled BYOK provider is available.",
                    true,
                    true,
                )
            })?;
            let secret = state.secret_service.get(provider.provider)?;
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Calling {:?}", provider.provider),
                )
                .map_err(task_error)?;
            let prompt = CompileService::provider_prompt(workspace, &language)?;
            let completion = state
                .llm_service
                .complete(&provider, secret.as_deref(), &prompt);
            let raw = crate::tasks::byok_progress::poll_with_progress(
                &state.task_service,
                task_id,
                "Generating",
                completion,
            )
            .await
            .map_err(|_| {
                crate::tasks::byok_progress::cancelled_error(
                    "COMPILE_CANCELLED",
                    "Wiki compile was cancelled.",
                )
            })??;
            Ok((route, CompileService::parse_manifest(&raw)?))
        }
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
            .find(|provider| provider.enabled && provider.provider == kind)
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
    for provider in providers.iter().filter(|provider| provider.enabled) {
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

fn finish_compile(
    state: &AppState,
    context: &ProjectContext,
    task_id: &str,
    route: CompileRoute,
    affected_paths: Vec<String>,
    initial_checkpoint: Option<String>,
) -> Result<(), BackendError> {
    let graph_path = context.app_dir.join("graph-cache.json");
    let mut graph_cache = if graph_path.exists() {
        state
            .file_store
            .read_json_file::<serde_json::Value>(&graph_path)?
    } else {
        serde_json::json!({})
    };
    let graph_object = graph_cache.as_object_mut().ok_or_else(|| {
        BackendError::new(
            "GRAPH_CACHE_INVALID",
            "Graph cache must be a JSON object.",
            true,
            false,
        )
    })?;
    graph_object.insert("status".into(), serde_json::Value::String("stale".into()));
    state
        .file_store
        .write_json_atomic(context, ".app/graph-cache.json", &graph_cache)?;
    state.search_service.scan_wiki(context)?;
    let mut checkpoint_paths = affected_paths.clone();
    checkpoint_paths.push(".app/graph-cache.json".into());
    let result_checkpoint = state.git_service.create_scoped_checkpoint(
        context,
        CheckpointPurpose::FinalResult,
        "Compile wiki",
        &checkpoint_paths,
    )?;
    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: format!("Wiki compiled through {:?}.", route),
                affected_paths,
                pending_action: None,
            },
        )
        .map_err(task_error)?;
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            format!(
                "Compile complete. Checkpoints: {:?}, {:?}",
                initial_checkpoint, result_checkpoint.commit_hash
            ),
        )
        .map_err(task_error)?;
    state
        .task_service
        .transition_status(task_id, TaskStatus::Succeeded)
        .map_err(task_error)?;
    Ok(())
}

#[tauri::command]
pub fn get_compile_conflict_details(
    state: State<'_, AppState>,
    request: GetCompileConflictRequest,
) -> Result<Vec<CompileConflictDetail>, BackendError> {
    let stored = state.confirmation_registry.peek(&request.action_id)?;
    let ConfirmationExecution::CompileMerge {
        project_id,
        root_path,
        manifest,
        ..
    } = stored.execution.ok_or_else(|| {
        BackendError::new(
            "CONFIRMATION_EXECUTION_MISSING",
            "Compile confirmation has no execution plan.",
            false,
            true,
        )
    })?
    else {
        return Err(BackendError::new(
            "CONFIRMATION_TYPE_INVALID",
            "Confirmation is not a compile action.",
            false,
            true,
        ));
    };
    let context = state.resolve_project_context(&project_id, &root_path)?;
    let generated: HashMap<&str, &str> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.content.as_str()))
        .collect();
    stored
        .action
        .affected_paths
        .into_iter()
        .map(|path| {
            let absolute = context.resolve_project_path(&path)?;
            let current_content = if absolute.exists() {
                Some(state.file_store.read_markdown(&context, &path)?)
            } else {
                None
            };
            Ok(CompileConflictDetail {
                generated_content: generated
                    .get(path.as_str())
                    .map(|value| (*value).to_string()),
                path,
                current_content,
            })
        })
        .collect()
}

#[tauri::command]
pub fn resolve_compile_conflict(
    state: State<'_, AppState>,
    request: ResolveCompileConflictRequest,
) -> Result<BackendTask, BackendError> {
    use crate::models::confirmation::ConfirmationStatus;
    let stored = state.confirmation_registry.peek(&request.action_id)?;
    let conflict_paths = stored.action.affected_paths.clone();
    let ConfirmationExecution::CompileMerge {
        project_id,
        root_path,
        task_id,
        route,
        manifest,
        current_hashes,
        checkpoint_hash,
    } = stored.execution.ok_or_else(|| {
        BackendError::new(
            "CONFIRMATION_EXECUTION_MISSING",
            "Compile confirmation has no execution plan.",
            false,
            true,
        )
    })?
    else {
        return Err(BackendError::new(
            "CONFIRMATION_TYPE_INVALID",
            "Confirmation is not a compile action.",
            false,
            true,
        ));
    };
    let task = state.task_service.get_task(&task_id).ok_or_else(|| {
        BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false)
    })?;
    if task.status != TaskStatus::WaitingForConfirmation {
        return Err(BackendError::new(
            "CONFIRMATION_STATE_MISMATCH",
            "Compile task is no longer waiting for confirmation.",
            true,
            true,
        ));
    }
    let context = state.resolve_project_context(&project_id, &root_path)?;
    ensure_checkpoint_head(&state, &context, checkpoint_hash.as_deref())?;
    let resolved_manifest = CompileService::resolve_conflict_manifest(
        &manifest,
        &conflict_paths,
        request.resolution,
        &request.manual_files,
    )?;
    state
        .confirmation_registry
        .confirm(&request.action_id, ConfirmationStatus::Confirmed)?;
    state
        .task_service
        .transition_status(&task_id, TaskStatus::Running)
        .map_err(task_error)?;
    let hashes: HashMap<String, String> = current_hashes.into_iter().collect();
    let backup = CompileService::backup_outputs(&context, &resolved_manifest)?;
    let affected_paths =
        match CompileService::apply_confirmed_manifest(&context, &resolved_manifest, &hashes) {
            Ok(paths) => paths,
            Err(error) => {
                let _ = CompileService::restore_outputs(&context, &backup);
                let _ = state.task_service.set_error(&task_id, error.clone());
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
                return Err(error);
            }
        };
    if let Err(error) = finish_compile(&state, &context, &task_id, route, affected_paths, None) {
        let _ = state
            .git_service
            .unstage_paths(&context, &compile_output_paths(&resolved_manifest));
        CompileService::restore_outputs(&context, &backup)?;
        let _ = state.task_service.set_error(&task_id, error.clone());
        let _ = state
            .task_service
            .transition_status(&task_id, TaskStatus::Failed);
        return Err(error);
    }
    state
        .task_service
        .get_task(&task_id)
        .ok_or_else(|| BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false))
}

#[tauri::command]
pub fn confirm_compile_action(
    state: State<'_, AppState>,
    request: ConfirmCompileRequest,
) -> Result<BackendTask, BackendError> {
    use crate::models::confirmation::ConfirmationStatus;
    let status = if request.confirmed {
        ConfirmationStatus::Confirmed
    } else {
        ConfirmationStatus::Cancelled
    };
    let stored = state
        .confirmation_registry
        .confirm(&request.action_id, status)?;
    let ConfirmationExecution::CompileMerge {
        project_id,
        root_path,
        task_id,
        route,
        manifest,
        current_hashes,
        checkpoint_hash,
    } = stored.execution.ok_or_else(|| {
        BackendError::new(
            "CONFIRMATION_EXECUTION_MISSING",
            "Compile confirmation has no execution plan.",
            false,
            true,
        )
    })?
    else {
        return Err(BackendError::new(
            "CONFIRMATION_TYPE_INVALID",
            "Confirmation is not a compile action.",
            false,
            true,
        ));
    };
    if !request.confirmed {
        state
            .task_service
            .cancel_task(&task_id)
            .map_err(task_error)?;
        return state.task_service.get_task(&task_id).ok_or_else(|| {
            BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false)
        });
    }
    let task = state.task_service.get_task(&task_id).ok_or_else(|| {
        BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false)
    })?;
    if task.status != TaskStatus::WaitingForConfirmation {
        return Err(BackendError::new(
            "CONFIRMATION_STATE_MISMATCH",
            "Compile task is no longer waiting for confirmation.",
            true,
            true,
        ));
    }
    state
        .task_service
        .transition_status(&task_id, TaskStatus::Running)
        .map_err(task_error)?;
    let context = state.resolve_project_context(&project_id, &root_path)?;
    if let Err(error) = ensure_checkpoint_head(&state, &context, checkpoint_hash.as_deref()) {
        let _ = state.task_service.set_error(&task_id, error.clone());
        let _ = state
            .task_service
            .transition_status(&task_id, TaskStatus::Failed);
        return Err(error);
    }
    let hashes: HashMap<String, String> = current_hashes.into_iter().collect();
    let backup = CompileService::backup_outputs(&context, &manifest)?;
    let affected_paths =
        match CompileService::apply_confirmed_manifest(&context, &manifest, &hashes) {
            Ok(paths) => paths,
            Err(error) => {
                let _ = CompileService::restore_outputs(&context, &backup);
                let _ = state.task_service.set_error(&task_id, error.clone());
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
                return Err(error);
            }
        };
    if let Err(error) = finish_compile(&state, &context, &task_id, route, affected_paths, None) {
        let _ = state
            .git_service
            .unstage_paths(&context, &compile_output_paths(&manifest));
        CompileService::restore_outputs(&context, &backup)?;
        let _ = state.task_service.set_error(&task_id, error.clone());
        let _ = state
            .task_service
            .transition_status(&task_id, TaskStatus::Failed);
        return Err(error);
    }
    state
        .task_service
        .get_task(&task_id)
        .ok_or_else(|| BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false))
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn ensure_checkpoint_head(
    state: &AppState,
    context: &ProjectContext,
    expected: Option<&str>,
) -> Result<(), BackendError> {
    if state
        .git_service
        .repository_status(context)?
        .head
        .as_deref()
        != expected
    {
        return Err(BackendError::new(
            "COMPILE_CHECKPOINT_CHANGED",
            "The project Git HEAD changed during compilation.",
            true,
            true,
        ));
    }
    Ok(())
}

fn compile_output_paths(manifest: &CompileManifest) -> Vec<String> {
    let mut paths: Vec<String> = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .chain(manifest.deletions.iter().cloned())
        .chain(std::iter::once(".app/graph-cache.json".into()))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

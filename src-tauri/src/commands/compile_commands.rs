use std::collections::HashMap;

use tauri::{AppHandle, Manager, State};

use crate::app_state::{AppState, ProjectWriteRootKind};
use crate::errors::BackendError;
use crate::models::compile::{
    CompileConsumptionRecord, CompileManifest, CompileRequest, CompileResult, CompileRoute,
    CompileRoutePreference, SourceVersionRef,
};
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskResultReference, TaskStatus, TaskType};
use crate::services::import_v2::source_registry::SourceRegistry;
use crate::services::{
    CompileExecutionServices, CompileGenerationPolicy, CompileService,
    NoopCompileGenerationObserver,
};
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

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCompileSourceVersionsRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn list_compile_source_versions(
    state: State<'_, AppState>,
    request: ListCompileSourceVersionsRequest,
) -> Result<Vec<SourceVersionRef>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    CompileService::list_source_versions(&context)
}

#[tauri::command]
pub fn start_wiki_compile(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CompileRequest,
) -> Result<BackendTask, BackendError> {
    let (context, task) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            state.require_project_content_write_root(context, ProjectWriteRootKind::Source)?;
            state.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)?;
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
            Ok((context.clone(), task))
        },
    )?;
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
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, current| {
            state.require_project_content_write_root(current, ProjectWriteRootKind::Source)?;
            state.require_project_content_write_root(current, ProjectWriteRootKind::Wiki)?;
            state
                .task_service
                .transition_status(task_id, TaskStatus::Running)
                .map_err(task_error)
        },
    )?;
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            "Resolving selected Source versions".into(),
        )
        .map_err(task_error)?;
    let resolved = CompileService::resolve_source_versions(context, &request.source_versions)?;
    let selected_sources = resolved
        .into_iter()
        .filter(|source| !source.already_consumed)
        .collect::<Vec<_>>();
    if selected_sources.is_empty() {
        return finish_duplicate_compile(state, request, task_id);
    }
    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Creating Git checkpoint".into())
        .map_err(task_error)?;
    let git_cancellation = state
        .task_service
        .get_cancellation_token(task_id)
        .ok_or_else(|| task_error(format!("Task cancellation token is unavailable: {task_id}")))?;
    let (checkpoint, baseline, workspace, protected_sources) = state
        .with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |_permit, current| {
                let checkpoint =
                    state
                        .git_service
                        .with_task_cancellation(git_cancellation, || {
                            state.git_service.create_checkpoint(
                                current,
                                CheckpointPurpose::HighRiskOperation,
                                "Before wiki compile",
                            )
                        })?;
                let baseline = CompileService::snapshot_wiki(current)?;
                let workspace = CompileService::create_workspace_for_sources(
                    current,
                    task_id,
                    &selected_sources,
                )?;
                let protected_sources = CompileService::snapshot_workspace_sources(&workspace)?;
                Ok((checkpoint, baseline, workspace, protected_sources))
            },
        )?;
    let outcome = async {
        let services = CompileExecutionServices {
            agent_service: &state.agent_service,
            llm_service: &state.llm_service,
            secret_service: &state.secret_service,
            settings_service: &state.settings_service,
            task_service: &state.task_service,
        };
        let concrete_route = CompileService::resolve_legacy_route(
            context,
            request.route,
            request.agent,
            request.provider,
            &services,
        )?;
        let execution_lease = state.begin_project_external_task(context, task_id)?;
        let mut observer = NoopCompileGenerationObserver;
        let candidate = CompileService::generate_candidate(
            context,
            &workspace,
            task_id,
            &baseline,
            &selected_sources,
            &protected_sources,
            concrete_route,
            CompileGenerationPolicy::LegacyNoDeletes,
            &services,
            &mut observer,
        )
        .await?;
        let route = candidate.route.legacy_kind();
        let plan = candidate.plan;
        let manifest = candidate.manifest;
        state.require_current_execution_epoch(context, &execution_lease)?;
        state.with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |_permit, context| {
        ensure_checkpoint_head(state, context, task_id, checkpoint.commit_hash.as_deref())?;
        if state.task_service.is_cancelled(task_id) {
            return Err(BackendError::new("COMPILE_CANCELLED", "Wiki compile was cancelled.", true, false));
        }
        let source_versions = selected_sources
            .iter()
            .map(|source| source.reference.clone())
            .collect::<Vec<_>>();
        let revalidated = CompileService::resolve_source_versions(context, &source_versions)?;
        if revalidated.iter().any(|source| source.already_consumed) {
            return Err(BackendError::new(
                "COMPILE_SOURCE_VERSION_STALE",
                "A selected Source version was consumed while Compile was running.",
                true,
                true,
            ));
        }
        let backup = CompileService::backup_outputs(context, &manifest)?;
        let applied = match CompileService::apply_manifest(context, &manifest, Some(&plan), &baseline) {
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
                plan, manifest, source_versions,
                current_hashes, checkpoint_hash: checkpoint.commit_hash.clone(),
            }))?;
            state.task_service.set_result(task_id, TaskResult { summary: "Compile requires conflict confirmation.".into(), affected_paths: applied.affected_paths, reference: None, pending_action: Some(action) }).map_err(task_error)?;
            state.task_service.transition_status(task_id, TaskStatus::WaitingForConfirmation).map_err(task_error)?;
            return Ok(());
        }
        if let Err(failure) = finish_compile(
            state,
            context,
            task_id,
            route,
            applied.affected_paths,
            checkpoint.commit_hash,
            &source_versions,
        ) {
            if !failure.durable {
                let _ = state
                    .git_service
                    .unstage_paths(context, &compile_output_paths(&manifest));
                CompileService::restore_outputs(context, &backup)?;
            }
            return Err(failure.error);
        }
        Ok(())
            },
        )
    }.await;
    let _ = std::fs::remove_dir_all(&workspace);
    outcome
}

struct FinishCompileFailure {
    error: BackendError,
    durable: bool,
}

impl FinishCompileFailure {
    fn reversible(error: BackendError) -> Self {
        Self {
            error,
            durable: false,
        }
    }

    fn durable(error: BackendError) -> Self {
        Self {
            error,
            durable: true,
        }
    }
}

fn compile_success_result(
    route: CompileRoute,
    result_paths: &[String],
    checkpoint: Option<String>,
    consumed_versions: &[SourceVersionRef],
) -> TaskResult {
    TaskResult {
        summary: format!("Wiki compiled through {:?}.", route),
        affected_paths: result_paths.to_vec(),
        reference: Some(TaskResultReference::Compile {
            result: CompileResult {
                route,
                affected_paths: result_paths.to_vec(),
                conflicts: Vec::new(),
                checkpoint,
                consumed_versions: consumed_versions.to_vec(),
            },
        }),
        pending_action: None,
    }
}

fn finish_compile(
    state: &AppState,
    context: &ProjectContext,
    task_id: &str,
    route: CompileRoute,
    affected_paths: Vec<String>,
    initial_checkpoint: Option<String>,
    source_versions: &[SourceVersionRef],
) -> Result<(), FinishCompileFailure> {
    let compile_record_path = format!(".app/compile/{task_id}.json");
    let mut result_paths = affected_paths.clone();
    result_paths.push(compile_record_path.clone());
    state
        .task_service
        .set_result(
            task_id,
            compile_success_result(
                route,
                &result_paths,
                initial_checkpoint.clone(),
                source_versions,
            ),
        )
        .map_err(task_error)
        .map_err(FinishCompileFailure::reversible)?;

    let consumed_at = chrono::Utc::now().to_rfc3339();
    let consumed_versions = SourceRegistry::record_compile_consumption(
        context,
        &state.file_store,
        &CompileConsumptionRecord {
            schema_version: 1,
            compile_task_id: task_id.into(),
            route,
            consumed_at,
            source_versions: source_versions.to_vec(),
            affected_paths: affected_paths.clone(),
            checkpoint: initial_checkpoint.clone(),
        },
    )
    .map_err(FinishCompileFailure::reversible)?;

    // The Compile output and its exact consumed versions are now a coherent
    // durable result. Cache refresh and the final-result checkpoint are
    // best-effort follow-up metadata: a failure must never roll the pages back
    // while leaving consumption recorded.
    let graph_path = context.app_dir.join("graph-cache.json");
    let mut graph_cache = if graph_path.exists() {
        state
            .file_store
            .read_json_file::<serde_json::Value>(&graph_path)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !graph_cache.is_object() {
        graph_cache = serde_json::json!({});
    }
    let graph_object = graph_cache.as_object_mut().expect("object normalized");
    graph_object.insert("status".into(), serde_json::Value::String("stale".into()));
    if let Err(error) =
        state
            .file_store
            .write_json_atomic(context, ".app/graph-cache.json", &graph_cache)
    {
        let _ = state.task_service.append_log(
            task_id,
            LogLevel::Warn,
            format!(
                "Compile completed, but the graph cache could not be marked stale: {}",
                error.message
            ),
        );
    }
    match state.bookmark_service.wiki_page_paths(context) {
        Ok(bookmark_paths) => {
            if let Err(error) = state.search_service.scan_wiki(context, &bookmark_paths) {
                let _ = state.task_service.append_log(
                    task_id,
                    LogLevel::Warn,
                    format!(
                        "Compile completed, but the search cache refresh failed: {}",
                        error.message
                    ),
                );
            }
        }
        Err(error) => {
            let _ = state.task_service.append_log(
                task_id,
                LogLevel::Warn,
                format!(
                    "Compile completed, but bookmarks could not be loaded for search refresh: {}",
                    error.message
                ),
            );
        }
    }
    let mut checkpoint_paths = affected_paths.clone();
    checkpoint_paths.push(".app/graph-cache.json".into());
    checkpoint_paths.push(compile_record_path);
    let source_paths = context
        .layout
        .source_paths()
        .map_err(FinishCompileFailure::reversible)?;
    for reference in source_versions
        .iter()
        .filter(|reference| !reference.source_id.starts_with("legacy-"))
    {
        checkpoint_paths.push(
            source_paths
                .manifest(&reference.source_id)
                .map_err(FinishCompileFailure::reversible)?,
        );
    }
    checkpoint_paths.sort();
    checkpoint_paths.dedup();
    let result_checkpoint = match with_compile_git_cancellation(state, task_id, || {
        state.git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::FinalResult,
            "Compile wiki",
            &checkpoint_paths,
        )
    }) {
        Ok(checkpoint) => checkpoint
            .commit_hash
            .or_else(|| initial_checkpoint.clone()),
        Err(error) => {
            let _ = state.git_service.unstage_paths(context, &checkpoint_paths);
            let _ = state.task_service.append_log(
                task_id,
                LogLevel::Warn,
                format!(
                    "Compile completed, but the final result checkpoint could not be created: {}",
                    error.message
                ),
            );
            initial_checkpoint.clone()
        }
    };
    state
        .task_service
        .set_result(
            task_id,
            compile_success_result(
                route,
                &result_paths,
                result_checkpoint.clone(),
                &consumed_versions,
            ),
        )
        .map_err(task_error)
        .map_err(FinishCompileFailure::durable)?;
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            format!(
                "Compile complete. Checkpoints: {:?}, {:?}",
                initial_checkpoint, result_checkpoint
            ),
        )
        .map_err(task_error)
        .map_err(FinishCompileFailure::durable)?;
    state
        .task_service
        .transition_status(task_id, TaskStatus::Succeeded)
        .map_err(task_error)
        .map_err(FinishCompileFailure::durable)?;
    Ok(())
}

fn finish_duplicate_compile(
    state: &AppState,
    request: &CompileRequest,
    task_id: &str,
) -> Result<(), BackendError> {
    let route = match request.route {
        CompileRoutePreference::Agent => CompileRoute::Agent,
        CompileRoutePreference::Auto | CompileRoutePreference::Byok => CompileRoute::Byok,
    };
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            "All selected Source versions were already consumed; no Wiki update was run.".into(),
        )
        .map_err(task_error)?;
    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: "Selected Source versions were already used to update the Wiki.".into(),
                affected_paths: Vec::new(),
                reference: Some(TaskResultReference::Compile {
                    result: CompileResult {
                        route,
                        affected_paths: Vec::new(),
                        conflicts: Vec::new(),
                        checkpoint: None,
                        consumed_versions: Vec::new(),
                    },
                }),
                pending_action: None,
            },
        )
        .map_err(task_error)?;
    state
        .task_service
        .transition_status(task_id, TaskStatus::Succeeded)
        .map(|_| ())
        .map_err(task_error)
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
        plan,
        manifest,
        source_versions,
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
    state.with_current_project_write_access(&project_id, &root_path, |_permit, context| {
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
        let revalidated = CompileService::resolve_source_versions(context, &source_versions)?;
        if revalidated.iter().any(|source| source.already_consumed) {
            return Err(BackendError::new(
                "COMPILE_SOURCE_VERSION_STALE",
                "A selected Source version was consumed while conflict review was open.",
                true,
                true,
            ));
        }
        ensure_checkpoint_head(&state, context, &task_id, checkpoint_hash.as_deref())?;
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
        let backup = CompileService::backup_outputs(context, &resolved_manifest)?;
        let affected_paths = match CompileService::apply_confirmed_manifest(
            context,
            &resolved_manifest,
            Some(&plan),
            &hashes,
        ) {
            Ok(paths) => paths,
            Err(error) => {
                let _ = CompileService::restore_outputs(context, &backup);
                let _ = state.task_service.set_error(&task_id, error.clone());
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
                return Err(error);
            }
        };
        if let Err(failure) = finish_compile(
            &state,
            context,
            &task_id,
            route,
            affected_paths,
            checkpoint_hash,
            &source_versions,
        ) {
            if !failure.durable {
                let _ = state
                    .git_service
                    .unstage_paths(context, &compile_output_paths(&resolved_manifest));
                CompileService::restore_outputs(context, &backup)?;
            }
            let error = failure.error;
            let _ = state.task_service.set_error(&task_id, error.clone());
            let _ = state
                .task_service
                .transition_status(&task_id, TaskStatus::Failed);
            return Err(error);
        }
        state.task_service.get_task(&task_id).ok_or_else(|| {
            BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false)
        })
    })
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
    let stored = state.confirmation_registry.peek(&request.action_id)?;
    let ConfirmationExecution::CompileMerge {
        project_id,
        root_path,
        task_id,
        route,
        plan,
        manifest,
        source_versions,
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
    state.with_current_project_write_access(&project_id, &root_path, |_permit, context| {
        state
            .confirmation_registry
            .confirm(&request.action_id, status)?;
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
        let revalidated = CompileService::resolve_source_versions(context, &source_versions)?;
        if revalidated.iter().any(|source| source.already_consumed) {
            return Err(BackendError::new(
                "COMPILE_SOURCE_VERSION_STALE",
                "A selected Source version was consumed while confirmation was open.",
                true,
                true,
            ));
        }
        if let Err(error) =
            ensure_checkpoint_head(&state, context, &task_id, checkpoint_hash.as_deref())
        {
            let _ = state.task_service.set_error(&task_id, error.clone());
            let _ = state
                .task_service
                .transition_status(&task_id, TaskStatus::Failed);
            return Err(error);
        }
        let hashes: HashMap<String, String> = current_hashes.into_iter().collect();
        let backup = CompileService::backup_outputs(context, &manifest)?;
        let affected_paths = match CompileService::apply_confirmed_manifest(
            context,
            &manifest,
            Some(&plan),
            &hashes,
        ) {
            Ok(paths) => paths,
            Err(error) => {
                let _ = CompileService::restore_outputs(context, &backup);
                let _ = state.task_service.set_error(&task_id, error.clone());
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
                return Err(error);
            }
        };
        if let Err(failure) = finish_compile(
            &state,
            context,
            &task_id,
            route,
            affected_paths,
            checkpoint_hash,
            &source_versions,
        ) {
            if !failure.durable {
                let _ = state
                    .git_service
                    .unstage_paths(context, &compile_output_paths(&manifest));
                CompileService::restore_outputs(context, &backup)?;
            }
            let error = failure.error;
            let _ = state.task_service.set_error(&task_id, error.clone());
            let _ = state
                .task_service
                .transition_status(&task_id, TaskStatus::Failed);
            return Err(error);
        }
        state.task_service.get_task(&task_id).ok_or_else(|| {
            BackendError::new("TASK_NOT_FOUND", "Compile task not found.", false, false)
        })
    })
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn ensure_checkpoint_head(
    state: &AppState,
    context: &ProjectContext,
    task_id: &str,
    expected: Option<&str>,
) -> Result<(), BackendError> {
    let status = with_compile_git_cancellation(state, task_id, || {
        state.git_service.repository_status(context)
    })?;
    if status.head.as_deref() != expected {
        return Err(BackendError::new(
            "COMPILE_CHECKPOINT_CHANGED",
            "The project Git HEAD changed during compilation.",
            true,
            true,
        ));
    }
    Ok(())
}

fn with_compile_git_cancellation<T>(
    state: &AppState,
    task_id: &str,
    operation: impl FnOnce() -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let token = state
        .task_service
        .get_cancellation_token(task_id)
        .ok_or_else(|| task_error(format!("Task cancellation token is unavailable: {task_id}")))?;
    state.git_service.with_task_cancellation(token, operation)
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

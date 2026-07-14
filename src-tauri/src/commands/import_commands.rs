use std::path::PathBuf;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::import::{ConfirmedImport, ImportPreview};
use crate::models::import_v2::{
    CommitImportSessionRequest, CommitItemDecision, ImportInput, ImportInputKind,
    ImportResourceMode,
};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskResultReference, TaskStatus, TaskType};
use crate::services::import_v2::activation::ImportV2ActivationService;
use crate::services::import_v2::legacy_route::LegacyPreviewAdapter;
use crate::services::import_v2::file_discovery::{new_import_inputs, FileDiscoveryService};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_paths: Vec<String>,
    pub allow_duplicates: bool,
    pub link_duplicates: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmImportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub preview: ImportPreview,
    /// When true, create a scoped Git checkpoint of the archived files plus
    /// `.app/source-index.json` and `.app/import-conflicts.json` after the
    /// import is confirmed, and return its hash in `ConfirmedImport`.
    /// Defaults to false for backward compatibility with existing callers.
    #[serde(default)]
    pub create_checkpoint: bool,
    /// V2 route session selected by the activation-aware import facade.
    /// Legacy callers omit this and keep the pre-cutover request shape.
    #[serde(default)]
    pub v2_session_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetImportPreviewRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractTextRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateUrlRequest {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagedImportKind {
    Clipboard,
    Url,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTextImportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub kind: StagedImportKind,
    pub source_name: String,
    pub content: String,
    pub title: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub source_locator: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchImportUrlRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceActionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListImportedSourcesRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceSourceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub target_path: String,
    pub replacement_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedImportUrl {
    pub url: String,
    pub html: String,
}

#[tauri::command]
pub fn list_imported_sources(
    state: State<'_, AppState>,
    request: ListImportedSourcesRequest,
) -> Result<Vec<crate::models::import::ImportedSource>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_service.list_imported_sources(&context)
}

#[tauri::command]
pub fn request_delete_source(
    state: State<'_, AppState>,
    request: SourceActionRequest,
) -> Result<PendingAction, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    ImportV2ActivationService::legacy_mutation_guard(&context)?;
    state
        .import_service
        .validate_imported_source_path(&context, &request.target_path)?;
    let target_hash = state.file_store.file_hash(&context, &request.target_path)?;
    let index = state
        .import_service
        .read_source_index(&context, &state.file_store)?;
    let artifacts = index
        .sources
        .get(&request.target_path)
        .cloned()
        .ok_or_else(|| {
            BackendError::new(
                "SOURCE_ARTIFACT_INDEX_MISSING",
                "This source predates artifact tracking and cannot be safely deleted automatically.",
                true,
                true,
            )
        })?;
    let action = PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: PendingActionType::DeleteSource,
        title: "Delete original source".to_string(),
        message: "The original source and its indexed extracted artifacts will be deleted after a Git checkpoint.".to_string(),
        risk_level: RiskLevel::Destructive,
        affected_paths: std::iter::once(request.target_path.clone())
            .chain(artifacts.iter().cloned())
            .collect(),
        preview: Some(ActionPreview {
            summary: format!("Delete {} and {} extracted artifact(s).", request.target_path, artifacts.len()),
            before: None,
            after: None,
            diff: None,
        }),
        expires_at: None,
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(ConfirmationExecution::DeleteSource {
            project_id: request.project_id,
            root_path: request.project_root_path,
            target_path: request.target_path,
            target_hash,
            artifacts,
        }),
    )?;
    Ok(action)
}

#[tauri::command]
pub fn request_replace_source(
    state: State<'_, AppState>,
    request: ReplaceSourceRequest,
) -> Result<PendingAction, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _guard = state.import_v2_service.acquire_migration_lock()?;
    ImportV2ActivationService::legacy_mutation_guard(&context)?;
    let target = state
        .import_service
        .validate_imported_source_path(&context, &request.target_path)?;
    let replacement = PathBuf::from(&request.replacement_path);
    if !replacement.is_file()
        || std::fs::symlink_metadata(&replacement)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(BackendError::new(
            "SOURCE_REPLACEMENT_INVALID",
            "The replacement must be an existing regular file and cannot be a symlink.",
            true,
            true,
        ));
    }
    if crate::services::classify_file(&target) != crate::services::classify_file(&replacement) {
        return Err(BackendError::new(
            "SOURCE_REPLACEMENT_TYPE_MISMATCH",
            "Replacement file type must match the archived source type.",
            true,
            true,
        ));
    }
    let index = state
        .import_service
        .read_source_index(&context, &state.file_store)?;
    let old_artifacts = index
        .sources
        .get(&request.target_path)
        .cloned()
        .ok_or_else(|| {
            BackendError::new(
            "SOURCE_ARTIFACT_INDEX_MISSING",
            "This source predates artifact tracking and cannot be safely replaced automatically.",
            true,
            true,
        )
        })?;
    let target_hash = state.file_store.file_hash(&context, &request.target_path)?;
    let replacement_hash = state.import_service.hash_external_file(&replacement)?;
    let extraction = state.extraction_service.extract_text(
        &context,
        &state.file_store,
        &replacement,
        &context.raw_dir.join("extracted"),
    )?;
    let mut new_artifacts = extraction.extracted_assets;
    if let Some(path) = extraction.extracted_text_path {
        new_artifacts.push(path);
    }
    let action = PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: PendingActionType::ReplaceSource,
        title: "Replace original source".to_string(),
        message: "The archived source and its extracted artifacts will be replaced after a Git checkpoint.".to_string(),
        risk_level: RiskLevel::Destructive,
        affected_paths: std::iter::once(request.target_path.clone())
            .chain(old_artifacts.iter().cloned())
            .chain(new_artifacts.iter().cloned())
            .collect(),
        preview: Some(ActionPreview {
            summary: format!("Replace {} with {}.", request.target_path, request.replacement_path),
            before: Some(target_hash.clone()),
            after: Some(replacement_hash.clone()),
            diff: None,
        }),
        expires_at: None,
        checkpoint_hash: None,
    };
    let registration = state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(ConfirmationExecution::ReplaceSource {
            project_id: request.project_id,
            root_path: request.project_root_path,
            target_path: request.target_path,
            target_hash,
            replacement_path: request.replacement_path,
            replacement_hash,
            old_artifacts: old_artifacts.clone(),
            new_artifacts: new_artifacts.clone(),
        }),
    );
    if let Err(error) = registration {
        state.import_service.cleanup_replacement_artifacts(
            &context,
            &old_artifacts,
            &new_artifacts,
        );
        return Err(error);
    }
    Ok(action)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn preview_import(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportPreviewRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    preview_import_v2(app, state, request, context)
}

#[tauri::command]
pub fn get_import_preview(
    state: State<'_, AppState>,
    request: GetImportPreviewRequest,
) -> Result<ImportPreview, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .get_task(&request.task_id)
        .ok_or_else(|| task_error(format!("Task not found: {}", request.task_id)))?;
    if task.project_id.as_deref() != Some(request.project_id.as_str())
        || task.task_type != TaskType::Import
        || task.status != TaskStatus::Succeeded
    {
        return Err(BackendError::new(
            "IMPORT_PREVIEW_TASK_INVALID",
            "The import preview task is not a completed task for this project.",
            true,
            true,
        ));
    }
    if let Some(TaskResultReference::ImportV2SessionPreview { session_id }) = task
        .result
        .as_ref()
        .and_then(|result| result.reference.as_ref())
    {
        let session = state.import_v2_service.load_session(
            &context,
            &state.file_store,
            session_id,
        )?;
        return LegacyPreviewAdapter::from_session(&session);
    }
    state.file_store.read_json(
        &context,
        &format!(".app/import-previews/{}.json", request.task_id),
    )
}

fn preview_import_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportPreviewRequest,
    context: ProjectContext,
) -> Result<BackendTask, BackendError> {
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root,
            "Preview Import V2 sources".into(),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = (|| -> Result<(), BackendError> {
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Running)
                .map_err(task_error)?;
            let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            let session = state.import_v2_service.create_session(
                &context,
                &state.file_store,
                ImportResourceMode::Balanced,
            )?;
            let roots = request
                .source_paths
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let scan = FileDiscoveryService::default().scan(
                &context,
                &roots,
                crate::models::import_v2_file::FileScanPolicy::default(),
                |_| {},
                || state.task_service.is_cancelled(&task_id),
            )?;
            let inputs = new_import_inputs(&session, scan.files);
            if inputs.is_empty() {
                return Err(BackendError::new(
                    "IMPORT_FILE_SCAN_EMPTY",
                    "No supported files were found for Import V2.",
                    true,
                    true,
                ));
            }
            let session = state.import_v2_service.add_inputs(
                &context,
                &state.file_store,
                &session.session_id,
                inputs,
            )?;
            let parent_token = state
                .task_service
                .get_cancellation_token(&task_id)
                .ok_or_else(|| {
                    task_error("Import preview cancellation state is unavailable.".into())
                })?;
            let mut first_error = None;
            for item in &session.items {
                if state.task_service.is_cancelled(&task_id) {
                    return Err(BackendError::new(
                        "IMPORT_V2_CANCELLED",
                        "Import V2 preview was cancelled.",
                        true,
                        false,
                    ));
                }
                let child = state
                    .task_service
                    .create_project_task(
                        TaskType::Import,
                        request.project_id.clone(),
                        context.root.clone(),
                        format!("Import V2 {}", item.input.display_name),
                        true,
                    )
                    .map_err(task_error)?;
                if let Err(error) = run_v2_item_with_parent_cancellation(
                    &state.import_v2_service,
                    &state.file_store,
                    &state.task_service,
                    &context,
                    &session.session_id,
                    &item.item_id,
                    &child.id,
                    &parent_token,
                ) {
                    first_error.get_or_insert(error);
                }
            }
            let session = state.import_v2_service.load_session(
                &context,
                &state.file_store,
                &session.session_id,
            )?;
            let preview = LegacyPreviewAdapter::from_session(&session)?;
            if preview.files.iter().all(|file| {
                matches!(
                    file.extraction_status,
                    crate::models::import::ExtractionStatus::Failed
                        | crate::models::import::ExtractionStatus::Pending
                )
            }) {
                if let Some(error) = first_error {
                    return Err(error);
                }
            }
            state
                .task_service
                .complete_running_with_result(
                    &task_id,
                    TaskResult {
                        summary: "Import V2 preview ready.".into(),
                        affected_paths: vec![format!(
                            ".app/import-sessions/{}/session.json",
                            session.session_id
                        )],
                        reference: Some(TaskResultReference::ImportV2SessionPreview {
                            session_id: session.session_id,
                        }),
                        pending_action: None,
                    },
                )
                .map_err(task_error)?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = state.task_service.set_error(&task_id, error);
            if !state.task_service.is_cancelled(&task_id) {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
            }
        }
    });
    Ok(task)
}

fn preview_text_import_v2(
    state: &AppState,
    request: &PreviewTextImportRequest,
    context: &ProjectContext,
) -> Result<ImportPreview, BackendError> {
    let session = state.import_v2_service.create_session(
        context,
        &state.file_store,
        ImportResourceMode::Balanced,
    )?;
    let session = if matches!(request.kind, StagedImportKind::Url) {
        if let Some(locator) = request.source_locator.as_deref() {
            state.import_v2_service.add_inputs(
                context,
                &state.file_store,
                &session.session_id,
                vec![ImportInput {
                    kind: ImportInputKind::Url,
                    display_name: request.source_name.clone(),
                    locator: locator.to_string(),
                    normalized_locator: None,
                    source_identity: None,
                }],
            )?
        } else {
            state.import_v2_service.add_text_input(
                context,
                &state.file_store,
                &session.session_id,
                &request.source_name,
                &request.content,
            )?
        }
    } else {
        state.import_v2_service.add_text_input(
            context,
            &state.file_store,
            &session.session_id,
            &request.source_name,
            &request.content,
        )?
    };
    let item = session.items.first().ok_or_else(|| {
        BackendError::new(
            "IMPORT_V2_STATE_INVALID",
            "The V2 text input did not create an item.",
            false,
            true,
        )
    })?;
    let child = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root.clone(),
            format!("Import V2 {}", item.input.display_name),
            true,
        )
        .map_err(task_error)?;
    state.import_v2_service.run_item(
        context,
        &state.file_store,
        &state.task_service,
        &session.session_id,
        &item.item_id,
        &child.id,
    )?;
    let session = state.import_v2_service.load_session(
        context,
        &state.file_store,
        &session.session_id,
    )?;
    LegacyPreviewAdapter::from_session(&session)
}

fn confirm_import_v2(
    state: &AppState,
    request: &ConfirmImportRequest,
    context: &ProjectContext,
    session_id: &str,
) -> Result<ConfirmedImport, BackendError> {
    let session = state.import_v2_service.load_session(
        context,
        &state.file_store,
        session_id,
    )?;
    let decisions = session
        .items
        .iter()
        .filter(|item| {
            item.selected
                && matches!(
                    item.status,
                    crate::models::import_v2::ImportItemStatus::PreviewReady
                        | crate::models::import_v2::ImportItemStatus::NeedsMerge
                )
        })
        .map(|item| CommitItemDecision {
            item_id: item.item_id.clone(),
            // The compatibility UI has no explicit V2 conflict action. Keep
            // conflicts unresolved rather than guessing CreateNew/KeepWiki.
            conflict_action: None,
            expected_wiki_hash: None,
        })
        .collect();
    state.import_v2_service.commit_items(
        context,
        &state.file_store,
        &state.git_service,
        &CommitImportSessionRequest {
            project_id: request.project_id.clone(),
            project_root_path: request.project_root_path.clone(),
            session_id: session_id.into(),
            decisions,
        },
    )?;
    Ok(ConfirmedImport {
        preview: request.preview.clone(),
        confirmed_at: chrono::Utc::now().to_rfc3339(),
        checkpoint_hash: None,
    })
}

fn run_v2_item_with_parent_cancellation(
    service: &crate::services::import_v2::ImportV2Service,
    files: &crate::services::FileStore,
    tasks: &crate::tasks::TaskService,
    context: &ProjectContext,
    session_id: &str,
    item_id: &str,
    child_task_id: &str,
    parent_token: &crate::tasks::task_model::CancellationToken,
) -> Result<crate::models::import_v2::ImportItem, BackendError> {
    let child_token = tasks
        .get_cancellation_token(child_task_id)
        .ok_or_else(|| task_error("Import child cancellation state is unavailable.".into()))?;
    let watcher_done = Arc::new(AtomicBool::new(false));
    let watcher_done_ref = Arc::clone(&watcher_done);
    let parent_token = parent_token.clone();
    let child_token_ref = child_token.clone();
    let watcher = thread::spawn(move || {
        while !watcher_done_ref.load(Ordering::SeqCst) {
            if parent_token.is_cancelled() {
                child_token_ref.cancel();
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
    });
    let result = service.run_item(
        context,
        files,
        tasks,
        session_id,
        item_id,
        child_task_id,
    );
    watcher_done.store(true, Ordering::SeqCst);
    let _ = watcher.join();
    result
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

#[tauri::command]
pub fn preview_text_import(
    state: State<'_, AppState>,
    request: PreviewTextImportRequest,
) -> Result<ImportPreview, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    preview_text_import_v2(&state, &request, &context)
}

#[tauri::command]
pub async fn fetch_import_url(
    state: State<'_, AppState>,
    request: FetchImportUrlRequest,
) -> Result<FetchedImportUrl, BackendError> {
    let _context =
        state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let parsed = url::Url::parse(&request.url).map_err(|_| {
        BackendError::new(
            "IMPORT_INVALID_URL",
            "The provided URL is not valid.",
            true,
            true,
        )
    })?;
    if !crate::utils::url_utils::is_safe_remote_url(parsed.as_str()) {
        return Err(BackendError::new(
            "IMPORT_URL_BLOCKED",
            "Local, private-network, and non-HTTP URL targets are blocked.",
            true,
            true,
        ));
    }

    let host = parsed.host_str().ok_or_else(|| {
        BackendError::new("IMPORT_INVALID_URL", "The URL has no host.", true, true)
    })?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            BackendError::new("IMPORT_URL_RESOLVE_FAILED", error.to_string(), true, false)
        })?;
    let public_address = addresses
        .into_iter()
        .find(|address| crate::utils::url_utils::is_public_ip(address.ip()))
        .ok_or_else(|| {
            BackendError::new(
                "IMPORT_URL_BLOCKED",
                "The URL resolves only to a local or private-network address.",
                true,
                true,
            )
        })?;

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none());
    if host.parse::<std::net::IpAddr>().is_err() {
        builder = builder.resolve(host, public_address);
    }
    let response = builder
        .build()
        .map_err(|error| {
            BackendError::new("IMPORT_URL_CLIENT_FAILED", error.to_string(), false, false)
        })?
        .get(parsed.clone())
        .header(reqwest::header::USER_AGENT, "LLM-Wiki-Desktop/0.1")
        .send()
        .await
        .map_err(|error| {
            BackendError::new("IMPORT_URL_FETCH_FAILED", error.to_string(), true, false)
        })?;
    if !response.status().is_success() {
        return Err(BackendError::new(
            "IMPORT_URL_HTTP_ERROR",
            format!("The URL returned HTTP {}.", response.status()),
            true,
            false,
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > 5 * 1024 * 1024)
    {
        return Err(BackendError::new(
            "IMPORT_URL_TOO_LARGE",
            "The URL response exceeds the 5 MB import limit.",
            true,
            false,
        ));
    }
    let bytes = response.bytes().await.map_err(|error| {
        BackendError::new("IMPORT_URL_READ_FAILED", error.to_string(), true, false)
    })?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(BackendError::new(
            "IMPORT_URL_TOO_LARGE",
            "The URL response exceeds the 5 MB import limit.",
            true,
            false,
        ));
    }
    let html = String::from_utf8(bytes.to_vec()).map_err(|_| {
        BackendError::new(
            "IMPORT_URL_ENCODING_UNSUPPORTED",
            "The URL response is not UTF-8 text.",
            true,
            false,
        )
    })?;
    Ok(FetchedImportUrl {
        url: parsed.to_string(),
        html,
    })
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn confirm_import_preview(
    state: State<'_, AppState>,
    request: ConfirmImportRequest,
) -> Result<ConfirmedImport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if let Some(session_id) = request.v2_session_id.as_deref() {
        return confirm_import_v2(&state, &request, &context, session_id);
    }
    Err(BackendError::new(
        "IMPORT_V2_REQUIRED",
        "Import confirmation requires an Import V2 session.",
        false,
        true,
    ))
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn extract_text_preview(
    state: State<'_, AppState>,
    request: ExtractTextRequest,
) -> Result<crate::models::import::ExtractResult, BackendError> {
    let _ = (state, request);
    Err(BackendError::new(
        "IMPORT_V2_REQUIRED",
        "Text extraction is owned by the Import V2 session pipeline.",
        false,
        true,
    ))
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn validate_import_url(
    _state: State<'_, AppState>,
    request: ValidateUrlRequest,
) -> Result<crate::models::import::SourceMetadata, BackendError> {
    if !crate::utils::url_utils::is_valid_url(&request.url) {
        return Err(BackendError::new(
            "IMPORT_INVALID_URL",
            "The provided URL is not valid.",
            true,
            true,
        ));
    }

    Ok(crate::models::import::SourceMetadata {
        title: None,
        author: None,
        created: Some(chrono::Utc::now().to_rfc3339()),
        modified: None,
        page_count: None,
        word_count: None,
        language: None,
    })
}

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::import::{ConfirmedImport, ImportPreview, ImportRequest};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::tasks::task_model::LogLevel;

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
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root.clone(),
            "Preview imported sources".to_string(),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_import_preview(&state, request, &context, &task_id) {
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

fn run_import_preview(
    state: &AppState,
    request: ImportPreviewRequest,
    context: &ProjectContext,
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    let paths = state
        .import_service
        .collect_source_paths(&request.source_paths)?;
    let total = paths.len() as u64;
    let output_dir = context.raw_dir.join("extracted");
    let mut extraction_results = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        if state.task_service.is_cancelled(task_id) {
            return Err(BackendError::new(
                "IMPORT_PREVIEW_CANCELLED",
                "Import preview was cancelled.",
                true,
                false,
            ));
        }
        state
            .task_service
            .update_progress(
                task_id,
                index as u64,
                Some(total),
                Some(format!("Extracting {}", path.to_string_lossy())),
            )
            .map_err(task_error)?;
        let source = path.to_string_lossy().into_owned();
        extraction_results.extend(state.extraction_service.extract_batch(
            context,
            &state.file_store,
            std::slice::from_ref(&source),
            &output_dir,
        ));
    }
    let import_request = ImportRequest {
        source_paths: request.source_paths,
        allow_duplicates: request.allow_duplicates,
        link_duplicates: request.link_duplicates,
    };
    let preview = state.import_service.preview_import(
        context,
        &state.file_store,
        &import_request,
        &extraction_results,
    )?;
    let preview_dir = ".app/import-previews";
    let preview_path = format!("{preview_dir}/{task_id}.json");
    state.file_store.ensure_dir(context, preview_dir)?;
    state
        .file_store
        .write_json_atomic(context, &preview_path, &preview)?;
    state
        .task_service
        .update_progress(
            task_id,
            total,
            Some(total),
            Some("Preview ready".to_string()),
        )
        .map_err(task_error)?;
    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: format!("Previewed {} source files.", preview.summary.total_files),
                affected_paths: vec![preview_path],
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
    state.file_store.read_json(
        &context,
        &format!(".app/import-previews/{}.json", request.task_id),
    )
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
    let extension = match request.kind {
        StagedImportKind::Clipboard => "md",
        StagedImportKind::Url => "url",
    };
    let staged = state.import_service.stage_text_source(
        &context,
        &state.file_store,
        &request.source_name,
        extension,
        &request.content,
    )?;
    let source_path = staged.to_string_lossy().into_owned();
    let output_dir = context.raw_dir.join("extracted");
    let extraction = state.extraction_service.extract_batch(
        &context,
        &state.file_store,
        std::slice::from_ref(&source_path),
        &output_dir,
    );
    let mut preview = state.import_service.preview_import(
        &context,
        &state.file_store,
        &ImportRequest {
            source_paths: vec![source_path],
            allow_duplicates: false,
            link_duplicates: false,
        },
        &extraction,
    )?;
    if let Some(entry) = preview.files.first_mut() {
        if let Some(metadata) = entry.metadata.as_mut() {
            metadata.title = request.title;
            metadata.author = request.author;
        }
    }
    Ok(preview)
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

    state
        .import_service
        .confirm_import(&context, &state.file_store, &request.preview)?;

    state
        .file_store
        .write_json_atomic(&context, ".app/import-conflicts.json", &request.preview)
        .map_err(|err| {
            BackendError::new("IMPORT_CONFLICT_WRITE_FAILED", err.message, true, false)
        })?;

    let checkpoint_hash = if request.create_checkpoint {
        state.create_import_checkpoint(&context, &request.preview)?
    } else {
        None
    };

    let confirmed_at = chrono::Utc::now().to_rfc3339();
    Ok(ConfirmedImport {
        preview: request.preview,
        confirmed_at,
        checkpoint_hash,
    })
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn extract_text_preview(
    state: State<'_, AppState>,
    request: ExtractTextRequest,
) -> Result<crate::models::import::ExtractResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;

    let output_dir = context.raw_dir.join("extracted");
    let path = PathBuf::from(&request.source_path);
    state
        .extraction_service
        .extract_text(&context, &state.file_store, &path, &output_dir)
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

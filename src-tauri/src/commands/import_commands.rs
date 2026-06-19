use std::path::PathBuf;

use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import::{ConfirmedImport, ImportPreview, ImportRequest};
use crate::models::paths::ProjectContext;

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

#[tauri::command]
#[allow(non_snake_case)]
pub fn preview_import(
    state: State<'_, AppState>,
    request: ImportPreviewRequest,
) -> Result<ImportPreview, BackendError> {
    let context = ProjectContext::new(
        request.project_id.clone(),
        PathBuf::from(&request.project_root_path),
    );

    let import_req = ImportRequest {
        source_paths: request.source_paths.clone(),
        allow_duplicates: request.allow_duplicates,
        link_duplicates: request.link_duplicates,
    };

    let output_dir = context.raw_dir.join("extracted");
    let extraction_results = state.extraction_service.extract_batch(
        &context,
        &state.file_store,
        &request.source_paths,
        &output_dir,
    );

    state.import_service.preview_import(
        &context,
        &state.file_store,
        &import_req,
        &extraction_results,
    )
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn confirm_import_preview(
    state: State<'_, AppState>,
    request: ConfirmImportRequest,
) -> Result<ConfirmedImport, BackendError> {
    let context = ProjectContext::new(
        request.project_id,
        PathBuf::from(&request.project_root_path),
    );

    state
        .import_service
        .confirm_import(&context, &state.file_store, &request.preview)?;

    state
        .file_store
        .write_json_atomic(&context, ".app/import-conflicts.json", &request.preview)
        .map_err(|err| {
            BackendError::new("IMPORT_CONFLICT_WRITE_FAILED", err.message, true, false)
        })?;

    let confirmed_at = chrono::Utc::now().to_rfc3339();
    Ok(ConfirmedImport {
        preview: request.preview,
        confirmed_at,
    })
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn extract_text_preview(
    state: State<'_, AppState>,
    request: ExtractTextRequest,
) -> Result<crate::models::import::ExtractResult, BackendError> {
    let context = ProjectContext::new(
        request.project_id,
        PathBuf::from(&request.project_root_path),
    );

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

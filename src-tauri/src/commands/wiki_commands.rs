use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::wiki::{
    ReadWikiPageRequest, SaveWikiPageRequest, SaveWikiPageResponse, ToggleBookmarkRequest,
    ToggleBookmarkResponse, WikiPageContent, WikiTree,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanWikiRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn scan_wiki(
    state: State<'_, AppState>,
    request: ScanWikiRequest,
) -> Result<WikiTree, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.search_service.scan_wiki(&context)
}

#[tauri::command]
pub fn read_wiki_page(
    state: State<'_, AppState>,
    request: ReadWikiPageRequest,
) -> Result<WikiPageContent, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .search_service
        .read_page(&context, &request.relative_path)
}

#[tauri::command]
pub fn save_wiki_page(
    state: State<'_, AppState>,
    request: SaveWikiPageRequest,
) -> Result<SaveWikiPageResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.search_service.save_page(
        &context,
        &request.relative_path,
        &request.contents,
        request.expected_hash,
    )
}

#[tauri::command]
pub fn toggle_bookmark(
    state: State<'_, AppState>,
    request: ToggleBookmarkRequest,
) -> Result<ToggleBookmarkResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .search_service
        .toggle_bookmark(&context, &request.relative_path)
}

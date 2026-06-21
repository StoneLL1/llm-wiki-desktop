use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::search::{SearchRequest, SearchResponse};

/// Full-text wiki search (keyword + type/tag/source filters, scoring, snippet).
///
/// This is the backend for the shell-level global search (top-bar searchbar,
/// ⌘K). The wiki view's tree filter is a separate, client-side filter over the
/// already-scanned page list and does not call this command. The top bar calls
/// this command directly and opens the selected result in the wiki view.
#[tauri::command]
pub fn search_wiki(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<SearchResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.search_service.search(&context, &request)
}

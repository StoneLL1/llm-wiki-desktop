use std::path::PathBuf;

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::search::{SearchRequest, SearchResponse};

/// Full-text wiki search (keyword + type/tag/source filters, scoring, snippet).
///
/// This is the backend for the shell-level global search (top-bar searchbar,
/// ⌘K). The wiki view's tree filter is a separate, client-side filter over the
/// already-scanned page list and does not call this command. UI wiring for the
/// global search is deferred to the dedicated search task; the command is
/// exposed now so the backend contract is stable.
#[tauri::command]
pub fn search_wiki(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<SearchResponse, BackendError> {
    let context = context_from_request(&request.project_id, &request.project_root_path);
    state.search_service.search(&context, &request)
}

fn context_from_request(project_id: &str, root_path: &str) -> ProjectContext {
    ProjectContext::new(project_id.to_string(), PathBuf::from(root_path))
}

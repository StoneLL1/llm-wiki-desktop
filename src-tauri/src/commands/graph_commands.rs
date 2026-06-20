use std::path::PathBuf;

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::graph::{GraphBuildResult, GraphData, GraphRequest, SaveGraphLayoutRequest};
use crate::models::paths::ProjectContext;

/// Read the graph for a project. Serves the cached topology when the live wiki
/// content hash matches; otherwise scans the wiki, rebuilds, and persists.
#[tauri::command]
pub fn get_graph(
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    let context = context_from_request(&request);
    let tree = state.search_service.scan_wiki(&context)?;
    state.graph_service.resolve(&context, &tree.pages)
}

/// Force a full rebuild and persist, ignoring any existing cache.
#[tauri::command]
pub fn build_graph(
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    let context = context_from_request(&request);
    let tree = state.search_service.scan_wiki(&context)?;
    state.graph_service.rebuild(&context, &tree.pages)
}

/// Persist a frontend-computed ForceAtlas2/Louvain layout. Returns the updated
/// graph when the cache exists and its content hash matches the request; returns
/// `None` (no-op) when the cache is stale or missing.
#[tauri::command]
pub fn save_graph_layout(
    state: State<'_, AppState>,
    request: SaveGraphLayoutRequest,
) -> Result<Option<GraphData>, BackendError> {
    let context = context_from_request(&GraphRequest {
        project_id: request.project_id.clone(),
        project_root_path: request.project_root_path.clone(),
    });
    state.graph_service.save_layout(&context, request)
}

fn context_from_request(request: &GraphRequest) -> ProjectContext {
    ProjectContext::new(
        request.project_id.clone(),
        PathBuf::from(&request.project_root_path),
    )
}

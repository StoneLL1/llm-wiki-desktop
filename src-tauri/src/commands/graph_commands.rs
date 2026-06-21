use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::graph::{GraphBuildResult, GraphData, GraphRequest, SaveGraphLayoutRequest};

/// Read the graph for a project. Serves the cached topology when the live wiki
/// content hash matches; otherwise scans the wiki, rebuilds, and persists.
#[tauri::command]
pub fn get_graph(
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let tree = state.search_service.scan_wiki(&context)?;
    state.graph_service.resolve(&context, &tree.pages)
}

/// Force a full rebuild and persist, ignoring any existing cache.
#[tauri::command]
pub fn build_graph(
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
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
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.graph_service.save_layout(&context, request)
}

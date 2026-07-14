use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::graph::{GraphBuildResult, GraphData, GraphRequest, SaveGraphLayoutRequest};
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::tasks::task_model::LogLevel;

/// Resolve graph data against the current wiki pages. The graph cache is a
/// recoverable acceleration layer: missing, corrupt, or stale cache content is
/// repaired synchronously through GraphService::resolve.
#[tauri::command]
pub fn get_graph(
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
    let tree = state.search_service.scan_wiki(&context, &bookmark_paths)?;
    state.graph_service.resolve(&context, &tree.pages)
}

/// Force a full rebuild in the background and return its cancellable task.
#[tauri::command]
pub fn build_graph(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::GraphBuild,
            request.project_id,
            context.root.clone(),
            "Build knowledge graph".to_string(),
            true,
        )
        .map_err(task_error)?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_graph_build(&state, &context, &task_id) {
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

fn run_graph_build(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Scanning wiki pages".to_string())
        .map_err(task_error)?;
    let bookmark_paths = state.bookmark_service.wiki_page_paths(context)?;
    let tree = state.search_service.scan_wiki(context, &bookmark_paths)?;
    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "GRAPH_BUILD_CANCELLED",
            "Graph build was cancelled.",
            true,
            false,
        ));
    }
    state
        .task_service
        .update_progress(
            task_id,
            1,
            Some(2),
            Some(format!("Building graph from {} pages", tree.pages.len())),
        )
        .map_err(task_error)?;
    let result = state.graph_service.rebuild(context, &tree.pages)?;
    state
        .task_service
        .update_progress(task_id, 2, Some(2), Some("Graph cache ready".to_string()))
        .map_err(task_error)?;
    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: format!("Built graph with {} pages.", result.data.nodes.len()),
                affected_paths: vec![".app/graph-cache.json".to_string()],
                reference: None,
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

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
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

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::graph::{GraphBuildResult, GraphData, GraphRequest, SaveGraphLayoutRequest};
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::GraphCachePolicy;
use crate::tasks::task_model::LogLevel;

/// Resolve graph data against the current wiki pages. The graph cache is a
/// recoverable acceleration layer: missing, corrupt, or stale cache content is
/// repaired synchronously through GraphService::resolve.
#[tauri::command]
pub fn get_graph(
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    get_graph_for_state(&state, request)
}

fn get_graph_for_state(
    state: &AppState,
    request: GraphRequest,
) -> Result<GraphBuildResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
    let tree = state.search_service.scan_wiki(&context, &bookmark_paths)?;
    match state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, current| {
            state
                .graph_service
                .resolve(current, &tree.pages, GraphCachePolicy::Persistent(permit))
        },
    ) {
        Ok(result) => Ok(result),
        Err(_) => state
            .graph_service
            .resolve(&context, &tree.pages, GraphCachePolicy::MemoryOnly),
    }
}

/// Force a full rebuild in the background and return its cancellable task.
#[tauri::command]
pub fn build_graph(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GraphRequest,
) -> Result<BackendTask, BackendError> {
    let project_id = request.project_id;
    let project_root_path = request.project_root_path;
    let (context, task) = state.with_current_project_write_access(
        &project_id,
        &project_root_path,
        |_permit, context| {
            let task = state
                .task_service
                .create_project_task(
                    TaskType::GraphBuild,
                    project_id.clone(),
                    context.root.clone(),
                    "Build knowledge graph".to_string(),
                    true,
                )
                .map_err(task_error)?;
            Ok((context.clone(), task))
        },
    )?;
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
    let result = state.with_current_project_write_access(
        &context.project_id,
        context.root.to_string_lossy().as_ref(),
        |permit, _| state.graph_service.rebuild(permit, &tree.pages),
    )?;
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
    let project_id = request.project_id.clone();
    let project_root_path = request.project_root_path.clone();
    state.with_current_project_write_access(&project_id, &project_root_path, |permit, _| {
        state.graph_service.save_layout(permit, request)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn request(project_id: &str, root: &std::path::Path) -> GraphRequest {
        GraphRequest {
            project_id: project_id.into(),
            project_root_path: root.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn restricted_compatible_graph_command_reads_without_creating_app_state() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".obsidian")).unwrap();
        fs::write(root.path().join("知识.md"), "# 知识\n\n[[另一个]]").unwrap();
        fs::write(root.path().join("另一个.md"), "# 另一个").unwrap();
        let state = AppState::default();
        state
            .project_registry
            .register("restricted", root.path())
            .unwrap();

        let result = get_graph_for_state(&state, request("restricted", root.path())).unwrap();

        assert_eq!(result.data.nodes.len(), 2);
        assert!(!root.path().join(".app").exists());
    }

    #[test]
    fn recovery_graph_command_leaves_corrupt_cache_bytes_unchanged() {
        let root = tempfile::tempdir().unwrap();
        for relative in ["raw/sources", "wiki", ".app/tasks", "exports", "skills"] {
            fs::create_dir_all(root.path().join(relative)).unwrap();
        }
        fs::write(root.path().join("purpose.md"), "# Purpose").unwrap();
        fs::write(root.path().join("schema.md"), "# Schema").unwrap();
        fs::write(root.path().join("wiki/index.md"), "# Index").unwrap();
        fs::write(root.path().join("wiki/page.md"), "# Page\n\n[[Index]]").unwrap();
        let cache = root.path().join(".app/graph-cache.json");
        fs::write(&cache, b"{ corrupt graph cache").unwrap();
        let before = fs::read(&cache).unwrap();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("recovery", root.path())
            .unwrap();

        let result = get_graph_for_state(&state, request("recovery", root.path())).unwrap();

        assert_eq!(result.data.nodes.len(), 2);
        assert_eq!(fs::read(cache).unwrap(), before);
    }

    #[test]
    fn trusted_read_only_graph_command_reads_without_rewriting_cache() {
        let root = tempfile::tempdir().unwrap();
        for relative in ["raw/sources", "wiki", ".app/tasks", "exports", "skills"] {
            fs::create_dir_all(root.path().join(relative)).unwrap();
        }
        fs::write(root.path().join("purpose.md"), "# Purpose").unwrap();
        fs::write(root.path().join("schema.md"), "# Schema").unwrap();
        fs::write(root.path().join("wiki/index.md"), "# Index").unwrap();
        fs::write(root.path().join("wiki/page.md"), "# Page\n\n[[Index]]").unwrap();
        let cache = root.path().join(".app/graph-cache.json");
        fs::write(&cache, b"{ stale but protected cache").unwrap();
        let before = fs::read(&cache).unwrap();
        let state = AppState::default();
        state
            .project_registry
            .register_trusted_native("read-only", root.path())
            .unwrap();
        state.project_service.force_read_only_for_test(root.path());

        let result = get_graph_for_state(&state, request("read-only", root.path())).unwrap();

        assert_eq!(result.data.nodes.len(), 2);
        assert_eq!(fs::read(cache).unwrap(), before);
    }
}

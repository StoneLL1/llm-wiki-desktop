use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::workflow::WorkflowDisplayStatus;
use crate::services::window_lifecycle::{
    validate_quit, ResolveWindowCloseRequest, WindowCloseAction, WindowCloseIntent,
    WindowCloseSnapshot, WindowLifecycle,
};

pub const WINDOW_CLOSE_REQUESTED_EVENT: &str = "app://close-requested";

pub fn request_window_close(
    app: &AppHandle,
    intent: WindowCloseIntent,
) -> Result<(), BackendError> {
    app.state::<WindowLifecycle>().request(intent)?;
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(window_error)?;
        window.set_focus().map_err(window_error)?;
        window
            .emit(WINDOW_CLOSE_REQUESTED_EVENT, ())
            .map_err(window_error)?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_window_close_state(
    lifecycle: State<'_, WindowLifecycle>,
    state: State<'_, AppState>,
) -> Result<WindowCloseSnapshot, BackendError> {
    lifecycle.snapshot(state.task_service.list_tasks(None))
}

#[tauri::command]
pub fn resolve_window_close(
    app: AppHandle,
    lifecycle: State<'_, WindowLifecycle>,
    state: State<'_, AppState>,
    request: ResolveWindowCloseRequest,
) -> Result<(), BackendError> {
    match request.action {
        WindowCloseAction::Cancel => lifecycle.dismiss(),
        WindowCloseAction::Background => {
            let window = app.get_webview_window("main").ok_or_else(|| {
                BackendError::new(
                    "APP_CLOSE_UNAVAILABLE",
                    "The main window is unavailable.",
                    true,
                    true,
                )
            })?;
            window.hide().map_err(window_error)?;
            lifecycle.dismiss()
        }
        WindowCloseAction::Quit => {
            lifecycle.reserve_quit(state.confirmation_registry.update_install_barrier(), || {
                let workflow_apply_active =
                    state.task_service.list_workflow_runs().iter().any(|run| {
                        run.display_status == WorkflowDisplayStatus::Running
                            && run.current_stage_id.as_deref().is_some_and(|stage| {
                                stage == "apply_changes" || stage.starts_with("apply_changes_")
                            })
                    });
                validate_quit(
                    &request,
                    &state.task_service.list_tasks(None),
                    workflow_apply_active,
                )
            })?;
            app.exit(0);
            Ok(())
        }
    }
}

fn window_error(error: tauri::Error) -> BackendError {
    BackendError::new("APP_CLOSE_UNAVAILABLE", error.to_string(), true, true)
}

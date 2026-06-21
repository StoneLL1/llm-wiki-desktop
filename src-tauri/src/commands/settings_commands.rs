use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::settings::{
    ProviderSecretStatusRequest, SaveSettingsRequest, Settings, SettingsProjectRequest,
};

#[tauri::command]
pub fn get_settings(
    state: State<'_, AppState>,
    request: SettingsProjectRequest,
) -> Result<Settings, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.settings_service.read_settings(&context)
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    request: SaveSettingsRequest,
) -> Result<Settings, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .settings_service
        .save_settings(&context, &request.settings)
}

#[tauri::command]
pub fn get_provider_secret_status(
    state: State<'_, AppState>,
    request: ProviderSecretStatusRequest,
) -> Result<Option<String>, BackendError> {
    state
        .settings_service
        .get_provider_secret_status(&state.secret_service, request.provider)
}

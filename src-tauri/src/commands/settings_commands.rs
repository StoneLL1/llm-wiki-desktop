use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::settings::{
    ChatConvenienceAuthorization, GlobalUiPreferences, ProviderSecretStatusRequest,
    SaveSettingsRequest, SetChatConvenienceAuthorizationRequest, Settings, SettingsProjectRequest,
};

#[tauri::command]
pub fn get_global_ui_preferences(
    state: State<'_, AppState>,
) -> Result<GlobalUiPreferences, BackendError> {
    state.settings_service.read_global_ui_preferences()
}

#[tauri::command]
pub fn save_global_ui_preferences(
    state: State<'_, AppState>,
    preferences: GlobalUiPreferences,
) -> Result<GlobalUiPreferences, BackendError> {
    state
        .settings_service
        .save_global_ui_preferences(preferences)
}

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
    let settings = state
        .settings_service
        .save_settings(&context, &request.settings)?;
    state.agent_service.invalidate_workflow_route_cache();
    Ok(settings)
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

#[tauri::command]
pub fn get_chat_convenience_authorization(
    state: State<'_, AppState>,
    request: SettingsProjectRequest,
) -> Result<ChatConvenienceAuthorization, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .settings_service
        .get_chat_convenience_authorization(&context)
}

#[tauri::command]
pub fn set_chat_convenience_authorization(
    state: State<'_, AppState>,
    request: SetChatConvenienceAuthorizationRequest,
) -> Result<ChatConvenienceAuthorization, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .settings_service
        .set_chat_convenience_authorization(&context, request.enabled)
}

#[tauri::command]
pub fn revoke_all_chat_convenience_authorizations(
    state: State<'_, AppState>,
) -> Result<(), BackendError> {
    state
        .settings_service
        .revoke_all_chat_convenience_authorizations()
}

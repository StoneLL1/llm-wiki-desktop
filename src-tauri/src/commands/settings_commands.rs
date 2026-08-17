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
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.require_external_ai_access(&context)?;
    let config = crate::services::LlmService::list_providers(&context)?
        .into_iter()
        .find(|config| config.provider == request.provider)
        .ok_or_else(|| {
            BackendError::new(
                "PROVIDER_CREDENTIAL_REAUTH_REQUIRED",
                "Save this provider destination and authorize its credential before using it.",
                true,
                true,
            )
        })?;
    let binding =
        crate::services::LlmService::credential_binding(&context, &config)?.ok_or_else(|| {
            BackendError::new(
                "PROVIDER_CREDENTIAL_REAUTH_REQUIRED",
                "Save this provider destination and authorize its credential before using it.",
                true,
                true,
            )
        })?;
    if binding.config_id != request.config_id
        || binding.revision != request.binding_revision
        || binding.canonical_origin != request.expected_canonical_origin
    {
        return Err(BackendError::new(
            "PROVIDER_CREDENTIAL_BINDING_CHANGED",
            "The provider destination changed; review it and authorize the credential again.",
            true,
            true,
        ));
    }
    Ok(crate::services::LlmService::bound_secret_available(
        &context,
        &state.secret_service,
        &config,
    )?
    .then(|| "configured".to_string()))
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

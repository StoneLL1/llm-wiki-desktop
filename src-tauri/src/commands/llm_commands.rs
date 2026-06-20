use std::path::PathBuf;

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::llm::{
    LlmProviderKind, ProviderProjectRequest, ProviderSecretRequest, ProviderStatus,
    ProviderTestResult, SaveProviderRequest,
};
use crate::models::paths::ProjectContext;

fn context(project_id: &str, root: &str) -> ProjectContext {
    ProjectContext::new(project_id.to_string(), PathBuf::from(root))
}

#[tauri::command]
pub fn list_llm_providers(
    state: State<'_, AppState>,
    request: ProviderProjectRequest,
) -> Result<Vec<ProviderStatus>, BackendError> {
    let context = context(&request.project_id, &request.project_root_path);
    crate::services::LlmService::list_providers(&context)?
        .into_iter()
        .map(|config| {
            let secret_mask = state.secret_service.mask(config.provider)?;
            Ok(ProviderStatus {
                has_secret: secret_mask.is_some(),
                secret_mask,
                config,
            })
        })
        .collect()
}

#[tauri::command]
pub fn save_llm_provider(request: SaveProviderRequest) -> Result<ProviderStatus, BackendError> {
    let context = context(&request.project_id, &request.project_root_path);
    crate::services::LlmService::save_provider(&context, request.config.clone())?;
    Ok(ProviderStatus {
        config: request.config,
        has_secret: false,
        secret_mask: None,
    })
}

#[tauri::command]
pub fn store_provider_secret(
    state: State<'_, AppState>,
    request: ProviderSecretRequest,
) -> Result<ProviderStatus, BackendError> {
    let secret = request.secret.as_deref().ok_or_else(|| {
        BackendError::new("SECRET_EMPTY", "Provider secret is required.", true, true)
    })?;
    state.secret_service.set(request.provider, secret)?;
    let mask = state.secret_service.mask(request.provider)?;
    Ok(ProviderStatus {
        config: default_config(request.provider),
        has_secret: true,
        secret_mask: mask,
    })
}

#[tauri::command]
pub fn delete_provider_secret(
    state: State<'_, AppState>,
    request: ProviderSecretRequest,
) -> Result<(), BackendError> {
    state.secret_service.delete(request.provider)
}

#[tauri::command]
pub fn provider_secret_status(
    state: State<'_, AppState>,
    request: ProviderSecretRequest,
) -> Result<Option<String>, BackendError> {
    state.secret_service.mask(request.provider)
}

#[tauri::command]
pub async fn test_llm_provider(
    state: State<'_, AppState>,
    request: SaveProviderRequest,
) -> Result<ProviderTestResult, BackendError> {
    let secret = state.secret_service.get(request.config.provider)?;
    let response = state
        .llm_service
        .complete(&request.config, secret.as_deref(), "Reply with OK only.")
        .await;
    Ok(ProviderTestResult {
        provider: request.config.provider,
        ok: response.is_ok(),
        message: response
            .map(|_| "Provider connection succeeded.".into())
            .unwrap_or_else(|error| error.message),
    })
}

fn default_config(provider: LlmProviderKind) -> crate::models::llm::LlmProviderConfig {
    crate::models::llm::LlmProviderConfig {
        provider,
        model: String::new(),
        base_url: String::new(),
        context_window: 0,
        enabled: false,
    }
}

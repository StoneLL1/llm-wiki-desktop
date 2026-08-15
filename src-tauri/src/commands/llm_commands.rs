use std::time::Duration;

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::llm::{
    LlmProviderKind, ProviderProjectRequest, ProviderSecretRequest, ProviderStatus,
    ProviderTestResult, SaveProviderRequest,
};

/// Reflect the live secret state into a ProviderStatus: `has_secret` plus the
/// last-4-characters mask the design (settings.html:349) shows inline. The mask
/// never reveals full key material (PRD-SET-002) — `SecretService::mask`
/// produces `····XXXX`.
fn status_with_secret(
    secret_service: &crate::services::SecretService,
    config: crate::models::llm::LlmProviderConfig,
) -> Result<ProviderStatus, BackendError> {
    let secret = secret_service.get(config.provider)?;
    Ok(ProviderStatus {
        has_secret: secret.is_some(),
        secret_mask: secret_service.mask(config.provider)?,
        config,
    })
}

#[tauri::command]
pub fn list_llm_providers(
    state: State<'_, AppState>,
    request: ProviderProjectRequest,
) -> Result<Vec<ProviderStatus>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    // Provider status is a live project-file + credential-store read. Keep
    // force_refresh in the typed contract so future caching cannot ignore it.
    let _force_refresh = request.force_refresh;
    crate::services::LlmService::list_providers(&context)?
        .into_iter()
        .map(|config| status_with_secret(&state.secret_service, config))
        .collect()
}

#[tauri::command]
pub fn save_llm_provider(
    state: State<'_, AppState>,
    request: SaveProviderRequest,
) -> Result<ProviderStatus, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    crate::services::LlmService::save_provider(&context, request.config.clone())?;
    status_with_secret(&state.secret_service, request.config)
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
    // Return a default config shell with the fresh mask so the UI can render
    // the row's "已配置" state immediately after saving a key. The full config
    // (model/baseUrl) is loaded separately via list_llm_providers.
    status_with_secret(&state.secret_service, default_config(request.provider))
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
    // Kept for backwards compat with older frontends; returns "configured" only.
    Ok(state
        .secret_service
        .get(request.provider)?
        .map(|_| "configured".to_string()))
}

/// Probe whether a local Ollama service is reachable at its configured
/// base URL (design settings.html:378-387 "服务未运行" / "启动" state). Returns
/// the model count when reachable, or an error the UI shows as "服务未运行".
#[tauri::command]
pub async fn check_ollama_reachable(
    state: State<'_, AppState>,
    request: ProviderProjectRequest,
) -> Result<OllamaReachability, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let providers = crate::services::LlmService::list_providers(&context)?;
    let ollama = providers
        .into_iter()
        .find(|config| config.provider == LlmProviderKind::Ollama);
    let base_url = ollama
        .map(|config| config.base_url)
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let tags_url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|error| {
            BackendError::new("OLLAMA_CLIENT_FAILED", error.to_string(), true, false)
        })?;
    let response =
        client.get(&tags_url).send().await.map_err(|error| {
            BackendError::new("OLLAMA_UNREACHABLE", error.to_string(), true, false)
        })?;
    if !response.status().is_success() {
        return Err(BackendError::new(
            "OLLAMA_UNREACHABLE",
            format!("Ollama returned HTTP {}.", response.status()),
            true,
            false,
        ));
    }
    let value: serde_json::Value = response.json().await.map_err(|_| {
        BackendError::new(
            "OLLAMA_RESPONSE_INVALID",
            "Ollama returned invalid JSON.",
            true,
            false,
        )
    })?;
    let model_count = value
        .get("models")
        .and_then(|models| models.as_array())
        .map(|models| models.len())
        .unwrap_or(0);
    Ok(OllamaReachability {
        reachable: true,
        base_url,
        model_count,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaReachability {
    pub reachable: bool,
    pub base_url: String,
    pub model_count: usize,
}

#[tauri::command]
pub async fn test_llm_provider(
    state: State<'_, AppState>,
    request: SaveProviderRequest,
) -> Result<ProviderTestResult, BackendError> {
    state.resolve_project_context(&request.project_id, &request.project_root_path)?;
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

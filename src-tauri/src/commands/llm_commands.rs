use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::llm::{
    LlmProviderConfig, LlmProviderKind, ProviderConnectionRequest, ProviderProjectRequest,
    ProviderSecretRequest, ProviderStatus, ProviderTestResult, SaveProviderRequest,
};
use crate::models::paths::ProjectContext;

/// Reflect the live secret state into a ProviderStatus: `has_secret` plus the
/// last-4-characters mask the design (settings.html:349) shows inline. The mask
/// never reveals full key material (PRD-SET-002) — `SecretService::mask`
/// produces `····XXXX`.
fn status_with_secret(
    context: &ProjectContext,
    secret_service: &crate::services::SecretService,
    config: LlmProviderConfig,
) -> Result<ProviderStatus, BackendError> {
    let binding = crate::services::LlmService::credential_binding(context, &config)?;
    let secret_mask = binding
        .as_ref()
        .map(|binding| secret_service.mask_bound(context, binding))
        .transpose()?
        .flatten();
    Ok(ProviderStatus {
        has_secret: secret_mask.is_some(),
        secret_mask,
        credential_binding: binding,
        config,
    })
}

#[tauri::command]
pub fn list_llm_providers(
    state: State<'_, AppState>,
    request: ProviderProjectRequest,
) -> Result<Vec<ProviderStatus>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.require_external_ai_access(&context)?;
    // Provider status is a live project-file + credential-store read. Keep
    // force_refresh in the typed contract so future caching cannot ignore it.
    let _force_refresh = request.force_refresh;
    crate::services::LlmService::list_providers(&context)?
        .into_iter()
        .map(|config| status_with_secret(&context, &state.secret_service, config))
        .collect()
}

#[tauri::command]
pub fn save_llm_provider(
    state: State<'_, AppState>,
    request: SaveProviderRequest,
) -> Result<ProviderStatus, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let (config, _) = crate::services::LlmService::save_provider_with_secret_invalidation(
                context,
                request.config,
                &state.secret_service,
            )?;
            status_with_secret(context, &state.secret_service, config)
        },
    )
}

#[tauri::command]
pub fn store_provider_secret(
    state: State<'_, AppState>,
    request: ProviderSecretRequest,
) -> Result<ProviderStatus, BackendError> {
    let secret = request.secret.as_deref().ok_or_else(|| {
        BackendError::new("SECRET_EMPTY", "Provider secret is required.", true, true)
    })?;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            crate::services::LlmService::approve_and_store_secret(
                context,
                &state.secret_service,
                request.provider,
                &request.config_id,
                request.binding_revision,
                &request.expected_canonical_origin,
                secret,
            )?;
            let config = crate::services::LlmService::list_providers(context)?
                .into_iter()
                .find(|config| config.provider == request.provider)
                .ok_or_else(|| {
                    BackendError::new(
                        "PROVIDER_CREDENTIAL_BINDING_CHANGED",
                        "The provider destination changed; review it and authorize the credential again.",
                        true,
                        true,
                    )
                })?;
            status_with_secret(context, &state.secret_service, config)
        },
    )
}

#[tauri::command]
pub fn delete_provider_secret(
    state: State<'_, AppState>,
    request: ProviderSecretRequest,
) -> Result<(), BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            crate::services::LlmService::delete_bound_secret(
                context,
                &state.secret_service,
                request.provider,
                &request.config_id,
                request.binding_revision,
                &request.expected_canonical_origin,
            )
        },
    )
}

#[tauri::command]
pub fn provider_secret_status(
    state: State<'_, AppState>,
    request: ProviderSecretRequest,
) -> Result<Option<String>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.require_external_ai_access(&context)?;
    let (config, _, _) = crate::services::LlmService::provider_with_bound_secret(
        &context,
        &state.secret_service,
        request.provider,
        Some((
            &request.config_id,
            request.binding_revision,
            &request.expected_canonical_origin,
        )),
    )?;
    let status = status_with_secret(&context, &state.secret_service, config)?;
    Ok(status.has_secret.then(|| "configured".to_string()))
}

/// Probe whether a local Ollama service is reachable at its configured
/// base URL (design settings.html:378-387 "服务未运行" / "启动" state). Returns
/// the model count when reachable, or an error the UI shows as "服务未运行".
#[tauri::command]
pub async fn check_ollama_reachable(
    state: State<'_, AppState>,
    request: ProviderConnectionRequest,
) -> Result<OllamaReachability, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.require_external_ai_access(&context)?;
    let (config, _, _) = crate::services::LlmService::provider_with_bound_secret(
        &context,
        &state.secret_service,
        LlmProviderKind::Ollama,
        Some((
            &request.config_id,
            request.binding_revision,
            &request.expected_canonical_origin,
        )),
    )?;
    let execution = state.begin_project_external_execution(
        &context,
        &format!("ollama-probe:{}", uuid::Uuid::new_v4()),
    )?;
    let (base_url, model_count) = state.llm_service.probe_ollama(&config).await?;
    state.require_current_execution_epoch(&context, &execution)?;
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

fn provider_test_result(
    provider: LlmProviderKind,
    response: Result<String, BackendError>,
) -> Result<ProviderTestResult, BackendError> {
    response?;
    Ok(ProviderTestResult {
        provider,
        ok: true,
        message: "Provider connection succeeded.".into(),
    })
}

#[tauri::command]
pub async fn test_llm_provider(
    state: State<'_, AppState>,
    request: ProviderConnectionRequest,
) -> Result<ProviderTestResult, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.require_external_ai_access(&context)?;
    let (config, _, secret) = crate::services::LlmService::provider_with_bound_secret(
        &context,
        &state.secret_service,
        request.provider,
        Some((
            &request.config_id,
            request.binding_revision,
            &request.expected_canonical_origin,
        )),
    )?;
    let execution = state.begin_project_external_execution(
        &context,
        &format!("provider-probe:{}", uuid::Uuid::new_v4()),
    )?;
    let response = state
        .llm_service
        .complete(&config, secret.as_deref(), "Reply with OK only.")
        .await;
    state.require_current_execution_epoch(&context, &execution)?;
    provider_test_result(config.provider, response)
}

#[cfg(test)]
mod tests {
    use super::provider_test_result;
    use crate::errors::BackendError;
    use crate::models::llm::LlmProviderKind;

    #[test]
    fn provider_test_preserves_structured_backend_errors() {
        let error = BackendError::new("LLM_AUTH_FAILED", "Authorization failed.", true, true);

        let propagated = provider_test_result(LlmProviderKind::Anthropic, Err(error)).unwrap_err();

        assert_eq!(propagated.code, "LLM_AUTH_FAILED");
        assert!(propagated.recoverable);
        assert!(propagated.user_action_required);
    }

    #[test]
    fn provider_test_returns_success_only_after_completion() {
        let result = provider_test_result(LlmProviderKind::OpenAi, Ok("OK".into())).unwrap();

        assert!(result.ok);
        assert_eq!(result.provider, LlmProviderKind::OpenAi);
    }
}

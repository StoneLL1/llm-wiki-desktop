use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crate::errors::BackendError;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind, ProviderCredentialBinding};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::url_policy::{
    ProviderNetworkClass, ProviderNetworkTarget, UrlPolicy,
};
use crate::services::{SecretService, SettingsService};
use reqwest::redirect::Policy;
use url::Host;

const OPENAI_ORIGIN: &str = "https://api.openai.com";
const ANTHROPIC_ORIGIN: &str = "https://api.anthropic.com";
const GOOGLE_ORIGIN: &str = "https://generativelanguage.googleapis.com";
static PROVIDER_CREDENTIAL_TRANSACTION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone)]
pub struct ProviderHttpRequest {
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: serde_json::Value,
}

#[derive(Default)]
pub struct LlmService;

impl LlmService {
    pub fn list_providers(
        context: &ProjectContext,
    ) -> Result<Vec<LlmProviderConfig>, BackendError> {
        SettingsService::default().list_providers(context)
    }

    pub fn save_provider(
        context: &ProjectContext,
        config: LlmProviderConfig,
    ) -> Result<(LlmProviderConfig, ProviderCredentialBinding), BackendError> {
        let settings_service = SettingsService::default();
        Self::save_provider_internal(context, config, None, &settings_service)
    }

    pub fn save_provider_with_secret_invalidation(
        context: &ProjectContext,
        config: LlmProviderConfig,
        secret_service: &SecretService,
    ) -> Result<(LlmProviderConfig, ProviderCredentialBinding), BackendError> {
        let settings_service = SettingsService::default();
        Self::save_provider_internal(context, config, Some(secret_service), &settings_service)
    }

    fn save_provider_internal(
        context: &ProjectContext,
        mut config: LlmProviderConfig,
        secret_service: Option<&SecretService>,
        settings_service: &SettingsService,
    ) -> Result<(LlmProviderConfig, ProviderCredentialBinding), BackendError> {
        let _transaction = provider_credential_transaction()?;
        let target = normalize_provider_config(&mut config)?;
        let canonical_origin = UrlPolicy.canonical_origin(&target);
        let settings = settings_service.read_settings(context)?;
        let previous_config = settings
            .llm_providers
            .iter()
            .find(|item| item.provider == config.provider)
            .cloned();
        let existing = settings
            .provider_credential_bindings
            .iter()
            .find(|binding| binding.provider_kind == config.provider);
        let reusable = existing.as_ref().is_some_and(|binding| {
            validate_binding(context, &config, binding).is_ok()
                && binding.canonical_origin == canonical_origin
        });
        let revision = existing.as_ref().map_or(1, |binding| {
            binding.revision.saturating_add(u64::from(!reusable))
        });
        let config_id = if reusable {
            existing
                .as_ref()
                .map(|binding| binding.config_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        let credential_account_id = SecretService::provider_binding_account_id(
            context,
            config.provider,
            &config_id,
            &canonical_origin,
            revision,
        )?;
        let binding = ProviderCredentialBinding {
            config_id,
            provider_kind: config.provider,
            canonical_origin,
            credential_account_id,
            approved_at: reusable
                .then(|| {
                    existing
                        .as_ref()
                        .and_then(|binding| binding.approved_at.clone())
                })
                .flatten(),
            revision,
        };
        let existing = existing.cloned();
        let retirement_required = !reusable
            && secret_service.is_some()
            && existing.as_ref().is_some_and(|binding| {
                SecretService::provider_binding_account_id(
                    context,
                    binding.provider_kind,
                    &binding.config_id,
                    &binding.canonical_origin,
                    binding.revision,
                )
                .is_ok_and(|expected| expected == binding.credential_account_id)
            });
        if retirement_required && previous_config.is_none() {
            return Err(provider_binding_changed());
        }
        commit_provider_binding_rotation(
            || {
                settings_service
                    .save_provider_with_binding(context, config.clone(), binding.clone())
                    .map(|_| ())
            },
            || {
                if retirement_required {
                    if let (Some(secret_service), Some(existing)) =
                        (secret_service, existing.as_ref())
                    {
                        if SecretService::provider_binding_account_id(
                            context,
                            existing.provider_kind,
                            &existing.config_id,
                            &existing.canonical_origin,
                            existing.revision,
                        )
                        .is_ok_and(|expected| expected == existing.credential_account_id)
                        {
                            secret_service.delete_bound(context, existing)?;
                        }
                    }
                }
                Ok(())
            },
            || {
                if retirement_required {
                    let (Some(previous_config), Some(existing)) =
                        (previous_config.clone(), existing.clone())
                    else {
                        return Err(provider_binding_changed());
                    };
                    settings_service
                        .save_provider_with_binding(context, previous_config, existing)
                        .map(|_| ())?;
                }
                Ok(())
            },
        )?;
        Ok((config, binding))
    }

    pub fn validate_config(config: &LlmProviderConfig) -> Result<(), BackendError> {
        let mut config = config.clone();
        normalize_provider_config(&mut config)?;
        Ok(())
    }

    pub fn credential_binding(
        context: &ProjectContext,
        config: &LlmProviderConfig,
    ) -> Result<Option<ProviderCredentialBinding>, BackendError> {
        let Some(binding) =
            SettingsService::default().provider_credential_binding(context, config.provider)?
        else {
            return Ok(None);
        };
        validate_binding(context, config, &binding)?;
        Ok(Some(binding))
    }

    pub fn provider_with_bound_secret(
        context: &ProjectContext,
        secret_service: &SecretService,
        provider: LlmProviderKind,
        expected_binding: Option<(&str, u64, &str)>,
    ) -> Result<(LlmProviderConfig, ProviderCredentialBinding, Option<String>), BackendError> {
        let config = Self::list_providers(context)?
            .into_iter()
            .find(|config| config.provider == provider)
            .ok_or_else(provider_binding_required)?;
        let binding =
            Self::credential_binding(context, &config)?.ok_or_else(provider_binding_required)?;
        if expected_binding.is_some_and(|(config_id, revision, canonical_origin)| {
            binding.config_id != config_id
                || binding.revision != revision
                || binding.canonical_origin != canonical_origin
        }) {
            return Err(provider_binding_changed());
        }
        let secret = if provider.requires_secret() {
            secret_service.get_bound(context, &binding)?
        } else {
            None
        };
        if provider.requires_secret() && secret.is_none() {
            return Err(provider_binding_required());
        }
        Ok((config, binding, secret))
    }

    pub fn bound_secret_for_config(
        context: &ProjectContext,
        secret_service: &SecretService,
        config: &LlmProviderConfig,
    ) -> Result<Option<String>, BackendError> {
        let binding =
            Self::credential_binding(context, config)?.ok_or_else(provider_binding_required)?;
        let secret = if config.provider.requires_secret() {
            secret_service.get_bound(context, &binding)?
        } else {
            None
        };
        if config.provider.requires_secret() && secret.is_none() {
            return Err(provider_binding_required());
        }
        Ok(secret)
    }

    pub fn bound_secret_available(
        context: &ProjectContext,
        secret_service: &SecretService,
        config: &LlmProviderConfig,
    ) -> Result<bool, BackendError> {
        let Some(binding) = Self::credential_binding(context, config)? else {
            return Ok(false);
        };
        Ok(!config.provider.requires_secret()
            || secret_service.get_bound(context, &binding)?.is_some())
    }

    pub fn approve_and_store_secret(
        context: &ProjectContext,
        secret_service: &SecretService,
        provider: LlmProviderKind,
        config_id: &str,
        binding_revision: u64,
        expected_canonical_origin: &str,
        secret: &str,
    ) -> Result<ProviderCredentialBinding, BackendError> {
        let settings_service = SettingsService::default();
        Self::approve_and_store_secret_internal(
            context,
            secret_service,
            provider,
            config_id,
            binding_revision,
            expected_canonical_origin,
            secret,
            &settings_service,
        )
    }

    fn approve_and_store_secret_internal(
        context: &ProjectContext,
        secret_service: &SecretService,
        provider: LlmProviderKind,
        config_id: &str,
        binding_revision: u64,
        expected_canonical_origin: &str,
        secret: &str,
        settings_service: &SettingsService,
    ) -> Result<ProviderCredentialBinding, BackendError> {
        let _transaction = provider_credential_transaction()?;
        let config = settings_service
            .list_providers(context)?
            .into_iter()
            .find(|config| config.provider == provider)
            .ok_or_else(provider_binding_required)?;
        let mut binding = settings_service
            .provider_credential_binding(context, config.provider)?
            .ok_or_else(provider_binding_required)?;
        validate_binding(context, &config, &binding)?;
        if binding.config_id != config_id
            || binding.revision != binding_revision
            || binding.canonical_origin != expected_canonical_origin
        {
            return Err(provider_binding_changed());
        }
        binding.approved_at = Some(chrono::Utc::now().to_rfc3339());
        secret_service.set_bound(context, &binding, secret)?;
        if let Err(error) =
            settings_service.save_provider_with_binding(context, config, binding.clone())
        {
            let _ = secret_service.delete_bound(context, &binding);
            return Err(error);
        }
        Ok(binding)
    }

    pub fn delete_bound_secret(
        context: &ProjectContext,
        secret_service: &SecretService,
        provider: LlmProviderKind,
        config_id: &str,
        binding_revision: u64,
        expected_canonical_origin: &str,
    ) -> Result<(), BackendError> {
        let settings_service = SettingsService::default();
        Self::delete_bound_secret_internal(
            context,
            secret_service,
            provider,
            config_id,
            binding_revision,
            expected_canonical_origin,
            &settings_service,
        )
    }

    fn delete_bound_secret_internal(
        context: &ProjectContext,
        secret_service: &SecretService,
        provider: LlmProviderKind,
        config_id: &str,
        binding_revision: u64,
        expected_canonical_origin: &str,
        settings_service: &SettingsService,
    ) -> Result<(), BackendError> {
        let _transaction = provider_credential_transaction()?;
        let config = settings_service
            .list_providers(context)?
            .into_iter()
            .find(|config| config.provider == provider)
            .ok_or_else(provider_binding_required)?;
        let mut binding = settings_service
            .provider_credential_binding(context, config.provider)?
            .ok_or_else(provider_binding_required)?;
        validate_binding(context, &config, &binding)?;
        if binding.config_id != config_id
            || binding.revision != binding_revision
            || binding.canonical_origin != expected_canonical_origin
        {
            return Err(provider_binding_changed());
        }
        secret_service.delete_bound(context, &binding)?;
        binding.approved_at = None;
        settings_service
            .save_provider_with_binding(context, config, binding)
            .map(|_| ())
    }

    pub async fn probe_ollama(
        &self,
        config: &LlmProviderConfig,
    ) -> Result<(String, usize), BackendError> {
        if config.provider != LlmProviderKind::Ollama {
            return Err(provider_binding_changed());
        }
        Self::validate_config(config)?;
        let tags_url = format!("{}/api/tags", config.base_url.trim_end_matches('/'));
        let (client, url) = validated_provider_client(&tags_url, Duration::from_secs(4)).await?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(provider_request_error)?;
        reject_redirect(&response)?;
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
            .map_or(0, Vec::len);
        Ok((config.base_url.clone(), model_count))
    }

    fn validate_config_shape(config: &LlmProviderConfig) -> Result<(), BackendError> {
        if config.model.trim().is_empty() {
            return Err(BackendError::new(
                "LLM_MODEL_REQUIRED",
                "Model is required.",
                true,
                true,
            ));
        }
        let url = url::Url::parse(&config.base_url).map_err(|_| invalid_provider_url())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(invalid_provider_url());
        }
        let forbidden_query_key = url.query_pairs().any(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "key" | "api_key" | "apikey" | "token" | "access_token" | "authorization"
            )
        });
        if !url.username().is_empty()
            || url.password().is_some()
            || forbidden_query_key
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(BackendError::new(
                "LLM_BASE_URL_SECRET_FORBIDDEN",
                "Provider base URL cannot contain credentials or secret query parameters.",
                true,
                true,
            ));
        }
        Ok(())
    }

    pub fn build_request(
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
    ) -> Result<ProviderHttpRequest, BackendError> {
        Self::validate_request_inputs(config, secret)?;
        let base = config.base_url.trim_end_matches('/');
        let mut headers = BTreeMap::new();
        headers.insert("content-type".into(), "application/json".into());
        let (url, body) = match config.provider {
            LlmProviderKind::OpenAi | LlmProviderKind::Custom => {
                headers.insert(
                    "authorization".into(),
                    format!("Bearer {}", secret.unwrap_or_default()),
                );
                (
                    format!("{base}/v1/chat/completions"),
                    serde_json::json!({
                        "model": config.model,
                        "messages": [{"role": "user", "content": prompt}],
                        "temperature": 0
                    }),
                )
            }
            LlmProviderKind::Anthropic => {
                headers.insert("x-api-key".into(), secret.unwrap_or_default().into());
                headers.insert("anthropic-version".into(), "2023-06-01".into());
                (
                    format!("{base}/v1/messages"),
                    serde_json::json!({
                        "model": config.model,
                        "max_tokens": config.context_window.min(8192),
                        "messages": [{"role": "user", "content": prompt}]
                    }),
                )
            }
            LlmProviderKind::Google => (
                {
                    headers.insert("x-goog-api-key".into(), secret.unwrap_or_default().into());
                    format!("{base}/v1beta/models/{}:generateContent", config.model)
                },
                serde_json::json!({"contents": [{"parts": [{"text": prompt}]}]}),
            ),
            LlmProviderKind::Ollama => (
                format!("{base}/api/chat"),
                serde_json::json!({
                    "model": config.model,
                    "stream": false,
                    "messages": [{"role": "user", "content": prompt}]
                }),
            ),
        };
        Ok(ProviderHttpRequest { url, headers, body })
    }

    /// Shared pre-flight check for both the batch and streaming request
    /// builders: a valid config + a present secret when the provider needs one.
    fn validate_request_inputs(
        config: &LlmProviderConfig,
        secret: Option<&str>,
    ) -> Result<(), BackendError> {
        Self::validate_config(config)?;
        if config.provider.requires_secret()
            && secret.filter(|value| !value.trim().is_empty()).is_none()
        {
            return Err(BackendError::new(
                "LLM_SECRET_MISSING",
                "The selected provider has no configured secret.",
                true,
                true,
            ));
        }
        Ok(())
    }

    /// Build a streaming request for [`complete_streaming`]. Same shape as
    /// [`build_request`] but with `"stream": true` (OpenAI/Anthropic/Ollama)
    /// and the streaming endpoint for Google (`:streamGenerateContent?alt=sse`
    /// so it emits SSE `data:` frames instead of one JSON blob).
    pub fn build_streaming_request(
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
    ) -> Result<ProviderHttpRequest, BackendError> {
        Self::validate_request_inputs(config, secret)?;
        let base = config.base_url.trim_end_matches('/');
        let mut headers = BTreeMap::new();
        headers.insert("content-type".into(), "application/json".into());
        let (url, body) = match config.provider {
            LlmProviderKind::OpenAi | LlmProviderKind::Custom => {
                headers.insert(
                    "authorization".into(),
                    format!("Bearer {}", secret.unwrap_or_default()),
                );
                (
                    format!("{base}/v1/chat/completions"),
                    serde_json::json!({
                        "model": config.model,
                        "messages": [{"role": "user", "content": prompt}],
                        "temperature": 0,
                        "stream": true
                    }),
                )
            }
            LlmProviderKind::Anthropic => {
                headers.insert("x-api-key".into(), secret.unwrap_or_default().into());
                headers.insert("anthropic-version".into(), "2023-06-01".into());
                (
                    format!("{base}/v1/messages"),
                    serde_json::json!({
                        "model": config.model,
                        "max_tokens": config.context_window.min(8192),
                        "messages": [{"role": "user", "content": prompt}],
                        "stream": true
                    }),
                )
            }
            LlmProviderKind::Google => {
                headers.insert("x-goog-api-key".into(), secret.unwrap_or_default().into());
                (
                    format!(
                        "{base}/v1beta/models/{}:streamGenerateContent?alt=sse",
                        config.model
                    ),
                    serde_json::json!({"contents": [{"parts": [{"text": prompt}]}]}),
                )
            }
            LlmProviderKind::Ollama => (
                format!("{base}/api/chat"),
                serde_json::json!({
                    "model": config.model,
                    "stream": true,
                    "messages": [{"role": "user", "content": prompt}]
                }),
            ),
        };
        Ok(ProviderHttpRequest { url, headers, body })
    }

    /// Build a streaming request for a bounded JSON candidate. Providers with
    /// a stable JSON-output switch receive it in addition to the textual
    /// contract; Anthropic keeps the prompt-only contract because this endpoint
    /// has no equivalent portable response-format field.
    pub fn build_structured_streaming_request(
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
    ) -> Result<ProviderHttpRequest, BackendError> {
        let mut request = Self::build_streaming_request(config, secret, prompt)?;
        match config.provider {
            LlmProviderKind::OpenAi | LlmProviderKind::Custom => {
                request.body["response_format"] = serde_json::json!({ "type": "json_object" });
            }
            LlmProviderKind::Google => {
                request.body["generationConfig"] =
                    serde_json::json!({ "responseMimeType": "application/json" });
            }
            LlmProviderKind::Ollama => {
                request.body["format"] = serde_json::json!("json");
            }
            LlmProviderKind::Anthropic => {}
        }
        Ok(request)
    }

    pub async fn complete(
        &self,
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
    ) -> Result<String, BackendError> {
        let request = Self::build_request(config, secret, prompt)?;
        let (client, url) =
            validated_provider_client(&request.url, Duration::from_secs(120)).await?;
        let mut builder = client.post(url).json(&request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.send().await.map_err(provider_request_error)?;
        reject_redirect(&response)?;
        let status = response.status();
        if !status.is_success() {
            return Err(BackendError::new(
                match status.as_u16() {
                    401 | 403 => "LLM_AUTH_FAILED",
                    429 => "LLM_RATE_LIMITED",
                    _ => "LLM_REQUEST_FAILED",
                },
                format!("Provider returned HTTP {status}."),
                status.as_u16() == 429 || status.is_server_error(),
                status.as_u16() == 401 || status.as_u16() == 403,
            ));
        }
        let value: serde_json::Value = response.json().await.map_err(|_| {
            BackendError::new(
                "LLM_RESPONSE_INVALID",
                "Provider returned invalid JSON.",
                true,
                false,
            )
        })?;
        extract_text(config.provider, &value).ok_or_else(|| {
            BackendError::new(
                "LLM_RESPONSE_INVALID",
                "Provider response contained no text.",
                true,
                false,
            )
        })
    }

    /// Stream a completion token-by-token from the provider.
    ///
    /// Unlike [`complete`](Self::complete) (one POST → full JSON), this opens a
    /// streaming response, parses SSE/NDJSON frames per provider, and invokes
    /// `on_delta` with each incremental text chunk as it arrives. The fully
    /// assembled text is returned for persistence; the caller also forwards
    /// deltas to the task stream channel so the UI can render the answer
    /// live.
    ///
    /// `is_cancelled` is polled between chunks so a user cancel is observed
    /// promptly (returns `LLM_CANCELLED`). It is a callback rather than a
    /// `&TaskService` reference to keep `LlmService` decoupled from the task
    /// layer and unit-testable without it.
    pub async fn complete_streaming<C, D>(
        &self,
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
        is_cancelled: C,
        on_delta: D,
    ) -> Result<String, BackendError>
    where
        C: Fn() -> bool + Send + Sync,
        D: FnMut(&str) + Send,
    {
        self.complete_streaming_with_timeout(
            config,
            secret,
            prompt,
            std::time::Duration::from_secs(120),
            false,
            is_cancelled,
            on_delta,
        )
        .await
    }

    /// Stream a completion for a user-visible background task whose output may
    /// legitimately take longer than an interactive chat response. Source AI
    /// uses this path so an OpenAI-compatible provider cannot fail merely
    /// because it buffered a long candidate near the interactive 120-second
    /// boundary.
    pub async fn complete_structured_long_running_streaming<C, D>(
        &self,
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
        is_cancelled: C,
        on_delta: D,
    ) -> Result<String, BackendError>
    where
        C: Fn() -> bool + Send + Sync,
        D: FnMut(&str) + Send,
    {
        self.complete_streaming_with_timeout(
            config,
            secret,
            prompt,
            std::time::Duration::from_secs(10 * 60),
            true,
            is_cancelled,
            on_delta,
        )
        .await
    }

    async fn complete_streaming_with_timeout<C, D>(
        &self,
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
        timeout: std::time::Duration,
        structured_json: bool,
        is_cancelled: C,
        mut on_delta: D,
    ) -> Result<String, BackendError>
    where
        C: Fn() -> bool + Send + Sync,
        D: FnMut(&str) + Send,
    {
        if is_cancelled() {
            return Err(llm_cancelled_error());
        }
        let request = if structured_json {
            Self::build_structured_streaming_request(config, secret, prompt)?
        } else {
            Self::build_streaming_request(config, secret, prompt)?
        };
        let (client, url) = validated_provider_client(&request.url, timeout).await?;
        let mut builder = client.post(url).json(&request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if is_cancelled() {
            return Err(llm_cancelled_error());
        }
        let send = builder.send();
        tokio::pin!(send);
        let response = loop {
            tokio::select! {
                result = &mut send => break result,
                _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {
                    if is_cancelled() {
                        return Err(llm_cancelled_error());
                    }
                }
            }
        }
        .map_err(provider_request_error)?;
        reject_redirect(&response)?;
        let status = response.status();
        if !status.is_success() {
            return Err(BackendError::new(
                match status.as_u16() {
                    401 | 403 => "LLM_AUTH_FAILED",
                    429 => "LLM_RATE_LIMITED",
                    _ => "LLM_REQUEST_FAILED",
                },
                format!("Provider returned HTTP {status}."),
                status.as_u16() == 429 || status.is_server_error(),
                status.as_u16() == 401 || status.as_u16() == 403,
            ));
        }

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let provider = config.provider;
        let mut full = String::new();
        let mut buf = String::new();
        let mut utf8_pending = Vec::new();
        let mut raw_response = Vec::new();
        loop {
            let next = stream.next();
            tokio::pin!(next);
            let chunk_result = loop {
                tokio::select! {
                    value = &mut next => break value,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {
                        if is_cancelled() {
                            return Err(llm_cancelled_error());
                        }
                    }
                }
            };
            let Some(chunk_result) = chunk_result else {
                break;
            };
            let chunk = chunk_result.map_err(|_| {
                BackendError::new(
                    "LLM_RESPONSE_INVALID",
                    "Provider stream was interrupted.",
                    true,
                    false,
                )
            })?;
            raw_response.extend_from_slice(&chunk);
            append_utf8_chunk(&mut utf8_pending, &chunk, &mut buf);
            // Process every complete line now in the buffer; keep any trailing
            // partial line for the next chunk.
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                if let Some(delta) = parse_stream_line(provider, line.trim_end()) {
                    if !delta.is_empty() {
                        full.push_str(&delta);
                        on_delta(&delta);
                    }
                }
            }
        }
        if !utf8_pending.is_empty() {
            buf.push_str(&String::from_utf8_lossy(&utf8_pending));
        }
        // Flush a trailing frame that lacked a final newline.
        if let Some(delta) = parse_stream_line(provider, buf.trim_end()) {
            if !delta.is_empty() {
                full.push_str(&delta);
                on_delta(&delta);
            }
        }
        if full.trim().is_empty() {
            if let Some(buffered) = extract_buffered_response(provider, &raw_response) {
                if !buffered.is_empty() {
                    on_delta(&buffered);
                    return Ok(buffered);
                }
            }
            return Err(BackendError::new(
                "LLM_RESPONSE_INVALID",
                "Provider stream produced no text.",
                true,
                false,
            ));
        }
        Ok(full)
    }
}

fn commit_provider_binding_rotation(
    persist_binding: impl FnOnce() -> Result<(), BackendError>,
    retire_previous_secret: impl FnOnce() -> Result<(), BackendError>,
    restore_previous_binding: impl FnOnce() -> Result<(), BackendError>,
) -> Result<(), BackendError> {
    // Advance the durable binding before retiring the old credential. If
    // settings persistence fails, the previously working binding and its
    // secret remain intact instead of being destroyed mid-rotation.
    persist_binding()?;
    if let Err(retirement_error) = retire_previous_secret() {
        return match restore_previous_binding() {
            Ok(()) => Err(retirement_error),
            Err(restore_error) => Err(BackendError::new(
                "PROVIDER_CREDENTIAL_ROTATION_INDETERMINATE",
                "The previous credential could not be retired and its binding could not be restored. Re-open provider settings and re-authenticate before sending data.",
                false,
                true,
            )
            .with_details(serde_json::json!({
                "retirementErrorCode": retirement_error.code,
                "restoreErrorCode": restore_error.code,
            }))),
        };
    }
    Ok(())
}

fn normalize_provider_config(
    config: &mut LlmProviderConfig,
) -> Result<ProviderNetworkTarget, BackendError> {
    LlmService::validate_config_shape(config)?;
    let target = UrlPolicy
        .normalize_provider_endpoint(&config.base_url)
        .map_err(provider_target_error)?;
    let canonical_origin = UrlPolicy.canonical_origin(&target);
    let path = target.session.request_url.path();
    match config.provider {
        LlmProviderKind::OpenAi => {
            require_official_origin(&target, &canonical_origin, OPENAI_ORIGIN, path)?
        }
        LlmProviderKind::Anthropic => {
            require_official_origin(&target, &canonical_origin, ANTHROPIC_ORIGIN, path)?
        }
        LlmProviderKind::Google => {
            require_official_origin(&target, &canonical_origin, GOOGLE_ORIGIN, path)?
        }
        LlmProviderKind::Ollama if target.class != ProviderNetworkClass::LoopbackHttp => {
            return Err(BackendError::new(
                "OLLAMA_LOOPBACK_REQUIRED",
                "Ollama endpoints must use loopback HTTP.",
                true,
                true,
            ));
        }
        LlmProviderKind::Ollama | LlmProviderKind::Custom => {}
    }
    let mut normalized = target.session.request_url.clone();
    normalized.set_query(None);
    normalized.set_fragment(None);
    config.base_url = normalized.to_string().trim_end_matches('/').to_string();
    Ok(target)
}

fn require_official_origin(
    target: &ProviderNetworkTarget,
    actual: &str,
    expected: &str,
    path: &str,
) -> Result<(), BackendError> {
    if target.class != ProviderNetworkClass::PublicHttps
        || actual != expected
        || !matches!(path, "" | "/")
    {
        return Err(BackendError::new(
            "LLM_OFFICIAL_ORIGIN_REQUIRED",
            "Official providers must use their reviewed HTTPS origin.",
            true,
            true,
        ));
    }
    Ok(())
}

fn validate_binding(
    context: &ProjectContext,
    config: &LlmProviderConfig,
    binding: &ProviderCredentialBinding,
) -> Result<(), BackendError> {
    let mut normalized = config.clone();
    let target = normalize_provider_config(&mut normalized)?;
    let canonical_origin = UrlPolicy.canonical_origin(&target);
    let expected_account = SecretService::provider_binding_account_id(
        context,
        config.provider,
        &binding.config_id,
        &canonical_origin,
        binding.revision,
    )?;
    if binding.provider_kind != config.provider
        || binding.canonical_origin != canonical_origin
        || binding.credential_account_id != expected_account
    {
        return Err(provider_binding_changed());
    }
    Ok(())
}

fn provider_credential_transaction() -> Result<MutexGuard<'static, ()>, BackendError> {
    PROVIDER_CREDENTIAL_TRANSACTION.lock().map_err(|_| {
        BackendError::new(
            "PROVIDER_CREDENTIAL_TRANSACTION_FAILED",
            "Provider credential state is temporarily unavailable.",
            true,
            false,
        )
    })
}

async fn validated_provider_client(
    request_url: &str,
    timeout: Duration,
) -> Result<(reqwest::Client, url::Url), BackendError> {
    let target = UrlPolicy
        .normalize_provider_endpoint(request_url)
        .map_err(provider_target_error)?;
    let port = target
        .session
        .request_url
        .port_or_known_default()
        .ok_or_else(invalid_provider_url)?;
    let (host, resolved) = match target.session.request_url.host() {
        Some(Host::Domain(domain)) => {
            let resolved = tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::lookup_host((domain, port)),
            )
            .await
            .map_err(|_| {
                BackendError::new(
                    "LLM_DNS_TIMEOUT",
                    "Provider DNS resolution timed out.",
                    true,
                    false,
                )
            })?
            .map_err(|_| {
                BackendError::new(
                    "LLM_DNS_FAILED",
                    "Provider DNS resolution failed.",
                    true,
                    false,
                )
            })?
            .map(|address| address.ip())
            .collect::<Vec<_>>();
            (domain.to_string(), resolved)
        }
        Some(Host::Ipv4(ip)) => (ip.to_string(), vec![IpAddr::V4(ip)]),
        Some(Host::Ipv6(ip)) => (ip.to_string(), vec![IpAddr::V6(ip)]),
        None => return Err(invalid_provider_url()),
    };
    let connected = *resolved.first().ok_or_else(|| {
        BackendError::new(
            "LLM_DNS_FAILED",
            "Provider DNS returned no addresses.",
            true,
            false,
        )
    })?;
    let trusted_public_host = official_origin_for_host(&host).is_some();
    UrlPolicy
        .validate_provider_resolution(&target, &resolved, connected, trusted_public_host)
        .map_err(provider_target_error)?;
    let client = provider_http_client(&host, &target, connected, port, timeout)?;
    Ok((client, target.session.request_url))
}

fn provider_http_client(
    host: &str,
    target: &ProviderNetworkTarget,
    connected: IpAddr,
    port: u16,
    timeout: Duration,
) -> Result<reqwest::Client, BackendError> {
    let mut builder = reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout);
    if matches!(target.session.request_url.host(), Some(Host::Domain(_))) {
        builder = builder.resolve(&host, SocketAddr::new(connected, port));
    }
    let client = builder
        .build()
        .map_err(|error| BackendError::new("LLM_CLIENT_FAILED", error.to_string(), true, false))?;
    Ok(client)
}

fn official_origin_for_host(host: &str) -> Option<&'static str> {
    match host {
        "api.openai.com" => Some(OPENAI_ORIGIN),
        "api.anthropic.com" => Some(ANTHROPIC_ORIGIN),
        "generativelanguage.googleapis.com" => Some(GOOGLE_ORIGIN),
        _ => None,
    }
}

fn reject_redirect(response: &reqwest::Response) -> Result<(), BackendError> {
    if response.status().is_redirection() {
        return Err(BackendError::new(
            "LLM_REDIRECT_REJECTED",
            "Provider redirects are disabled; review and save the destination explicitly.",
            true,
            true,
        ));
    }
    Ok(())
}

fn provider_request_error(error: reqwest::Error) -> BackendError {
    let (code, message) = if error.is_timeout() {
        ("LLM_REQUEST_TIMEOUT", "Provider request timed out.")
    } else {
        ("LLM_REQUEST_FAILED", "Provider request failed.")
    };
    BackendError::new(code, message, true, false)
}

fn provider_target_error(error: BackendError) -> BackendError {
    BackendError::new(
        "LLM_DESTINATION_BLOCKED",
        "The provider destination was blocked by the network safety policy.",
        false,
        true,
    )
    .with_details(serde_json::json!({ "reasonCode": error.code }))
}

fn invalid_provider_url() -> BackendError {
    BackendError::new(
        "LLM_BASE_URL_INVALID",
        "Provider base URL is invalid.",
        true,
        true,
    )
}

fn provider_binding_required() -> BackendError {
    BackendError::new(
        "PROVIDER_CREDENTIAL_REAUTH_REQUIRED",
        "Save this provider destination and authorize its credential before using it.",
        true,
        true,
    )
}

fn provider_binding_changed() -> BackendError {
    BackendError::new(
        "PROVIDER_CREDENTIAL_BINDING_CHANGED",
        "The provider destination changed; review it and authorize the credential again.",
        true,
        true,
    )
}

fn extract_buffered_response(provider: LlmProviderKind, response: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(response).ok()?;
    extract_text(provider, &value)
}

fn llm_cancelled_error() -> BackendError {
    BackendError::new("LLM_CANCELLED", "Generation was cancelled.", true, false)
}

/// Append a byte-stream chunk without replacing a multibyte UTF-8 character
/// that happens to cross an HTTP chunk boundary. Invalid bytes are replaced
/// deliberately, but only after the decoder has established they are invalid.
fn append_utf8_chunk(pending: &mut Vec<u8>, chunk: &[u8], output: &mut String) {
    pending.extend_from_slice(chunk);
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                return;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    // `valid_up_to` bytes are guaranteed valid UTF-8.
                    output.push_str(std::str::from_utf8(&pending[..valid_up_to]).unwrap());
                    pending.drain(..valid_up_to);
                }
                if let Some(error_len) = error.error_len() {
                    let invalid_end = error_len.min(pending.len());
                    output.push_str(&String::from_utf8_lossy(&pending[..invalid_end]));
                    pending.drain(..invalid_end);
                    continue;
                }
                // An incomplete trailing code point stays buffered for the
                // next network chunk.
                return;
            }
        }
    }
}

fn extract_text(provider: LlmProviderKind, value: &serde_json::Value) -> Option<String> {
    match provider {
        LlmProviderKind::OpenAi | LlmProviderKind::Custom => value
            .pointer("/choices/0/message/content")?
            .as_str()
            .map(str::to_string),
        LlmProviderKind::Anthropic => value
            .pointer("/content/0/text")?
            .as_str()
            .map(str::to_string),
        LlmProviderKind::Google => value
            .pointer("/candidates/0/content/parts/0/text")?
            .as_str()
            .map(str::to_string),
        LlmProviderKind::Ollama => value
            .pointer("/message/content")?
            .as_str()
            .map(str::to_string),
    }
}

/// Extract a single incremental text delta from one streamed provider frame.
/// Returns `None` for frames that carry no text (e.g. OpenAI role deltas,
/// Anthropic `message_start` / `content_block_start`, SSE comments).
fn extract_stream_delta(provider: LlmProviderKind, value: &serde_json::Value) -> Option<String> {
    match provider {
        LlmProviderKind::OpenAi | LlmProviderKind::Custom => value
            .pointer("/choices/0/delta/content")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        LlmProviderKind::Anthropic => {
            // Only `content_block_delta` events carry text deltas; other event
            // types (message_start, content_block_start, message_delta) do not.
            if value.pointer("/type").and_then(|v| v.as_str()) == Some("content_block_delta") {
                value
                    .pointer("/delta/text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            } else {
                None
            }
        }
        LlmProviderKind::Google => value
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        LlmProviderKind::Ollama => value
            .pointer("/message/content")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

/// Parse one complete line from a provider stream into a text delta (if any).
///
/// OpenAI / Anthropic / Google emit SSE: `data: <json>` frames separated by
/// blank lines; OpenAI terminates with `data: [DONE]`. Ollama emits bare NDJSON
/// (one JSON object per line, no `data:` prefix). Non-data lines (SSE `event:`
/// routing lines, `:` keep-alive comments, blank lines) are ignored.
fn parse_stream_line(provider: LlmProviderKind, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if let Some(payload) = line.strip_prefix("data:") {
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        return extract_stream_delta(provider, &value);
    }
    // Ollama streams bare NDJSON (no "data:" prefix).
    if matches!(provider, LlmProviderKind::Ollama) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            return extract_stream_delta(provider, &value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider: LlmProviderKind, base_url: &str) -> LlmProviderConfig {
        LlmProviderConfig {
            provider,
            model: "test-model".into(),
            base_url: base_url.into(),
            context_window: 8_000,
            enabled: true,
        }
    }

    #[test]
    fn utf8_decoder_preserves_characters_split_across_chunks() {
        let mut pending = Vec::new();
        let mut output = String::new();
        append_utf8_chunk(&mut pending, &[0xE4], &mut output);
        append_utf8_chunk(&mut pending, &[0xBD, 0xA0, 0xE5, 0xA5], &mut output);
        append_utf8_chunk(&mut pending, &[0xBD], &mut output);
        assert_eq!(output, "你好");
        assert!(pending.is_empty());
    }

    #[test]
    fn extracts_openai_compatible_non_streaming_fallback() {
        let response = br#"{"choices":[{"message":{"content":"{\"overview\":\"ok\"}"}}]}"#;
        assert_eq!(
            extract_buffered_response(LlmProviderKind::Custom, response).as_deref(),
            Some("{\"overview\":\"ok\"}")
        );
        let response_with_newline = b"{\"choices\":[{\"message\":{\"content\":\"candidate\"}}]}\n";
        assert_eq!(
            extract_buffered_response(LlmProviderKind::OpenAi, response_with_newline).as_deref(),
            Some("candidate")
        );
    }

    #[test]
    fn builds_provider_specific_requests_without_leaking_secret_into_body() {
        let openai = LlmService::build_request(
            &config(LlmProviderKind::OpenAi, "https://api.openai.com"),
            Some("sk-test"),
            "compile",
        )
        .unwrap();
        assert!(openai.url.ends_with("/v1/chat/completions"));
        assert_eq!(
            openai.headers.get("authorization").unwrap(),
            "Bearer sk-test"
        );
        assert!(!openai.body.to_string().contains("sk-test"));
        let ollama = LlmService::build_request(
            &config(LlmProviderKind::Ollama, "http://localhost:11434"),
            None,
            "compile",
        )
        .unwrap();
        assert!(ollama.url.ends_with("/api/chat"));
    }

    #[test]
    fn rejects_missing_secret_and_unsafe_custom_urls() {
        let missing = LlmService::build_request(
            &config(LlmProviderKind::Anthropic, "https://api.anthropic.com"),
            None,
            "compile",
        )
        .unwrap_err();
        assert_eq!(missing.code, "LLM_SECRET_MISSING");
        let unsafe_url = LlmService::build_request(
            &config(LlmProviderKind::Custom, "file:///tmp/model"),
            Some("secret"),
            "compile",
        )
        .unwrap_err();
        assert_eq!(unsafe_url.code, "LLM_BASE_URL_INVALID");
        let embedded_secret = LlmService::build_request(
            &config(
                LlmProviderKind::Custom,
                "https://user:password@example.test?api_key=secret",
            ),
            Some("secret"),
            "compile",
        )
        .unwrap_err();
        assert_eq!(embedded_secret.code, "LLM_BASE_URL_SECRET_FORBIDDEN");
    }

    #[test]
    fn origin_change_deletes_old_bound_secret_and_requires_fresh_approval() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-origin-change-{stamp}"));
        let config_dir = std::env::temp_dir().join(format!("llm-origin-config-{stamp}"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&config_dir).unwrap();
        let context = ProjectContext::new("origin-change", root.clone());
        let settings_service = SettingsService::with_config_dir(config_dir.clone());
        let secrets = SecretService::memory();

        let (saved, mut first_binding) = LlmService::save_provider_internal(
            &context,
            config(LlmProviderKind::Custom, "https://one.example"),
            Some(&secrets),
            &settings_service,
        )
        .unwrap();
        first_binding.approved_at = Some("2026-08-18T00:00:00Z".into());
        secrets
            .set_bound(&context, &first_binding, "origin-one-secret")
            .unwrap();
        settings_service
            .save_provider_with_binding(&context, saved, first_binding.clone())
            .unwrap();

        let (_, unchanged) = LlmService::save_provider_internal(
            &context,
            config(LlmProviderKind::Custom, "https://one.example/"),
            Some(&secrets),
            &settings_service,
        )
        .unwrap();
        assert_eq!(unchanged.config_id, first_binding.config_id);
        assert_eq!(unchanged.revision, first_binding.revision);
        assert_eq!(
            secrets.get_bound(&context, &unchanged).unwrap().as_deref(),
            Some("origin-one-secret")
        );

        let (_, changed) = LlmService::save_provider_internal(
            &context,
            config(LlmProviderKind::Custom, "https://two.example"),
            Some(&secrets),
            &settings_service,
        )
        .unwrap();
        assert_ne!(changed.config_id, first_binding.config_id);
        assert!(changed.revision > first_binding.revision);
        assert!(changed.approved_at.is_none());
        assert_eq!(secrets.get_bound(&context, &first_binding).unwrap(), None);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn failed_provider_binding_persistence_never_retires_the_previous_secret() {
        let retired = std::cell::Cell::new(false);
        let error = commit_provider_binding_rotation(
            || {
                Err(BackendError::new(
                    "SETTINGS_WRITE_FAILED",
                    "fixture persistence failure",
                    true,
                    false,
                ))
            },
            || {
                retired.set(true);
                Ok(())
            },
            || Ok(()),
        )
        .expect_err("failed settings persistence must abort credential retirement");

        assert_eq!(error.code, "SETTINGS_WRITE_FAILED");
        assert!(!retired.get());
    }

    #[test]
    fn failed_previous_secret_retirement_restores_the_previous_binding() {
        let restored = std::cell::Cell::new(false);
        let error = commit_provider_binding_rotation(
            || Ok(()),
            || {
                Err(BackendError::new(
                    "CREDENTIAL_DELETE_FAILED",
                    "fixture retirement failure",
                    true,
                    true,
                ))
            },
            || {
                restored.set(true);
                Ok(())
            },
        )
        .expect_err("failed retirement must not leave the new binding committed");

        assert_eq!(error.code, "CREDENTIAL_DELETE_FAILED");
        assert!(restored.get());
    }

    #[test]
    fn failed_provider_rotation_compensation_reports_an_indeterminate_terminal() {
        let error = commit_provider_binding_rotation(
            || Ok(()),
            || {
                Err(BackendError::new(
                    "CREDENTIAL_DELETE_FAILED",
                    "fixture retirement failure",
                    true,
                    true,
                ))
            },
            || {
                Err(BackendError::new(
                    "SETTINGS_WRITE_FAILED",
                    "fixture compensation failure",
                    true,
                    false,
                ))
            },
        )
        .expect_err("double failure must expose an explicit terminal");

        assert_eq!(error.code, "PROVIDER_CREDENTIAL_ROTATION_INDETERMINATE");
        assert_eq!(
            error.details.unwrap()["retirementErrorCode"],
            "CREDENTIAL_DELETE_FAILED"
        );
    }

    #[test]
    fn concurrent_stale_secret_store_cannot_survive_origin_change() {
        use std::sync::{Arc, Barrier};

        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        let config_dir = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("provider-race", root.path().to_path_buf());
        let settings = Arc::new(SettingsService::with_config_dir(
            config_dir.path().to_path_buf(),
        ));
        let secrets = SecretService::memory();
        let (_, old_binding) = LlmService::save_provider_internal(
            &context,
            config(LlmProviderKind::Custom, "https://one.example"),
            Some(&secrets),
            &settings,
        )
        .unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let store_context = context.clone();
        let store_settings = Arc::clone(&settings);
        let store_secrets = secrets.clone();
        let store_barrier = Arc::clone(&barrier);
        let store_binding = old_binding.clone();
        let store_thread = std::thread::spawn(move || {
            store_barrier.wait();
            LlmService::approve_and_store_secret_internal(
                &store_context,
                &store_secrets,
                LlmProviderKind::Custom,
                &store_binding.config_id,
                store_binding.revision,
                &store_binding.canonical_origin,
                "stale-secret",
                &store_settings,
            )
        });

        let save_context = context.clone();
        let save_settings = Arc::clone(&settings);
        let save_secrets = secrets.clone();
        let save_barrier = Arc::clone(&barrier);
        let save_thread = std::thread::spawn(move || {
            save_barrier.wait();
            LlmService::save_provider_internal(
                &save_context,
                config(LlmProviderKind::Custom, "https://two.example"),
                Some(&save_secrets),
                &save_settings,
            )
        });

        barrier.wait();
        let store_result = store_thread.join().unwrap();
        let (_, current_binding) = save_thread.join().unwrap().unwrap();

        assert_eq!(current_binding.canonical_origin, "https://two.example");
        assert_eq!(
            settings.list_providers(&context).unwrap()[0].base_url,
            "https://two.example"
        );
        assert_eq!(
            secrets
                .get_account(&old_binding.credential_account_id)
                .unwrap(),
            None
        );
        if let Err(error) = store_result {
            assert_eq!(error.code, "PROVIDER_CREDENTIAL_BINDING_CHANGED");
        }
    }

    #[tokio::test]
    async fn self_signed_tls_is_rejected_before_any_http_request_or_secret_send() {
        use base64::Engine as _;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tokio::io::AsyncReadExt;
        use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        use tokio_rustls::rustls::ServerConfig;
        use tokio_rustls::TlsAcceptor;

        const CERT_DER: &str = "MIIC1DCCAbygAwIBAgIIG5gyR6DTJ2wwDQYJKoZIhvcNAQELBQAwGjEYMBYGA1UEAxMPc2VsZnNpZ25lZC50ZXN0MB4XDTI2MDgxNjE2NDEyMloXDTI2MDkxNjE2NDEyMlowGjEYMBYGA1UEAxMPc2VsZnNpZ25lZC50ZXN0MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAnCqtegFqCRV/XJPp4wSLv6ZoL5PKiC8auMbD147GyDxkIdpC2tBhwnMTeUEq7Rrnhg5T/zA1N0K6AzhuyvYasFO+MKPKivsPTVom47Aasy5ZLzRl0E47hBMfKwKKS7yFRt8MFIJOLkieTa018iSp5r76vyEdpFcZEi0YhtxjxhU9DA6hEf+Zah0nCNFV6XRRrvLMHP6OXdr0wtPemw60TKvQSM3/RWGoI8aeBDjgasuuomP0A3dqfG1WKVCUZ94myEjVQLezQdz44phhiTqyE3Bz9aKshE+kBW4/jk68LJznxnUNBT7qYoPeSL8nyvEUIBvmBD0V9dZ24TcPTKfP9QIDAQABox4wHDAaBgNVHREEEzARgg9zZWxmc2lnbmVkLnRlc3QwDQYJKoZIhvcNAQELBQADggEBADiwzhiQZ6gRWXWuEWe50xhQjcqh7wTQBt1zzTWuIBYhxHQuufvKIlDJ0FCR28Gene+Jc7d6fnXtueT5qmnyrw3ZsCiUCO09DUcNH9blO13+Yh5VqbnzGbDlG+jDnMiNYJcTNR1zciZBhaoPTbJ0gB8Bs7sdv8sTNvbn118hM5wW8Ot5xS47dE9LE+KW8qxhNEPXweVPx/QgQOWOuYjwIiS19qQt1BktGGgT/AWIi9RpJeYoiU0qaCVJaDPLfI8wVPOLVMGIquLFl9hHbTMXLnKstgZeMWvuQ7aEj2KK4YQbtB5x4x53Z99Ws+zIDJ3w54qOY6LNQgFemBKjIFhbkRY=";
        const KEY_DER: &str = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCcKq16AWoJFX9ck+njBIu/pmgvk8qILxq4xsPXjsbIPGQh2kLa0GHCcxN5QSrtGueGDlP/MDU3QroDOG7K9hqwU74wo8qK+w9NWibjsBqzLlkvNGXQTjuEEx8rAopLvIVG3wwUgk4uSJ5NrTXyJKnmvvq/IR2kVxkSLRiG3GPGFT0MDqER/5lqHScI0VXpdFGu8swc/o5d2vTC096bDrRMq9BIzf9FYagjxp4EOOBqy66iY/QDd2p8bVYpUJRn3ibISNVAt7NB3PjimGGJOrITcHP1oqyET6QFbj+OTrwsnOfGdQ0FPupig95IvyfK8RQgG+YEPRX11nbhNw9Mp8/1AgMBAAECggEAe1mGXqkBTR2K1OAMTIFJtN5GytWskrbKH4r4I6olvwFcghS428begM2OYycjNdcbapqkpBs63WQ6MtL/SBbt67qprhehovc9FfcQYqW14TPJw+xaQxeYEPFdnAZMoBfPGbSSAR0PjaVUTLx0sMde3+CXhCIvHKCjL+Uoy1UHBeyCq3i/7cqGdT7e2m7M8v8zSu9lONUdaBRWKEhINqJiobHtjd8s51PvI5Fkd1/ZDdwdXjPfsCoBe3anhPJNKogMTOSMOuxMWCfHSxYF1eDSdCX/SadhD8LG31/aGupNULQt0yKsXpNfLUdThZs71Eya4F4ccGOwo36KjiKgtfns3QKBgQDNRTWDl4LMq00ye6fJckQBYvpZ6lx1T9HCCkVThX+tj39RZlGNgzNytrnNKIuDT6Z/4Fn8kwGfiHQX8DQxV2lu4e3lFMX9ges6NIC8Y0I5gvM5SGcg6w2ilEomNxIgRHPOFRycAif/ZF0bJ5Jzgct477LGFoBj4NZqnMzt2lNqPwKBgQDCwtcZso52r6mxKJlIpR8A0/novlR2oFPup2nuCuiB7x/ypzdtVvqzJCRXUMYBL9TE1dMLDr/51pofcJWpwUnaLAMmu3Ks64IKT6Xix1n36McTvPXwmTHAd9u3IXVK12cq9Ke74nS4FNLedppmyw+kh/lz+ADPEPa9iaPGejlwywKBgGKNbPD+CD2NrSWkutz78GyeAcazv6pPJU09MyWzfaZts9n3/wWrTUMxOamnYrwrvKu+olWimu/mSp7Ho7dg2Wz0KgyHWbup6a7rUDeijEQie/YvrdvfHo/FFIiefiRh2RvDhRXd7yguHomQCT9NvMwWgUWbvg61/xv2pmk4Hj5vAoGAasyxa7QQj2Dwqudadw2lHK0hI9ILOyncHMjNO+3bZjUczdGIgXrq6wVssDzo94mlIXMn0a5686QMzCTOzVHjD7KG39x2nABhRQo8K0mqOln5oQdDznYTZDnV0GyWhz3roxCaUltyKeexYrCjJq8/mre9wSxENUhWJcWue45WpVUCgYEAt2ZBgRQgKLESM+yRyE93usH1DL2JmDxRBFbhCjkaanOQjRlTcN06hssuTpQLsFtPVX39P6YiKmBTz7hWLmeOg3/wRLeK5cx2WMLdg7LsThAUgDg8kycIPgIgTYO7gfMzVKsxtrkaHUwFiC4dcKtYNfV8Kx1jmDsYWbCgrXAye24=";

        let certificate = CertificateDer::from(
            base64::engine::general_purpose::STANDARD
                .decode(CERT_DER)
                .unwrap(),
        );
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            base64::engine::general_purpose::STANDARD
                .decode(KEY_DER)
                .unwrap(),
        ));
        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], private_key)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server_config));
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let request_count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&request_count);
        let worker = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            if let Ok(mut stream) = acceptor.accept(stream).await {
                let mut request = [0_u8; 4_096];
                if stream.read(&mut request).await.unwrap_or(0) > 0 {
                    observed.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        let target = UrlPolicy
            .normalize_provider_endpoint(&format!(
                "https://selfsigned.test:{port}/v1/chat/completions"
            ))
            .unwrap();
        let client = provider_http_client(
            "selfsigned.test",
            &target,
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port,
            Duration::from_secs(5),
        )
        .unwrap();
        let error = client
            .post(target.session.request_url)
            .header("Authorization", "Bearer tls-secret")
            .send()
            .await
            .unwrap_err();

        worker.await.unwrap();
        assert!(error.is_connect() || error.is_request());
        assert_eq!(request_count.load(Ordering::SeqCst), 0);
        assert!(!error.to_string().contains("tls-secret"));
    }

    #[test]
    fn streaming_request_enables_stream_and_uses_streaming_endpoints() {
        // OpenAI: stream flag on, same endpoint.
        let openai = LlmService::build_streaming_request(
            &config(LlmProviderKind::OpenAi, "https://api.openai.com"),
            Some("sk-test"),
            "compile",
        )
        .unwrap();
        assert!(openai.url.ends_with("/v1/chat/completions"));
        assert_eq!(openai.body["stream"], serde_json::json!(true));
        assert!(!openai.body.to_string().contains("sk-test"));

        // Anthropic: stream flag on.
        let anthropic = LlmService::build_streaming_request(
            &config(LlmProviderKind::Anthropic, "https://api.anthropic.com"),
            Some("sk-ant"),
            "compile",
        )
        .unwrap();
        assert_eq!(anthropic.body["stream"], serde_json::json!(true));

        // Google: switches to streamGenerateContent?alt=sse.
        let google = LlmService::build_streaming_request(
            &config(
                LlmProviderKind::Google,
                "https://generativelanguage.googleapis.com",
            ),
            Some("goog-key"),
            "compile",
        )
        .unwrap();
        assert!(google.url.ends_with(":streamGenerateContent?alt=sse"));

        // Ollama: stream flag on.
        let ollama = LlmService::build_streaming_request(
            &config(LlmProviderKind::Ollama, "http://localhost:11434"),
            None,
            "compile",
        )
        .unwrap();
        assert_eq!(ollama.body["stream"], serde_json::json!(true));
    }

    #[test]
    fn structured_streaming_request_uses_provider_json_output_contracts() {
        let custom = LlmService::build_structured_streaming_request(
            &config(LlmProviderKind::Custom, "https://api.deepseek.com"),
            Some("secret"),
            "Return JSON.",
        )
        .unwrap();
        assert_eq!(
            custom.body["response_format"],
            serde_json::json!({ "type": "json_object" })
        );
        assert_eq!(custom.body["stream"], serde_json::json!(true));

        let google = LlmService::build_structured_streaming_request(
            &config(
                LlmProviderKind::Google,
                "https://generativelanguage.googleapis.com",
            ),
            Some("secret"),
            "Return JSON.",
        )
        .unwrap();
        assert_eq!(
            google.body["generationConfig"]["responseMimeType"],
            serde_json::json!("application/json")
        );

        let ollama = LlmService::build_structured_streaming_request(
            &config(LlmProviderKind::Ollama, "http://localhost:11434"),
            None,
            "Return JSON.",
        )
        .unwrap();
        assert_eq!(ollama.body["format"], serde_json::json!("json"));
    }

    #[test]
    fn streaming_request_still_rejects_missing_secret() {
        let err = LlmService::build_streaming_request(
            &config(LlmProviderKind::Anthropic, "https://api.anthropic.com"),
            None,
            "compile",
        )
        .unwrap_err();
        assert_eq!(err.code, "LLM_SECRET_MISSING");
    }

    #[test]
    fn parse_stream_line_extracts_deltas_per_provider() {
        // OpenAI delta frame.
        let openai = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        assert_eq!(
            parse_stream_line(LlmProviderKind::OpenAi, openai).as_deref(),
            Some("Hello")
        );
        // OpenAI role-only delta (no content) → None.
        let role = r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_stream_line(LlmProviderKind::OpenAi, role), None);
        // OpenAI terminator.
        assert_eq!(
            parse_stream_line(LlmProviderKind::OpenAi, "data: [DONE]"),
            None
        );

        // Anthropic: only content_block_delta carries text.
        let anthropic_text =
            r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hi"}}"#;
        assert_eq!(
            parse_stream_line(LlmProviderKind::Anthropic, anthropic_text).as_deref(),
            Some("Hi")
        );
        let anthropic_start = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        assert_eq!(
            parse_stream_line(LlmProviderKind::Anthropic, anthropic_start),
            None
        );
        // SSE routing line is ignored.
        assert_eq!(
            parse_stream_line(LlmProviderKind::Anthropic, "event: ping"),
            None
        );

        // Google SSE frame.
        let google = r#"data: {"candidates":[{"content":{"parts":[{"text":"world"}]}}]}"#;
        assert_eq!(
            parse_stream_line(LlmProviderKind::Google, google).as_deref(),
            Some("world")
        );

        // Ollama NDJSON (no "data:" prefix).
        let ollama = r#"{"message":{"content":"ollama-token"},"done":false}"#;
        assert_eq!(
            parse_stream_line(LlmProviderKind::Ollama, ollama).as_deref(),
            Some("ollama-token")
        );
        let ollama_done = r#"{"message":{"content":""},"done":true}"#;
        // Empty content delta is extracted as "" — the streaming loop skips
        // empty deltas before forwarding, but the parser surfaces it faithfully.
        assert_eq!(
            parse_stream_line(LlmProviderKind::Ollama, ollama_done).as_deref(),
            Some("")
        );

        // Blank / comment lines are ignored.
        assert_eq!(parse_stream_line(LlmProviderKind::OpenAi, ""), None);
        assert_eq!(
            parse_stream_line(LlmProviderKind::OpenAi, ": keepalive"),
            None
        );
    }
}

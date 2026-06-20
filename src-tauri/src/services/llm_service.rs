use std::collections::BTreeMap;

use crate::errors::BackendError;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::services::FileStore;

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
        let path = context.resolve_project_path(".app/settings.json")?;
        if !path.exists() {
            return Ok(Vec::new());
        }
        let value: serde_json::Value = FileStore.read_json(context, ".app/settings.json")?;
        match value.get("llmProviders") {
            Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
                BackendError::new("SETTINGS_INVALID", error.to_string(), true, true)
            }),
            None => Ok(Vec::new()),
        }
    }

    pub fn save_provider(
        context: &ProjectContext,
        config: LlmProviderConfig,
    ) -> Result<(), BackendError> {
        Self::validate_config(&config)?;
        let path = context.resolve_project_path(".app/settings.json")?;
        let mut value: serde_json::Value = if path.exists() {
            FileStore.read_json(context, ".app/settings.json")?
        } else {
            serde_json::json!({})
        };
        let mut providers = Self::list_providers(context)?;
        providers.retain(|item| item.provider != config.provider);
        providers.push(config);
        providers.sort_by_key(|item| format!("{:?}", item.provider));
        value["llmProviders"] = serde_json::to_value(providers).map_err(|error| {
            BackendError::new("SETTINGS_SERIALIZE_FAILED", error.to_string(), false, false)
        })?;
        FileStore.write_json_atomic(context, ".app/settings.json", &value)
    }

    pub fn validate_config(config: &LlmProviderConfig) -> Result<(), BackendError> {
        if config.model.trim().is_empty() {
            return Err(BackendError::new(
                "LLM_MODEL_REQUIRED",
                "Model is required.",
                true,
                true,
            ));
        }
        let url = url::Url::parse(&config.base_url).map_err(|_| {
            BackendError::new(
                "LLM_BASE_URL_INVALID",
                "Provider base URL is invalid.",
                true,
                true,
            )
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(BackendError::new(
                "LLM_BASE_URL_INVALID",
                "Provider URL must use HTTP or HTTPS.",
                true,
                true,
            ));
        }
        let forbidden_query_key = url.query_pairs().any(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "key" | "api_key" | "apikey" | "token" | "access_token" | "authorization"
            )
        });
        if !url.username().is_empty() || url.password().is_some() || forbidden_query_key {
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

    pub async fn complete(
        &self,
        config: &LlmProviderConfig,
        secret: Option<&str>,
        prompt: &str,
    ) -> Result<String, BackendError> {
        let request = Self::build_request(config, secret, prompt)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|error| {
                BackendError::new("LLM_CLIENT_FAILED", error.to_string(), true, false)
            })?;
        let mut builder = client.post(&request.url).json(&request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.send().await.map_err(|error| {
            let (code, message) = if error.is_timeout() {
                ("LLM_REQUEST_TIMEOUT", "Provider request timed out.")
            } else {
                ("LLM_REQUEST_FAILED", "Provider request failed.")
            };
            BackendError::new(code, message, true, false)
        })?;
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
}

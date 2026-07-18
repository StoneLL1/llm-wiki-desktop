use std::collections::BTreeMap;

use crate::errors::BackendError;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::services::SettingsService;

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
    ) -> Result<(), BackendError> {
        Self::validate_config(&config)?;
        SettingsService::default().save_provider(context, config)?;
        Ok(())
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
        mut on_delta: D,
    ) -> Result<String, BackendError>
    where
        C: Fn() -> bool + Send + Sync,
        D: FnMut(&str) + Send,
    {
        if is_cancelled() {
            return Err(llm_cancelled_error());
        }
        let request = Self::build_streaming_request(config, secret, prompt)?;
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
        .map_err(|error| {
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

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let provider = config.provider;
        let mut full = String::new();
        let mut buf = String::new();
        let mut utf8_pending = Vec::new();
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

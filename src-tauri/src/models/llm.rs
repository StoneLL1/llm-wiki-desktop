use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderKind {
    OpenAi,
    Anthropic,
    Google,
    Ollama,
    Custom,
}

impl LlmProviderKind {
    pub const ALL: [Self; 5] = [
        Self::OpenAi,
        Self::Anthropic,
        Self::Google,
        Self::Ollama,
        Self::Custom,
    ];

    pub fn credential_account(self) -> &'static str {
        match self {
            Self::OpenAi => "provider.openai",
            Self::Anthropic => "provider.anthropic",
            Self::Google => "provider.google",
            Self::Ollama => "provider.ollama",
            Self::Custom => "provider.custom",
        }
    }

    pub fn requires_secret(self) -> bool {
        !matches!(self, Self::Ollama)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderConfig {
    pub provider: LlmProviderKind,
    pub model: String,
    pub base_url: String,
    pub context_window: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub config: LlmProviderConfig,
    pub has_secret: bool,
    pub secret_mask: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub config: LlmProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSecretRequest {
    pub provider: LlmProviderKind,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub provider: LlmProviderKind,
    pub ok: bool,
    pub message: String,
}

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

    pub fn binding_slug(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Ollama => "ollama",
            Self::Custom => "custom",
        }
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
pub struct ProviderCredentialBinding {
    pub config_id: String,
    pub provider_kind: LlmProviderKind,
    pub canonical_origin: String,
    pub credential_account_id: String,
    pub approved_at: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub config: LlmProviderConfig,
    pub credential_binding: Option<ProviderCredentialBinding>,
    pub has_secret: bool,
    pub secret_mask: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default)]
    pub force_refresh: bool,
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
    pub project_id: String,
    pub project_root_path: String,
    pub provider: LlmProviderKind,
    pub config_id: String,
    pub binding_revision: u64,
    pub expected_canonical_origin: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub provider: LlmProviderKind,
    pub config_id: String,
    pub binding_revision: u64,
    pub expected_canonical_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub provider: LlmProviderKind,
    pub ok: bool,
    pub message: String,
}

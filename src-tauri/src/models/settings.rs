use serde::{Deserialize, Serialize};

use crate::models::agent::AgentKind;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::project::ProjectTemplate;

fn default_language() -> String {
    "en".into()
}

fn default_context_window() -> u64 {
    32_000
}

fn default_check_updates() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    Auto,
}

impl ThemePreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    MinimizeToTray,
    Quit,
}

impl CloseBehavior {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MinimizeToTray => "minimize_to_tray",
            Self::Quit => "quit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub theme: ThemePreference,
    #[serde(default)]
    pub close_behavior: CloseBehavior,
    #[serde(default = "default_context_window")]
    pub context_window: u64,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(default)]
    pub agent_default: Option<AgentKind>,
    #[serde(default)]
    pub llm_providers: Vec<LlmProviderConfig>,
    #[serde(default)]
    pub template: ProjectTemplate,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: default_language(),
            theme: ThemePreference::Auto,
            close_behavior: CloseBehavior::MinimizeToTray,
            context_window: default_context_window(),
            check_updates: default_check_updates(),
            agent_default: None,
            llm_providers: Vec::new(),
            template: ProjectTemplate::General,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSettingsFile {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub theme: ThemePreference,
    #[serde(default)]
    pub close_behavior: CloseBehavior,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
}

impl Default for GlobalSettingsFile {
    fn default() -> Self {
        Self {
            language: default_language(),
            theme: ThemePreference::default(),
            close_behavior: CloseBehavior::default(),
            check_updates: default_check_updates(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsFile {
    #[serde(default = "default_context_window")]
    pub context_window: u64,
    #[serde(default)]
    pub agent_default: Option<AgentKind>,
    #[serde(default)]
    pub llm_providers: Vec<LlmProviderConfig>,
    #[serde(default)]
    pub template: ProjectTemplate,
}

impl Settings {
    pub fn apply_global(&mut self, global: GlobalSettingsFile) {
        self.language = global.language;
        self.theme = global.theme;
        self.close_behavior = global.close_behavior;
        self.check_updates = global.check_updates;
    }

    pub fn apply_project(&mut self, project: ProjectSettingsFile) {
        self.context_window = project.context_window;
        self.agent_default = project.agent_default;
        self.llm_providers = project.llm_providers;
        self.template = project.template;
    }

    pub fn to_global_file(&self) -> GlobalSettingsFile {
        GlobalSettingsFile {
            language: self.language.clone(),
            theme: self.theme,
            close_behavior: self.close_behavior,
            check_updates: self.check_updates,
        }
    }

    pub fn to_project_file(&self) -> ProjectSettingsFile {
        ProjectSettingsFile {
            context_window: self.context_window,
            agent_default: self.agent_default,
            llm_providers: self.llm_providers.clone(),
            template: self.template,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSecretStatusRequest {
    pub provider: LlmProviderKind,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CloseBehavior, Settings, ThemePreference};
    use crate::models::agent::AgentKind;

    #[test]
    fn settings_serde_uses_camel_case_fields() {
        let settings = Settings {
            language: "zh-CN".into(),
            theme: ThemePreference::Dark,
            close_behavior: CloseBehavior::Quit,
            context_window: 128_000,
            check_updates: false,
            agent_default: Some(AgentKind::Codex),
            ..Settings::default()
        };

        let value = serde_json::to_value(&settings).unwrap();

        assert_eq!(value["language"], json!("zh-CN"));
        assert_eq!(value["theme"], json!("dark"));
        assert_eq!(value["closeBehavior"], json!("quit"));
        assert_eq!(value["contextWindow"], json!(128_000));
        assert_eq!(value["checkUpdates"], json!(false));
        assert_eq!(value["agentDefault"], json!("codex"));
        assert!(value.get("close_behavior").is_none());
        assert!(value.get("context_window").is_none());
        assert!(value.get("check_updates").is_none());
        assert!(value.get("agent_default").is_none());
    }

    #[test]
    fn settings_round_trip_preserves_known_fields() {
        let raw = json!({
            "language": "en",
            "theme": "auto",
            "closeBehavior": "minimize_to_tray",
            "contextWindow": 32000,
            "checkUpdates": true,
            "agentDefault": "claude"
        });

        let settings: Settings = serde_json::from_value(raw.clone()).unwrap();

        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme, ThemePreference::Auto);
        assert_eq!(settings.close_behavior, CloseBehavior::MinimizeToTray);
        assert_eq!(settings.context_window, 32_000);
        assert!(settings.check_updates);
        assert_eq!(settings.agent_default, Some(AgentKind::Claude));
        assert_eq!(
            serde_json::to_value(settings).unwrap()["agentDefault"],
            raw["agentDefault"]
        );
    }
}

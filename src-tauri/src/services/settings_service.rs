use std::path::PathBuf;

use crate::errors::BackendError;
use crate::models::agent::AgentConfig;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::settings::{CloseBehavior, GlobalSettingsFile, ProjectSettingsFile, Settings};
use crate::services::{FileStore, SecretService};

pub struct SettingsService {
    config_dir: PathBuf,
}

impl Default for SettingsService {
    fn default() -> Self {
        Self {
            config_dir: default_config_dir(),
        }
    }
}

impl SettingsService {
    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub fn read_settings(&self, context: &ProjectContext) -> Result<Settings, BackendError> {
        let mut settings = Settings::default();
        settings.apply_global(self.read_global_settings()?);

        let project_path = context.resolve_project_path(".app/settings.json")?;
        if project_path.exists() {
            let project: ProjectSettingsFile =
                FileStore.read_json(context, ".app/settings.json")?;
            settings.apply_project(project);
        }

        let agent_config_path = context.resolve_project_path(".app/agent-config.json")?;
        if settings.agent_default.is_none() && agent_config_path.exists() {
            let agent_config: AgentConfig =
                FileStore.read_json(context, ".app/agent-config.json")?;
            settings.agent_default = agent_config.default_agent;
        }

        Ok(settings)
    }

    pub fn save_settings(
        &self,
        context: &ProjectContext,
        settings: &Settings,
    ) -> Result<Settings, BackendError> {
        let store = FileStore;
        store.ensure_absolute_dir(&self.config_dir)?;
        store
            .write_json_atomic_absolute(&self.global_settings_path(), &settings.to_global_file())?;
        store.write_json_atomic(context, ".app/settings.json", &settings.to_project_file())?;
        store.write_json_atomic(
            context,
            ".app/agent-config.json",
            &AgentConfig {
                default_agent: settings.agent_default,
            },
        )?;
        self.read_settings(context)
    }

    pub fn list_providers(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<LlmProviderConfig>, BackendError> {
        Ok(self.read_settings(context)?.llm_providers)
    }

    pub fn merge_providers(
        &self,
        context: &ProjectContext,
        providers: Vec<LlmProviderConfig>,
    ) -> Result<Settings, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings.llm_providers = providers;
        self.save_settings(context, &settings)
    }

    pub fn save_provider(
        &self,
        context: &ProjectContext,
        config: LlmProviderConfig,
    ) -> Result<Settings, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings
            .llm_providers
            .retain(|item| item.provider != config.provider);
        settings.llm_providers.push(config);
        settings
            .llm_providers
            .sort_by_key(|item| item.provider.credential_account().to_string());
        self.save_settings(context, &settings)
    }

    pub fn get_provider_secret_status(
        &self,
        secret_service: &SecretService,
        provider: LlmProviderKind,
    ) -> Result<Option<String>, BackendError> {
        Ok(secret_service
            .get(provider)?
            .map(|_| "configured".to_string()))
    }

    pub fn read_global_settings(&self) -> Result<GlobalSettingsFile, BackendError> {
        let global_path = self.global_settings_path();
        if !global_path.exists() {
            return Ok(GlobalSettingsFile::default());
        }
        FileStore.read_json_file(&global_path)
    }

    pub fn read_close_behavior(&self) -> CloseBehavior {
        self.read_global_settings()
            .map(|settings| settings.close_behavior)
            .unwrap_or_default()
    }

    fn global_settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
}

fn default_config_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("llm-wiki-desktop");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("llm-wiki-desktop");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("llm-wiki-desktop");
    }
    std::env::temp_dir().join("llm-wiki-desktop")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use super::SettingsService;
    use crate::models::agent::AgentKind;
    use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
    use crate::models::paths::ProjectContext;
    use crate::models::settings::{CloseBehavior, GlobalSettingsFile};
    use crate::services::{FileStore, SecretService};

    fn tmp_paths(suffix: &str) -> (ProjectContext, PathBuf, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-settings-{stamp}-{suffix}"));
        let config =
            std::env::temp_dir().join(format!("llm-wiki-settings-config-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        (ProjectContext::new("project-1", root.clone()), root, config)
    }

    #[test]
    fn reads_defaults_when_project_and_global_settings_are_missing() {
        let (context, root, config_dir) = tmp_paths("defaults");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let settings = service.read_settings(&context).unwrap();

        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme.as_str(), "auto");
        assert_eq!(settings.close_behavior.as_str(), "minimize_to_tray");
        assert!(settings.check_updates);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn saves_project_settings_without_secrets_and_keeps_global_settings_separate() {
        let (context, root, config_dir) = tmp_paths("save");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let secrets = SecretService::memory();
        secrets
            .set(LlmProviderKind::OpenAi, "sk-secret-1234")
            .unwrap();

        let mut settings = service.read_settings(&context).unwrap();
        settings.language = "zh-CN".into();
        settings.check_updates = false;
        settings.agent_default = Some(AgentKind::Codex);
        settings.llm_providers = vec![LlmProviderConfig {
            provider: LlmProviderKind::OpenAi,
            model: "gpt-4.1".into(),
            base_url: "https://api.openai.com".into(),
            context_window: 128_000,
            enabled: true,
        }];

        service.save_settings(&context, &settings).unwrap();

        let project_value: Value = FileStore.read_json(&context, ".app/settings.json").unwrap();
        let global_value: Value = FileStore
            .read_json_file(&config_dir.join("settings.json"))
            .unwrap();

        assert!(project_value.get("llmProviders").is_some());
        assert!(project_value.get("agentDefault").is_some());
        assert!(project_value.get("language").is_none());
        assert!(project_value.get("theme").is_none());
        assert!(project_value.to_string().contains("gpt-4.1"));
        assert!(!project_value.to_string().contains("sk-secret-1234"));
        assert_eq!(global_value["language"], "zh-CN");
        assert_eq!(global_value["checkUpdates"], false);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn provider_secret_status_never_reveals_any_secret_characters() {
        let service = SettingsService::default();
        let secrets = SecretService::memory();
        secrets
            .set(LlmProviderKind::Anthropic, "anthropic-secret-9876")
            .unwrap();

        let status = service
            .get_provider_secret_status(&secrets, LlmProviderKind::Anthropic)
            .unwrap();

        assert_eq!(status.as_deref(), Some("configured"));
        assert!(!status.unwrap().contains("9876"));
    }

    #[test]
    fn reads_close_behavior_from_global_settings() {
        let (_context, _root, config_dir) = tmp_paths("close-behavior");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let store = FileStore;
        store.ensure_absolute_dir(&config_dir).unwrap();
        store
            .write_json_atomic_absolute(
                &config_dir.join("settings.json"),
                &GlobalSettingsFile {
                    close_behavior: CloseBehavior::Quit,
                    ..GlobalSettingsFile::default()
                },
            )
            .unwrap();

        assert_eq!(service.read_close_behavior(), CloseBehavior::Quit);

        std::fs::remove_dir_all(config_dir).unwrap();
    }
}

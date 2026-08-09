use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::errors::BackendError;
use crate::models::agent::{AgentConfig, AgentKind};
use crate::models::import_v2_agent::AgentAssistancePolicy;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::settings::{
    ChatConvenienceAuthorization, CloseBehavior, GlobalSettingsFile, GlobalUiPreferences,
    ProjectSettingsFile, Settings,
};
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

        if let Some(settings_path) = context.layout.settings_path.as_deref() {
            let project_path = context.resolve_project_path(settings_path)?;
            if project_path.exists() {
                let project: ProjectSettingsFile = FileStore.read_json(context, settings_path)?;
                settings.apply_project(project);
            }
        }

        if let Some(agent_config_path) = context.layout.agent_config_path.as_deref() {
            let agent_config_file = context.resolve_project_path(agent_config_path)?;
            if agent_config_file.exists() {
                let agent_config: AgentConfig = FileStore.read_json(context, agent_config_path)?;
                settings.agent_default = agent_config.default_agent;
            }
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
        let _guard = self.lock_global_settings()?;
        let mut global = settings.to_global_file();
        let existing_global = self.read_global_settings()?;
        global.chat_convenience_authorizations = existing_global.chat_convenience_authorizations;
        global.remote_provider_disclosure_revision =
            existing_global.remote_provider_disclosure_revision;
        store.write_json_atomic_absolute(&self.global_settings_path(), &global)?;
        let settings_path =
            project_state_path(context, context.layout.settings_path.as_deref(), "settings")?;
        let agent_config_path = project_state_path(
            context,
            context.layout.agent_config_path.as_deref(),
            "agent configuration",
        )?;
        store.write_json_atomic(context, settings_path, &settings.to_project_file())?;
        store.write_json_atomic(
            context,
            agent_config_path,
            &AgentConfig {
                default_agent: settings.agent_default,
            },
        )?;
        self.read_settings(context)
    }

    pub fn save_agent_default(
        &self,
        context: &ProjectContext,
        agent: Option<AgentKind>,
    ) -> Result<AgentConfig, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings.agent_default = agent;
        let config = AgentConfig {
            default_agent: agent,
        };
        let store = FileStore;
        let settings_path =
            project_state_path(context, context.layout.settings_path.as_deref(), "settings")?;
        let agent_config_path = project_state_path(
            context,
            context.layout.agent_config_path.as_deref(),
            "agent configuration",
        )?;
        store.write_json_atomic(context, agent_config_path, &config)?;
        store.write_json_atomic(context, settings_path, &settings.to_project_file())?;
        Ok(config)
    }

    pub fn get_import_agent_policy(
        &self,
        context: &ProjectContext,
    ) -> Result<AgentAssistancePolicy, BackendError> {
        Ok(self.read_settings(context)?.import_agent_policy)
    }

    pub fn set_import_agent_policy(
        &self,
        context: &ProjectContext,
        policy: AgentAssistancePolicy,
        local_agent_kind: Option<AgentKind>,
    ) -> Result<AgentAssistancePolicy, BackendError> {
        if policy.max_attempts_per_item == 0 || policy.max_attempts_per_item > 3 {
            return Err(BackendError::new(
                "IMPORT_AGENT_POLICY_INVALID",
                "Agent assistance attempt budget must be between one and three.",
                false,
                true,
            ));
        }
        let mut settings = self.read_settings(context)?;
        settings.import_agent_policy = policy.clone();
        settings.agent_default = local_agent_kind;
        self.save_settings(context, &settings)?;
        Ok(policy)
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

    pub fn read_global_ui_preferences(&self) -> Result<GlobalUiPreferences, BackendError> {
        let settings = self.read_global_settings()?;
        Ok(GlobalUiPreferences {
            language: settings.language,
            theme: settings.theme,
        })
    }

    pub fn save_global_ui_preferences(
        &self,
        preferences: GlobalUiPreferences,
    ) -> Result<GlobalUiPreferences, BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        settings.language = preferences.language;
        settings.theme = preferences.theme;
        let store = FileStore;
        store.ensure_absolute_dir(&self.config_dir)?;
        store.write_json_atomic_absolute(&self.global_settings_path(), &settings)?;
        Ok(GlobalUiPreferences {
            language: settings.language,
            theme: settings.theme,
        })
    }

    pub fn is_remote_provider_disclosure_acknowledged(
        &self,
        revision: &str,
    ) -> Result<bool, BackendError> {
        Ok(self
            .read_global_settings()?
            .remote_provider_disclosure_revision
            .as_deref()
            == Some(revision))
    }

    pub fn acknowledge_remote_provider_disclosure(
        &self,
        revision: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        settings.remote_provider_disclosure_revision = Some(revision.to_string());
        let store = FileStore;
        store.ensure_absolute_dir(&self.config_dir)?;
        store.write_json_atomic_absolute(&self.global_settings_path(), &settings)
    }

    /// Read the user's UI/content language preference from global settings.
    /// Used by the tray menu (no project context available at tray-build
    /// time) and anywhere else that needs the language without a project.
    pub fn read_language(&self) -> String {
        self.read_global_settings()
            .map(|settings| settings.language)
            .unwrap_or_else(|_| "en".to_string())
    }

    pub fn get_chat_convenience_authorization(
        &self,
        context: &ProjectContext,
    ) -> Result<ChatConvenienceAuthorization, BackendError> {
        let root_path_fingerprint = project_root_fingerprint(&context.root);
        let settings = self.read_global_settings()?;

        Ok(settings
            .chat_convenience_authorizations
            .into_iter()
            .rev()
            .find(|authorization| {
                authorization.project_id == context.project_id
                    && authorization.root_path_fingerprint == root_path_fingerprint
            })
            .unwrap_or(ChatConvenienceAuthorization {
                enabled: false,
                confirmed_at: String::new(),
                project_id: context.project_id.clone(),
                root_path_fingerprint,
            }))
    }

    pub fn set_chat_convenience_authorization(
        &self,
        context: &ProjectContext,
        enabled: bool,
    ) -> Result<ChatConvenienceAuthorization, BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        let root_path_fingerprint = project_root_fingerprint(&context.root);
        settings
            .chat_convenience_authorizations
            .retain(|authorization| {
                authorization.project_id != context.project_id
                    || authorization.root_path_fingerprint != root_path_fingerprint
            });

        let authorization = if enabled {
            ChatConvenienceAuthorization {
                enabled,
                confirmed_at: chrono::Utc::now().to_rfc3339(),
                project_id: context.project_id.clone(),
                root_path_fingerprint,
            }
        } else {
            ChatConvenienceAuthorization {
                enabled: false,
                confirmed_at: String::new(),
                project_id: context.project_id.clone(),
                root_path_fingerprint,
            }
        };
        if enabled {
            settings
                .chat_convenience_authorizations
                .push(authorization.clone());
        }

        let store = FileStore;
        store.ensure_absolute_dir(&self.config_dir)?;
        store.write_json_atomic_absolute(&self.global_settings_path(), &settings)?;

        Ok(authorization)
    }

    pub fn revoke_all_chat_convenience_authorizations(&self) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        settings.chat_convenience_authorizations.clear();
        let store = FileStore;
        store.ensure_absolute_dir(&self.config_dir)?;
        store.write_json_atomic_absolute(&self.global_settings_path(), &settings)
    }

    fn global_settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    fn lock_global_settings(&self) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        global_settings_lock().lock().map_err(|_| {
            BackendError::new(
                "SETTINGS_LOCKED",
                "Settings are currently unavailable.",
                true,
                false,
            )
        })
    }
}

fn project_state_path<'a>(
    _context: &ProjectContext,
    path: Option<&'a str>,
    feature: &str,
) -> Result<&'a str, BackendError> {
    path.ok_or_else(|| {
        BackendError::new(
            "PROJECT_LAYOUT_STATE_UNAVAILABLE",
            format!(
                "Project {feature} state is unavailable until compatible features are enabled."
            ),
            true,
            true,
        )
    })
}

fn global_settings_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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

fn project_root_fingerprint(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use super::SettingsService;
    use crate::models::agent::{AgentConfig, AgentKind};
    use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
    use crate::models::paths::ProjectContext;
    use crate::models::settings::{
        CloseBehavior, GlobalSettingsFile, GlobalUiPreferences, Settings, ThemePreference,
    };
    use crate::services::{AgentService, FileStore, SecretService};

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
    fn agent_config_is_canonical_when_legacy_settings_disagree() {
        let (context, root, config_dir) = tmp_paths("agent-canonical");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let mut settings = service.read_settings(&context).unwrap();
        settings.agent_default = Some(AgentKind::Claude);
        service.save_settings(&context, &settings).unwrap();
        AgentService::save_config(
            &context,
            &AgentConfig {
                default_agent: Some(AgentKind::Codex),
            },
        )
        .unwrap();

        let reloaded = service.read_settings(&context).unwrap();

        assert_eq!(reloaded.agent_default, Some(AgentKind::Codex));
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn save_agent_default_synchronizes_project_files() {
        let (context, root, config_dir) = tmp_paths("agent-save");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let saved = service
            .save_agent_default(&context, Some(AgentKind::Codex))
            .unwrap();

        assert_eq!(saved.default_agent, Some(AgentKind::Codex));
        assert_eq!(
            service.read_settings(&context).unwrap().agent_default,
            Some(AgentKind::Codex)
        );
        let project: Value = FileStore.read_json(&context, ".app/settings.json").unwrap();
        assert_eq!(project["agentDefault"], "codex");
        assert_eq!(
            AgentService::load_config(&context).unwrap().default_agent,
            Some(AgentKind::Codex)
        );

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

    #[test]
    fn read_language_returns_global_preference_or_english_default() {
        // The tray menu reads language through read_language() with no project
        // context, so it must reflect the global settings file.
        let (_context, _root, config_dir) = tmp_paths("language");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let store = FileStore;
        store.ensure_absolute_dir(&config_dir).unwrap();
        store
            .write_json_atomic_absolute(
                &config_dir.join("settings.json"),
                &GlobalSettingsFile {
                    language: "zh-CN".into(),
                    ..GlobalSettingsFile::default()
                },
            )
            .unwrap();

        assert_eq!(service.read_language(), "zh-CN");

        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn read_language_defaults_to_english_when_settings_missing() {
        let (_context, _root, config_dir) = tmp_paths("language-default");
        let service = SettingsService::with_config_dir(config_dir.clone());
        // No settings file written.
        assert_eq!(service.read_language(), "en");
    }

    #[test]
    fn chat_convenience_authorization_is_global_only() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-auth");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let saved = service
            .set_chat_convenience_authorization(&context, true)
            .unwrap();

        assert!(saved.enabled);
        assert_eq!(saved.project_id, context.project_id);
        assert!(saved.root_path_fingerprint.len() >= 16);

        let global: serde_json::Value = FileStore
            .read_json_file(&config_dir.join("settings.json"))
            .unwrap();
        assert!(global["chatConvenienceAuthorizations"].is_array());
        assert!(!context
            .resolve_project_path(".app/settings.json")
            .unwrap()
            .exists());

        let loaded = service
            .get_chat_convenience_authorization(&context)
            .unwrap();
        assert!(loaded.enabled);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn chat_convenience_authorization_can_be_revoked_for_project() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-revoke");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .set_chat_convenience_authorization(&context, true)
            .unwrap();
        let revoked = service
            .set_chat_convenience_authorization(&context, false)
            .unwrap();

        assert!(!revoked.enabled);
        assert!(
            !service
                .get_chat_convenience_authorization(&context)
                .unwrap()
                .enabled
        );
        let global: serde_json::Value = FileStore
            .read_json_file(&config_dir.join("settings.json"))
            .unwrap();
        assert_eq!(
            global["chatConvenienceAuthorizations"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn save_settings_preserves_chat_convenience_authorizations() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-save-preserves");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .set_chat_convenience_authorization(&context, true)
            .unwrap();
        let mut settings = service.read_settings(&context).unwrap();
        settings.language = "zh-CN".into();

        service.save_settings(&context, &settings).unwrap();

        assert!(
            service
                .get_chat_convenience_authorization(&context)
                .unwrap()
                .enabled
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn remote_provider_disclosure_is_global_versioned_and_survives_ui_saves() {
        let (context, root, config_dir) = tmp_paths("workflow-remote-disclosure");
        let service = SettingsService::with_config_dir(config_dir.clone());

        assert!(!service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v1")
            .unwrap());
        service
            .acknowledge_remote_provider_disclosure("workflow-remote-provider-v1")
            .unwrap();
        assert!(service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v1")
            .unwrap());
        assert!(!service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v2")
            .unwrap());

        let mut settings = service.read_settings(&context).unwrap();
        settings.language = "zh-CN".into();
        service.save_settings(&context, &settings).unwrap();
        assert!(service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v1")
            .unwrap());
        let project_settings: serde_json::Value =
            FileStore.read_json(&context, ".app/settings.json").unwrap();
        assert!(project_settings
            .get("remoteProviderDisclosureRevision")
            .is_none());

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn saves_global_ui_preferences_without_project_context() {
        let (_context, root, config_dir) = tmp_paths("global-ui-preferences");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let saved = service
            .save_global_ui_preferences(GlobalUiPreferences {
                language: "zh-CN".into(),
                theme: ThemePreference::Dark,
            })
            .unwrap();

        assert_eq!(saved.language, "zh-CN");
        assert_eq!(saved.theme, ThemePreference::Dark);
        assert_eq!(
            service.read_global_ui_preferences().unwrap(),
            GlobalUiPreferences {
                language: "zh-CN".into(),
                theme: ThemePreference::Dark,
            }
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn save_settings_does_not_overwrite_unreadable_global_settings() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-corrupt-global");
        let service = SettingsService::with_config_dir(config_dir.clone());
        std::fs::create_dir_all(&config_dir).unwrap();
        let global_path = config_dir.join("settings.json");
        std::fs::write(&global_path, "{not-json").unwrap();
        let settings = Settings::default();

        let error = service
            .save_settings(&context, &settings)
            .expect_err("corrupt global settings must not be overwritten");

        assert_eq!(error.code, "JSON_PARSE_FAILED");
        assert_eq!(std::fs::read_to_string(&global_path).unwrap(), "{not-json");

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }
}

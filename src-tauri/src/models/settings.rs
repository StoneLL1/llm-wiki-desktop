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

fn default_max_tokens() -> u64 {
    4096
}

fn default_temperature() -> f64 {
    0.3
}

fn default_agent_task_timeout_secs() -> u64 {
    300
}

fn default_install_command_display_only() -> bool {
    true
}

fn default_prompt_on_new_agent() -> bool {
    true
}

fn default_skill_autoload() -> bool {
    true
}

fn default_max_concurrent_tasks() -> u64 {
    2
}

fn default_auto_git_checkpoint() -> bool {
    true
}

fn default_manual_edit_protection() -> bool {
    true
}

fn default_raw_sources_immutable() -> bool {
    true
}

fn default_associate_md_files() -> bool {
    true
}

fn default_prompt_changelog_before_install() -> bool {
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
#[serde(rename_all = "camelCase")]
pub enum ColorThemePresetId {
    #[default]
    Codex,
    Paper,
    Graphite,
    Mint,
    Night,
    HighContrast,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    MinimizeToTray,
    Ask,
    Quit,
}

impl CloseBehavior {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MinimizeToTray => "minimize_to_tray",
            Self::Ask => "ask",
            Self::Quit => "quit",
        }
    }
}

/// UI density (design app.css `.seg` 紧凑/标准/舒适). Drives spacing tokens.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DensityPreference {
    Compact,
    #[default]
    Standard,
    Comfortable,
}

impl DensityPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Comfortable => "comfortable",
        }
    }
}

/// App startup behavior (design settings.html:118-130).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartupBehavior {
    #[default]
    OpenLastProject,
    ShowProjectPicker,
    AutoOpenByCondition,
}

/// Agent content output language preference (design settings.html:268-282).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputLanguage {
    #[default]
    FollowUi,
    AlwaysChinese,
    AlwaysEnglish,
    FollowSource,
}

impl AgentOutputLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FollowUi => "follow_ui",
            Self::AlwaysChinese => "always_chinese",
            Self::AlwaysEnglish => "always_english",
            Self::FollowSource => "follow_source",
        }
    }
}

/// Automatic update check frequency (design settings.html:596-605).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    #[default]
    Daily,
    Weekly,
    Never,
}

impl UpdateFrequency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Never => "never",
        }
    }
}

/// What a notification click opens (design settings.html:549-562).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationClickBehavior {
    #[default]
    ResultPage,
    ErrorLog,
    DiffConfirmPage,
    ActivateWindowOnly,
}

impl NotificationClickBehavior {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResultPage => "result_page",
            Self::ErrorLog => "error_log",
            Self::DiffConfirmPage => "diff_confirm_page",
            Self::ActivateWindowOnly => "activate_window_only",
        }
    }
}

/// Per-provider system-notification toggle set (design settings.html:536-547).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemNotificationPrefs {
    #[serde(default = "default_true")]
    pub on_task_completed: bool,
    #[serde(default = "default_true")]
    pub on_task_failed: bool,
    #[serde(default = "default_true")]
    pub on_confirmation_needed: bool,
    #[serde(default)]
    pub on_long_task_progress: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SystemNotificationPrefs {
    fn default() -> Self {
        Self {
            on_task_completed: true,
            on_task_failed: true,
            on_confirmation_needed: true,
            on_long_task_progress: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatConvenienceAuthorization {
    pub enabled: bool,
    pub confirmed_at: String,
    pub project_id: String,
    pub root_path_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    // --- Global (cross-project UI / tray / notification / update) ---
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub theme: ThemePreference,
    #[serde(default)]
    pub color_theme_preset: ColorThemePresetId,
    #[serde(default)]
    pub density: DensityPreference,
    #[serde(default)]
    pub ui_font: String,
    #[serde(default)]
    pub reading_font: String,
    #[serde(default)]
    pub code_font: String,
    #[serde(default)]
    pub agent_output_language: AgentOutputLanguage,
    #[serde(default)]
    pub close_behavior: CloseBehavior,
    #[serde(default)]
    pub system_notifications: SystemNotificationPrefs,
    #[serde(default)]
    pub notification_click_behavior: NotificationClickBehavior,
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: u64,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(default)]
    pub update_frequency: UpdateFrequency,
    #[serde(default)]
    pub auto_download_updates: bool,
    #[serde(default = "default_prompt_changelog_before_install")]
    pub prompt_changelog_before_install: bool,
    #[serde(default)]
    pub startup_behavior: StartupBehavior,
    #[serde(default)]
    pub default_project_location: String,
    #[serde(default)]
    pub external_editor: String,
    #[serde(default = "default_associate_md_files")]
    pub associate_md_files: bool,
    #[serde(default)]
    pub associate_wiki_folders: bool,
    #[serde(default)]
    pub chat_convenience_authorizations: Vec<ChatConvenienceAuthorization>,
    // --- Project (affects this project's Agent / Git / context behavior) ---
    #[serde(default = "default_context_window")]
    pub context_window: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_agent_task_timeout_secs")]
    pub agent_task_timeout_secs: u64,
    #[serde(default)]
    pub allow_agent_install: bool,
    #[serde(default = "default_install_command_display_only")]
    pub install_command_display_only: bool,
    #[serde(default = "default_prompt_on_new_agent")]
    pub prompt_on_new_agent: bool,
    #[serde(default = "default_skill_autoload")]
    pub skill_autoload: bool,
    #[serde(default = "default_auto_git_checkpoint")]
    pub auto_git_checkpoint: bool,
    #[serde(default = "default_manual_edit_protection")]
    pub manual_edit_protection: bool,
    #[serde(default = "default_raw_sources_immutable")]
    pub raw_sources_immutable: bool,
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
            color_theme_preset: ColorThemePresetId::Codex,
            density: DensityPreference::Standard,
            ui_font: String::new(),
            reading_font: String::new(),
            code_font: String::new(),
            agent_output_language: AgentOutputLanguage::FollowUi,
            close_behavior: CloseBehavior::MinimizeToTray,
            system_notifications: SystemNotificationPrefs::default(),
            notification_click_behavior: NotificationClickBehavior::ResultPage,
            max_concurrent_tasks: default_max_concurrent_tasks(),
            check_updates: default_check_updates(),
            update_frequency: UpdateFrequency::Daily,
            auto_download_updates: false,
            prompt_changelog_before_install: default_prompt_changelog_before_install(),
            startup_behavior: StartupBehavior::OpenLastProject,
            default_project_location: String::new(),
            external_editor: String::new(),
            associate_md_files: default_associate_md_files(),
            associate_wiki_folders: false,
            chat_convenience_authorizations: Vec::new(),
            context_window: default_context_window(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            agent_task_timeout_secs: default_agent_task_timeout_secs(),
            allow_agent_install: false,
            install_command_display_only: default_install_command_display_only(),
            prompt_on_new_agent: default_prompt_on_new_agent(),
            skill_autoload: default_skill_autoload(),
            auto_git_checkpoint: default_auto_git_checkpoint(),
            manual_edit_protection: default_manual_edit_protection(),
            raw_sources_immutable: default_raw_sources_immutable(),
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
    pub color_theme_preset: ColorThemePresetId,
    #[serde(default)]
    pub density: DensityPreference,
    #[serde(default)]
    pub ui_font: String,
    #[serde(default)]
    pub reading_font: String,
    #[serde(default)]
    pub code_font: String,
    #[serde(default)]
    pub agent_output_language: AgentOutputLanguage,
    #[serde(default)]
    pub close_behavior: CloseBehavior,
    #[serde(default)]
    pub system_notifications: SystemNotificationPrefs,
    #[serde(default)]
    pub notification_click_behavior: NotificationClickBehavior,
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: u64,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(default)]
    pub update_frequency: UpdateFrequency,
    #[serde(default)]
    pub auto_download_updates: bool,
    #[serde(default = "default_prompt_changelog_before_install")]
    pub prompt_changelog_before_install: bool,
    #[serde(default)]
    pub startup_behavior: StartupBehavior,
    #[serde(default)]
    pub default_project_location: String,
    #[serde(default)]
    pub external_editor: String,
    #[serde(default = "default_associate_md_files")]
    pub associate_md_files: bool,
    #[serde(default)]
    pub associate_wiki_folders: bool,
    #[serde(default)]
    pub chat_convenience_authorizations: Vec<ChatConvenienceAuthorization>,
}

impl Default for GlobalSettingsFile {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            language: settings.language,
            theme: settings.theme,
            color_theme_preset: settings.color_theme_preset,
            density: settings.density,
            ui_font: settings.ui_font,
            reading_font: settings.reading_font,
            code_font: settings.code_font,
            agent_output_language: settings.agent_output_language,
            close_behavior: settings.close_behavior,
            system_notifications: settings.system_notifications,
            notification_click_behavior: settings.notification_click_behavior,
            max_concurrent_tasks: settings.max_concurrent_tasks,
            check_updates: settings.check_updates,
            update_frequency: settings.update_frequency,
            auto_download_updates: settings.auto_download_updates,
            prompt_changelog_before_install: settings.prompt_changelog_before_install,
            startup_behavior: settings.startup_behavior,
            default_project_location: settings.default_project_location,
            external_editor: settings.external_editor,
            associate_md_files: settings.associate_md_files,
            associate_wiki_folders: settings.associate_wiki_folders,
            chat_convenience_authorizations: settings.chat_convenience_authorizations,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsFile {
    #[serde(default = "default_context_window")]
    pub context_window: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_agent_task_timeout_secs")]
    pub agent_task_timeout_secs: u64,
    #[serde(default)]
    pub allow_agent_install: bool,
    #[serde(default = "default_install_command_display_only")]
    pub install_command_display_only: bool,
    #[serde(default = "default_prompt_on_new_agent")]
    pub prompt_on_new_agent: bool,
    #[serde(default = "default_skill_autoload")]
    pub skill_autoload: bool,
    #[serde(default = "default_auto_git_checkpoint")]
    pub auto_git_checkpoint: bool,
    #[serde(default = "default_manual_edit_protection")]
    pub manual_edit_protection: bool,
    #[serde(default = "default_raw_sources_immutable")]
    pub raw_sources_immutable: bool,
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
        self.color_theme_preset = global.color_theme_preset;
        self.density = global.density;
        self.ui_font = global.ui_font;
        self.reading_font = global.reading_font;
        self.code_font = global.code_font;
        self.agent_output_language = global.agent_output_language;
        self.close_behavior = global.close_behavior;
        self.system_notifications = global.system_notifications;
        self.notification_click_behavior = global.notification_click_behavior;
        self.max_concurrent_tasks = global.max_concurrent_tasks;
        self.check_updates = global.check_updates;
        self.update_frequency = global.update_frequency;
        self.auto_download_updates = global.auto_download_updates;
        self.prompt_changelog_before_install = global.prompt_changelog_before_install;
        self.startup_behavior = global.startup_behavior;
        self.default_project_location = global.default_project_location;
        self.external_editor = global.external_editor;
        self.associate_md_files = global.associate_md_files;
        self.associate_wiki_folders = global.associate_wiki_folders;
        self.chat_convenience_authorizations = global.chat_convenience_authorizations;
    }

    pub fn apply_project(&mut self, project: ProjectSettingsFile) {
        self.context_window = project.context_window;
        self.max_tokens = project.max_tokens;
        self.temperature = project.temperature;
        self.agent_task_timeout_secs = project.agent_task_timeout_secs;
        self.allow_agent_install = project.allow_agent_install;
        self.install_command_display_only = project.install_command_display_only;
        self.prompt_on_new_agent = project.prompt_on_new_agent;
        self.skill_autoload = project.skill_autoload;
        self.auto_git_checkpoint = project.auto_git_checkpoint;
        self.manual_edit_protection = project.manual_edit_protection;
        self.raw_sources_immutable = project.raw_sources_immutable;
        self.agent_default = project.agent_default;
        self.llm_providers = project.llm_providers;
        self.template = project.template;
    }

    pub fn to_global_file(&self) -> GlobalSettingsFile {
        GlobalSettingsFile {
            language: self.language.clone(),
            theme: self.theme,
            color_theme_preset: self.color_theme_preset,
            density: self.density,
            ui_font: self.ui_font.clone(),
            reading_font: self.reading_font.clone(),
            code_font: self.code_font.clone(),
            agent_output_language: self.agent_output_language,
            close_behavior: self.close_behavior,
            system_notifications: self.system_notifications.clone(),
            notification_click_behavior: self.notification_click_behavior,
            max_concurrent_tasks: self.max_concurrent_tasks,
            check_updates: self.check_updates,
            update_frequency: self.update_frequency,
            auto_download_updates: self.auto_download_updates,
            prompt_changelog_before_install: self.prompt_changelog_before_install,
            startup_behavior: self.startup_behavior,
            default_project_location: self.default_project_location.clone(),
            external_editor: self.external_editor.clone(),
            associate_md_files: self.associate_md_files,
            associate_wiki_folders: self.associate_wiki_folders,
            chat_convenience_authorizations: self.chat_convenience_authorizations.clone(),
        }
    }

    pub fn to_project_file(&self) -> ProjectSettingsFile {
        ProjectSettingsFile {
            context_window: self.context_window,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            agent_task_timeout_secs: self.agent_task_timeout_secs,
            allow_agent_install: self.allow_agent_install,
            install_command_display_only: self.install_command_display_only,
            prompt_on_new_agent: self.prompt_on_new_agent,
            skill_autoload: self.skill_autoload,
            auto_git_checkpoint: self.auto_git_checkpoint,
            manual_edit_protection: self.manual_edit_protection,
            raw_sources_immutable: self.raw_sources_immutable,
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
pub struct SetChatConvenienceAuthorizationRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSecretStatusRequest {
    pub provider: LlmProviderKind,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CloseBehavior, ColorThemePresetId, Settings, ThemePreference};
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

    #[test]
    fn legacy_settings_file_without_new_fields_still_deserializes_with_defaults() {
        // A project/global settings JSON written before the P0/P1 field
        // expansion must still load — every new field has #[serde(default)].
        let legacy = json!({
            "language": "zh-CN",
            "theme": "dark",
            "closeBehavior": "quit",
            "contextWindow": 64000,
            "checkUpdates": false,
            "agentDefault": "codex",
            "llmProviders": [],
            "template": "research"
        });

        let settings: Settings = serde_json::from_value(legacy).unwrap();

        // Legacy values preserved.
        assert_eq!(settings.language, "zh-CN");
        assert_eq!(settings.close_behavior, CloseBehavior::Quit);
        assert_eq!(settings.context_window, 64_000);
        // New fields fall back to their defaults, not panics.
        assert_eq!(settings.density.as_str(), "standard");
        assert_eq!(settings.agent_output_language.as_str(), "follow_ui");
        assert_eq!(settings.update_frequency.as_str(), "daily");
        assert_eq!(settings.max_tokens, 4096);
        assert!((settings.temperature - 0.3).abs() < f64::EPSILON);
        assert_eq!(settings.agent_task_timeout_secs, 300);
        assert!(settings.auto_git_checkpoint);
        assert!(settings.skill_autoload);
        assert_eq!(settings.max_concurrent_tasks, 2);
        assert_eq!(settings.notification_click_behavior.as_str(), "result_page");
        assert!(settings.system_notifications.on_task_completed);
        assert!(!settings.system_notifications.on_long_task_progress);
    }

    #[test]
    fn new_settings_fields_round_trip_through_serde() {
        let settings = Settings {
            density: super::DensityPreference::Compact,
            agent_output_language: super::AgentOutputLanguage::AlwaysChinese,
            update_frequency: super::UpdateFrequency::Weekly,
            notification_click_behavior: super::NotificationClickBehavior::ErrorLog,
            max_concurrent_tasks: 4,
            max_tokens: 8192,
            temperature: 0.7,
            agent_task_timeout_secs: 600,
            allow_agent_install: true,
            install_command_display_only: false,
            prompt_on_new_agent: false,
            skill_autoload: false,
            auto_git_checkpoint: false,
            manual_edit_protection: false,
            raw_sources_immutable: false,
            auto_download_updates: true,
            prompt_changelog_before_install: false,
            startup_behavior: super::StartupBehavior::ShowProjectPicker,
            default_project_location: "~/Documents/wiki".into(),
            external_editor: "Visual Studio Code".into(),
            associate_md_files: false,
            associate_wiki_folders: true,
            ui_font: "SF Pro Text".into(),
            reading_font: "Georgia".into(),
            code_font: "Menlo".into(),
            system_notifications: super::SystemNotificationPrefs {
                on_task_completed: false,
                on_task_failed: true,
                on_confirmation_needed: false,
                on_long_task_progress: true,
            },
            ..Settings::default()
        };

        let value = serde_json::to_value(&settings).unwrap();
        let restored: Settings = serde_json::from_value(value).unwrap();

        assert_eq!(restored, settings);
    }

    #[test]
    fn close_behavior_includes_ask_option_from_design() {
        // Design settings.html:528-533 requires a third "询问/Ask" option.
        let value = serde_json::to_value(CloseBehavior::Ask).unwrap();
        assert_eq!(value, json!("ask"));
        let restored: CloseBehavior = serde_json::from_value(json!("ask")).unwrap();
        assert_eq!(restored, CloseBehavior::Ask);
    }

    #[test]
    fn color_theme_preset_is_global_and_legacy_safe() {
        let legacy = json!({
            "language": "en",
            "theme": "auto",
            "contextWindow": 32000
        });

        let settings: Settings = serde_json::from_value(legacy).unwrap();

        assert_eq!(settings.color_theme_preset, ColorThemePresetId::Codex);
        assert_eq!(
            serde_json::to_value(settings.color_theme_preset).unwrap(),
            json!("codex")
        );

        let global = settings.to_global_file();
        let project = settings.to_project_file();
        let global_value = serde_json::to_value(global).unwrap();
        let project_value = serde_json::to_value(project).unwrap();

        assert_eq!(global_value["colorThemePreset"], json!("codex"));
        assert!(project_value.get("colorThemePreset").is_none());
    }

    #[test]
    fn global_and_project_file_split_keeps_secrets_out_and_new_fields_in() {
        // Global file carries the cross-project UI/notification/update fields;
        // project file carries the Agent/Git/context fields. Neither carries a
        // secret (secrets live only in the keychain — PRD-SET-002 boundary).
        let settings = Settings {
            language: "zh-CN".into(),
            density: super::DensityPreference::Comfortable,
            update_frequency: super::UpdateFrequency::Never,
            auto_git_checkpoint: false,
            max_tokens: 2048,
            temperature: 0.5,
            ..Settings::default()
        };

        let global = settings.to_global_file();
        let project = settings.to_project_file();

        // Global-only fields appear in the global file, not the project file.
        let global_value = serde_json::to_value(&global).unwrap();
        let project_value = serde_json::to_value(&project).unwrap();
        assert_eq!(global_value["density"], json!("comfortable"));
        assert_eq!(global_value["updateFrequency"], json!("never"));
        assert!(project_value.get("density").is_none());
        assert!(project_value.get("updateFrequency").is_none());

        // Project-only fields appear in the project file, not the global file.
        assert_eq!(project_value["maxTokens"], json!(2048));
        assert!((project_value["temperature"].as_f64().unwrap() - 0.5).abs() < f64::EPSILON);
        assert_eq!(project_value["autoGitCheckpoint"], json!(false));
        assert!(global_value.get("maxTokens").is_none());
        assert!(global_value.get("autoGitCheckpoint").is_none());

        // Re-applying both files reconstructs the original Settings.
        let mut merged = Settings::default();
        merged.apply_global(global);
        merged.apply_project(project);
        assert_eq!(merged, settings);
    }
}

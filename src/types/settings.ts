import type { AgentKind } from "./agent";
import type { LlmProviderConfig, LlmProviderKind } from "./llm";
import type { ProjectTemplate } from "./project";

export type ThemePreference = "light" | "dark" | "auto";
export type ColorThemePresetId =
  | "codex"
  | "paper"
  | "graphite"
  | "mint"
  | "night"
  | "highContrast";
export type DensityPreference = "compact" | "standard" | "comfortable";
export type CloseBehavior = "minimize_to_tray" | "ask" | "quit";
export type AppLanguage = "en" | "zh-CN";
export type StartupBehavior = "open_last_project" | "show_project_picker" | "auto_open_by_condition";
export type AgentOutputLanguage = "follow_ui" | "always_chinese" | "always_english" | "follow_source";
export type UpdateFrequency = "daily" | "weekly" | "never";
export type NotificationClickBehavior =
  | "result_page"
  | "error_log"
  | "diff_confirm_page"
  | "activate_window_only";

export interface SystemNotificationPrefs {
  onTaskCompleted: boolean;
  onTaskFailed: boolean;
  onConfirmationNeeded: boolean;
  onLongTaskProgress: boolean;
}

export interface ChatConvenienceAuthorization {
  enabled: boolean;
  confirmedAt: string;
  projectId: string;
  rootPathFingerprint: string;
}

export interface Settings {
  // Global
  language: AppLanguage;
  theme: ThemePreference;
  colorThemePreset: ColorThemePresetId;
  density: DensityPreference;
  uiFont: string;
  readingFont: string;
  codeFont: string;
  agentOutputLanguage: AgentOutputLanguage;
  closeBehavior: CloseBehavior;
  systemNotifications: SystemNotificationPrefs;
  notificationClickBehavior: NotificationClickBehavior;
  maxConcurrentTasks: number;
  checkUpdates: boolean;
  updateFrequency: UpdateFrequency;
  autoDownloadUpdates: boolean;
  promptChangelogBeforeInstall: boolean;
  startupBehavior: StartupBehavior;
  defaultProjectLocation: string;
  externalEditor: string;
  associateMdFiles: boolean;
  associateWikiFolders: boolean;
  // Project
  contextWindow: number;
  maxTokens: number;
  temperature: number;
  agentTaskTimeoutSecs: number;
  allowAgentInstall: boolean;
  installCommandDisplayOnly: boolean;
  promptOnNewAgent: boolean;
  skillAutoload: boolean;
  autoGitCheckpoint: boolean;
  manualEditProtection: boolean;
  rawSourcesImmutable: boolean;
  agentDefault: AgentKind | null;
  llmProviders: LlmProviderConfig[];
  template: ProjectTemplate;
}

export interface SettingsProjectRequest {
  projectId: string;
  projectRootPath: string;
}

export interface SaveSettingsRequest extends SettingsProjectRequest {
  settings: Settings;
}

export interface ProviderSecretStatusRequest {
  provider: LlmProviderKind;
}

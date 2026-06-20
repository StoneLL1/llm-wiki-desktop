import type { AgentKind } from "./agent";
import type { LlmProviderConfig, LlmProviderKind } from "./llm";
import type { ProjectTemplate } from "./project";

export type ThemePreference = "light" | "dark" | "auto";
export type CloseBehavior = "minimize_to_tray" | "quit";
export type AppLanguage = "en" | "zh-CN";

export interface Settings {
  language: AppLanguage;
  theme: ThemePreference;
  closeBehavior: CloseBehavior;
  contextWindow: number;
  checkUpdates: boolean;
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

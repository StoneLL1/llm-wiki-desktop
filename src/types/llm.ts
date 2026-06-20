export type LlmProviderKind = "open_ai" | "anthropic" | "google" | "ollama" | "custom";

export interface LlmProviderConfig {
  provider: LlmProviderKind;
  model: string;
  baseUrl: string;
  contextWindow: number;
  enabled: boolean;
}

export interface ProviderStatus {
  config: LlmProviderConfig;
  hasSecret: boolean;
  secretMask: string | null;
}

export interface ProviderTestResult {
  ok: boolean;
  message: string;
}

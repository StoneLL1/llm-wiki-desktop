export type LlmProviderKind = "open_ai" | "anthropic" | "google" | "ollama" | "custom";

export interface LlmProviderConfig {
  provider: LlmProviderKind;
  model: string;
  baseUrl: string;
  contextWindow: number;
  enabled: boolean;
}

export interface ProviderCredentialBinding {
  configId: string;
  providerKind: LlmProviderKind;
  canonicalOrigin: string;
  credentialAccountId: string;
  approvedAt: string | null;
  revision: number;
}

export interface ProviderStatus {
  config: LlmProviderConfig;
  credentialBinding?: ProviderCredentialBinding | null;
  hasSecret: boolean;
  secretMask: string | null;
}

export interface ProviderTestResult {
  ok: boolean;
  message: string;
}

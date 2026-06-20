export type AgentKind = "claude" | "codex" | "openclaw" | "hermes";
export type AgentDetectionState = "installed" | "missing" | "failed";

export interface AgentInfo {
  kind: AgentKind;
  command: string;
  state: AgentDetectionState;
  version: string | null;
  executablePath: string | null;
  isDefault: boolean;
  installGuidance: string;
  error: string | null;
}

import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export interface CompileRequest {
  projectId: string;
  projectRootPath: string;
  route: "auto" | "agent" | "byok";
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
}

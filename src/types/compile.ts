import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export interface SourceVersionRef {
  sourceId: string;
  versionId: string;
  contentHash: string;
}

export interface CompileRequest {
  projectId: string;
  projectRootPath: string;
  route: "auto" | "agent" | "byok";
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
  sourceVersions: SourceVersionRef[];
}

export interface CompileResult {
  route: "agent" | "byok";
  affectedPaths: string[];
  conflicts: string[];
  checkpoint: string | null;
  consumedVersions: SourceVersionRef[];
}

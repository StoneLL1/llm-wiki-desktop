import type { PendingAction } from "./backend";

export type { PendingAction };

export type ProjectTemplate =
  | "general"
  | "research"
  | "reading"
  | "personal-growth"
  | "business";

export type IndexState = "indexed" | "stale" | "missing";

export type GraphState = "cached" | "stale" | "missing";

export type AgentRoute = "agent" | "byok" | "unconfigured";

export interface ProjectHealthReport {
  isWikiProject: boolean;
  hasPurpose: boolean;
  hasSchema: boolean;
  hasAppState: boolean;
  hasObsidian: boolean;
  missingPaths: string[];
}

export interface ProjectSummary {
  projectId: string;
  name: string;
  rootPath: string;
  template: ProjectTemplate;
  wikiPageCount: number;
  sourceCount: number;
  taskCount: number;
  indexState: IndexState;
  graphState: GraphState;
  agentRoute: AgentRoute;
  health: ProjectHealthReport;
}

export interface RecentProject {
  projectId: string;
  name: string;
  rootPath: string;
  template: ProjectTemplate;
  openedAt: string;
}

export type OpenProjectKind = "opened" | "needs_confirmation";

export interface OpenProjectResponse {
  kind: OpenProjectKind;
  summary?: ProjectSummary;
  pendingAction?: PendingAction;
}

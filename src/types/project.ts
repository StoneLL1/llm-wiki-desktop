export type ProjectTemplate =
  | "general"
  | "research"
  | "reading"
  | "personal-growth"
  | "business";

export type IndexState = "indexed" | "stale" | "missing";

export type GraphState = "cached" | "stale" | "missing";

export type AgentRoute = "agent" | "byok" | "unconfigured";

export type RiskLevel = "low" | "medium" | "high" | "destructive";

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

export interface ActionPreview {
  summary: string;
  before?: string | null;
  after?: string | null;
  diff?: string | null;
}

export interface PendingAction {
  id: string;
  actionType: string;
  title: string;
  message: string;
  riskLevel: RiskLevel;
  affectedPaths: string[];
  preview?: ActionPreview | null;
  expiresAt?: string | null;
}

export type OpenProjectKind = "opened" | "needs_confirmation";

export interface OpenProjectResponse {
  kind: OpenProjectKind;
  summary?: ProjectSummary;
  pendingAction?: PendingAction;
}

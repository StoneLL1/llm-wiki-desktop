import type { PendingAction } from "./backend";

export type { PendingAction };

export type ProjectMarkdownRootRole = "source" | "wiki" | "mixed";

export interface ProjectMarkdownRoot {
  path: string;
  role: ProjectMarkdownRootRole;
  exclude?: string[];
}

export interface ProjectContextDocument {
  readPath?: string;
  writePath?: string;
  inferred?: boolean;
}

export interface ProjectLayout {
  appStateRoot?: string;
  evidenceRoot?: string;
  markdownRoots: ProjectMarkdownRoot[];
  sourceWriteRoot?: string;
  wikiWriteRoot?: string;
  wikiIndexPath?: string;
  wikiOverviewPath?: string;
  activityLogPath?: string;
  queriesWriteRoot?: string;
  exportRoot?: string;
  skillsRoot?: string;
  importStateRoot?: string;
  sourceStateRoot?: string;
  compileStateRoot?: string;
  chatStateRoot?: string;
  taskStateRoot?: string;
  workflowStateRoot?: string;
  graphCachePath?: string;
  lintReportRoot?: string;
  lintIgnorePath?: string;
  exportRecordPath?: string;
  bookmarksPath?: string;
  settingsPath?: string;
  agentConfigPath?: string;
  purposeContext?: ProjectContextDocument;
  schemaContext?: ProjectContextDocument;
}

export type ProjectLayoutConfidence = "high" | "medium" | "low";

export type ProjectLayoutWarningCode =
  | "LOW_CONFIDENCE"
  | "DISCOVERY_LIMIT_REACHED"
  | "UNSAFE_ENTRY_SKIPPED";

export interface ProjectLayoutWarning {
  code: ProjectLayoutWarningCode;
  message: string;
  path?: string;
}

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
  wikiPageCount: number;
  sourceCount: number;
  taskCount: number;
  indexState: IndexState;
  graphState: GraphState;
  missing: boolean;
}

export type OpenProjectKind = "opened" | "needs_confirmation";

export interface OpenProjectResponse {
  kind: OpenProjectKind;
  summary?: ProjectSummary;
  pendingAction?: PendingAction;
}

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

export type ProjectFormat =
  | "native_current"
  | "native_legacy"
  | "nashsu_llm_wiki"
  | "obsidian_vault"
  | "markdown_vault"
  | "ambiguous_markdown"
  | "ordinary_materials"
  | "unknown";

export type ProjectTrustState = "trusted" | "untrusted";
export type ProjectFilesystemAccess = "writable" | "read_only";
export type ProjectHealth = "healthy" | "repairable" | "recovery" | "unreadable";

export type ProjectCapability =
  | "read_markdown"
  | "local_search"
  | "in_memory_graph"
  | "local_health_check"
  | "external_ai"
  | "project_write"
  | "git_checkpoint"
  | "enable_compatible_features";

export interface ProjectMarker {
  kind: string;
  path: string;
}

export interface ProjectAssessmentWarning {
  code: string;
  message: string;
  path?: string;
}

export interface ProjectGitAssessment {
  isRepository: boolean;
  branch: string | null;
  head: string | null;
  hasChanges: boolean;
}

export interface ProjectOpenAssessment {
  assessmentId: string;
  canonicalRootPath: string;
  canonicalIdentityKey: string;
  identityRevision: string;
  format: ProjectFormat;
  trust: ProjectTrustState;
  filesystemAccess: ProjectFilesystemAccess;
  health: ProjectHealth;
  layout: ProjectLayout;
  confidence: ProjectLayoutConfidence;
  markers: ProjectMarker[];
  capabilities: ProjectCapability[];
  warnings: ProjectAssessmentWarning[];
  layoutWarnings: ProjectLayoutWarning[];
  git: ProjectGitAssessment;
}

export type AssessmentOperationStatus = "running" | "completed" | "failed";

export interface ProjectAssessmentOperation {
  assessmentOperationId: string;
  status: AssessmentOperationStatus;
  assessment?: ProjectOpenAssessment;
  error?: { code: string; message: string };
}

export interface StartProjectOpenAssessmentResult {
  assessmentOperationId: string;
}

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

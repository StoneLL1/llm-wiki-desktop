import type { AgentKind } from "./agent";
import type { PendingActionType, RiskLevel } from "./backend";
import type { LlmProviderKind } from "./llm";
import type { TaskStatus } from "./task";

export const WORKFLOW_SCHEMA_VERSION = 1 as const;

export type WorkflowKind =
  | "update_wiki"
  | "health_check"
  | "generate_content";

export type WorkflowDisplayStatus =
  | "queued"
  | "running"
  | "waiting_for_confirmation"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

export const toWorkflowDisplayStatus = (
  status: TaskStatus,
): WorkflowDisplayStatus => {
  switch (status) {
    case "queued":
      return "queued";
    case "running":
    case "cancelling":
      return "running";
    case "waiting_for_confirmation":
      return "waiting_for_confirmation";
    case "succeeded":
      return "completed";
    case "failed":
      return "failed";
    case "cancelled":
      return "cancelled";
    case "interrupted":
      return "interrupted";
  }
};

export type WorkflowRoute =
  | { kind: "local"; routeRevision: string }
  | {
      kind: "agent";
      agent: AgentKind;
      model: string | null;
      routeRevision: string;
    }
  | {
      kind: "byok";
      provider: LlmProviderKind;
      model: string;
      routeRevision: string;
    };

export type WorkflowRouteSelection =
  | { kind: "agent"; agent: AgentKind }
  | { kind: "byok"; provider: LlmProviderKind };

export interface WorkflowSourceVersionRef {
  sourceId: string;
  versionId: string;
}

export type UpdateWikiMode = "changed_sources" | "full_recompile";
export type HealthCheckMode = "local_quick" | "complete";
export type WorkflowArtifactType =
  | "beautiful_read"
  | "knowledge_card"
  | "concept_map"
  | "project_report";

export type WorkflowScope =
  | {
      kind: "update_wiki";
      mode: UpdateWikiMode;
      sourceVersions: WorkflowSourceVersionRef[];
    }
  | { kind: "health_check"; mode: HealthCheckMode }
  | {
      kind: "generate_content";
      artifactType: WorkflowArtifactType;
      pagePaths: string[];
      outputPath: string | null;
    };

export type WorkflowStageStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "waiting"
  | "skipped";

export interface WorkflowCountProgress {
  current: number;
  total: number | null;
}

export type WorkflowCandidateReference =
  | { kind: "task_owned"; candidateId: string }
  | { kind: "project_relative"; path: string };

export interface WorkflowPendingAction {
  id: string;
  actionType: PendingActionType;
  riskLevel: RiskLevel;
  affectedPaths: string[];
  candidate: WorkflowCandidateReference | null;
  expiresAt: string | null;
  checkpointHash: string | null;
}

export interface WorkflowStage {
  id: string;
  ordinal: number;
  status: WorkflowStageStatus;
  labelKey: string;
  startedAt: string | null;
  completedAt: string | null;
  currentItem: string | null;
  progress: WorkflowCountProgress | null;
  decision: WorkflowPendingAction | null;
}

export interface WorkflowBaselineSummary {
  fingerprint: string;
  capturedAt: string;
  itemCount: number;
}

export type WorkflowProjectTrust = "trusted" | "untrusted";
export type WorkflowFilesystemAccess = "writable" | "read_only";
export type WorkflowPersistenceMode = "persistent" | "memory_only";
export type WorkflowGitState = "clean" | "dirty" | "unavailable";

export interface WorkflowProjectAccessSummary {
  projectId: string;
  canonicalIdentityKey: string;
  identityRevision: string;
  trust: WorkflowProjectTrust;
  filesystemAccess: WorkflowFilesystemAccess;
  persistence: WorkflowPersistenceMode;
  gitState: WorkflowGitState;
}

export type WorkflowPrerequisiteAction =
  | "open_or_create_project"
  | "trust_project"
  | "make_writable"
  | "configure_git"
  | "resolve_dirty_git"
  | "import_sources"
  | "update_wiki"
  | "configure_execution_route"
  | "choose_execution_route"
  | "prepare_again"
  | "acknowledge_remote_provider"
  | "acknowledge_restricted_content";

export interface WorkflowPrerequisite {
  code: string;
  messageKey: string;
  blocking: boolean;
  action: WorkflowPrerequisiteAction;
}

export type WorkflowGitPolicy =
  | "not_required"
  | "required_before_write"
  | "required_before_overwrite";

export interface WorkflowOutputSummary {
  labelKey: string;
  location: string | null;
  mayChangeWiki: boolean;
}

export interface WorkflowPreparation {
  schemaVersion: number;
  preparationId: string;
  preparationRevision: string;
  projectAccess: WorkflowProjectAccessSummary;
  kind: WorkflowKind;
  scope: WorkflowScope;
  baseline: WorkflowBaselineSummary;
  route: WorkflowRoute | null;
  prerequisites: WorkflowPrerequisite[];
  output: WorkflowOutputSummary;
  gitPolicy: WorkflowGitPolicy;
  requiresScopeConfirmation: boolean;
  quickRerunEligible: boolean;
}

export type WorkflowResult =
  | {
      kind: "update_wiki";
      created: number;
      updated: number;
      skipped: number;
      deleted: number;
      conflicted: number;
      affectedPaths: string[];
      checkpointHash: string | null;
      finalCommit: string | null;
    }
  | {
      kind: "health_check";
      reportId: string | null;
      persistent: boolean;
      errorCount: number;
      warningCount: number;
      infoCount: number;
    }
  | {
      kind: "generate_content";
      artifactType: WorkflowArtifactType;
      recordId: string | null;
      outputPaths: string[];
      validationPassed: boolean;
    };

export interface WorkflowErrorSummary {
  code: string;
  messageKey: string;
  recoverable: boolean;
  userActionRequired: boolean;
  suggestedAction: WorkflowPrerequisiteAction | null;
}

export interface WorkflowRetryLink {
  attemptOf: string;
  attemptNumber: number;
}

export interface WorkflowRun {
  schemaVersion: number;
  taskId: string;
  projectId: string;
  canonicalIdentityKey: string;
  identityRevision: string;
  kind: WorkflowKind;
  displayStatus: WorkflowDisplayStatus;
  scope: WorkflowScope;
  route: WorkflowRoute | null;
  fingerprint: string;
  baselineFingerprint: string;
  stages: WorkflowStage[];
  currentStageId: string | null;
  queuePosition: number | null;
  continuationRequired: boolean;
  retry: WorkflowRetryLink | null;
  pendingAction: WorkflowPendingAction | null;
  result: WorkflowResult | null;
  error: WorkflowErrorSummary | null;
  startedAt: string;
  updatedAt: string;
  completedAt: string | null;
}

export type WorkflowOverviewState =
  | "ready"
  | "needs_prerequisite"
  | "queued"
  | "running"
  | "waiting_for_confirmation"
  | "failed"
  | "interrupted"
  | "up_to_date";

export interface WorkflowOverviewRow {
  kind: WorkflowKind;
  state: WorkflowOverviewState;
  recommended: boolean;
  activeTaskId: string | null;
  lastCompletedAt: string | null;
  prerequisite: WorkflowPrerequisite | null;
}

export interface WorkflowsOverview {
  schemaVersion: number;
  projectAccess: WorkflowProjectAccessSummary | null;
  rows: WorkflowOverviewRow[];
}

export type WorkflowStartOutcome =
  | { kind: "created"; run: WorkflowRun }
  | { kind: "existing"; run: WorkflowRun };

export interface WorkflowRunPage {
  runs: WorkflowRun[];
  nextCursor: string | null;
}

export interface WorkflowProjectRequest {
  projectId: string;
  projectRootPath: string;
}

export interface PrepareWorkflowRequest extends WorkflowProjectRequest {
  kind: WorkflowKind;
  scope: WorkflowScope | null;
  routeSelection: WorkflowRouteSelection | null;
}

export interface StartWorkflowRequest extends WorkflowProjectRequest {
  preparationId: string;
  preparationRevision: string;
  acknowledgeRestrictedContent?: boolean;
  acknowledgeRemoteProvider?: boolean;
}

export interface ListWorkflowRunsRequest extends WorkflowProjectRequest {
  workflowKind: WorkflowKind | null;
  displayStatus: WorkflowDisplayStatus | null;
  cursor: string | null;
  limit: number;
}

export interface WorkflowRunRequest extends WorkflowProjectRequest {
  taskId: string;
}

export interface ReorderQueuedWorkflowRequest extends WorkflowRunRequest {
  beforeTaskId: string | null;
}

export interface ConfirmWorkflowActionRequest extends WorkflowRunRequest {
  actionId: string;
}

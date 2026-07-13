import type { AgentKind } from "./agent";
import type { ImportArtifact, QualityReport } from "./importV2";

export interface AgentAssistancePolicy {
  autoLocalOnHardFailure: boolean;
  autoLocalOnQualityWarning: boolean;
  autoByok: boolean;
  maxAttemptsPerItem: number;
}

export const balancedAgentAssistancePolicy = (autoLocalOnHardFailure = false): AgentAssistancePolicy => ({
  autoLocalOnHardFailure,
  autoLocalOnQualityWarning: false,
  autoByok: false,
  maxAttemptsPerItem: 1,
});

export type AgentAssistanceTrigger = "deterministic_hard_failure" | "manual" | "quality_optimization";

export const AGENT_RECOVERY_ACTIONS = [
  "invoke_local_agent",
  "request_byok",
  "compare_candidate",
  "discard_candidate",
] as const;
export type AgentRecoveryAction = (typeof AGENT_RECOVERY_ACTIONS)[number];

export type AgentToolGrant =
  | "inspect_source"
  | "run_deterministic_route"
  | "run_ocr"
  | "run_asr"
  | "parse_sanitized_snapshot"
  | "validate_candidate";

export interface AgentInvocationRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  trigger: AgentAssistanceTrigger;
  agentKind: AgentKind;
}

export interface SendScopeFile {
  relativePath: string;
  sha256: string;
  sizeBytes: number;
  estimatedTokens: number;
  redactions: string[];
}

export interface AgentSendScope {
  approvalId: string;
  itemId: string;
  provider: string;
  model: string;
  files: SendScopeFile[];
  estimatedInputTokens: number;
  estimatedCostMicros: number | null;
  scopeSha256: string;
  expiresAt: string;
}

export interface AgentCandidate {
  candidateId: string;
  taskId: string;
  trigger: AgentAssistanceTrigger;
  markdown: ImportArtifact;
  assets: ImportArtifact[];
  quality: QualityReport;
  processingSummary: string;
  toolsUsed: string[];
  uncertainties: string[];
  warnings: string[];
  sourceSnapshotSha256: string;
  createdAt: string;
}

export interface AgentCandidateDiff {
  candidateId: string;
  baselineMarkdown: string;
  currentMarkdown: string | null;
  agentMarkdown: string;
  unifiedDiff: string;
  needsThreeWayMerge: boolean;
}

export interface AgentCandidateView {
  projectId: string;
  sessionId: string;
  itemId: string;
  candidate: AgentCandidate;
  diff: AgentCandidateDiff;
}

export interface AgentCandidateActionResult {
  projectId: string;
  sessionId: string;
  itemId: string;
  candidateId: string;
  item: import("./importV2").ImportItem;
}

export interface AcceptImportAgentCandidateRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  taskId: string;
}

export interface SelectImportAgentCandidateRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  candidateId: string;
  mergedMarkdown: string | null;
  expectedCurrentWikiSha256: string | null;
}

export interface DiscardImportAgentCandidateRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  candidateId: string;
}

export interface AgentAuditRecord {
  auditId: string;
  taskId: string;
  sessionId: string;
  itemId: string;
  trigger: AgentAssistanceTrigger;
  route: string;
  approvedScopeSha256: string | null;
  grantedTools: AgentToolGrant[];
  inputHashes: string[];
  outputHashes: string[];
  startedAt: string;
  completedAt: string | null;
  outcome: string;
  warnings: string[];
}

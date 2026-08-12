import type { AgentKind } from "./agent";
import type { PendingAction } from "./backend";
import type { LlmProviderKind } from "./llm";
import type { HealthCheckMode, WorkflowRoute } from "./workflow";

export const WIKI_LINT_SCHEMA_VERSION = 1 as const;
export const WIKI_LINT_SKILL_ID = "builtin.wiki-lint" as const;
export const WIKI_LINT_SKILL_VERSION = "2026-08-12.1" as const;
export const WIKI_LINT_SKILL_SHA256 =
  "29e903710745451da287de9d08297ae6863de944bd7d9abd7f4243b5b9f76eb0" as const;

export interface WikiLintSkillRef {
  id: typeof WIKI_LINT_SKILL_ID;
  version: typeof WIKI_LINT_SKILL_VERSION;
  sha256: typeof WIKI_LINT_SKILL_SHA256;
}

export type AgentLintRepairOperation = "analyze" | "repair";

export interface AgentLintRepairPreparation {
  preparationId: string;
  preparationRevision: string;
  reportId: string;
  selectionRevision: string;
  selectedFindingIds: string[];
  route: WorkflowRoute;
  skill: WikiLintSkillRef;
  authorizedPaths: string[];
  authorizedPathHashes: Record<string, string | null>;
  baselineFingerprint: string;
  expectedGitHead: string;
  pendingAction: PendingAction;
}

export type LintSeverity = "error" | "warning" | "info";

export type LintIssueSource = "local" | "agent";

export type LintIssueType =
  // Local deterministic rules.
  | "dead_link"
  | "orphan_page"
  | "missing_frontmatter"
  | "index_drift"
  | "empty_page"
  | "duplicate_filename"
  | "path_case"
  | "missing_resource"
  | "missing_source_section"
  | "invalid_page_type"
  // Agent deep-lint rules.
  | "duplicate_topic"
  | "weak_cross_reference"
  | "missing_source"
  | "schema_mismatch"
  | "outdated_content"
  | "contradiction";

export type Fixability = "none" | "safe" | "high_risk";

export interface LintRange {
  line: number;
  column?: number;
}

export interface LintIssue {
  id: string;
  source: LintIssueSource;
  origins?: LintIssueSource[];
  severity: LintSeverity;
  issueType: LintIssueType;
  path: string;
  /** Hash captured by the backend at scan time. */
  scanHash?: string | null;
  range?: LintRange;
  message: string;
  evidence?: string;
  target?: string;
  fixability: Fixability;
  suggestedAction?: string;
}

export interface LintReport {
  issues: LintIssue[];
  generatedAt: string;
  scannedPages: number;
}

export interface DeepLintReport {
  issues: LintIssue[];
  rawOutput: string;
  generatedAt: string;
}

export interface AgentLintRepairFinding {
  id: string;
  issueType: Extract<
    LintIssueType,
    | "duplicate_topic"
    | "weak_cross_reference"
    | "missing_source"
    | "schema_mismatch"
    | "outdated_content"
    | "contradiction"
  >;
  severity: LintSeverity;
  path: string;
  message: string;
  evidence?: string | null;
  suggestedAction?: string | null;
}

export interface AgentLintRepairRoundSummary {
  round: number;
  affectedPaths: string[];
  unresolvedFindingIds: string[];
  summary: string;
}

export interface AgentLintRepairRequest {
  schemaVersion: typeof WIKI_LINT_SCHEMA_VERSION;
  operation: "repair";
  skill: WikiLintSkillRef;
  reportId: string;
  selectionRevision: string;
  round: number;
  maxRounds: 3;
  findings: AgentLintRepairFinding[];
  priorRounds: AgentLintRepairRoundSummary[];
  writablePaths: string[];
  creatableRoots: string[];
  readOnlyRoots: string[];
  purpose?: string | null;
  schema?: string | null;
  language: string;
}

export type AgentLintRepairFindingStatus =
  | "attempted"
  | "skipped"
  | "needs_review"
  | "failed";

export interface AgentLintRepairFindingResult {
  findingId: string;
  status: AgentLintRepairFindingStatus;
  message: string;
}

export type AgentLintRepairDeclaredChangeOperation =
  | "create"
  | "update"
  | "delete";

export interface AgentLintRepairDeclaredChange {
  path: string;
  operation: AgentLintRepairDeclaredChangeOperation;
}

export interface AgentLintRepairRoundOutput {
  schemaVersion: typeof WIKI_LINT_SCHEMA_VERSION;
  operation: "repair";
  skill: WikiLintSkillRef;
  reportId: string;
  selectionRevision: string;
  round: number;
  findingResults: AgentLintRepairFindingResult[];
  declaredChanges: AgentLintRepairDeclaredChange[];
  summary: string;
}

export type AgentLintRepairOutcome =
  | "succeeded"
  | "partially_completed"
  | "manual_review_required"
  | "cancelled"
  | "failed"
  | "interrupted"
  | "rolled_back";

export interface AgentLintRepairCorrelation {
  resolvedFindingIds: string[];
  unresolvedFindingIds: string[];
  introducedFindingIds: string[];
  skippedFindingIds: string[];
}

export interface HealthCheckCoverage {
  scannedPages: number;
  sourcePages: number;
  wikiPages: number;
  deepCoveredPages?: number | null;
  deepTruncated?: boolean;
  notApplicableRules: string[];
}

export interface HealthCheckReport {
  reportId: string;
  taskId: string;
  mode: HealthCheckMode;
  route: WorkflowRoute;
  persistent: boolean;
  issues: LintIssue[];
  findingOrigins: Record<string, LintIssueSource[]>;
  coverage: HealthCheckCoverage;
  errorCount: number;
  warningCount: number;
  infoCount: number;
  findingsByType: Partial<Record<LintIssueType, number>>;
  durationMs: number;
  generatedAt: string;
}

export type LintRoutePreference = "auto" | "agent" | "byok";

export type LintReportKind = "local" | "deep" | "health_check";

export interface LintHistoryEntry {
  id: string;
  kind: LintReportKind;
  createdAt: string;
  issueCount: number;
  errorCount: number;
  warningCount: number;
  infoCount: number;
  scannedPages?: number | null;
  taskId?: string | null;
  route?: LintRoutePreference | null;
  workflowRoute?: WorkflowRoute | null;
  healthCheckMode?: HealthCheckMode | null;
  durationMs?: number | null;
  persistent?: boolean;
}

export interface LintHistoryFile {
  version: number;
  entries: LintHistoryEntry[];
}

export interface PersistedLintReport {
  entry: LintHistoryEntry;
  localReport?: LintReport | null;
  deepReport?: DeepLintReport | null;
  healthCheckReport?: HealthCheckReport | null;
}

export interface ListLintHistoryRequest {
  projectId: string;
  projectRootPath: string;
}

export interface ReadLintHistoryReportRequest {
  projectId: string;
  projectRootPath: string;
  id: string;
}

export interface StartDeepLintRequest {
  projectId: string;
  projectRootPath: string;
  route: LintRoutePreference;
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
}

export interface GetDeepLintReportRequest {
  projectId: string;
  projectRootPath: string;
  taskId: string;
}

export type LintFixOutcomeKind = "applied" | "needs_confirmation";

export interface LintFixOutcome {
  kind: LintFixOutcomeKind;
  affectedPaths: string[];
  checkpoint?: string;
  finalCommit?: string;
  pendingAction?: PendingAction;
}

export interface ApplyLintFixRequest {
  projectId: string;
  projectRootPath: string;
  issue: LintIssue;
  confirmHighRisk: boolean;
  expectedHash?: string | null;
  actionId?: string | null;
}

/** Inline high-risk confirm surfaced when a fix returns needs_confirmation. */
export interface LintFixConfirmRequest {
  issue: LintIssue;
  pendingAction: PendingAction;
  expectedHash: string;
}

// --- Batch auto-fix (PRD-LINT-003) ---------------------------------------

/** View-mode filter for the lint list / summary cards. */
export type LintMode = "all" | "local" | "agent";

export interface ApplyLintFixesBatchRequest {
  projectId: string;
  projectRootPath: string;
  issues: LintIssue[];
  /** `{path -> sha256}` captured at scan time; the optimistic-lock baseline
   * for each safe fix. Paths missing here are skipped with LINT_FIX_HASH_REQUIRED. */
  expectedHashes: Record<string, string>;
}

/** A high-risk fix awaiting user confirmation after a batch run. */
export interface LintBatchConfirmation {
  issue: LintIssue;
  pendingAction: PendingAction;
}

/** An issue the batch could not handle (non-fixable, stale, missing hash). */
export interface LintBatchSkip {
  issueId: string;
  path: string;
  reasonCode: string;
  reason: string;
}

export interface LintBatchOutcome {
  /** Single Git checkpoint hash covering every applied safe fix. */
  checkpoint?: string;
  finalCommit?: string;
  applied: LintFixOutcome[];
  needsConfirmation: LintBatchConfirmation[];
  skipped: LintBatchSkip[];
}

// --- lint-ignore (.app/lint-ignore.json) ---------------------------------

/** One persisted ignore entry. Match key is `(path, rule)`. */
export interface LintIgnoreEntry {
  path: string;
  rule: LintIssueType;
  createdAt: string;
}

export interface LintIgnoreFile {
  ignored: LintIgnoreEntry[];
}

export interface AddLintIgnoreRequest {
  projectId: string;
  projectRootPath: string;
  path: string;
  rule: LintIssueType;
}

export interface RemoveLintIgnoreRequest {
  projectId: string;
  projectRootPath: string;
  path: string;
  rule: LintIssueType;
}

export interface ListLintIgnoresRequest {
  projectId: string;
  projectRootPath: string;
}

// --- Safety preferences (UI-side; backend checkpoint is a hard boundary) --

export interface LintSafetyPrefs {
  /** Git checkpoint before writing — hard boundary, always enforced. */
  checkpoint: boolean;
  /** Commit immediately after the fix — fused with the checkpoint commit. */
  commitAfter: boolean;
  /** Open Update Wiki preparation after a successful fix. */
  recompile: boolean;
}

/** Radio choice on the detail panel's 修复方案 group. */
export type LintFixChoice = "fix" | "ignore";

export const SEVERITY_ORDER: Record<LintSeverity, number> = {
  error: 0,
  warning: 1,
  info: 2,
};

export const SEVERITY_ICONS: Record<LintSeverity, string> = {
  error: "alert-circle",
  warning: "alert-triangle",
  info: "info",
};

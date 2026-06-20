import type { AgentKind } from "./agent";
import type { PendingAction } from "./backend";
import type { LlmProviderKind } from "./llm";

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
  severity: LintSeverity;
  issueType: LintIssueType;
  path: string;
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

export type LintRoutePreference = "auto" | "agent" | "byok";

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
  pendingAction?: PendingAction;
}

export interface ApplyLintFixRequest {
  projectId: string;
  projectRootPath: string;
  issue: LintIssue;
  confirmHighRisk: boolean;
  expectedHash?: string | null;
}

/** Inline high-risk confirm surfaced when a fix returns needs_confirmation. */
export interface LintFixConfirmRequest {
  issue: LintIssue;
  pendingAction: PendingAction;
  expectedHash: string;
}

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

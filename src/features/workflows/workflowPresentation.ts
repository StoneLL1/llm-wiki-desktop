import type {
  WorkflowArtifactType,
  WorkflowDisplayStatus,
  WorkflowKind,
  WorkflowOverviewRow,
  WorkflowOverviewState,
  WorkflowPrerequisiteAction,
  WorkflowRoute,
  WorkflowRun,
  WorkflowRunOutcomeSummary,
  WorkflowStageStatus,
} from "../../types/workflow";
import type { PendingActionType, RiskLevel } from "../../types/backend";

export const WORKFLOW_KINDS: WorkflowKind[] = [
  "update_wiki",
  "health_check",
  "generate_content",
];

export const WORKFLOW_STATUSES: WorkflowDisplayStatus[] = [
  "queued",
  "running",
  "waiting_for_confirmation",
  "completed",
  "failed",
  "cancelled",
  "interrupted",
];

export function workflowKindKey(kind: WorkflowKind): string {
  return `workflows.kind.${kind}`;
}

export function workflowKindDescriptionKey(kind: WorkflowKind): string {
  return `workflows.kind.${kind}.description`;
}

export function workflowStatusKey(status: WorkflowDisplayStatus | WorkflowOverviewState): string {
  return `workflows.status.${status}`;
}

export function workflowStageStatusClass(status: WorkflowStageStatus): string {
  if (status === "completed") return "is-complete";
  if (status === "running") return "is-running";
  if (status === "waiting") return "is-waiting";
  if (status === "failed") return "is-failed";
  if (status === "skipped") return "is-skipped";
  return "is-pending";
}

export function workflowRiskKey(risk: RiskLevel): string {
  return `workflows.risk.${risk}`;
}

export function workflowActionTypeKey(actionType: PendingActionType): string {
  return `workflows.actionType.${actionType}`;
}

export function workflowPrerequisiteActionKey(action: WorkflowPrerequisiteAction): string {
  return `workflows.prerequisiteAction.${action}`;
}

export function workflowArtifactTypeKey(artifactType: WorkflowArtifactType): string {
  const keys: Record<WorkflowArtifactType, string> = {
    beautiful_read: "workflows.artifact.beautifulRead",
    knowledge_card: "workflows.artifact.knowledgeCard",
    concept_map: "workflows.artifact.conceptMap",
    project_report: "workflows.artifact.projectReport",
  };
  return keys[artifactType];
}

export function workflowRouteKey(route: WorkflowRoute | null): string {
  return `workflows.route.${route?.kind ?? "none"}`;
}

export type WorkflowResultValue =
  | { kind: "count"; value: number }
  | { kind: "boolean"; value: boolean }
  | { kind: "text"; value: string | null; mono?: boolean }
  | { kind: "translation"; key: string }
  | { kind: "duration"; milliseconds: number };

export interface WorkflowResultRowPresentation {
  labelKey: string;
  value: WorkflowResultValue;
}

export interface WorkflowResultPresentation {
  titleKey: string;
  summaryKey: string;
  primaryActionKey: string;
  rows: WorkflowResultRowPresentation[];
  paths: string[];
}

export function presentWorkflowResult(run: WorkflowRun): WorkflowResultPresentation | null {
  const result = run.result;
  if (!result) return null;

  const commonRows: WorkflowResultRowPresentation[] = [];
  const duration = workflowDurationMs(run.startedAt, run.completedAt);
  if (duration !== null) {
    commonRows.push({ labelKey: "workflows.result.duration", value: { kind: "duration", milliseconds: duration } });
  }
  commonRows.push({ labelKey: "workflows.result.route", value: { kind: "translation", key: workflowRouteKey(run.route) } });

  switch (result.kind) {
    case "update_wiki":
      return {
        titleKey: "workflows.result.update_wiki.title",
        summaryKey: "workflows.result.update_wiki.summary",
        primaryActionKey: "workflows.action.viewUpdates",
        paths: result.affectedPaths,
        rows: [
          { labelKey: "workflows.result.created", value: { kind: "count", value: result.created } },
          { labelKey: "workflows.result.updated", value: { kind: "count", value: result.updated } },
          { labelKey: "workflows.result.skipped", value: { kind: "count", value: result.skipped } },
          { labelKey: "workflows.result.deleted", value: { kind: "count", value: result.deleted } },
          { labelKey: "workflows.result.conflicted", value: { kind: "count", value: result.conflicted } },
          { labelKey: "workflows.result.checkpointHash", value: { kind: "text", value: result.checkpointHash, mono: true } },
          { labelKey: "workflows.result.finalCommit", value: { kind: "text", value: result.finalCommit, mono: true } },
          ...commonRows,
        ],
      };
    case "health_check": {
      const findingRows = Object.entries(result.findingsByType ?? {})
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([findingType, count]) => ({
          labelKey: `lint.issueType.${findingType}`,
          value: { kind: "count", value: count } as WorkflowResultValue,
        }));
      return {
        titleKey: "workflows.result.health_check.title",
        summaryKey: "workflows.result.health_check.summary",
        primaryActionKey: "workflows.action.openLintResults",
        paths: [],
        rows: [
          { labelKey: "workflows.result.errorCount", value: { kind: "count", value: result.errorCount } },
          { labelKey: "workflows.result.warningCount", value: { kind: "count", value: result.warningCount } },
          { labelKey: "workflows.result.infoCount", value: { kind: "count", value: result.infoCount } },
          { labelKey: "workflows.result.findingTypes", value: { kind: "count", value: findingRows.length } },
          ...findingRows,
          { labelKey: "workflows.result.coverage", value: result.coverage
            ? { kind: "translation", key: `workflows.result.coverage.${result.coverage.mode}` }
            : { kind: "text", value: null } },
          { labelKey: "workflows.result.scannedPages", value: result.coverage
            ? { kind: "count", value: result.coverage.scannedPages }
            : { kind: "text", value: null } },
          { labelKey: "workflows.result.deepCoveredPages", value: result.coverage?.deepCoveredPages !== null && result.coverage?.deepCoveredPages !== undefined
            ? { kind: "count", value: result.coverage.deepCoveredPages }
            : { kind: "text", value: null } },
          { labelKey: "workflows.result.deepTruncated", value: result.coverage
            ? { kind: "boolean", value: result.coverage.deepTruncated }
            : { kind: "text", value: null } },
          { labelKey: "workflows.result.persistent", value: { kind: "boolean", value: result.persistent } },
          { labelKey: "workflows.result.reportId", value: { kind: "text", value: result.reportId, mono: true } },
          ...commonRows,
        ],
      };
    }
    case "generate_content":
      return {
        titleKey: "workflows.result.generate_content.title",
        summaryKey: "workflows.result.generate_content.summary",
        primaryActionKey: "workflows.action.viewGeneratedResult",
        paths: result.outputPaths,
        rows: [
          { labelKey: "workflows.result.artifactType", value: { kind: "translation", key: workflowArtifactTypeKey(result.artifactType) } },
          { labelKey: "workflows.result.artifactCount", value: result.artifactCount === undefined
            ? { kind: "text", value: null }
            : { kind: "count", value: result.artifactCount } },
          { labelKey: "workflows.result.validationPassed", value: { kind: "boolean", value: result.validationPassed } },
          { labelKey: "workflows.result.recordId", value: { kind: "text", value: result.recordId, mono: true } },
          ...commonRows,
        ],
      };
    case "agent_lint_repair":
      {
        const roundRows = result.rounds.flatMap((round) => [
          { labelKey: "workflows.result.agent_lint_repair.roundEvidence", value: { kind: "text", value: `#${round.round}: ${round.summary}` } as WorkflowResultValue },
          { labelKey: "workflows.result.agent_lint_repair.roundAffectedPaths", value: { kind: "count", value: round.affectedPaths.length } as WorkflowResultValue },
          { labelKey: "workflows.result.agent_lint_repair.roundUnresolved", value: { kind: "count", value: round.unresolvedFindingIds.length } as WorkflowResultValue },
        ]);
      return {
        titleKey: "workflows.result.agent_lint_repair.title",
        summaryKey: "workflows.result.agent_lint_repair.summary",
        primaryActionKey: "workflows.action.openLintResults",
        paths: result.affectedPaths,
        rows: [
          { labelKey: "workflows.result.agent_lint_repair.outcome", value: { kind: "translation", key: `workflows.result.agent_lint_repair.outcome.${result.outcome}` } },
          { labelKey: "workflows.result.agent_lint_repair.resolved", value: { kind: "count", value: result.resolvedFindingIds.length } },
          { labelKey: "workflows.result.agent_lint_repair.unresolved", value: { kind: "count", value: result.unresolvedFindingIds.length } },
          { labelKey: "workflows.result.agent_lint_repair.introduced", value: { kind: "count", value: result.introducedFindingIds.length } },
          { labelKey: "workflows.result.agent_lint_repair.skipped", value: { kind: "count", value: result.skippedFindingIds.length } },
          { labelKey: "workflows.result.agent_lint_repair.rounds", value: { kind: "count", value: result.rounds.length } },
          { labelKey: "workflows.result.agent_lint_repair.diff", value: { kind: "boolean", value: result.diffAvailable } },
          { labelKey: "workflows.result.agent_lint_repair.rollback", value: { kind: "boolean", value: result.rollbackAvailable } },
          ...roundRows,
          { labelKey: "workflows.result.checkpointHash", value: { kind: "text", value: result.checkpointHash, mono: true } },
          { labelKey: "workflows.result.finalCommit", value: { kind: "text", value: result.finalCommit, mono: true } },
          { labelKey: "workflows.result.indexRefreshWarnings", value: { kind: "count", value: result.indexRefreshWarnings.length } },
          ...commonRows,
        ],
      };
      }
  }
}

export function workflowDurationMs(startedAt: string | null, completedAt: string | null): number | null {
  if (!startedAt || !completedAt) return null;
  const start = Date.parse(startedAt);
  const completed = Date.parse(completedAt);
  if (!Number.isFinite(start) || !Number.isFinite(completed) || completed < start) return null;
  return completed - start;
}

export function attentionRun(runs: WorkflowRun[]): WorkflowRun | null {
  return (
    runs.find((run) => run.displayStatus === "waiting_for_confirmation") ??
    runs.find((run) => run.displayStatus === "running") ??
    runs.find((run) => run.displayStatus === "queued") ??
    runs.find((run) => run.displayStatus === "failed" || run.displayStatus === "interrupted") ??
    null
  );
}

export function attentionWorkflowRow(rows: WorkflowOverviewRow[]): WorkflowOverviewRow | null {
  return (
    rows.find((row) => row.activeTaskId && row.state === "waiting_for_confirmation") ??
    rows.find((row) => row.activeTaskId && row.state === "running") ??
    rows.find((row) => row.activeTaskId && row.state === "queued") ??
    rows.find((row) => row.activeTaskId && (row.state === "failed" || row.state === "interrupted")) ??
    null
  );
}

export function isAttentionRun(run: Pick<WorkflowRun, "displayStatus">): boolean {
  return run.displayStatus === "queued"
    || run.displayStatus === "running"
    || run.displayStatus === "waiting_for_confirmation"
    || run.displayStatus === "failed"
    || run.displayStatus === "interrupted";
}

export function isQueueOwningStatus(status: WorkflowDisplayStatus | WorkflowOverviewState): boolean {
  return status === "queued"
    || status === "running"
    || status === "waiting_for_confirmation";
}

export function workflowDateTimeLabel(value: string, language: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(language, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

export function workflowDurationLabel(
  milliseconds: number,
  language: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  const seconds = Math.max(0, Math.round(milliseconds / 1_000));
  const formatter = new Intl.NumberFormat(language);
  if (seconds < 60) return t("workflows.duration.seconds", { count: formatter.format(seconds) });
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return remainingSeconds === 0
    ? t("workflows.duration.minutes", { count: formatter.format(minutes) })
    : t("workflows.duration.minutesSeconds", {
        minutes: formatter.format(minutes),
        seconds: formatter.format(remainingSeconds),
      });
}

export function workflowHistoryOutcomeLabel(
  outcome: WorkflowRunOutcomeSummary | null | undefined,
  language: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string | null {
  if (!outcome) return null;
  const formatter = new Intl.NumberFormat(language);
  switch (outcome.kind) {
    case "update_wiki":
      return t("workflows.history.outcome.updateWiki", {
        created: formatter.format(outcome.created),
        updated: formatter.format(outcome.updated),
        skipped: formatter.format(outcome.skipped),
      });
    case "health_check":
      return t("workflows.history.outcome.healthCheck", {
        errors: formatter.format(outcome.errorCount),
        warnings: formatter.format(outcome.warningCount),
        info: formatter.format(outcome.infoCount),
      });
    case "generate_content":
      return t("workflows.history.outcome.generateContent", {
        artifact: t(workflowArtifactTypeKey(outcome.artifactType)),
        count: formatter.format(outcome.artifactCount),
        validation: t(`workflows.result.${outcome.validationPassed ? "yes" : "no"}`),
      });
    case "agent_lint_repair":
      return t("workflows.history.outcome.agentLintRepair", {
        resolved: formatter.format(outcome.resolvedCount),
        unresolved: formatter.format(outcome.unresolvedCount),
        introduced: formatter.format(outcome.introducedCount),
        outcome: t(`workflows.result.agent_lint_repair.outcome.${outcome.outcome}`),
      });
  }
}

export function groupWorkflowAttempts<T extends Pick<WorkflowRun, "taskId" | "retry">>(runs: T[]): Array<{
  key: string;
  runs: T[];
}> {
  const groups = new Map<string, T[]>();
  for (const run of runs) {
    const key = run.retry?.attemptOf ?? run.taskId;
    const attempts = groups.get(key);
    if (attempts) attempts.push(run);
    else groups.set(key, [run]);
  }
  return [...groups.entries()].map(([key, attempts]) => ({
    key,
    runs: attempts.sort((a, b) => (a.retry?.attemptNumber ?? 1) - (b.retry?.attemptNumber ?? 1)),
  }));
}

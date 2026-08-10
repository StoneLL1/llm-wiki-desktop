import type {
  WorkflowDisplayStatus,
  WorkflowKind,
  WorkflowOverviewRow,
  WorkflowOverviewState,
  WorkflowRun,
  WorkflowStageStatus,
} from "../../types/workflow";

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

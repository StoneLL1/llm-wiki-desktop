import type {
  WorkflowDisplayStatus,
  WorkflowKind,
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
    runs.find((run) => run.displayStatus === "failed" || run.displayStatus === "interrupted") ??
    runs.find((run) => run.displayStatus === "running") ??
    null
  );
}

export function groupWorkflowAttempts(runs: WorkflowRun[]): Array<{
  key: string;
  runs: WorkflowRun[];
}> {
  const groups = new Map<string, WorkflowRun[]>();
  for (const run of runs) {
    const key = run.retry?.attemptOf ?? run.taskId;
    groups.set(key, [...(groups.get(key) ?? []), run]);
  }
  return [...groups.entries()].map(([key, attempts]) => ({
    key,
    runs: attempts.sort((a, b) => (a.retry?.attemptNumber ?? 1) - (b.retry?.attemptNumber ?? 1)),
  }));
}

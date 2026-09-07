import type { WorkflowRun, WorkflowRunSummary } from "../types/workflow";

/** TaskService revisions are decimal strings, never floating point clocks. */
export function compareWorkflowRevision(
  left: Pick<WorkflowRunSummary, "revision" | "updatedAt">,
  right: Pick<WorkflowRunSummary, "revision" | "updatedAt">,
): number {
  if (left.revision !== undefined || right.revision !== undefined) {
    const a = (left.revision ?? "0").replace(/^0+(?=\d)/, "");
    const b = (right.revision ?? "0").replace(/^0+(?=\d)/, "");
    return a.length - b.length || a.localeCompare(b);
  }
  // Read compatibility for pre-revision snapshots; never used by new IPC.
  return left.updatedAt.localeCompare(right.updatedAt);
}

export function workflowRunSummary(run: WorkflowRun | WorkflowRunSummary): WorkflowRunSummary {
  if (!("scope" in run)) return run;
  const result = run.result;
  const outcome = result?.kind === "update_wiki"
    ? { kind: result.kind, created: result.created, updated: result.updated, skipped: result.skipped }
    : result?.kind === "health_check"
      ? { kind: result.kind, errorCount: result.errorCount, warningCount: result.warningCount, infoCount: result.infoCount }
      : result?.kind === "generate_content"
        ? { kind: result.kind, artifactType: result.artifactType, artifactCount: result.artifactCount ?? result.outputPaths.length, validationPassed: result.validationPassed }
        : result?.kind === "agent_lint_repair"
          ? { kind: result.kind, outcome: result.outcome, resolvedCount: result.resolvedFindingIds.length, unresolvedCount: result.unresolvedFindingIds.length, introducedCount: result.introducedFindingIds.length }
          : null;
  const currentStage = run.stages.find((stage) => stage.id === run.currentStageId);
  return {
    schemaVersion: run.schemaVersion, revision: run.revision, sessionId: run.sessionId,
    taskId: run.taskId, projectId: run.projectId, canonicalIdentityKey: run.canonicalIdentityKey,
    identityRevision: run.identityRevision, kind: run.kind, operation: run.operation,
    displayStatus: run.displayStatus, retry: run.retry, outcome,
    startedAt: run.startedAt, updatedAt: run.updatedAt, completedAt: run.completedAt,
    queuePosition: run.queuePosition, continuationRequired: run.continuationRequired,
    currentStageId: run.currentStageId,
    currentStage: currentStage ? { ...currentStage, decision: null } : null,
    stages: run.stages.map((stage) => ({ ...stage, decision: null })),
    cancellable: run.cancellable,
  };
}

/** Reconcile a query with facts delivered after the request began. */
export function mergeWorkflowOverview(
  overview: import("../types/workflow").WorkflowsOverview,
  facts: readonly WorkflowRunSummary[],
): import("../types/workflow").WorkflowsOverview {
  const access = overview.projectAccess;
  if (!access) return overview;
  const known = facts.filter((run) => (!overview.sessionId || run.sessionId === overview.sessionId)
    && run.projectId === access.projectId
    && run.canonicalIdentityKey === access.canonicalIdentityKey && run.identityRevision === access.identityRevision);
  const recent = new Map((overview.recentRuns ?? []).map((run) => [run.taskId, run]));
  for (const run of known) {
    const previous = recent.get(run.taskId);
    if (!previous || compareWorkflowRevision(run, previous) > 0) recent.set(run.taskId, run);
  }
  const rows = overview.rows.map((row) => {
    const matching = known.filter((run) => run.kind === row.kind);
    const active = matching.filter((run) => ["waiting_for_confirmation", "running", "queued"].includes(run.displayStatus));
    const priority = (run: WorkflowRunSummary) => ["waiting_for_confirmation", "running", "queued"].indexOf(run.displayStatus);
    active.sort((a, b) => priority(a) - priority(b) || a.startedAt.localeCompare(b.startedAt));
    const completed = matching.filter((run) => run.displayStatus === "completed")
      .sort((a, b) => (b.completedAt ?? b.updatedAt).localeCompare(a.completedAt ?? a.updatedAt))[0];
    const lastCompletedAt = completed?.completedAt && (!row.lastCompletedAt || completed.completedAt > row.lastCompletedAt)
      ? completed.completedAt : row.lastCompletedAt;
    const failed = matching.filter((run) => ["failed", "interrupted"].includes(run.displayStatus)
      && (!lastCompletedAt || run.startedAt > lastCompletedAt)
      && !matching.some((later) => later.taskId !== run.taskId && later.startedAt > run.startedAt))
      .sort((a, b) => b.startedAt.localeCompare(a.startedAt))[0];
    const attention = active[0] ?? failed;
    const previousAttention = row.activeTaskId ? known.find((run) => run.taskId === row.activeTaskId) : undefined;
    const clearAttention = previousAttention && ["completed", "cancelled"].includes(previousAttention.displayStatus)
      || (["failed", "interrupted"].includes(row.state) && completed && completed.completedAt === lastCompletedAt);
    return {
      ...row,
      ...(lastCompletedAt !== row.lastCompletedAt ? { lastCompletedAt, lastCompletedTaskId: completed.taskId } : {}),
      ...(attention ? {
        activeTaskId: attention.taskId,
        activeContinuationRequired: attention.continuationRequired ?? false,
        state: attention.displayStatus as typeof row.state,
      } : clearAttention ? { activeTaskId: null, activeContinuationRequired: false, state: "ready" as const } : {}),
    };
  });
  return { ...overview, rows, recentRuns: [...recent.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 5) };
}

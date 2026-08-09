import type { BackendEvent, BackendTask, LogLine, TaskActivity } from "../../types/task";
import type {
  WorkflowDecisionReview,
  WorkflowPreparation,
  WorkflowRun,
  WorkflowsOverview,
} from "../../types/workflow";

export const WORKFLOW_BASELINE_SIZES = Object.freeze({
  workflowEvents: 200,
  drawerEvents: 1_000,
  markdownFiles: 1_000,
  agentProbes: 3,
  progressUpdates: 500,
  scopeOptions: 10_000,
  historyAttempts: 10_000,
  diffFiles: 500,
  diffBytes: 20 * 1_024,
});

export const baselineAccess = Object.freeze({
  projectId: "project-baseline",
  canonicalIdentityKey: "identity-baseline",
  identityRevision: "revision-a",
  trust: "trusted" as const,
  filesystemAccess: "writable" as const,
  persistence: "persistent" as const,
  gitState: "clean" as const,
});

export function makeBaselineOverview(
  identityRevision = baselineAccess.identityRevision,
): WorkflowsOverview {
  return {
    schemaVersion: 1,
    projectAccess: { ...baselineAccess, identityRevision },
    rows: [],
  };
}

export function makeBaselineRun(index = 0, overrides: Partial<WorkflowRun> = {}): WorkflowRun {
  const second = String(index % 60).padStart(2, "0");
  const timestamp = `2026-08-09T00:00:${second}Z`;
  return {
    schemaVersion: 1,
    taskId: `workflow-${String(index).padStart(5, "0")}`,
    projectId: baselineAccess.projectId,
    canonicalIdentityKey: baselineAccess.canonicalIdentityKey,
    identityRevision: baselineAccess.identityRevision,
    kind: "health_check",
    displayStatus: "running",
    scope: { kind: "health_check", mode: "local_quick" },
    route: { kind: "local", routeRevision: "local-v1" },
    fingerprint: `fingerprint-${index}`,
    baselineFingerprint: `baseline-${index}`,
    stages: [],
    currentStageId: null,
    queuePosition: null,
    continuationRequired: false,
    retry: null,
    pendingAction: null,
    result: null,
    error: null,
    startedAt: timestamp,
    updatedAt: timestamp,
    completedAt: null,
    ...overrides,
  };
}

export function makeWorkflowEventBurst(
  count = WORKFLOW_BASELINE_SIZES.workflowEvents,
  runOverrides: Partial<WorkflowRun> = {},
) {
  return Array.from({ length: count }, (_, index) => {
    const terminal = index === count - 1;
    const timestamp = new Date(Date.UTC(2026, 7, 9, 0, 0, 0, index * 10)).toISOString();
    const run = makeBaselineRun(0, {
      ...runOverrides,
      displayStatus: terminal ? "completed" : "running",
      updatedAt: timestamp,
      completedAt: terminal ? timestamp : null,
    });
    return {
      eventId: `workflow-event-${String(index).padStart(4, "0")}`,
      eventType: "workflow_updated",
      projectId: run.projectId,
      taskId: run.taskId,
      timestamp: run.updatedAt,
      payload: run,
    } satisfies BackendEvent<WorkflowRun>;
  });
}

export function makeProgressUpdates(count = WORKFLOW_BASELINE_SIZES.progressUpdates) {
  return Array.from({ length: count }, (_, index) => ({
    current: index + 1,
    total: count,
    atMs: index * 20,
    currentItem: `wiki/scale/page-${String(index).padStart(4, "0")}.md`,
  }));
}

export function makeMarkdownPaths(count = WORKFLOW_BASELINE_SIZES.markdownFiles) {
  return Array.from(
    { length: count },
    (_, index) => `wiki/scale/页面-${String(index).padStart(4, "0")}.md`,
  );
}

export function makeScopeOptions(count = WORKFLOW_BASELINE_SIZES.scopeOptions) {
  return Array.from({ length: count }, (_, index) => ({
    sourceId: `source-${String(index).padStart(5, "0")}`,
    versionId: `version-${String(index).padStart(5, "0")}`,
  }));
}

export function makePreparationWithOptions(count = WORKFLOW_BASELINE_SIZES.scopeOptions): WorkflowPreparation {
  const options = makeScopeOptions(count);
  return {
    schemaVersion: 1,
    preparationId: "preparation-scale",
    preparationRevision: "revision-scale",
    projectAccess: { ...baselineAccess },
    kind: "update_wiki",
    scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
    baseline: {
      fingerprint: "a".repeat(64),
      capturedAt: "2026-08-09T00:00:00Z",
      itemCount: count,
    },
    route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route-baseline" },
    prerequisites: [],
    output: { labelKey: "workflows.output.wiki", location: "wiki", mayChangeWiki: true },
    gitPolicy: "required_before_write",
    requiresScopeConfirmation: true,
    quickRerunEligible: false,
    availableSourceVersions: options,
  };
}

export function makeHistoryAttempts(count = WORKFLOW_BASELINE_SIZES.historyAttempts) {
  return Array.from({ length: count }, (_, index) => makeBaselineRun(index, {
    displayStatus: "completed",
    completedAt: `2026-08-09T00:00:${String(index % 60).padStart(2, "0")}Z`,
  }));
}

function fixedAsciiBytes(prefix: string, bytes: number): string {
  if (prefix.length >= bytes) return prefix.slice(0, bytes);
  return `${prefix}${"x".repeat(bytes - prefix.length)}`;
}

export function makeDecisionReview(
  count = WORKFLOW_BASELINE_SIZES.diffFiles,
  diffBytes = WORKFLOW_BASELINE_SIZES.diffBytes,
): WorkflowDecisionReview {
  return {
    reason: "Deterministic scale fixture",
    counts: { created: 0, modified: count, overwritten: 0, deleted: 0 },
    userEditsDetected: true,
    fileDiffs: Array.from({ length: count }, (_, index) => {
      const path = `wiki/scale/diff-${String(index).padStart(4, "0")}.md`;
      return {
        path,
        diff: fixedAsciiBytes(`--- a/${path}\n+++ b/${path}\n`, diffBytes),
      };
    }),
  };
}

export function makeDrawerTask(index: number): BackendTask {
  const timestamp = `2026-08-09T00:00:${String(index % 60).padStart(2, "0")}Z`;
  return {
    id: `task-${String(index).padStart(4, "0")}`,
    taskType: "agent_run",
    projectId: baselineAccess.projectId,
    title: `Task ${index}`,
    status: "running",
    progress: { current: index, total: WORKFLOW_BASELINE_SIZES.drawerEvents, label: null },
    startedAt: timestamp,
    updatedAt: timestamp,
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

export function makeDrawerEventPayload(index: number): {
  task: BackendTask;
  log: LogLine;
  activity: TaskActivity;
  output: string;
} {
  const task = makeDrawerTask(index);
  return {
    task,
    log: {
      timestamp: task.updatedAt,
      level: "info",
      message: `Log ${index}`,
    },
    activity: { kind: "phase", name: `Activity ${index}`, status: "started" },
    output: `output-${index}\n`,
  };
}

export function baselineFixtureSignature(): string {
  const events = makeWorkflowEventBurst();
  const drawer = Array.from(
    { length: WORKFLOW_BASELINE_SIZES.drawerEvents },
    (_, index) => makeDrawerEventPayload(index),
  );
  const markdown = makeMarkdownPaths();
  const progress = makeProgressUpdates();
  const options = makeScopeOptions();
  const attempts = makeHistoryAttempts();
  const review = makeDecisionReview();
  let hash = 0x811c9dc5;
  for (const value of [
    WORKFLOW_BASELINE_SIZES,
    events,
    drawer,
    markdown,
    progress,
    options,
    attempts,
    review,
  ]) {
    const serialized = JSON.stringify(value);
    for (let index = 0; index < serialized.length; index += 1) {
      hash ^= serialized.charCodeAt(index);
      hash = Math.imul(hash, 0x01000193);
    }
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

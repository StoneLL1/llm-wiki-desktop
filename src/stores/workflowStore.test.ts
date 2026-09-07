import { beforeEach, describe, expect, it } from "vitest";

import type { WorkflowRun, WorkflowsOverview } from "../types/workflow";
import { useWorkflowStore } from "./workflowStore";
import { recordWorkflowFacts, useTaskStore } from "./taskStore";
import { workflowRunSummary } from "../services/workflowTaskSnapshot";

function run(taskId: string, updatedAt: string): WorkflowRun {
  return {
    schemaVersion: 1,
    taskId,
    projectId: "project-a",
    canonicalIdentityKey: "identity-a",
    identityRevision: "revision-a",
    kind: "health_check",
    operation: { kind: "built_in" },
    displayStatus: "completed",
    scope: { kind: "health_check", mode: "local_quick" },
    route: { kind: "local", routeRevision: "local-1" },
    fingerprint: `fingerprint-${taskId}`,
    baselineFingerprint: "baseline-a",
    stages: [],
    currentStageId: null,
    queuePosition: null,
    continuationRequired: false,
    retry: null,
    pendingAction: null,
    result: null,
    error: null,
    startedAt: updatedAt,
    updatedAt,
    completedAt: updatedAt,
  };
}

describe("workflowStore", () => {
  beforeEach(() => {
    useWorkflowStore.getState().reset();
    useTaskStore.setState({ workflowById: {}, workflowSessionId: null, retiredWorkflowSessions: [] });
  });

  it("compares decimal revisions without losing precision and rejects retired sessions", () => {
    const old = { ...run("versioned", "2026-08-01T00:00:00Z"), revision: "9007199254740992", sessionId: "session-a" };
    const newer = { ...old, revision: "9007199254740993", displayStatus: "failed" as const };
    recordWorkflowFacts([old]);
    recordWorkflowFacts([newer], false);
    recordWorkflowFacts([old], false);
    expect(useTaskStore.getState().workflowById.versioned?.revision).toBe(newer.revision);
    recordWorkflowFacts([{ ...old, taskId: "lost-memory-task", displayStatus: "queued" }]);
    const restored = { ...old, revision: "5", sessionId: "session-b", displayStatus: "interrupted" as const };
    recordWorkflowFacts([restored]);
    recordWorkflowFacts([newer], false);
    recordWorkflowFacts([newer], true, "session-a");
    expect(useTaskStore.getState().workflowById.versioned).toMatchObject({ sessionId: "session-b", revision: "5" });
    expect(useTaskStore.getState().workflowById["lost-memory-task"]).toBeUndefined();
  });

  it("keeps a late first detail and history page behind the canonical task summary", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const old = { ...run("late", "2026-08-01T00:00:00Z"), revision: "1", sessionId: "session-a", displayStatus: "running" as const };
    recordWorkflowFacts([{ ...old, revision: "2", displayStatus: "cancelled" }]);
    useWorkflowStore.getState().upsertRun(old);
    useWorkflowStore.getState().replaceHistoryPage([workflowRunSummary(old)], null);
    expect(useWorkflowStore.getState().runs[0]).toMatchObject({ revision: "2", displayStatus: "cancelled", pendingAction: null, result: null });
    expect(useWorkflowStore.getState().detailRevisionById.late).toBe("1");
    expect(useWorkflowStore.getState().historyRuns[0]).toMatchObject({ revision: "2", displayStatus: "cancelled" });
  });

  it("opens older history after filling the bounded detail cache", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    for (let index = 0; index < 20; index++) useWorkflowStore.getState().upsertRun(run(`recent-${index}`, "2026-08-02T00:00:00Z"));
    useWorkflowStore.getState().upsertRun(run("old-history", "2020-01-01T00:00:00Z"));
    useWorkflowStore.getState().selectRun("old-history");
    expect(useWorkflowStore.getState().runs).toHaveLength(16);
    expect(Object.keys(useWorkflowStore.getState().detailRevisionById)).toHaveLength(16);
    expect(useWorkflowStore.getState().selectedTaskId).toBe("old-history");
  });

  it("clears old details when a backend restart publishes lower recovered revisions", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.getState().setOverviewSnapshot({ ...overview("identity-a", "revision-a"), sessionId: "session-a" });
    useWorkflowStore.getState().upsertRun({ ...run("restored", "2026-08-01T00:00:00Z"), revision: "50", sessionId: "session-a" });
    useWorkflowStore.getState().selectRun("restored");
    useWorkflowStore.getState().setOverviewSnapshot({ ...overview("identity-a", "revision-a"), sessionId: "session-b" });
    expect(useWorkflowStore.getState().runs).toEqual([]);
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
  });

  it("resets project-scoped state and advances the request epoch", () => {
    const firstEpoch = useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.getState().upsertRun(run("run-a", "2026-08-01T01:00:00Z"));
    const secondEpoch = useWorkflowStore.getState().activateProject("project-b\0D:/b");
    expect(secondEpoch).toBeGreaterThan(firstEpoch);
    expect(useWorkflowStore.getState().runs).toEqual([]);
    expect(useWorkflowStore.getState().overviewStatus).toBe("idle");
    expect(useWorkflowStore.getState().projectKey).toContain("project-b");
  });

  it("marks an overview ready before history is available", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.getState().setOverviewStatus("loading");
    useWorkflowStore.getState().setOverviewSnapshot(overview("identity-a", "revision-a"));

    expect(useWorkflowStore.getState()).toMatchObject({
      overviewStatus: "ready",
      runs: [],
      historyCursor: null,
    });
  });

  it("upserts event snapshots by task id and keeps newest runs first", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.getState().upsertRun(run("older", "2026-08-01T01:00:00Z"));
    useWorkflowStore.getState().upsertRun(run("newer", "2026-08-01T02:00:00Z"));
    useWorkflowStore.getState().upsertRun({ ...run("older", "2026-08-01T03:00:00Z"), displayStatus: "failed" });
    expect(useWorkflowStore.getState().runs.map((item) => item.taskId)).toEqual(["older", "newer"]);
    expect(useWorkflowStore.getState().runs[0]?.displayStatus).toBe("failed");
  });

  it("does not let an older event or list snapshot regress a run", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.getState().upsertRun(run("run-a", "2026-08-01T03:00:00Z"));
    useWorkflowStore.getState().upsertRun({ ...run("run-a", "2026-08-01T02:00:00Z"), displayStatus: "running" });
    useWorkflowStore.getState().replaceRuns([{ ...run("run-a", "2026-08-01T01:00:00Z"), displayStatus: "queued" }]);
    expect(useWorkflowStore.getState().runs[0]?.displayStatus).toBe("completed");
    expect(useWorkflowStore.getState().runs[0]?.updatedAt).toBe("2026-08-01T03:00:00Z");
  });

  it("does not let late progress reopen a terminal workflow run", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.getState().upsertRun(run("run-a", "2026-08-01T03:00:00Z"));
    useWorkflowStore.getState().upsertRuns([{
      ...run("run-a", "2026-08-01T04:00:00Z"),
      displayStatus: "running",
      completedAt: null,
    }]);
    expect(useWorkflowStore.getState().runs[0]?.displayStatus).toBe("completed");
  });

  it("preserves hydrated decision evidence when list and event snapshots omit it", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const decisionReview = {
      reason: "review",
      counts: { created: 1, modified: 2, overwritten: 0, deleted: 1 },
      userEditsDetected: true,
      fileDiffs: [{ path: "wiki/a.md", diff: "diff" }],
    };
    useWorkflowStore.getState().upsertRun({
      ...run("run-a", "2026-08-01T03:00:00Z"),
      displayStatus: "waiting_for_confirmation",
      pendingAction: {
        id: "action-a",
        actionType: "batch_rewrite",
        riskLevel: "high",
        affectedPaths: ["wiki/a.md"],
        candidate: null,
        expiresAt: null,
        checkpointHash: null,
      },
      decisionReview,
    });
    useWorkflowStore.getState().replaceRuns([
      {
        ...run("run-a", "2026-08-01T04:00:00Z"),
        displayStatus: "waiting_for_confirmation",
        pendingAction: {
          id: "action-a",
          actionType: "batch_rewrite",
          riskLevel: "high",
          affectedPaths: ["wiki/a.md"],
          candidate: null,
          expiresAt: null,
          checkpointHash: null,
        },
      },
    ]);
    expect(useWorkflowStore.getState().runs[0]?.decisionReview).toEqual(decisionReview);
  });

  it("atomically clears project-scoped presentation when the canonical identity rotates", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const firstOverview = overview("identity-a", "revision-a");
    useWorkflowStore.getState().setOverviewSnapshot(firstOverview);
    useWorkflowStore.getState().upsertRun(run("old-run", "2026-08-01T03:00:00Z"));
    useWorkflowStore.getState().replaceHistoryPage([], "old-cursor");
    useWorkflowStore.setState({
      preparation: {} as never,
      selectedTaskId: "old-run",
      surface: "detail",
    });

    useWorkflowStore.getState().setOverviewSnapshot(overview("identity-b", "revision-b"));

    expect(useWorkflowStore.getState()).toMatchObject({
      runs: [],
      preparation: null,
      selectedTaskId: null,
      surface: "overview",
      historyCursor: null,
    });
  });

  it("keeps pending and errors scoped to their owning operations", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const reconcile = useWorkflowStore.getState().beginOperation("overview:reconcile");
    const prepare = useWorkflowStore.getState().beginOperation("prepare:health_check");
    useWorkflowStore.getState().failOperation("prepare:health_check", prepare, {
      summary: "Preparation failed",
      technicalDetails: "WORKFLOW_PREPARE_FAILED: details",
    });

    expect(useWorkflowStore.getState().operations).toMatchObject({
      "overview:reconcile": { pending: true, error: null, requestId: reconcile },
      "prepare:health_check": {
        pending: false,
        error: { summary: "Preparation failed" },
        requestId: prepare,
      },
    });
    expect(useWorkflowStore.getState().operations["overview:reconcile"]?.error).toBeNull();
  });

  it("does not let an older operation completion clear a newer request", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const older = useWorkflowStore.getState().beginOperation("prepare:health_check");
    const newer = useWorkflowStore.getState().beginOperation("prepare:health_check");
    useWorkflowStore.getState().finishOperation("prepare:health_check", older);
    expect(useWorkflowStore.getState().operations["prepare:health_check"]).toMatchObject({
      pending: true,
      requestId: newer,
    });
  });

  it("does not reuse an operation token after the active project changes", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const projectARequest = useWorkflowStore.getState().beginOperation("overview:init");
    useWorkflowStore.getState().activateProject("project-b\0D:/b");
    const projectBRequest = useWorkflowStore.getState().beginOperation("overview:init");

    useWorkflowStore.getState().finishOperation("overview:init", projectARequest);

    expect(projectBRequest).toBeGreaterThan(projectARequest);
    expect(useWorkflowStore.getState().operations["overview:init"]).toMatchObject({
      pending: true,
      requestId: projectBRequest,
    });
  });

  it("merges an older hydrated review into the newer snapshot for the same action", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const pendingAction = {
      id: "action-a",
      actionType: "batch_rewrite" as const,
      riskLevel: "high" as const,
      affectedPaths: ["wiki/a.md"],
      candidate: null,
      expiresAt: null,
      checkpointHash: null,
    };
    const decisionReview = {
      reason: "review",
      counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 },
      userEditsDetected: false,
      fileDiffs: [{ path: "wiki/a.md", diff: "+review" }],
    };
    useWorkflowStore.getState().upsertRun({
      ...run("run-a", "2026-08-01T03:00:00Z"),
      displayStatus: "waiting_for_confirmation",
      pendingAction,
    });

    useWorkflowStore.getState().hydrateDecisionReview("run-a", "action-a", decisionReview);

    expect(useWorkflowStore.getState().runs[0]).toMatchObject({
      updatedAt: "2026-08-01T03:00:00Z",
      decisionReview,
    });
  });
});

function overview(canonicalIdentityKey: string, identityRevision: string): WorkflowsOverview {
  return {
    schemaVersion: 1,
    projectAccess: {
      projectId: "project-a",
      canonicalIdentityKey,
      identityRevision,
      trust: "trusted",
      filesystemAccess: "writable",
      persistence: "persistent",
      gitState: "clean",
    },
    rows: [],
  };
}

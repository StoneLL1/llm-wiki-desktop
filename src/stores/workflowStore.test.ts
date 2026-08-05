import { beforeEach, describe, expect, it } from "vitest";

import type { WorkflowRun, WorkflowsOverview } from "../types/workflow";
import { useWorkflowStore } from "./workflowStore";

function run(taskId: string, updatedAt: string): WorkflowRun {
  return {
    schemaVersion: 1,
    taskId,
    projectId: "project-a",
    canonicalIdentityKey: "identity-a",
    identityRevision: "revision-a",
    kind: "health_check",
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
  beforeEach(() => useWorkflowStore.getState().reset());

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
      decisionReview,
    });
    useWorkflowStore.getState().replaceRuns([
      { ...run("run-a", "2026-08-01T04:00:00Z"), displayStatus: "waiting_for_confirmation" },
    ]);
    expect(useWorkflowStore.getState().runs[0]?.decisionReview).toEqual(decisionReview);
  });

  it("atomically clears project-scoped presentation when the canonical identity rotates", () => {
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    const firstOverview = overview("identity-a", "revision-a");
    useWorkflowStore.getState().setProjectSnapshot(
      firstOverview,
      [run("old-run", "2026-08-01T03:00:00Z")],
      "old-cursor",
    );
    useWorkflowStore.setState({
      preparation: {} as never,
      selectedTaskId: "old-run",
      surface: "detail",
    });

    useWorkflowStore.getState().setProjectSnapshot(
      overview("identity-b", "revision-b"),
      [{
        ...run("new-run", "2026-08-01T04:00:00Z"),
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      }],
      null,
    );

    expect(useWorkflowStore.getState()).toMatchObject({
      runs: [expect.objectContaining({ taskId: "new-run" })],
      preparation: null,
      selectedTaskId: null,
      surface: "overview",
      historyCursor: null,
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

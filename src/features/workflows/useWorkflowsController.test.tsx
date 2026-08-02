import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent } from "../../types/task";
import type { WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { useWorkflowStore } from "../../stores/workflowStore";

const mocks = vi.hoisted(() => ({
  getOverview: vi.fn(),
  listRuns: vi.fn(),
  prepare: vi.fn(),
  listener: null as ((event: BackendEvent) => void) | null,
}));

vi.mock("../../services/workflowApi", () => ({
  getWorkflowsOverview: mocks.getOverview,
  listWorkflowRuns: mocks.listRuns,
  prepareWorkflow: mocks.prepare, startWorkflow: vi.fn(), cancelWorkflowRun: vi.fn(),
  undoCancelQueuedWorkflow: vi.fn(), reorderQueuedWorkflow: vi.fn(), retryWorkflow: vi.fn(),
  confirmWorkflowAction: vi.fn(), discardWorkflowResult: vi.fn(), continueQueuedWorkflows: vi.fn(),
}));
vi.mock("../../hooks/useTaskEvents", () => ({
  registerTaskEventListener: (listener: (event: BackendEvent) => void) => {
    mocks.listener = listener;
    return () => { mocks.listener = null; };
  },
}));

import { useWorkflowsController } from "./useWorkflowsController";

const project = {
  projectId: "project-a", name: "A", rootPath: "D:/a", template: "general" as const,
  wikiPageCount: 1, sourceCount: 1, taskCount: 0, indexState: "indexed" as const,
  graphState: "cached" as const, agentRoute: "byok" as const,
  health: { isWikiProject: true, hasPurpose: true, hasSchema: true, hasAppState: true, hasObsidian: false, missingPaths: [] },
};
const overview: WorkflowsOverview = {
  schemaVersion: 1,
  projectAccess: { projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", trust: "trusted", filesystemAccess: "writable", persistence: "persistent", gitState: "clean" },
  rows: [],
};
const run: WorkflowRun = {
  schemaVersion: 1, taskId: "run-a", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a",
  kind: "health_check", displayStatus: "running", scope: { kind: "health_check", mode: "local_quick" }, route: { kind: "local", routeRevision: "local" },
  fingerprint: "f", baselineFingerprint: "b", stages: [], currentStageId: null, queuePosition: null, continuationRequired: false,
  retry: null, pendingAction: null, result: null, error: null, startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:00:00Z", completedAt: null,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("useWorkflowsController", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    useWorkflowStore.getState().reset();
    mocks.listener = null;
    mocks.getOverview.mockReset().mockResolvedValue(overview);
    mocks.listRuns.mockReset().mockResolvedValue({ runs: [], nextCursor: null });
  });
  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    vi.clearAllMocks();
  });

  it("accepts only workflow events matching the active canonical identity revision", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => mocks.listener?.({ eventId: "1", eventType: "workflow_updated", projectId: "project-a", taskId: "run-a", timestamp: run.updatedAt, payload: { ...run, identityRevision: "stale" } }));
    expect(useWorkflowStore.getState().runs).toEqual([]);
    act(() => mocks.listener?.({ eventId: "2", eventType: "workflow_updated", projectId: "project-a", taskId: "run-a", timestamp: run.updatedAt, payload: run }));
    expect(useWorkflowStore.getState().runs[0]?.taskId).toBe("run-a");
  });

  it("buffers an event received before overview access and applies it after identity validation", async () => {
    const pendingOverview = deferred<WorkflowsOverview>();
    mocks.getOverview
      .mockReset()
      .mockReturnValueOnce(pendingOverview.promise)
      .mockResolvedValue(overview);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.listener).not.toBeNull());

    act(() => mocks.listener?.({
      eventId: "before-overview",
      eventType: "workflow_updated",
      projectId: "project-a",
      taskId: run.taskId,
      timestamp: run.updatedAt,
      payload: run,
    }));
    expect(useWorkflowStore.getState().runs).toEqual([]);

    await act(async () => pendingOverview.resolve(overview));

    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.taskId).toBe(run.taskId));
    expect(mocks.getOverview).toHaveBeenCalledTimes(1);
  });

  it("keeps only the newest buffered event for the same task", async () => {
    const pendingOverview = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReset().mockReturnValueOnce(pendingOverview.promise);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.listener).not.toBeNull());
    const newerRun = {
      ...run,
      displayStatus: "succeeded" as const,
      updatedAt: "2026-08-01T02:00:00Z",
      completedAt: "2026-08-01T02:00:00Z",
    };

    act(() => {
      mocks.listener?.({
        eventId: "newer",
        eventType: "workflow_updated",
        projectId: "project-a",
        taskId: run.taskId,
        timestamp: newerRun.updatedAt,
        payload: newerRun,
      });
      mocks.listener?.({
        eventId: "older",
        eventType: "workflow_updated",
        projectId: "project-a",
        taskId: run.taskId,
        timestamp: run.updatedAt,
        payload: run,
      });
    });
    await act(async () => pendingOverview.resolve(overview));

    await waitFor(() => expect(useWorkflowStore.getState().runs[0]).toMatchObject({
      taskId: run.taskId,
      displayStatus: "succeeded",
      updatedAt: newerRun.updatedAt,
    }));
  });

  it("retries one coalesced refresh after a buffered-event refresh fails", async () => {
    const failedOverview = deferred<WorkflowsOverview>();
    mocks.getOverview
      .mockReset()
      .mockReturnValueOnce(failedOverview.promise)
      .mockResolvedValueOnce(overview);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.listener).not.toBeNull());

    act(() => mocks.listener?.({
      eventId: "before-failure",
      eventType: "workflow_updated",
      projectId: "project-a",
      taskId: run.taskId,
      timestamp: run.updatedAt,
      payload: run,
    }));
    await act(async () => failedOverview.reject(new Error("overview unavailable")));

    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.taskId).toBe(run.taskId));
  });

  it("drops buffered events for another project or canonical identity", async () => {
    const pendingOverview = deferred<WorkflowsOverview>();
    mocks.getOverview
      .mockReset()
      .mockReturnValueOnce(pendingOverview.promise)
      .mockResolvedValue(overview);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.listener).not.toBeNull());

    act(() => {
      mocks.listener?.({
        eventId: "other-project",
        eventType: "workflow_updated",
        projectId: "project-b",
        taskId: "run-b",
        timestamp: run.updatedAt,
        payload: { ...run, taskId: "run-b", projectId: "project-b" },
      });
      mocks.listener?.({
        eventId: "stale-identity",
        eventType: "workflow_updated",
        projectId: "project-a",
        taskId: run.taskId,
        timestamp: run.updatedAt,
        payload: { ...run, identityRevision: "stale" },
      });
    });

    await act(async () => pendingOverview.resolve(overview));

    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    expect(useWorkflowStore.getState().runs).toEqual([]);
  });

  it("clears buffered events when the active project changes", async () => {
    const projectB = { ...project, name: "B", rootPath: "D:/b" };
    const overviewB: WorkflowsOverview = { ...overview };
    const pendingA = deferred<WorkflowsOverview>();
    const pendingB = deferred<WorkflowsOverview>();
    mocks.getOverview
      .mockReset()
      .mockReturnValueOnce(pendingA.promise)
      .mockReturnValueOnce(pendingB.promise)
      .mockResolvedValue(overviewB);
    const { rerender } = renderHook(
      ({ currentProject }) => useWorkflowsController(currentProject, true),
      { initialProps: { currentProject: project } },
    );
    await waitFor(() => expect(mocks.listener).not.toBeNull());
    act(() => mocks.listener?.({
      eventId: "project-a-pending",
      eventType: "workflow_updated",
      projectId: "project-a",
      taskId: run.taskId,
      timestamp: run.updatedAt,
      payload: run,
    }));

    rerender({ currentProject: projectB });
    await act(async () => {
      pendingA.resolve(overview);
      pendingB.resolve(overviewB);
    });

    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overviewB));
    expect(useWorkflowStore.getState().runs).toEqual([]);
  });

  it("lets the current project refresh while the previous project refresh is still in flight", async () => {
    const projectB = { ...project, projectId: "project-b", name: "B", rootPath: "D:/b" };
    const overviewB: WorkflowsOverview = {
      ...overview,
      projectAccess: {
        ...overview.projectAccess!,
        projectId: "project-b",
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    };
    const runB: WorkflowRun = {
      ...run,
      taskId: "run-b",
      projectId: "project-b",
      canonicalIdentityKey: "identity-b",
      identityRevision: "revision-b",
    };
    const pendingA = deferred<WorkflowsOverview>();
    const pendingB = deferred<WorkflowsOverview>();
    mocks.getOverview
      .mockReset()
      .mockReturnValueOnce(pendingA.promise)
      .mockReturnValueOnce(pendingB.promise)
      .mockResolvedValue(overviewB);
    const { rerender } = renderHook(
      ({ currentProject }) => useWorkflowsController(currentProject, true),
      { initialProps: { currentProject: project } },
    );
    await waitFor(() => expect(mocks.listener).not.toBeNull());

    rerender({ currentProject: projectB });
    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(2));
    act(() => mocks.listener?.({
      eventId: "project-b-pending",
      eventType: "workflow_updated",
      projectId: "project-b",
      taskId: runB.taskId,
      timestamp: runB.updatedAt,
      payload: runB,
    }));
    await act(async () => pendingB.reject(new Error("project B overview unavailable")));

    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(3));
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.taskId).toBe(runB.taskId));
    await act(async () => pendingA.resolve(overview));
  });

  it("loads workflow history through the backend cursor", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockResolvedValueOnce({ runs: [{ ...run, taskId: "run-b", updatedAt: "2026-08-01T01:00:00Z" }], nextCursor: null });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));
    await act(() => result.current.loadHistoryMore());
    expect(mocks.listRuns).toHaveBeenLastCalledWith(expect.objectContaining({ cursor: "cursor-a", limit: 100 }));
    expect(useWorkflowStore.getState().runs.map((item) => item.taskId)).toContain("run-b");
    expect(useWorkflowStore.getState().historyCursor).toBeNull();
  });
});

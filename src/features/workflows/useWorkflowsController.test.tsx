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

describe("useWorkflowsController", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    useWorkflowStore.getState().reset();
    mocks.getOverview.mockResolvedValue(overview);
    mocks.listRuns.mockResolvedValue({ runs: [], nextCursor: null });
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

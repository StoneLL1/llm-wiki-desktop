import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent } from "../../types/task";
import type { WorkflowPreparation, WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { useNavigationStore } from "../../stores/navigationStore";
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
const preparation: WorkflowPreparation = {
  schemaVersion: 1,
  preparationId: "prep-a",
  preparationRevision: "revision-1",
  projectAccess: overview.projectAccess!,
  kind: "health_check",
  scope: { kind: "health_check", mode: "complete" },
  baseline: {
    fingerprint: "baseline-a",
    capturedAt: "2026-08-01T00:00:00Z",
    itemCount: 1,
  },
  route: {
    kind: "byok",
    provider: "ollama",
    model: "qwen",
    routeRevision: "route-1",
  },
  prerequisites: [],
  output: {
    labelKey: "workflows.output.session",
    location: null,
    mayChangeWiki: false,
  },
  gitPolicy: "not_required",
  requiresScopeConfirmation: false,
  quickRerunEligible: false,
};

describe("useWorkflowsController", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    useWorkflowStore.getState().reset();
    useNavigationStore.setState({
      activeView: "workflows",
      settingsOpen: false,
      settingsSection: "general",
      workflowSettingsReturnIntent: null,
    });
    mocks.listener = null;
    mocks.getOverview.mockReset().mockResolvedValue(overview);
    mocks.listRuns.mockReset().mockResolvedValue({ runs: [], nextCursor: null });
    mocks.prepare.mockReset().mockResolvedValue(preparation);
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

  it("does not let an older same-root refresh restore a replaced identity", async () => {
    const oldOverview = deferred<WorkflowsOverview>();
    const newOverview = {
      ...overview,
      projectAccess: {
        ...overview.projectAccess!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    };
    mocks.getOverview
      .mockReset()
      .mockReturnValueOnce(oldOverview.promise)
      .mockResolvedValueOnce(newOverview);
    mocks.listRuns
      .mockReset()
      .mockResolvedValueOnce({ runs: [{ ...run, taskId: "old-run" }], nextCursor: null })
      .mockResolvedValueOnce({
        runs: [{
          ...run,
          taskId: "new-run",
          canonicalIdentityKey: "identity-b",
          identityRevision: "revision-b",
        }],
        nextCursor: null,
      });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(1));

    await act(() => result.current.refresh());
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.taskId).toBe("new-run"));
    await act(async () => oldOverview.resolve(overview));

    expect(useWorkflowStore.getState().overview?.projectAccess).toMatchObject({
      canonicalIdentityKey: "identity-b",
      identityRevision: "revision-b",
    });
    expect(useWorkflowStore.getState().runs.map((item) => item.taskId)).toEqual(["new-run"]);
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

  it("opens AI Settings with the run scope and route instead of preparing first", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({
      surface: "detail",
      selectedTaskId: run.taskId,
    });

    await act(() => result.current.adjustAndPrepare({
      ...run,
      route: {
        kind: "byok",
        provider: "ollama",
        model: "qwen",
        routeRevision: "route-1",
      },
    }, true));

    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(useNavigationStore.getState()).toMatchObject({
      settingsOpen: true,
      settingsSection: "ai",
      workflowSettingsReturnIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "health_check",
        scope: run.scope,
        routeSelection: { kind: "byok", provider: "ollama" },
        source: "adjust",
        expectedSurface: "detail",
        expectedTaskId: run.taskId,
      },
    });
  });

  it("re-prepares the current structured preparation for prepare_again", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({ preparation, surface: "preparation" });

    act(() => result.current.handlePrerequisite("prepare_again"));

    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledWith({
      projectId: project.projectId,
      projectRootPath: project.rootPath,
      kind: preparation.kind,
      scope: preparation.scope,
      routeSelection: { kind: "byok", provider: "ollama" },
    }));
  });

  it("delegates project authority prerequisites without pretending to grant access", async () => {
    const onProjectPrerequisite = vi.fn();
    const { result } = renderHook(() =>
      useWorkflowsController(project, true, { onProjectPrerequisite }),
    );
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({ preparation, surface: "preparation" });

    act(() => result.current.handlePrerequisite("trust_project"));

    expect(onProjectPrerequisite).toHaveBeenCalledWith(
      "trust_project",
      expect.objectContaining({ project, preparation, prepareAgain: expect.any(Function) }),
    );
    expect(mocks.prepare).not.toHaveBeenCalled();
  });

  it("reports an honest project-flow recovery when no authority handler is connected", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({ preparation, surface: "preparation", error: null });

    act(() => result.current.handlePrerequisite("configure_git"));

    expect(useWorkflowStore.getState().error).toBeTruthy();
    expect(mocks.prepare).not.toHaveBeenCalled();
  });
});

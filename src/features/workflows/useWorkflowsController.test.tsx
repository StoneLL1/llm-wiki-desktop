import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent } from "../../types/task";
import type { WorkflowPreparation, WorkflowRun, WorkflowRunSummary, WorkflowsOverview } from "../../types/workflow";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { useTaskStore } from "../../stores/taskStore";
import { workflowRunSummary } from "../../services/workflowTaskSnapshot";

const mocks = vi.hoisted(() => ({
  getOverview: vi.fn(),
  listRuns: vi.fn(),
  getRun: vi.fn(),
  prepare: vi.fn(),
  start: vi.fn(),
  cancel: vi.fn(),
  confirm: vi.fn(),
  discard: vi.fn(),
  listener: null as ((event: BackendEvent) => void) | null,
}));

vi.mock("../../services/workflowApi", () => ({
  getWorkflowsOverview: mocks.getOverview,
  listWorkflowRuns: mocks.listRuns,
  getWorkflowRun: mocks.getRun,
  prepareWorkflow: mocks.prepare, startWorkflow: mocks.start, cancelWorkflowRun: mocks.cancel,
  undoCancelQueuedWorkflow: vi.fn(), reorderQueuedWorkflow: vi.fn(), retryWorkflow: vi.fn(),
  confirmWorkflowAction: mocks.confirm, discardWorkflowResult: mocks.discard, continueQueuedWorkflows: vi.fn(),
}));
vi.mock("../../services/taskEventDispatcher", () => ({
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
  recentRuns: [],
  activeRuns: [],
  sessionId: "session-a",
};
const noProjectOverview: WorkflowsOverview = {
  schemaVersion: 1,
  projectAccess: null,
  rows: [],
  recentRuns: [],
  activeRuns: [],
  sessionId: "session-a",
};
const run: WorkflowRun = {
  schemaVersion: 2, revision: "1", sessionId: "session-a", taskId: "run-a", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a",
  kind: "health_check", operation: { kind: "built_in" }, displayStatus: "running", scope: { kind: "health_check", mode: "local_quick" }, route: { kind: "local", routeRevision: "local" },
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

function emitRun(value: WorkflowRun | WorkflowRunSummary) {
  const payload = workflowRunSummary(value);
  mocks.listener?.({ eventId: `event-${payload.taskId}-${payload.revision}`, eventType: "workflow_updated",
    projectId: payload.projectId, taskId: payload.taskId, timestamp: payload.updatedAt, payload });
}

describe("useWorkflowsController", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    useWorkflowStore.getState().reset();
    useTaskStore.setState({ workflowById: {}, workflowSessionId: null, retiredWorkflowSessions: [], drawerOpen: false, selectedTaskId: null });
    useProjectStore.setState({
      currentProject: project,
      authority: {
        projectId: project.projectId,
        canonicalRootPath: project.rootPath,
        canonicalIdentityKey: "identity-a",
        identityRevision: "revision-a",
      } as never,
    });
    useNavigationStore.setState({
      activeView: "workflows",
      settingsOpen: false,
      settingsSection: "general",
      workflowSettingsReturnIntent: null,
      workflowLaunchIntent: null,
    });
    mocks.listener = null;
    mocks.getOverview.mockReset().mockResolvedValue(overview);
    mocks.listRuns.mockReset().mockResolvedValue({ runs: [], nextCursor: null });
    mocks.getRun.mockReset().mockResolvedValue(run);
    mocks.prepare.mockReset().mockResolvedValue(preparation);
    mocks.start.mockReset().mockResolvedValue({ kind: "created", run });
    mocks.cancel.mockReset().mockResolvedValue({ ...run, displayStatus: "cancelled" });
    mocks.confirm.mockReset().mockResolvedValue(run);
    mocks.discard.mockReset().mockResolvedValue({ ...run, displayStatus: "cancelled" });
  });
  afterEach(() => {
    vi.useRealTimers();
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    vi.clearAllMocks();
  });


  it("retains global task facts while the page is hidden without loading queries or taking over UI", async () => {
    const { rerender } = renderHook(({ enabled }) => useWorkflowsController(project, enabled), {
      initialProps: { enabled: false },
    });
    expect(mocks.listener).not.toBeNull();
    expect(mocks.getOverview).not.toHaveBeenCalled();
    act(() => emitRun({ ...run, revision: "2", displayStatus: "completed" }));
    expect(useTaskStore.getState().workflowById[run.taskId]?.displayStatus).toBe("completed");
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
    expect(useTaskStore.getState().drawerOpen).toBe(false);
    expect(mocks.getRun).not.toHaveBeenCalled();
    rerender({ enabled: true });
    await waitFor(() => expect(useWorkflowStore.getState().overview?.recentRuns?.[0]?.displayStatus).toBe("completed"));
    expect(mocks.listRuns).not.toHaveBeenCalled();
    expect(mocks.prepare).not.toHaveBeenCalled();
  });

  it("keeps foreign identity facts global without merging them into the active overview", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => emitRun({ ...run, taskId: "foreign-run", identityRevision: "stale", displayStatus: "completed" }));
    expect(useTaskStore.getState().workflowById["foreign-run"]).toBeDefined();
    expect(useWorkflowStore.getState().overview?.recentRuns).toEqual([]);
    act(() => emitRun({ ...run, revision: "2", displayStatus: "completed" }));
    expect(useWorkflowStore.getState().overview?.recentRuns?.[0]?.taskId).toBe(run.taskId);
    expect(useWorkflowStore.getState().runs).toEqual([]);
  });

  it("keeps the current running task as attention when a same-kind task joins its queue", async () => {
    const rowOverview: WorkflowsOverview = { ...overview, rows: [{ kind: "health_check", state: "ready",
      recommended: false, activeTaskId: null, activeContinuationRequired: false,
      lastCompletedAt: null, lastCompletedTaskId: null, prerequisite: null }] };
    mocks.getOverview.mockResolvedValue(rowOverview);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(rowOverview));
    act(() => emitRun(run));
    expect(useWorkflowStore.getState().overview?.rows[0]).toMatchObject({ state: "running", activeTaskId: run.taskId });
    await act(async () => emitRun({ ...run, taskId: "run-b", displayStatus: "queued", queuePosition: 1 }));
    expect(useWorkflowStore.getState().overview?.rows[0]).toMatchObject({ state: "running", activeTaskId: run.taskId });
    expect(useTaskStore.getState().workflowById["run-b"]?.displayStatus).toBe("queued");
  });

  it("does not resurrect an old failure after its later retry is cancelled", async () => {
    const rowOverview: WorkflowsOverview = { ...overview, rows: [{ kind: "health_check", state: "ready",
      recommended: false, activeTaskId: null, activeContinuationRequired: false,
      lastCompletedAt: null, lastCompletedTaskId: null, prerequisite: null }] };
    mocks.getOverview.mockResolvedValue(rowOverview);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(rowOverview));
    const retry = { ...run, taskId: "retry-b", startedAt: "2026-08-01T01:00:00Z", retry: { attemptOf: run.taskId, attemptNumber: 2 } };
    await act(async () => emitRun({ ...run, displayStatus: "failed" }));
    await act(async () => emitRun({ ...retry, displayStatus: "queued" }));
    await act(async () => emitRun({ ...retry, revision: "2", displayStatus: "cancelled" }));
    expect(useWorkflowStore.getState().overview?.rows[0]).toMatchObject({ state: "ready", activeTaskId: null });
    expect(useTaskStore.getState().workflowById[run.taskId]?.displayStatus).toBe("failed");
  });

  it("counts overview, history, detail, automatic preparation, and start independently", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    await act(() => result.current.openRun(run.taskId));
    await act(() => result.current.prepare("health_check"));
    await act(() => result.current.startPrepared(false, false));
    expect({ overview: mocks.getOverview.mock.calls.length, history: mocks.listRuns.mock.calls.length,
      detail: mocks.getRun.mock.calls.length, prepare: mocks.prepare.mock.calls.length, start: mocks.start.mock.calls.length,
    }).toEqual({ overview: 2, history: 0, detail: 1, prepare: 2, start: 1 });
  });

  it("handles 200 summary events with one terminal overview refresh and no history, preparation, or detail IPC", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      for (let index = 1; index <= 200; index += 1) emitRun({ ...run, revision: String(index),
        displayStatus: index === 200 ? "completed" : "running" });
    });
    expect(useTaskStore.getState().workflowById[run.taskId]).toMatchObject({ revision: "200", displayStatus: "completed" });
    expect(useWorkflowStore.getState().runs).toEqual([]);
    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
    expect(mocks.listRuns).not.toHaveBeenCalled();
    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.getRun).not.toHaveBeenCalled();
  });

  it("keeps history and preparation off all 50 hot page switches", async () => {
    const { rerender } = renderHook(({ enabled }) => useWorkflowsController(project, enabled), { initialProps: { enabled: true } });
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    for (let index = 0; index < 50; index += 1) {
      rerender({ enabled: false });
      rerender({ enabled: true });
      await act(async () => { await Promise.resolve(); });
    }
    expect(mocks.getOverview).toHaveBeenCalledTimes(51);
    expect(mocks.listRuns).not.toHaveBeenCalled();
    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.getRun).not.toHaveBeenCalled();
  });

  it("runs one overview request at a time with one dirty retry and no self-sustaining wave", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const pending = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReturnValueOnce(pending.promise);
    const requests: Promise<void>[] = [];
    act(() => { for (let index = 0; index < 50; index += 1) requests.push(result.current.refresh()); });
    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
    expect(useWorkflowStore.getState().operations["overview:reconcile"]?.pending).toBe(true);
    await act(async () => { pending.resolve(overview); await Promise.all(requests); });
    expect(mocks.getOverview).toHaveBeenCalledTimes(3);
    expect(useWorkflowStore.getState().operations["overview:reconcile"]?.pending).toBe(false);
    expect(mocks.listRuns).not.toHaveBeenCalled();
  });

  it("merges early events with an older overview snapshot by decimal revision", async () => {
    const pending = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReturnValueOnce(pending.promise);
    renderHook(() => useWorkflowsController(project, true));
    act(() => emitRun({ ...run, revision: "9007199254740993", displayStatus: "completed" }));
    expect(useTaskStore.getState().workflowById[run.taskId]?.revision).toBe("9007199254740993");
    await act(async () => pending.resolve({ ...overview, recentRuns: [workflowRunSummary({ ...run, revision: "9007199254740992" })] }));
    expect(useWorkflowStore.getState().overview?.recentRuns?.[0]?.revision).toBe("9007199254740993");
    act(() => emitRun({ ...run, revision: "9007199254740992", updatedAt: "2099-01-01T00:00:00Z" }));
    expect(useTaskStore.getState().workflowById[run.taskId]?.displayStatus).toBe("completed");
    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
  });

  it("accepts a recovered session snapshot and rejects events from its retired process", async () => {
    mocks.getOverview.mockResolvedValueOnce({ ...overview, activeRuns: [workflowRunSummary({ ...run, revision: "50" })] });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useTaskStore.getState().workflowById[run.taskId]?.revision).toBe("50"));
    const recovered = { ...run, revision: "42", sessionId: "session-b", displayStatus: "interrupted" as const };
    mocks.getOverview.mockResolvedValueOnce({ ...overview, sessionId: "session-b", activeRuns: [workflowRunSummary(recovered)] });
    await act(() => result.current.refresh());
    expect(useTaskStore.getState().workflowById[run.taskId]).toMatchObject({ sessionId: "session-b", revision: "42", displayStatus: "interrupted" });
    act(() => emitRun({ ...run, revision: "999", displayStatus: "completed" }));
    expect(useTaskStore.getState().workflowById[run.taskId]?.sessionId).toBe("session-b");
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
  });

  it("updates a selected task's actual stage progress without fetching details", async () => {
    const runningOverview: WorkflowsOverview = { ...overview, activeRuns: [workflowRunSummary(run)], rows: [{
      kind: "health_check", state: "running", activeTaskId: run.taskId, activeContinuationRequired: false,
      lastCompletedAt: null, lastCompletedTaskId: null, prerequisite: null, recommended: false,
    }] };
    mocks.getOverview.mockResolvedValue(runningOverview);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview?.rows[0]?.state).toBe("running"));
    const stage = { id: "compile", ordinal: 1, status: "running" as const, labelKey: "workflows.stage.compile",
      startedAt: run.startedAt, completedAt: null, currentItem: "wiki/a.md", progress: { current: 1, total: 3 }, decision: null };
    act(() => {
      useWorkflowStore.getState().upsertRun({ ...run, stages: [stage], currentStageId: stage.id });
      useWorkflowStore.getState().selectRun(run.taskId);
      emitRun({ ...workflowRunSummary(run), revision: "2", currentStageId: stage.id,
        currentStage: { ...stage, currentItem: "wiki/页面.md", progress: { current: 2, total: 3 } } });
    });
    expect(useWorkflowStore.getState().runs[0]?.stages[0]?.progress?.current).toBe(2);
    expect(mocks.getRun).not.toHaveBeenCalled();
    expect(mocks.getOverview).toHaveBeenCalledTimes(1);
  });

  it("hydrates a selected semantic boundary once for repeated summary events", async () => {
    const pending = deferred<WorkflowRun>();
    mocks.getRun.mockReturnValueOnce(pending.promise);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const waiting = { ...run, revision: "2", displayStatus: "waiting_for_confirmation" as const,
      pendingAction: { id: "action-a", actionType: "batch_rewrite" as const, riskLevel: "high" as const,
        affectedPaths: ["wiki/a.md"], candidate: null, expiresAt: null, checkpointHash: "checkpoint-a" } };
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      for (let index = 0; index < 10; index += 1) emitRun(waiting);
    });
    expect(mocks.getRun).toHaveBeenCalledTimes(1);
    await act(async () => pending.resolve(waiting));
    expect(useWorkflowStore.getState().runs[0]?.pendingAction?.id).toBe("action-a");
    expect(mocks.listRuns).not.toHaveBeenCalled();
  });

  it("loads a newer waiting boundary after an older detail request finishes", async () => {
    const first = deferred<WorkflowRun>();
    const pendingAction = { id: "action-b", actionType: "batch_rewrite" as const, riskLevel: "high" as const,
      affectedPaths: ["wiki/b.md"], candidate: null, expiresAt: null, checkpointHash: "checkpoint-b" };
    const latest = { ...run, revision: "3", displayStatus: "waiting_for_confirmation" as const, pendingAction };
    mocks.getRun.mockReturnValueOnce(first.promise).mockResolvedValueOnce(latest);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      emitRun({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" });
      emitRun(latest);
    });
    expect(mocks.getRun).toHaveBeenCalledTimes(1);
    await act(async () => first.resolve({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" }));
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.pendingAction?.id).toBe("action-b"));
    expect(mocks.getRun).toHaveBeenCalledTimes(2);
  });

  it("does not show a superseded boundary's hydration error after cancellation", async () => {
    const pending = deferred<WorkflowRun>();
    mocks.getRun.mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce({ ...run, revision: "3", displayStatus: "cancelled" });
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      emitRun({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" });
      emitRun({ ...run, revision: "3", displayStatus: "cancelled" });
    });
    await act(async () => pending.reject(new Error("old candidate disappeared")));
    expect(useWorkflowStore.getState().operations[`task:${run.taskId}:hydrate:boundary`]?.error ?? null).toBeNull();
    expect(useWorkflowStore.getState().runs[0]?.displayStatus).toBe("cancelled");
  });

  it("does not let an older boundary detail replace a newer cancelled task", async () => {
    const pending = deferred<WorkflowRun>();
    mocks.getRun.mockReturnValueOnce(pending.promise);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      emitRun({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" });
      emitRun({ ...run, revision: "3", displayStatus: "cancelled" });
    });
    await act(async () => pending.resolve({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" }));
    expect(useWorkflowStore.getState().runs[0]).toMatchObject({ revision: "3", displayStatus: "cancelled", pendingAction: null });
  });

  it("stops detail retries when the backend keeps returning an unchanged older revision", async () => {
    mocks.getRun.mockResolvedValue(run);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    await act(async () => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      emitRun({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" });
    });
    await waitFor(() => expect(useWorkflowStore.getState().operations[`task:${run.taskId}:hydrate:boundary`]?.error?.technicalDetails).toContain("WORKFLOW_DETAIL_STALE"));
    expect(mocks.getRun.mock.calls.length).toBeLessThanOrEqual(2);
    expect(useTaskStore.getState().workflowById[run.taskId]?.revision).toBe("2");
  });

  it("does not hydrate a waiting task that has not been opened", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => emitRun({ ...run, revision: "2", displayStatus: "waiting_for_confirmation" }));
    expect(useTaskStore.getState().workflowById[run.taskId]?.displayStatus).toBe("waiting_for_confirmation");
    expect(useWorkflowStore.getState().runs).toEqual([]);
    expect(mocks.getRun).not.toHaveBeenCalled();
  });

  it("keeps overview available when independently opened history fails", async () => {
    mocks.listRuns.mockRejectedValueOnce(new Error("history unavailable"));
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    expect(mocks.listRuns).not.toHaveBeenCalled();
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().operations["history:filter"]?.error?.technicalDetails).toContain("history unavailable"));
    expect(useWorkflowStore.getState().overviewStatus).toBe("ready");
    expect(useWorkflowStore.getState().overview).toEqual(overview);
  });

  it("lets project B load while project A is pending and retains A facts without navigation", async () => {
    const pending = deferred<WorkflowsOverview>();
    const projectB = { ...project, projectId: "project-b", rootPath: "D:/b" };
    const accessB = { ...overview.projectAccess!, projectId: "project-b", canonicalIdentityKey: "identity-b", identityRevision: "revision-b" };
    mocks.getOverview.mockReturnValueOnce(pending.promise).mockResolvedValue({ ...overview, projectAccess: accessB });
    const { rerender } = renderHook(({ current }) => useWorkflowsController(current, true), { initialProps: { current: project } });
    act(() => {
      useProjectStore.setState({ currentProject: projectB, authority: { ...useProjectStore.getState().authority!, ...accessB, canonicalRootPath: projectB.rootPath, filesystemAccess: "writable" } });
      rerender({ current: projectB });
    });
    await waitFor(() => expect(useWorkflowStore.getState().overview?.projectAccess).toEqual(accessB));
    act(() => emitRun({ ...run, revision: "2", displayStatus: "completed" }));
    await act(async () => pending.resolve(overview));
    expect(useTaskStore.getState().workflowById[run.taskId]?.displayStatus).toBe("completed");
    expect(useWorkflowStore.getState().overview?.projectAccess).toEqual(accessB);
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
    expect(useTaskStore.getState().drawerOpen).toBe(false);
    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
  });

  it("rejects an older prepare response after a same-root identity replacement", async () => {
    const oldPreparation = deferred<WorkflowPreparation>();
    const replacementOverview: WorkflowsOverview = {
      ...overview,
      projectAccess: { ...overview.projectAccess!, identityRevision: "revision-b" },
    };
    mocks.prepare.mockReset().mockReturnValueOnce(oldPreparation.promise);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));

    let request!: Promise<void>;
    act(() => { request = result.current.prepare("health_check"); });
    act(() => useWorkflowStore.getState().setOverviewSnapshot(replacementOverview));
    await act(async () => {
      oldPreparation.resolve(preparation);
      await request;
    });

    expect(useWorkflowStore.getState().overview?.projectAccess?.identityRevision).toBe("revision-b");
    expect(useWorkflowStore.getState().preparation).toBeNull();
  });

  it("invalidates in-flight work when live authority rotates before workflow state", async () => {
    const oldPreparation = deferred<WorkflowPreparation>();
    const replacementOverview: WorkflowsOverview = {
      ...overview,
      projectAccess: {
        ...overview.projectAccess!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    };
    mocks.prepare.mockReturnValueOnce(oldPreparation.promise);
    mocks.getOverview.mockResolvedValueOnce(overview).mockResolvedValue(replacementOverview);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    let request!: Promise<void>;
    act(() => { request = result.current.prepare("health_check"); });
    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce());

    act(() => useProjectStore.setState({
      authority: {
        ...useProjectStore.getState().authority!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    }));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(replacementOverview));
    await act(async () => {
      oldPreparation.resolve(preparation);
      await request;
    });

    expect(useWorkflowStore.getState().preparation).toBeNull();
    expect(useWorkflowStore.getState().overview?.projectAccess?.identityRevision).toBe("revision-b");
  });

  it("does not commit an old overview error after authority rotates", async () => {
    const oldOverview = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReset().mockReturnValueOnce(oldOverview.promise).mockResolvedValue(overview);
    const failOperation = vi.spyOn(useWorkflowStore.getState(), "failOperation");
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledOnce());

    await act(async () => {
      useProjectStore.setState({
        authority: {
          ...useProjectStore.getState().authority!,
          canonicalIdentityKey: "identity-b",
          identityRevision: "revision-b",
        },
      });
      oldOverview.reject(new Error("old identity failed"));
      await oldOverview.promise.catch(() => undefined);
    });

    expect(failOperation).not.toHaveBeenCalledWith(
      "overview:init",
      expect.any(Number),
      expect.objectContaining({ technicalDetails: expect.stringContaining("old identity failed") }),
    );
    failOperation.mockRestore();
  });

  it("rejects a stale response immediately when currentProject changes before effects run", async () => {
    const oldPreparation = deferred<WorkflowPreparation>();
    mocks.prepare.mockReturnValueOnce(oldPreparation.promise);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    let request!: Promise<void>;
    act(() => { request = result.current.prepare("health_check"); });
    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce());

    await act(async () => {
      useProjectStore.setState({ currentProject: { ...project, projectId: "project-b" } });
      oldPreparation.resolve(preparation);
      await request;
    });

    expect(useWorkflowStore.getState().preparation).toBeNull();
  });

  it("rejects an in-flight start result after a same-root identity replacement", async () => {
    const replacementOverview: WorkflowsOverview = {
      ...overview,
      projectAccess: { ...overview.projectAccess!, identityRevision: "revision-b" },
    };
    const staleStart = deferred<{ kind: "created"; run: WorkflowRun }>();
    const postStartOverview = deferred<WorkflowsOverview>();
    mocks.start.mockReset().mockReturnValueOnce(staleStart.promise);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({
      preparation,
    });
    mocks.getOverview.mockReturnValueOnce(postStartOverview.promise);

    let request!: Promise<void>;
    act(() => { request = result.current.startPrepared(false, false); });
    await waitFor(() => expect(mocks.start).toHaveBeenCalledTimes(1));
    expect(mocks.start).toHaveBeenCalledWith(expect.objectContaining({
      preparationId: preparation.preparationId,
      preparationRevision: preparation.preparationRevision,
    }));
    act(() => {
      useWorkflowStore.getState().setOverviewSnapshot(replacementOverview);
      useWorkflowStore.setState({
        preparation: {
          ...preparation,
          projectAccess: replacementOverview.projectAccess!,
          preparationRevision: "revision-b",
        },
      });
    });
    await act(async () => staleStart.resolve({ kind: "created", run }));
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
    expect(useWorkflowStore.getState().runs).toEqual([]);

    await act(async () => {
      postStartOverview.resolve(replacementOverview);
      await request;
    });
  });

  it("coalesces rapid duplicate starts for the same prepared revision", async () => {
    const pendingStart = deferred<{ kind: "created"; run: WorkflowRun }>();
    mocks.start.mockReset().mockReturnValue(pendingStart.promise);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({ preparation });

    let first!: Promise<void>;
    let second!: Promise<void>;
    act(() => {
      first = result.current.startPrepared(false, false);
      second = result.current.startPrepared(false, false);
    });

    await waitFor(() => expect(mocks.start).toHaveBeenCalledTimes(1));
    expect(useWorkflowStore.getState().operations[`start:${preparation.preparationId}`]?.pending).toBe(true);

    await act(async () => {
      pendingStart.resolve({ kind: "created", run });
      await Promise.all([first, second]);
    });
  });

  it("loads the backend no-project overview instead of treating an empty project as uninitialized", async () => {
    const emptyProject = { ...project, projectId: "", name: "", rootPath: "" };
    mocks.getOverview.mockResolvedValueOnce(noProjectOverview);
    useProjectStore.setState({ currentProject: emptyProject, authority: null });

    renderHook(() => useWorkflowsController(emptyProject, true));

    await waitFor(() => expect(useWorkflowStore.getState()).toMatchObject({
      overview: noProjectOverview,
      overviewStatus: "ready",
    }));
    expect(mocks.getOverview).toHaveBeenCalledWith({ projectId: "", projectRootPath: "" });
    expect(mocks.listRuns).not.toHaveBeenCalled();
  });

  it("exposes an overview load failure as a retryable error state", async () => {
    mocks.getOverview.mockRejectedValueOnce(new Error("overview unavailable"));

    renderHook(() => useWorkflowsController(project, true));

    await waitFor(() => expect(useWorkflowStore.getState()).toMatchObject({
      overview: null,
      overviewStatus: "error",
    }));
    expect(useWorkflowStore.getState().operations["overview:init"]?.error?.technicalDetails).toContain("overview unavailable");
  });

  it("loads workflow history through the backend cursor", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockResolvedValueOnce({ runs: [{ ...run, taskId: "run-b", updatedAt: "2026-08-01T01:00:00Z" }], nextCursor: null });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));
    await act(() => result.current.loadHistoryMore());
    expect(mocks.listRuns).toHaveBeenLastCalledWith(expect.objectContaining({ cursor: "cursor-a", limit: 50 }));
    expect(useWorkflowStore.getState().historyRuns.map((item) => item.taskId)).toContain("run-b");
    expect(useWorkflowStore.getState().historyCursor).toBeNull();
  });

  it("reloads history once with server filters and clears the previous cursor", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockResolvedValueOnce({ runs: [], nextCursor: null });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    await act(() => result.current.filterHistory("health_check", "failed"));

    expect(mocks.listRuns).toHaveBeenCalledTimes(2);
    expect(mocks.listRuns).toHaveBeenLastCalledWith(expect.objectContaining({
      workflowKind: "health_check",
      displayStatus: "failed",
      cursor: null,
      limit: 50,
    }));
    expect(useWorkflowStore.getState()).toMatchObject({
      historyKind: "health_check",
      historyStatus: "failed",
      historyCursor: null,
    });
  });

  it("keeps a stale filter response from overwriting the latest filter page", async () => {
    const stale = deferred<{ runs: WorkflowRun[]; nextCursor: string | null }>();
    const latest = { ...run, taskId: "latest-filter-run", displayStatus: "failed" as const };
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: null })
      .mockReturnValueOnce(stale.promise)
      .mockResolvedValueOnce({ runs: [latest], nextCursor: null });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(1));

    let stalePromise!: Promise<void>;
    act(() => { stalePromise = result.current.filterHistory("health_check", null); });
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(2));
    await act(() => result.current.filterHistory("health_check", "failed"));
    await act(async () => {
      stale.resolve({ runs: [{ ...run, taskId: "stale-filter-run" }], nextCursor: "stale-cursor" });
      await stalePromise;
    });

    expect(useWorkflowStore.getState().historyRuns.some((item) => item.taskId === "stale-filter-run")).toBe(false);
    expect(useWorkflowStore.getState().historyRuns.some((item) => item.taskId === latest.taskId)).toBe(true);
    expect(useWorkflowStore.getState().historyCursor).toBeNull();
  });

  it("keeps a stale full history refresh from overwriting a newer filter page", async () => {
    const stale = deferred<{ runs: WorkflowRun[]; nextCursor: string | null }>();
    const latest = { ...run, taskId: "latest-filter-run", displayStatus: "failed" as const };
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: null })
      .mockReturnValueOnce(stale.promise)
      .mockResolvedValueOnce({ runs: [latest], nextCursor: null });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(1));
    let refreshPromise!: Promise<void>;
    act(() => { refreshPromise = result.current.filterHistory(null, null); });
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(2));
    await act(() => result.current.filterHistory("health_check", "failed"));
    await act(async () => {
      stale.resolve({ runs: [{ ...run, taskId: "stale-refresh-run" }], nextCursor: "stale-cursor" });
      await refreshPromise;
    });

    expect(useWorkflowStore.getState().historyRuns.map((item) => item.taskId)).toEqual([latest.taskId]);
    expect(useWorkflowStore.getState().historyCursor).toBeNull();
    expect(useWorkflowStore.getState().overview?.projectAccess?.identityRevision).toBe("revision-a");
  });

  it("rejects a paginated history page from a different canonical identity", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockResolvedValueOnce({
        runs: [{ ...run, taskId: "foreign-run", canonicalIdentityKey: "identity-b", identityRevision: "revision-b" }],
        nextCursor: null,
      });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    await act(() => result.current.loadHistoryMore());

    expect(useWorkflowStore.getState().historyRuns.map((item) => item.taskId)).toEqual([run.taskId]);
    expect(useWorkflowStore.getState().historyCursor).toBeNull();
    expect(useWorkflowStore.getState().operations["history:page"]?.error).toBeTruthy();

    await act(() => result.current.loadHistoryMore());
    expect(mocks.listRuns).toHaveBeenCalledTimes(2);
  });

  it("invalidates an oversized history page before it can replay the cursor", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockRejectedValueOnce({
        code: "WORKFLOW_HISTORY_PAGE_TOO_LARGE",
        message: "history page exceeded the bounded response size",
      });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    await act(() => result.current.loadHistoryMore());

    expect(useWorkflowStore.getState().historyCursor).toBeNull();
    await act(() => result.current.loadHistoryMore());
    expect(mocks.listRuns).toHaveBeenCalledTimes(2);
  });

  it("invalidates a stale history cursor so recovery reloads from the first page", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockRejectedValueOnce({
        code: "WORKFLOW_CURSOR_SCOPE_MISMATCH",
        message: "cursor is stale",
      });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    await act(() => result.current.loadHistoryMore());

    expect(useWorkflowStore.getState().historyCursor).toBeNull();
    expect(useWorkflowStore.getState().operations["history:page"]?.error).toMatchObject({
      summary: "This history page changed while it was open. Reload the current filters from the first page.",
    });
  });

  it("clears a superseded page error when a new server filter succeeds", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockResolvedValueOnce({ runs: [], nextCursor: null });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));
    useWorkflowStore.setState({
      operations: {
        ...useWorkflowStore.getState().operations,
        "history:page": { requestId: 999, pending: false, error: { summary: "old page error", technicalDetails: null } },
      },
    });

    await act(() => result.current.filterHistory("health_check", "completed"));

    expect(useWorkflowStore.getState().operations["history:page"]?.error ?? null).toBeNull();
    expect(useWorkflowStore.getState().historyKind).toBe("health_check");
    expect(useWorkflowStore.getState().historyStatus).toBe("completed");
  });

  it("does not let a superseded history page overwrite a full refresh with the same cursor", async () => {
    const pendingPage = deferred<{ runs: WorkflowRun[]; nextCursor: string | null }>();
    const freshRun = { ...run, taskId: "fresh-run", updatedAt: "2026-08-01T02:00:00Z" };
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockReturnValueOnce(pendingPage.promise)
      .mockResolvedValueOnce({ runs: [freshRun], nextCursor: "cursor-a" });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    let loadMorePromise!: Promise<void>;
    act(() => {
      loadMorePromise = result.current.loadHistoryMore();
    });
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(2));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await act(() => result.current.filterHistory(null, null));
    await waitFor(() => expect(useWorkflowStore.getState().historyRuns.some((item) => item.taskId === freshRun.taskId)).toBe(true));

    await act(async () => {
      pendingPage.resolve({ runs: [{ ...run, taskId: "superseded-page-run" }], nextCursor: null });
      await loadMorePromise;
    });

    expect(useWorkflowStore.getState().historyRuns.some((item) => item.taskId === "superseded-page-run")).toBe(false);
    expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a");
    expect(useWorkflowStore.getState().operations["history:page"]?.error ?? null).toBeNull();
  });

  it("prepares the selected source versions before cancelling a scope review and links the new attempt", async () => {
    const waiting: WorkflowRun = { ...run, kind: "update_wiki", revision: "2", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "source-a", versionId: "old" }] },
      pendingAction: { id: "review-scope", actionType: "review_scope", riskLevel: "high", affectedPaths: ["raw/sources/source-a.md"], candidate: null, expiresAt: null, checkpointHash: null } };
    const fresh: WorkflowPreparation = { ...preparation, kind: "update_wiki", route: waiting.route,
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "source-a", versionId: "new" }] },
      availableSourceVersions: [{ sourceId: "source-a", versionId: "new" }, { sourceId: "source-b", versionId: "unselected" }] };
    mocks.prepare.mockResolvedValue(fresh);
    mocks.cancel.mockResolvedValue({ ...waiting, revision: "3", displayStatus: "cancelled", pendingAction: null });
    mocks.start.mockResolvedValue({ kind: "created", run: { ...waiting, taskId: "new-attempt", revision: "1", displayStatus: "running", pendingAction: null,
      scope: fresh.scope, retry: { attemptOf: waiting.taskId, attemptNumber: 2 } } });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    await act(() => result.current.adjustAndPrepare(waiting));
    expect(mocks.prepare).toHaveBeenNthCalledWith(2, expect.objectContaining({ scope: fresh.scope }));
    expect(mocks.cancel).toHaveBeenCalledWith(expect.objectContaining({ taskId: waiting.taskId }));
    expect(mocks.prepare.mock.invocationCallOrder[1]).toBeLessThan(mocks.cancel.mock.invocationCallOrder[0]);
    expect(mocks.start).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState()).toMatchObject({ preparation: fresh, retryOfTaskId: waiting.taskId, surface: "preparation" });
    await act(() => result.current.startPrepared(false, false));
    expect(mocks.start).toHaveBeenCalledWith(expect.objectContaining({ retryOfTaskId: waiting.taskId }));
    expect(useWorkflowStore.getState().selectedTaskId).toBe("new-attempt");
  });

  it("leaves the old scope-review task intact when preparation fails before cancellation", async () => {
    const waiting: WorkflowRun = { ...run, kind: "update_wiki", revision: "2", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "source-a", versionId: "old" }] },
      pendingAction: { id: "review-scope", actionType: "review_scope", riskLevel: "high", affectedPaths: [], candidate: null, expiresAt: null, checkpointHash: null } };
    mocks.prepare.mockRejectedValueOnce(new Error("source currently unavailable"));
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => { useWorkflowStore.getState().upsertRun(waiting); useWorkflowStore.getState().selectRun(waiting.taskId); });
    await act(() => result.current.adjustAndPrepare(waiting));
    expect(mocks.cancel).not.toHaveBeenCalled();
    expect(mocks.start).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState().runs[0]).toEqual(waiting);
    expect(useWorkflowStore.getState().operations[`task:${waiting.taskId}:review-scope`]?.error?.technicalDetails).toContain("source currently unavailable");
  });

  it("does not cancel or open an old scope-review task after the current project changes", async () => {
    const pending = deferred<WorkflowPreparation>();
    const waiting: WorkflowRun = { ...run, kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "source-a", versionId: "old" }] },
      pendingAction: { id: "review-scope", actionType: "review_scope", riskLevel: "high", affectedPaths: [], candidate: null, expiresAt: null, checkpointHash: null } };
    mocks.prepare.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    let request!: Promise<void>;
    act(() => { request = result.current.adjustAndPrepare(waiting); });
    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce());
    await act(async () => {
      useProjectStore.setState({ currentProject: { ...project, projectId: "project-b", rootPath: "D:/b" } });
      pending.resolve(preparation);
      await request;
    });
    expect(mocks.cancel).not.toHaveBeenCalled();
    expect(mocks.prepare).toHaveBeenCalledTimes(1);
    expect(useWorkflowStore.getState().preparation).toBeNull();
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
  });

  it.each(["project", "surface", "hidden"])("abandons a start when %s changes before the action chunk resumes", async (change) => {
    const { result, rerender } = renderHook(({ enabled }) => useWorkflowsController(project, enabled), { initialProps: { enabled: true } });
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setPreparation(preparation));
    let starting!: Promise<void>;
    act(() => {
      starting = result.current.startPrepared(false, false);
      if (change === "project") useProjectStore.setState({ currentProject: { ...project, projectId: "project-b", rootPath: "D:/b" } });
      if (change === "surface") result.current.backToOverview();
    });
    if (change === "hidden") rerender({ enabled: false });
    await act(() => starting);
    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.start).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState().operations[`start:${preparation.preparationId}`]?.pending).toBe(false);
  });

  it("takes the start lock before loading the action implementation", async () => {
    const pending = deferred<WorkflowPreparation>();
    mocks.prepare.mockReturnValue(pending.promise);
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => useWorkflowStore.getState().setPreparation(preparation));
    let first!: Promise<void>;
    let duplicate!: Promise<void>;
    act(() => { first = result.current.startPrepared(false, false); duplicate = result.current.startPrepared(false, false); });
    expect(useWorkflowStore.getState().operations[`start:${preparation.preparationId}`]?.pending).toBe(true);
    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce());
    await act(async () => { pending.resolve(preparation); await Promise.all([first, duplicate]); });
    expect(mocks.start).toHaveBeenCalledOnce();
  });

  it("takes the scope-review lock before loading and stops on an immediate project change", async () => {
    const waiting: WorkflowRun = { ...run, kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "source-a", versionId: "old" }] },
      pendingAction: { id: "review-scope", actionType: "review_scope", riskLevel: "high", affectedPaths: [], candidate: null, expiresAt: null, checkpointHash: null } };
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    let first!: Promise<void>;
    let duplicate!: Promise<void>;
    act(() => {
      first = result.current.adjustAndPrepare(waiting);
      duplicate = result.current.adjustAndPrepare(waiting);
      expect(useWorkflowStore.getState().operations[`task:${waiting.taskId}:review-scope`]?.pending).toBe(true);
      useProjectStore.setState({ currentProject: { ...project, projectId: "project-b", rootPath: "D:/b" } });
    });
    await act(() => Promise.all([first, duplicate]));
    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.cancel).not.toHaveBeenCalled();
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

    await act(async () => { await result.current.handlePrerequisite("prepare_again"); });

    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledWith({
      projectId: project.projectId,
      projectRootPath: project.rootPath,
      kind: preparation.kind,
      scope: preparation.scope,
      routeSelection: { kind: "byok", provider: "ollama" },
    }));
  });

  it("carries an edited preparation draft through Settings without starting", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const draft = {
      scope: {
        kind: "generate_content" as const,
        artifactType: "knowledge_card" as const,
        pagePaths: ["wiki/a.md", "wiki/b.md"],
        outputPath: "exports/draft.html",
      },
      routeSelection: { kind: "byok" as const, provider: "open_ai" as const },
    };
    useWorkflowStore.setState({
      preparation: { ...preparation, kind: "generate_content", scope: draft.scope },
      surface: "preparation",
    });

    await act(async () => { await result.current.handlePrerequisite("configure_execution_route", draft); });

    expect(useNavigationStore.getState()).toMatchObject({
      settingsOpen: true,
      settingsSection: "ai",
      workflowSettingsReturnIntent: {
        scope: draft.scope,
        routeSelection: draft.routeSelection,
        source: "prerequisite",
        expectedPreparationId: preparation.preparationId,
      },
    });
    expect(mocks.start).not.toHaveBeenCalled();
  });

  it("preserves an explicit automatic route draft across Settings and Import", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const draft = { scope: preparation.scope, routeSelection: null };
    useWorkflowStore.setState({ preparation, surface: "preparation" });

    await act(async () => { await result.current.handlePrerequisite("configure_execution_route", draft); });
    expect(useNavigationStore.getState().workflowSettingsReturnIntent?.routeSelection).toBeNull();

    await act(async () => { await result.current.handlePrerequisite("import_sources", draft); });
    expect(useNavigationStore.getState().workflowLaunchIntent).toMatchObject({
      routeSelection: null,
      expectedCanonicalIdentityKey: preparation.projectAccess.canonicalIdentityKey,
      expectedIdentityRevision: preparation.projectAccess.identityRevision,
    });
  });

  it("defers an edited import prerequisite draft for identity-guarded return", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const draft = {
      scope: { kind: "update_wiki" as const, mode: "changed_sources" as const, sourceVersions: [] },
      routeSelection: { kind: "byok" as const, provider: "open_ai" as const },
    };
    useWorkflowStore.setState({
      preparation: { ...preparation, kind: "update_wiki", scope: draft.scope },
      surface: "preparation",
    });

    await act(async () => { await result.current.handlePrerequisite("import_sources", draft); });

    expect(useNavigationStore.getState()).toMatchObject({
      activeView: "import",
      workflowLaunchIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "update_wiki",
        origin: "workflows",
        scopePreset: draft.scope,
        routeSelection: draft.routeSelection,
        expectedCanonicalIdentityKey: preparation.projectAccess.canonicalIdentityKey,
        expectedIdentityRevision: preparation.projectAccess.identityRevision,
      },
    });
    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.start).not.toHaveBeenCalled();
  });

  it("delegates project authority prerequisites without pretending to grant access", async () => {
    const onProjectPrerequisite = vi.fn();
    const { result } = renderHook(() =>
      useWorkflowsController(project, true, { onProjectPrerequisite }),
    );
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({ preparation, surface: "preparation" });

    await act(async () => { await result.current.handlePrerequisite("trust_project"); });

    expect(onProjectPrerequisite).toHaveBeenCalledWith(
      "trust_project",
      expect.objectContaining({ project, preparation, prepareAgain: expect.any(Function) }),
    );
    expect(mocks.prepare).not.toHaveBeenCalled();
  });

  it("scopes project prerequisite pending and errors to the captured identity", async () => {
    const prerequisite = deferred<void>();
    const onProjectPrerequisite = vi.fn().mockReturnValue(prerequisite.promise);
    const { result } = renderHook(() =>
      useWorkflowsController(project, true, { onProjectPrerequisite }),
    );
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));

    act(() => {
      result.current.handlePrerequisite("trust_project");
      result.current.handlePrerequisite("trust_project");
    });
    await waitFor(() => expect(onProjectPrerequisite).toHaveBeenCalledTimes(1));
    expect(useWorkflowStore.getState().operations["prerequisite:project:trust_project"]?.pending).toBe(true);

    act(() => useProjectStore.setState({
      authority: {
        ...useProjectStore.getState().authority!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    }));
    await act(async () => prerequisite.reject(new Error("old prerequisite failed")));

    expect(useWorkflowStore.getState().operations["prerequisite:project:trust_project"]).toBeUndefined();
  });

  it("reports an honest project-flow recovery when no authority handler is connected", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    useWorkflowStore.setState({ preparation, surface: "preparation" });

    await act(async () => { await result.current.handlePrerequisite("configure_git"); });

    expect(useWorkflowStore.getState().operations["prerequisite:project:configure_git"]?.error).toBeTruthy();
    expect(mocks.prepare).not.toHaveBeenCalled();
  });
});

import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent } from "../../types/task";
import type { WorkflowPreparation, WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { makeWorkflowEventBurst } from "./workflowBaselineFixtures";

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
const noProjectOverview: WorkflowsOverview = {
  schemaVersion: 1,
  projectAccess: null,
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

  it("does not attach the workflow event controller outside the Workflows route", async () => {
    const { rerender } = renderHook(
      ({ enabled }) => useWorkflowsController(project, enabled),
      { initialProps: { enabled: false } },
    );

    expect(mocks.listener).toBeNull();
    expect(mocks.getOverview).not.toHaveBeenCalled();

    rerender({ enabled: true });
    await waitFor(() => expect(mocks.listener).not.toBeNull());
    rerender({ enabled: false });
    expect(mocks.listener).toBeNull();
  });

  it("accepts only workflow events matching the active canonical identity revision", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => mocks.listener?.({ eventId: "1", eventType: "workflow_updated", projectId: "project-a", taskId: "run-a", timestamp: run.updatedAt, payload: { ...run, identityRevision: "stale" } }));
    expect(useWorkflowStore.getState().runs).toEqual([]);
    act(() => mocks.listener?.({ eventId: "2", eventType: "workflow_updated", projectId: "project-a", taskId: "run-a", timestamp: run.updatedAt, payload: run }));
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.taskId).toBe("run-a"));
  });

  it("counts overview, history, detail, prepare, and start calls independently", async () => {
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));

    await act(() => result.current.openRun(run.taskId));
    await act(() => result.current.prepare("health_check"));
    await act(() => result.current.startPrepared(false, false));

    expect({
      overview: mocks.getOverview.mock.calls.length,
      history: mocks.listRuns.mock.calls.length,
      detail: mocks.getRun.mock.calls.length,
      prepare: mocks.prepare.mock.calls.length,
      start: mocks.start.mock.calls.length,
    }).toEqual({ overview: 2, history: 1, detail: 1, prepare: 1, start: 1 });
  });

  it("coalesces a 200-event burst without refreshing history and preserves the terminal event", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));

    const events = makeWorkflowEventBurst(200, {
      taskId: run.taskId,
      projectId: run.projectId,
      canonicalIdentityKey: run.canonicalIdentityKey,
      identityRevision: run.identityRevision,
    });
    const terminalRun = events.at(-1)!.payload;
    const refreshGate = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReturnValue(refreshGate.promise);

    act(() => {
      for (const event of events) {
        mocks.listener?.(event);
      }
    });

    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
    expect(mocks.listRuns).toHaveBeenCalledTimes(1);
    expect(mocks.getRun).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState().runs[0]).toMatchObject({
      taskId: run.taskId,
      displayStatus: "completed",
      completedAt: terminalRun.completedAt,
    });

    await act(async () => refreshGate.resolve(overview));
    await waitFor(() => expect(useWorkflowStore.getState().operations["overview:reconcile"]?.pending ?? false).toBe(false));
  });

  it("batches ordinary progress near 10Hz without overview or history reconciliation", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    vi.useFakeTimers();
    const overviewCalls = mocks.getOverview.mock.calls.length;
    const historyCalls = mocks.listRuns.mock.calls.length;
    let observedCommits = 0;
    const unsubscribe = useWorkflowStore.subscribe((state, previous) => {
      if (state.runs !== previous.runs) observedCommits += 1;
    });

    const events = makeWorkflowEventBurst(50, {
      taskId: run.taskId,
      projectId: run.projectId,
      canonicalIdentityKey: run.canonicalIdentityKey,
      identityRevision: run.identityRevision,
    }).slice(0, -1);
    act(() => events.forEach((event) => mocks.listener?.(event)));
    expect(useWorkflowStore.getState().runs).toEqual([]);

    act(() => vi.advanceTimersByTime(100));
    expect(useWorkflowStore.getState().runs[0]?.updatedAt).toBe(events.at(-1)?.payload.updatedAt);
    expect(observedCommits).toBe(1);
    expect(mocks.getOverview).toHaveBeenCalledTimes(overviewCalls);
    expect(mocks.listRuns).toHaveBeenCalledTimes(historyCalls);
    unsubscribe();
  });

  it("keeps one overview invoke in flight and schedules at most one trailing reconcile", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const firstReconcile = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReturnValueOnce(firstReconcile.promise).mockResolvedValue(overview);
    const boundary = { ...run, displayStatus: "completed" as const, completedAt: "2026-08-01T00:00:02Z" };

    act(() => {
      for (let index = 0; index < 50; index += 1) {
        mocks.listener?.({
          eventId: `terminal-${index}`,
          eventType: "workflow_updated",
          projectId: run.projectId,
          taskId: run.taskId,
          timestamp: boundary.completedAt,
          payload: { ...boundary, updatedAt: `2026-08-01T00:00:${String(index).padStart(2, "0")}Z` },
        });
      }
    });

    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
    await act(async () => firstReconcile.resolve(overview));
    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(3));
    expect(mocks.listRuns).toHaveBeenCalledTimes(1);
  });

  it("starts a fresh wave when a history boundary arrives during the trailing pass", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const firstPass = deferred<WorkflowsOverview>();
    const trailingPass = deferred<WorkflowsOverview>();
    mocks.getOverview
      .mockReturnValueOnce(firstPass.promise)
      .mockReturnValueOnce(trailingPass.promise)
      .mockResolvedValue(overview);
    const boundary = { ...run, displayStatus: "completed" as const, completedAt: "2026-08-01T00:00:02Z" };

    act(() => mocks.listener?.({
      eventId: "first-boundary",
      eventType: "workflow_updated",
      projectId: run.projectId,
      taskId: run.taskId,
      timestamp: boundary.completedAt,
      payload: boundary,
    }));
    act(() => mocks.listener?.({
      eventId: "coalesced-boundary",
      eventType: "workflow_updated",
      projectId: run.projectId,
      taskId: run.taskId,
      timestamp: boundary.completedAt,
      payload: { ...boundary, updatedAt: "2026-08-01T00:00:03Z" },
    }));
    await act(async () => firstPass.resolve(overview));
    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(3));

    act(() => useWorkflowStore.getState().setSurface("history"));
    act(() => useWorkflowStore.getState().setHistoryFilters("health_check", "failed"));
    act(() => mocks.listener?.({
      eventId: "history-boundary-during-trailing",
      eventType: "workflow_updated",
      projectId: run.projectId,
      taskId: run.taskId,
      timestamp: boundary.completedAt,
      payload: { ...boundary, updatedAt: "2026-08-01T00:00:04Z" },
    }));
    await act(async () => trailingPass.resolve(overview));

    await waitFor(() => expect(mocks.getOverview).toHaveBeenCalledTimes(4));
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(2));
    expect(mocks.listRuns).toHaveBeenLastCalledWith(expect.objectContaining({
      workflowKind: "health_check",
      displayStatus: "failed",
      cursor: null,
      limit: 100,
    }));
  });

  it("keeps the measured workflow store slice within 25 commits for 200 events over two seconds", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    vi.useFakeTimers();
    const events = makeWorkflowEventBurst(200, {
      taskId: run.taskId,
      projectId: run.projectId,
      canonicalIdentityKey: run.canonicalIdentityKey,
      identityRevision: run.identityRevision,
    });
    let commits = 0;
    const emittedAt = new Map<string, number>();
    const progressLatencies: number[] = [];
    let terminalLatency = Number.POSITIVE_INFINITY;
    const unsubscribe = useWorkflowStore.subscribe((state, previous) => {
      if (
        state.runs !== previous.runs
        || state.overview !== previous.overview
        || state.historyCursor !== previous.historyCursor
      ) commits += 1;
      const visible = state.runs[0];
      if (visible && visible !== previous.runs[0]) {
        const started = emittedAt.get(visible.updatedAt);
        if (started !== undefined) {
          const latency = Date.now() - started;
          if (visible.displayStatus === "running") progressLatencies.push(latency);
          else terminalLatency = latency;
        }
      }
    });

    await act(async () => {
      for (const event of events) {
        emittedAt.set(event.payload.updatedAt, Date.now());
        mocks.listener?.(event);
        vi.advanceTimersByTime(10);
      }
      await Promise.resolve();
    });

    expect(commits).toBeLessThanOrEqual(25);
    const sortedLatencies = [...progressLatencies].sort((a, b) => a - b);
    const p95Index = Math.max(0, Math.ceil(sortedLatencies.length * 0.95) - 1);
    expect(sortedLatencies[p95Index]).toBeLessThanOrEqual(150);
    expect(terminalLatency).toBeLessThanOrEqual(250);
    expect(useWorkflowStore.getState().runs[0]?.displayStatus).toBe("completed");
    expect(mocks.listRuns).toHaveBeenCalledTimes(1);
    unsubscribe();
  });

  it("keeps real event-to-store visibility within the local latency budget", async () => {
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    const ordinary = makeWorkflowEventBurst(50, {
      taskId: run.taskId,
      projectId: run.projectId,
      canonicalIdentityKey: run.canonicalIdentityKey,
      identityRevision: run.identityRevision,
    }).slice(0, -1);
    const progressStarted = performance.now();
    let resolveProgress!: (latency: number) => void;
    const progressVisible = new Promise<number>((resolve) => { resolveProgress = resolve; });
    const unsubscribeProgress = useWorkflowStore.subscribe((state) => {
      if (state.runs[0]?.updatedAt === ordinary.at(-1)?.payload.updatedAt) {
        resolveProgress(performance.now() - progressStarted);
      }
    });
    act(() => ordinary.forEach((event) => mocks.listener?.(event)));
    const progressLatency = await progressVisible;
    unsubscribeProgress();

    const terminalUpdatedAt = "2026-08-10T00:10:00.000Z";
    const terminal = {
      ...ordinary.at(-1)!,
      eventId: "real-latency-terminal",
      timestamp: terminalUpdatedAt,
      payload: {
        ...ordinary.at(-1)!.payload,
        displayStatus: "completed" as const,
        updatedAt: terminalUpdatedAt,
        completedAt: terminalUpdatedAt,
      },
    };
    const terminalStarted = performance.now();
    let resolveTerminal!: (latency: number) => void;
    const terminalVisible = new Promise<number>((resolve) => { resolveTerminal = resolve; });
    const unsubscribeTerminal = useWorkflowStore.subscribe((state) => {
      if (state.runs[0]?.displayStatus === "completed") {
        resolveTerminal(performance.now() - terminalStarted);
      }
    });
    act(() => mocks.listener?.(terminal));
    const terminalLatency = await terminalVisible;
    unsubscribeTerminal();

    expect(progressLatency).toBeLessThanOrEqual(150);
    expect(terminalLatency).toBeLessThanOrEqual(250);
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

  it("hydrates a selected waiting run exactly once during a rapid event burst", async () => {
    const waiting = {
      ...run,
      displayStatus: "waiting_for_confirmation" as const,
      pendingAction: {
        id: "action-a",
        actionType: "batch_rewrite" as const,
        riskLevel: "high" as const,
        affectedPaths: ["wiki/a.md"],
        candidate: null,
        expiresAt: null,
        checkpointHash: "checkpoint-a",
      },
      updatedAt: "2026-08-01T01:00:00Z",
    };
    const hydrated = {
      ...waiting,
      decisionReview: {
        reason: "review",
        counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 },
        userEditsDetected: false,
        fileDiffs: [{ path: "wiki/a.md", diff: "+review" }],
      },
    };
    const detail = deferred<WorkflowRun>();
    mocks.getRun.mockReset().mockReturnValue(detail.promise);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      for (let index = 0; index < 10; index += 1) {
        mocks.listener?.({
          eventId: `waiting-${index}`,
          eventType: "workflow_updated",
          projectId: project.projectId,
          taskId: waiting.taskId,
          timestamp: waiting.updatedAt,
          payload: waiting,
        });
      }
    });

    await waitFor(() => expect(mocks.getRun).toHaveBeenCalledTimes(1));
    act(() => mocks.listener?.({
      eventId: "waiting-newer",
      eventType: "workflow_updated",
      projectId: project.projectId,
      taskId: waiting.taskId,
      timestamp: "2026-08-01T02:00:00Z",
      payload: { ...waiting, updatedAt: "2026-08-01T02:00:00Z" },
    }));
    await act(async () => detail.resolve(hydrated));
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]).toMatchObject({
      updatedAt: "2026-08-01T02:00:00Z",
      decisionReview: hydrated.decisionReview,
    }));
  });

  it("does not hydrate a non-selected waiting run", async () => {
    const waiting = {
      ...run,
      taskId: "waiting-background",
      displayStatus: "waiting_for_confirmation" as const,
      pendingAction: {
        id: "action-background",
        actionType: "batch_rewrite" as const,
        riskLevel: "high" as const,
        affectedPaths: ["wiki/a.md"],
        candidate: null,
        expiresAt: null,
        checkpointHash: null,
      },
      updatedAt: "2026-08-01T01:00:00Z",
    };
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));

    act(() => mocks.listener?.({
      eventId: "waiting-background",
      eventType: "workflow_updated",
      projectId: project.projectId,
      taskId: waiting.taskId,
      timestamp: waiting.updatedAt,
      payload: waiting,
    }));

    expect(mocks.getRun).not.toHaveBeenCalled();
  });

  it("drops waiting hydration after identity replacement or pending-action removal", async () => {
    const waiting = {
      ...run,
      displayStatus: "waiting_for_confirmation" as const,
      pendingAction: {
        id: "action-a",
        actionType: "batch_rewrite" as const,
        riskLevel: "high" as const,
        affectedPaths: ["wiki/a.md"],
        candidate: null,
        expiresAt: null,
        checkpointHash: null,
      },
      updatedAt: "2026-08-01T01:00:00Z",
    };
    const hydrated = {
      ...waiting,
      decisionReview: {
        reason: "stale review",
        counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 },
        userEditsDetected: false,
        fileDiffs: [{ path: "wiki/a.md", diff: "+stale" }],
      },
    };
    const detail = deferred<WorkflowRun>();
    mocks.getRun.mockReset().mockReturnValue(detail.promise);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      mocks.listener?.({
        eventId: "waiting-a",
        eventType: "workflow_updated",
        projectId: project.projectId,
        taskId: waiting.taskId,
        timestamp: waiting.updatedAt,
        payload: waiting,
      });
    });
    await waitFor(() => expect(mocks.getRun).toHaveBeenCalledOnce());

    act(() => {
      useWorkflowStore.getState().setOverviewSnapshot({
        ...overview,
        projectAccess: { ...overview.projectAccess!, identityRevision: "revision-b" },
      });
      useWorkflowStore.getState().upsertRun({
        ...waiting,
        identityRevision: "revision-b",
        displayStatus: "cancelled",
        pendingAction: null,
        updatedAt: "2026-08-01T02:00:00Z",
      });
    });
    await act(async () => detail.resolve(hydrated));

    expect(useWorkflowStore.getState().runs[0]?.decisionReview).toBeUndefined();
    expect(useWorkflowStore.getState().runs[0]?.pendingAction).toBeNull();
  });

  it("does not let an old action review overwrite a newly hydrated action", async () => {
    const action = (id: string) => ({
      id,
      actionType: "batch_rewrite" as const,
      riskLevel: "high" as const,
      affectedPaths: [`wiki/${id}.md`],
      candidate: null,
      expiresAt: null,
      checkpointHash: null,
    });
    const waitingA = {
      ...run,
      displayStatus: "waiting_for_confirmation" as const,
      pendingAction: action("action-a"),
      updatedAt: "2026-08-01T01:00:00Z",
    };
    const waitingB = {
      ...waitingA,
      pendingAction: action("action-b"),
      updatedAt: "2026-08-01T02:00:00Z",
    };
    const reviewA = {
      reason: "review a",
      counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 },
      userEditsDetected: false,
      fileDiffs: [{ path: "wiki/action-a.md", diff: "+a" }],
    };
    const reviewB = {
      ...reviewA,
      reason: "review b",
      fileDiffs: [{ path: "wiki/action-b.md", diff: "+b" }],
    };
    const detailA = deferred<WorkflowRun>();
    mocks.getRun
      .mockReset()
      .mockReturnValueOnce(detailA.promise)
      .mockResolvedValueOnce({ ...waitingB, decisionReview: reviewB });
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      mocks.listener?.({ eventId: "action-a", eventType: "workflow_updated", projectId: project.projectId, taskId: run.taskId, timestamp: waitingA.updatedAt, payload: waitingA });
      mocks.listener?.({ eventId: "action-b", eventType: "workflow_updated", projectId: project.projectId, taskId: run.taskId, timestamp: waitingB.updatedAt, payload: waitingB });
    });

    await waitFor(() => expect(mocks.getRun).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.decisionReview).toEqual(reviewB));
    await act(async () => detailA.resolve({ ...waitingA, decisionReview: reviewA }));

    expect(useWorkflowStore.getState().runs[0]?.pendingAction?.id).toBe("action-b");
    expect(useWorkflowStore.getState().runs[0]?.decisionReview).toEqual(reviewB);
  });

  it("does not expose a hydration error after the pending action changes", async () => {
    const action = (id: string) => ({
      id,
      actionType: "batch_rewrite" as const,
      riskLevel: "high" as const,
      affectedPaths: [`wiki/${id}.md`],
      candidate: null,
      expiresAt: null,
      checkpointHash: null,
    });
    const detail = deferred<WorkflowRun>();
    const waitingA = { ...run, displayStatus: "waiting_for_confirmation" as const, pendingAction: action("action-a") };
    const waitingB = { ...waitingA, pendingAction: action("action-b"), updatedAt: "2026-08-01T02:00:00Z" };
    mocks.getRun.mockReset().mockReturnValueOnce(detail.promise).mockResolvedValue(waitingB);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      mocks.listener?.({ eventId: "action-a", eventType: "workflow_updated", projectId: project.projectId, taskId: run.taskId, timestamp: waitingA.updatedAt, payload: waitingA });
    });
    await waitFor(() => expect(mocks.getRun).toHaveBeenCalledOnce());
    act(() => mocks.listener?.({ eventId: "action-b", eventType: "workflow_updated", projectId: project.projectId, taskId: run.taskId, timestamp: waitingB.updatedAt, payload: waitingB }));
    await act(async () => detail.reject(new Error("old action failed")));

    expect(useWorkflowStore.getState().operations["task:run-a:hydrate:action-a"]?.error ?? null).toBeNull();
  });

  it("does not restore a waiting review after discard wins the race", async () => {
    const waiting = {
      ...run,
      displayStatus: "waiting_for_confirmation" as const,
      pendingAction: {
        id: "action-a",
        actionType: "batch_rewrite" as const,
        riskLevel: "high" as const,
        affectedPaths: ["wiki/a.md"],
        candidate: null,
        expiresAt: null,
        checkpointHash: null,
      },
      updatedAt: "2026-08-01T01:00:00Z",
    };
    const detail = deferred<WorkflowRun>();
    mocks.getRun.mockReset().mockReturnValue(detail.promise);
    mocks.discard.mockResolvedValueOnce({
      ...waiting,
      displayStatus: "cancelled",
      pendingAction: null,
      updatedAt: "2026-08-01T02:00:00Z",
    });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    act(() => {
      useWorkflowStore.getState().upsertRun(run);
      useWorkflowStore.getState().selectRun(run.taskId);
      mocks.listener?.({ eventId: "waiting", eventType: "workflow_updated", projectId: project.projectId, taskId: run.taskId, timestamp: waiting.updatedAt, payload: waiting });
    });
    await waitFor(() => expect(mocks.getRun).toHaveBeenCalledOnce());

    await act(() => result.current.discard(run.taskId));
    await act(async () => detail.resolve({
      ...waiting,
      decisionReview: {
        reason: "discarded review",
        counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 },
        userEditsDetected: false,
        fileDiffs: [{ path: "wiki/a.md", diff: "+discarded" }],
      },
    }));

    expect(useWorkflowStore.getState().runs[0]).toMatchObject({
      displayStatus: "cancelled",
      pendingAction: null,
    });
    expect(useWorkflowStore.getState().runs[0]?.decisionReview).toBeUndefined();
  });

  it("tracks reconcile pending independently from the initial overview operation", async () => {
    const reconcile = deferred<WorkflowsOverview>();
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().overview).toEqual(overview));
    mocks.getOverview.mockReturnValueOnce(reconcile.promise);

    act(() => mocks.listener?.({
      eventId: "reconcile",
      eventType: "workflow_updated",
      projectId: project.projectId,
      taskId: run.taskId,
      timestamp: run.updatedAt,
      payload: { ...run, displayStatus: "completed", completedAt: run.updatedAt },
    }));

    expect(useWorkflowStore.getState().operations["overview:init"]?.pending ?? false).toBe(false);
    expect(useWorkflowStore.getState().operations["overview:reconcile"]?.pending).toBe(true);
    await act(async () => reconcile.resolve(overview));
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

  it("does not commit a partial overview snapshot when run history fails", async () => {
    mocks.listRuns.mockRejectedValueOnce(new Error("run history unavailable"));

    renderHook(() => useWorkflowsController(project, true));

    await waitFor(() => expect(useWorkflowStore.getState()).toMatchObject({
      overview: null,
      overviewStatus: "error",
      runs: [],
    }));
    await waitFor(() => expect(useWorkflowStore.getState().operations["overview:init"]?.error?.technicalDetails).toContain("run history unavailable"));
  });

  it("rejects history from a different canonical identity in the same refresh", async () => {
    mocks.listRuns.mockResolvedValueOnce({
      runs: [{ ...run, canonicalIdentityKey: "stale-identity", identityRevision: "stale-revision" }],
      nextCursor: "stale-cursor",
    });

    renderHook(() => useWorkflowsController(project, true));

    await waitFor(() => expect(useWorkflowStore.getState()).toMatchObject({
      overview: null,
      overviewStatus: "error",
      runs: [],
      historyCursor: null,
    }));
    await waitFor(() => expect(useWorkflowStore.getState().operations["overview:init"]?.error).toBeTruthy());
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
    expect(mocks.getOverview).toHaveBeenCalledTimes(2);
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

  it("bounds unique events buffered before project access is known", async () => {
    const pendingOverview = deferred<WorkflowsOverview>();
    mocks.getOverview.mockReset().mockReturnValueOnce(pendingOverview.promise);
    renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(mocks.listener).not.toBeNull());

    act(() => {
      for (let index = 0; index < 300; index += 1) {
        const buffered = {
          ...run,
          taskId: `buffered-${index}`,
          updatedAt: new Date(Date.parse(run.updatedAt) + index * 1000).toISOString(),
        };
        mocks.listener?.({
          eventId: `buffered-${index}`,
          eventType: "workflow_updated",
          projectId: project.projectId,
          taskId: buffered.taskId,
          timestamp: buffered.updatedAt,
          payload: buffered,
        });
      }
    });
    await act(async () => pendingOverview.resolve(overview));

    await waitFor(() => expect(useWorkflowStore.getState().runs).toHaveLength(256));
    expect(useWorkflowStore.getState().runs.some((item) => item.taskId === "buffered-299")).toBe(true);
    expect(useWorkflowStore.getState().runs.some((item) => item.taskId === "buffered-0")).toBe(false);
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

    useProjectStore.setState({ currentProject: projectB });
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

    useProjectStore.setState({
      currentProject: projectB,
      authority: {
        ...useProjectStore.getState().authority!,
        projectId: projectB.projectId,
        canonicalRootPath: projectB.rootPath,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    });
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
    useProjectStore.setState({ authority: null });
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

    let refreshPromise!: Promise<void>;
    act(() => {
      refreshPromise = result.current.refresh();
    });
    expect(mocks.getOverview).toHaveBeenCalledTimes(1);
    await act(async () => oldOverview.resolve(overview));
    await act(async () => refreshPromise);
    await waitFor(() => expect(useWorkflowStore.getState().runs[0]?.taskId).toBe("new-run"));

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

  it("rejects a paginated history page from a different canonical identity", async () => {
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockResolvedValueOnce({
        runs: [{ ...run, taskId: "foreign-run", canonicalIdentityKey: "identity-b", identityRevision: "revision-b" }],
        nextCursor: null,
      });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    await act(() => result.current.loadHistoryMore());

    expect(useWorkflowStore.getState().runs.map((item) => item.taskId)).toEqual([run.taskId]);
    expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a");
    expect(useWorkflowStore.getState().operations["history:page"]?.error).toBeTruthy();
  });

  it("does not let a superseded history page overwrite a full refresh with the same cursor", async () => {
    const pendingPage = deferred<{ runs: WorkflowRun[]; nextCursor: string | null }>();
    const freshRun = { ...run, taskId: "fresh-run", updatedAt: "2026-08-01T02:00:00Z" };
    mocks.listRuns
      .mockResolvedValueOnce({ runs: [run], nextCursor: "cursor-a" })
      .mockReturnValueOnce(pendingPage.promise)
      .mockResolvedValueOnce({ runs: [freshRun], nextCursor: "cursor-a" });
    const { result } = renderHook(() => useWorkflowsController(project, true));
    await waitFor(() => expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a"));

    let loadMorePromise!: Promise<void>;
    act(() => {
      loadMorePromise = result.current.loadHistoryMore();
    });
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(2));
    act(() => useWorkflowStore.getState().setSurface("history"));
    await act(() => result.current.refresh());
    await waitFor(() => expect(useWorkflowStore.getState().runs.some((item) => item.taskId === freshRun.taskId)).toBe(true));

    await act(async () => {
      pendingPage.resolve({ runs: [{ ...run, taskId: "superseded-page-run" }], nextCursor: null });
      await loadMorePromise;
    });

    expect(useWorkflowStore.getState().runs.some((item) => item.taskId === "superseded-page-run")).toBe(false);
    expect(useWorkflowStore.getState().historyCursor).toBe("cursor-a");
    expect(useWorkflowStore.getState().operations["history:page"]?.error ?? null).toBeNull();
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
    expect(onProjectPrerequisite).toHaveBeenCalledTimes(1);
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

    act(() => result.current.handlePrerequisite("configure_git"));

    expect(useWorkflowStore.getState().operations["prerequisite:project:configure_git"]?.error).toBeTruthy();
    expect(mocks.prepare).not.toHaveBeenCalled();
  });
});

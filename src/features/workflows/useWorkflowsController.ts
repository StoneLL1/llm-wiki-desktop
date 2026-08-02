import { useCallback, useEffect, useMemo, useRef } from "react";

import { registerTaskEventListener } from "../../hooks/useTaskEvents";
import {
  cancelWorkflowRun,
  confirmWorkflowAction,
  continueQueuedWorkflows,
  discardWorkflowResult,
  getWorkflowsOverview,
  listWorkflowRuns,
  prepareWorkflow,
  reorderQueuedWorkflow,
  retryWorkflow,
  startWorkflow,
  undoCancelQueuedWorkflow,
} from "../../services/workflowApi";
import { useWorkflowStore } from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import {
  hydrateAndSelectWorkflowRun,
  openWorkflowResult,
} from "../../services/workflowNavigation";
import type { ProjectSummary } from "../../types/project";
import type {
  WorkflowKind,
  WorkflowProjectAccessSummary,
  WorkflowPrerequisiteAction,
  WorkflowRouteSelection,
  WorkflowRun,
  WorkflowScope,
  WorkflowStartOutcome,
} from "../../types/workflow";

interface PendingWorkflowEvent {
  eventProjectId: string | null;
  run: WorkflowRun;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function messageOf(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function workflowEventMatchesAccess(
  event: PendingWorkflowEvent,
  projectId: string,
  access: WorkflowProjectAccessSummary,
): boolean {
  return event.eventProjectId === projectId
    && event.run.projectId === projectId
    && event.run.canonicalIdentityKey === access.canonicalIdentityKey
    && event.run.identityRevision === access.identityRevision;
}

function keepLatestPendingEvent(
  pending: Map<string, PendingWorkflowEvent>,
  event: PendingWorkflowEvent,
): void {
  const previous = pending.get(event.run.taskId);
  if (
    previous
    && Date.parse(previous.run.updatedAt) > Date.parse(event.run.updatedAt)
  ) {
    return;
  }
  pending.set(event.run.taskId, event);
}

export interface WorkflowsController {
  refresh: () => Promise<void>;
  prepare: (kind: WorkflowKind, scope?: WorkflowScope | null, routeSelection?: WorkflowRouteSelection | null) => Promise<void>;
  startPrepared: (acknowledgeRestrictedContent: boolean, acknowledgeRemoteProvider: boolean) => Promise<void>;
  cancel: (taskId: string) => Promise<void>;
  undoCancel: (taskId: string) => Promise<void>;
  reorder: (taskId: string, beforeTaskId: string | null) => Promise<void>;
  retry: (taskId: string) => Promise<void>;
  adjustAndPrepare: (run: WorkflowRun, openSettingsAfter?: boolean) => Promise<void>;
  openRun: (taskId: string) => Promise<void>;
  openResult: (run: WorkflowRun) => Promise<void>;
  confirm: (taskId: string, actionId: string) => Promise<void>;
  discard: (taskId: string) => Promise<void>;
  continueQueue: () => Promise<void>;
  loadHistoryMore: () => Promise<void>;
  handlePrerequisite: (action: WorkflowPrerequisiteAction) => void;
  backToOverview: () => void;
}

export function useWorkflowsController(
  project: ProjectSummary,
  enabled: boolean,
): WorkflowsController {
  const projectKey = `${project.projectId}\0${project.rootPath}`;
  const activeKeyRef = useRef(projectKey);
  const refreshRequestRef = useRef(0);
  const refreshInFlightRef = useRef<Map<string, number>>(new Map());
  const pendingEventsRef = useRef<Map<string, PendingWorkflowEvent>>(new Map());
  const pendingRefreshVersionRef = useRef<Map<string, number>>(new Map());
  const attemptedPendingRefreshVersionRef = useRef<Map<string, number>>(new Map());
  const prepareRequestRef = useRef(0);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openSettings = useNavigationStore((state) => state.openSettings);

  const request = useCallback(
    () => ({ projectId: project.projectId, projectRootPath: project.rootPath }),
    [project.projectId, project.rootPath],
  );

  const commitOutcome = useCallback((outcome: WorkflowStartOutcome) => {
    useWorkflowStore.getState().upsertRun(outcome.run);
    useWorkflowStore.getState().selectRun(outcome.run.taskId);
  }, []);

  const refresh = useCallback(async () => {
    if (!enabled || !project.projectId || !project.rootPath || !hasTauri()) return;
    const state = useWorkflowStore.getState();
    const refreshRequest = ++refreshRequestRef.current;
    refreshInFlightRef.current.set(
      projectKey,
      (refreshInFlightRef.current.get(projectKey) ?? 0) + 1,
    );
    const epoch = state.requestEpoch;
    const expectedKey = projectKey;
    state.setLoading(true);
    state.setError(null);
    try {
      const [overview, page] = await Promise.all([
        getWorkflowsOverview(request()),
        listWorkflowRuns({
          ...request(),
          workflowKind: null,
          displayStatus: null,
          cursor: null,
          limit: 100,
        }),
      ]);
      const latest = useWorkflowStore.getState();
      if (latest.projectKey !== expectedKey || latest.requestEpoch !== epoch || refreshRequestRef.current !== refreshRequest) return;
      latest.setOverview(overview);
      latest.replaceRuns(page.runs);
      if (overview.projectAccess) {
        const pendingEvents = [...pendingEventsRef.current.values()];
        pendingEventsRef.current.clear();
        for (const pendingEvent of pendingEvents) {
          if (workflowEventMatchesAccess(pendingEvent, project.projectId, overview.projectAccess)) {
            latest.upsertRun(pendingEvent.run);
          }
        }
        pendingRefreshVersionRef.current.delete(projectKey);
        attemptedPendingRefreshVersionRef.current.delete(projectKey);
      }
      latest.setHistoryCursor(page.nextCursor);
    } catch (error) {
      const latest = useWorkflowStore.getState();
      if (latest.projectKey === expectedKey && latest.requestEpoch === epoch && refreshRequestRef.current === refreshRequest) {
        latest.setError(messageOf(error));
      }
    } finally {
      const remainingRefreshes = Math.max(
        0,
        (refreshInFlightRef.current.get(projectKey) ?? 1) - 1,
      );
      if (remainingRefreshes === 0) refreshInFlightRef.current.delete(projectKey);
      else refreshInFlightRef.current.set(projectKey, remainingRefreshes);
      const latest = useWorkflowStore.getState();
      if (latest.projectKey === expectedKey && latest.requestEpoch === epoch && refreshRequestRef.current === refreshRequest) {
        latest.setLoading(false);
      }
      if (remainingRefreshes === 0 && activeKeyRef.current === projectKey) {
        const pendingVersion = pendingRefreshVersionRef.current.get(projectKey) ?? 0;
        const attemptedVersion = attemptedPendingRefreshVersionRef.current.get(projectKey) ?? 0;
        if (pendingVersion > attemptedVersion) {
          // A buffered event gets one coalesced follow-up after the current
          // refresh wave. Keeping the version until a successful identity
          // check prevents a failed or superseded request from losing the
          // event, without creating an unbounded retry loop.
          attemptedPendingRefreshVersionRef.current.set(projectKey, pendingVersion);
          void refresh();
        }
      }
    }
  }, [enabled, project.projectId, project.rootPath, projectKey, request]);

  useEffect(() => {
    activeKeyRef.current = projectKey;
    pendingEventsRef.current.clear();
    pendingRefreshVersionRef.current.clear();
    attemptedPendingRefreshVersionRef.current.clear();
    useWorkflowStore.getState().activateProject(projectKey);
  }, [projectKey]);

  useEffect(() => {
    if (enabled) void refresh();
  }, [enabled, refresh]);

  useEffect(
    () =>
      registerTaskEventListener((event) => {
        if (event.eventType !== "workflow_updated" || activeKeyRef.current !== projectKey) return;
        const run = event.payload as WorkflowRun;
        const state = useWorkflowStore.getState();
        const access = state.overview?.projectAccess;
        if (event.projectId !== project.projectId || run.projectId !== project.projectId) {
          return;
        }
        if (!access) {
          keepLatestPendingEvent(pendingEventsRef.current, {
            eventProjectId: event.projectId,
            run,
          });
          pendingRefreshVersionRef.current.set(
            projectKey,
            (pendingRefreshVersionRef.current.get(projectKey) ?? 0) + 1,
          );
          if ((refreshInFlightRef.current.get(projectKey) ?? 0) === 0) void refresh();
          return;
        }
        if (
          run.canonicalIdentityKey !== access.canonicalIdentityKey ||
          run.identityRevision !== access.identityRevision
        ) {
          return;
        }
        state.upsertRun(run);
        void refresh();
      }),
    [project.projectId, projectKey, refresh],
  );

  const perform = useCallback(
    async (operation: () => Promise<WorkflowRun | WorkflowStartOutcome | { runs: WorkflowRun[] }>) => {
      const state = useWorkflowStore.getState();
      const expectedKey = state.projectKey;
      state.setLoading(true);
      state.setError(null);
      try {
        const result = await operation();
        const latest = useWorkflowStore.getState();
        if (latest.projectKey !== expectedKey) return;
        if ("kind" in result && (result.kind === "created" || result.kind === "existing")) {
          commitOutcome(result);
        } else if ("runs" in result) {
          state.replaceRuns(result.runs);
        } else {
          state.upsertRun(result);
          state.selectRun(result.taskId);
        }
        await refresh();
      } catch (error) {
        if (useWorkflowStore.getState().projectKey === expectedKey) state.setError(messageOf(error));
      } finally {
        if (useWorkflowStore.getState().projectKey === expectedKey) state.setLoading(false);
      }
    },
    [commitOutcome, refresh],
  );

  const prepareKind = useCallback(
    async (kind: WorkflowKind, scope: WorkflowScope | null = null, routeSelection: WorkflowRouteSelection | null = null) => {
      const state = useWorkflowStore.getState();
      const prepareRequest = ++prepareRequestRef.current;
      const expectedKey = state.projectKey;
      state.setLoading(true);
      state.setError(null);
      try {
        const preparation = await prepareWorkflow({
          ...request(),
          kind,
          scope,
          routeSelection,
        });
        const latest = useWorkflowStore.getState();
        if (latest.projectKey !== expectedKey || prepareRequestRef.current !== prepareRequest) return;
        latest.setPreparation(preparation);
        latest.setSurface("preparation");
      } catch (error) {
        const latest = useWorkflowStore.getState();
        if (latest.projectKey === expectedKey && prepareRequestRef.current === prepareRequest) latest.setError(messageOf(error));
      } finally {
        const latest = useWorkflowStore.getState();
        if (latest.projectKey === expectedKey && prepareRequestRef.current === prepareRequest) latest.setLoading(false);
      }
    },
    [request],
  );

  const loadHistoryMore = useCallback(async () => {
    const state = useWorkflowStore.getState();
    const cursor = state.historyCursor;
    if (!enabled || !cursor || !hasTauri()) return;
    const expectedKey = state.projectKey;
    state.setLoading(true);
    state.setError(null);
    try {
      const page = await listWorkflowRuns({
        ...request(),
        workflowKind: null,
        displayStatus: null,
        cursor,
        limit: 100,
      });
      const latest = useWorkflowStore.getState();
      if (latest.projectKey !== expectedKey || latest.historyCursor !== cursor) return;
      latest.replaceRuns(page.runs);
      latest.setHistoryCursor(page.nextCursor);
    } catch (error) {
      const latest = useWorkflowStore.getState();
      if (latest.projectKey === expectedKey) latest.setError(messageOf(error));
    } finally {
      const latest = useWorkflowStore.getState();
      if (latest.projectKey === expectedKey) latest.setLoading(false);
    }
  }, [enabled, request]);

  return useMemo(
    () => ({
      refresh,
      prepare: prepareKind,
      startPrepared: async (acknowledgeRestrictedContent, acknowledgeRemoteProvider) => {
        const preparation = useWorkflowStore.getState().preparation;
        if (!preparation) return;
        await perform(() =>
          startWorkflow({
            ...request(),
            preparationId: preparation.preparationId,
            preparationRevision: preparation.preparationRevision,
            acknowledgeRestrictedContent,
            acknowledgeRemoteProvider,
          }),
        );
      },
      cancel: (taskId) => perform(() => cancelWorkflowRun({ ...request(), taskId })),
      undoCancel: (taskId) => perform(() => undoCancelQueuedWorkflow({ ...request(), taskId })),
      reorder: (taskId, beforeTaskId) =>
        perform(() => reorderQueuedWorkflow({ ...request(), taskId, beforeTaskId })),
      retry: (taskId) => perform(() => retryWorkflow({ ...request(), taskId })),
      adjustAndPrepare: async (run, openSettingsAfter = false) => {
        const routeSelection =
          run.route?.kind === "agent"
            ? { kind: "agent" as const, agent: run.route.agent }
            : run.route?.kind === "byok"
              ? { kind: "byok" as const, provider: run.route.provider }
              : null;
        await prepareKind(run.kind, run.scope, routeSelection);
        if (openSettingsAfter) openSettings();
      },
      openRun: async (taskId) => {
        const state = useWorkflowStore.getState();
        try {
          await hydrateAndSelectWorkflowRun(
            { projectId: project.projectId, rootPath: project.rootPath },
            taskId,
          );
        } catch (error) {
          if (useWorkflowStore.getState().projectKey === state.projectKey) {
            useWorkflowStore.getState().setError(messageOf(error));
          }
        }
      },
      openResult: (run) =>
        openWorkflowResult(
          { projectId: project.projectId, rootPath: project.rootPath },
          run,
        ),
      confirm: (taskId, actionId) =>
        perform(() => confirmWorkflowAction({ ...request(), taskId, actionId })),
      discard: (taskId) => perform(() => discardWorkflowResult({ ...request(), taskId })),
      continueQueue: () => perform(() => continueQueuedWorkflows(request())),
      loadHistoryMore,
      handlePrerequisite: (action) => {
        if (action === "import_sources") {
          setActiveView("import");
          return;
        }
        if (action === "update_wiki") {
          void prepareKind("update_wiki");
          return;
        }
        if (action === "configure_execution_route" || action === "choose_execution_route") {
          openSettings();
          return;
        }
        void refresh();
      },
      backToOverview: () => {
        const state = useWorkflowStore.getState();
        state.setPreparation(null);
        state.selectRun(null);
        state.setSurface("overview");
      },
    }),
    [loadHistoryMore, openSettings, perform, prepareKind, refresh, request, setActiveView],
  );
}

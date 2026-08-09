import { useCallback, useEffect, useMemo, useRef } from "react";

import { i18next } from "../../i18n";

import { registerTaskEventListener } from "../../hooks/useTaskEvents";
import {
  cancelWorkflowRun,
  confirmWorkflowAction,
  continueQueuedWorkflows,
  discardWorkflowResult,
  getWorkflowRun,
  getWorkflowsOverview,
  listWorkflowRuns,
  prepareWorkflow,
  reorderQueuedWorkflow,
  retryWorkflow,
  startWorkflow,
  undoCancelQueuedWorkflow,
} from "../../services/workflowApi";
import {
  captureWorkflowRequestGuard,
  useWorkflowStore,
  workflowOperationPending,
  workflowRequestGuardMatches,
  workflowRunMatchesGuard,
  type WorkflowOperationError,
  type WorkflowRequestGuard,
} from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
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
  WorkflowPreparation,
  WorkflowRun,
  WorkflowScope,
  WorkflowStartOutcome,
} from "../../types/workflow";

interface PendingWorkflowEvent {
  eventProjectId: string | null;
  run: WorkflowRun;
}

const MAX_PENDING_WORKFLOW_EVENTS = 256;

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function operationError(summaryKey: string, error: unknown): WorkflowOperationError {
  const summary = i18next.t(summaryKey);
  if (typeof error !== "object" || error === null) {
    return { summary, technicalDetails: String(error) };
  }
  const candidate = error as { code?: unknown; message?: unknown; details?: unknown };
  const code = typeof candidate.code === "string" ? candidate.code : null;
  const message = typeof candidate.message === "string" ? candidate.message : String(error);
  let details: string | null = null;
  if (candidate.details !== undefined && candidate.details !== null) {
    try {
      details = JSON.stringify(candidate.details, null, 2);
    } catch {
      details = String(candidate.details);
    }
  }
  return {
    summary,
    technicalDetails: [code, message, details].filter(Boolean).join("\n") || null,
  };
}

function workflowRequestScopeMatches(guard: WorkflowRequestGuard): boolean {
  const state = useWorkflowStore.getState();
  const current = useProjectStore.getState().currentProject;
  return `${current.projectId}\0${current.rootPath}` === guard.projectKey
    && state.projectKey === guard.projectKey
    && state.requestEpoch === guard.requestEpoch;
}

function workflowAuthorityIdentity(
  project: Pick<ProjectSummary, "projectId" | "rootPath">,
): string | null {
  const state = useProjectStore.getState();
  if (
    state.currentProject.projectId !== project.projectId
    || state.currentProject.rootPath !== project.rootPath
    || state.authority?.projectId !== project.projectId
  ) return null;
  return `${state.authority.canonicalIdentityKey}\0${state.authority.identityRevision}`;
}

function workflowRequestGuardMatchesAuthority(
  guard: WorkflowRequestGuard,
  project: Pick<ProjectSummary, "projectId" | "rootPath">,
): boolean {
  if (!workflowRequestGuardMatches(guard)) return false;
  const projectState = useProjectStore.getState();
  if (
    projectState.currentProject.projectId !== project.projectId
    || projectState.currentProject.rootPath !== project.rootPath
  ) return false;
  const authority = projectState.authority;
  return !authority
    || authority.projectId !== project.projectId
    || (authority.canonicalIdentityKey === guard.canonicalIdentityKey
      && authority.identityRevision === guard.identityRevision);
}

type SettledResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: unknown };

function settle<T>(promise: Promise<T>): Promise<SettledResult<T>> {
  return promise.then(
    (value) => ({ ok: true, value }),
    (error: unknown) => ({ ok: false, error }),
  );
}

function workflowEventMatchesAccess(
  event: PendingWorkflowEvent,
  projectId: string,
  access: WorkflowProjectAccessSummary,
): boolean {
  return event.eventProjectId === projectId
    && workflowRunMatchesAccess(event.run, projectId, access);
}

function workflowRunMatchesAccess(
  run: WorkflowRun,
  projectId: string,
  access: WorkflowProjectAccessSummary,
): boolean {
  return run.projectId === projectId
    && run.canonicalIdentityKey === access.canonicalIdentityKey
    && run.identityRevision === access.identityRevision;
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
  if (!previous && pending.size >= MAX_PENDING_WORKFLOW_EVENTS) {
    const oldestTaskId = pending.keys().next().value as string | undefined;
    if (oldestTaskId) pending.delete(oldestTaskId);
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

export type WorkflowProjectPrerequisiteAction = Extract<
  WorkflowPrerequisiteAction,
  | "open_or_create_project"
  | "trust_project"
  | "make_writable"
  | "configure_git"
  | "resolve_dirty_git"
>;

export interface WorkflowProjectPrerequisiteContext {
  project: ProjectSummary;
  preparation: WorkflowPreparation | null;
  prepareAgain: () => Promise<void>;
}

export interface WorkflowsControllerOptions {
  onProjectPrerequisite?: (
    action: WorkflowProjectPrerequisiteAction,
    context: WorkflowProjectPrerequisiteContext,
  ) => Promise<void> | void;
}

const PROJECT_PREREQUISITE_ACTIONS = new Set<WorkflowPrerequisiteAction>([
  "open_or_create_project",
  "trust_project",
  "make_writable",
  "configure_git",
  "resolve_dirty_git",
]);

function routeSelectionOf(
  route: WorkflowPreparation["route"] | WorkflowRun["route"],
): WorkflowRouteSelection | null {
  if (route?.kind === "agent") return { kind: "agent", agent: route.agent };
  if (route?.kind === "byok") return { kind: "byok", provider: route.provider };
  return null;
}

export function useWorkflowsController(
  project: ProjectSummary,
  enabled: boolean,
  options?: WorkflowsControllerOptions,
): WorkflowsController {
  const projectKey = `${project.projectId}\0${project.rootPath}`;
  const authorityIdentity = useProjectStore((state) => {
    const authority = state.authority;
    return authority?.projectId === project.projectId
      ? `${authority.canonicalIdentityKey}\0${authority.identityRevision}`
      : null;
  });
  const onProjectPrerequisite = options?.onProjectPrerequisite;
  const activeKeyRef = useRef(projectKey);
  const refreshRequestRef = useRef(0);
  const historyRequestRef = useRef(0);
  const refreshInFlightRef = useRef<Map<string, number>>(new Map());
  const pendingEventsRef = useRef<Map<string, PendingWorkflowEvent>>(new Map());
  const pendingRefreshVersionRef = useRef<Map<string, number>>(new Map());
  const attemptedPendingRefreshVersionRef = useRef<Map<string, number>>(new Map());
  const prepareRequestRef = useRef(0);
  const waitingHydrationRef = useRef<Map<string, Promise<void>>>(new Map());
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openSettings = useNavigationStore((state) => state.openSettings);

  const request = useCallback(
    () => ({ projectId: project.projectId, projectRootPath: project.rootPath }),
    [project.projectId, project.rootPath],
  );

  const commitOutcome = useCallback((outcome: WorkflowStartOutcome) => {
    const state = useWorkflowStore.getState();
    state.upsertRun(outcome.run);
    useWorkflowStore.getState().selectRun(outcome.run.taskId);
  }, []);

  const hydrateSelectedWaitingRun = useCallback((candidate: WorkflowRun) => {
    const state = useWorkflowStore.getState();
    const actionId = candidate.pendingAction?.id;
    if (
      candidate.displayStatus !== "waiting_for_confirmation"
      || candidate.decisionReview
      || !actionId
      || state.selectedTaskId !== candidate.taskId
    ) {
      return;
    }
    const guard = captureWorkflowRequestGuard(state);
    if (!workflowRunMatchesGuard(candidate, project.projectId, guard)) return;
    const inFlightKey = [
      guard.projectKey,
      guard.identityRevision ?? "no-identity",
      candidate.taskId,
      actionId,
    ].join("\0");
    if (waitingHydrationRef.current.has(inFlightKey)) return;

    const operationKey = `task:${candidate.taskId}:hydrate:${actionId}`;
    const operationRequest = state.beginOperation(operationKey);
    const hydration = getWorkflowRun({
      ...request(),
      taskId: candidate.taskId,
    }).then((hydrated) => {
      const latest = useWorkflowStore.getState();
      const current = latest.runs.find((run) => run.taskId === candidate.taskId);
      if (
        !workflowRequestGuardMatchesAuthority(guard, project)
        || latest.selectedTaskId !== candidate.taskId
        || current?.displayStatus !== "waiting_for_confirmation"
        || current.pendingAction?.id !== actionId
        || hydrated.pendingAction?.id !== actionId
        || !hydrated.decisionReview
        || !workflowRunMatchesGuard(hydrated, project.projectId, guard)
      ) {
        return;
      }
      latest.hydrateDecisionReview(candidate.taskId, actionId, hydrated.decisionReview);
    }).catch((error: unknown) => {
      const latest = useWorkflowStore.getState();
      const current = latest.runs.find((run) => run.taskId === candidate.taskId);
      if (
        workflowRequestGuardMatchesAuthority(guard, project)
        && latest.selectedTaskId === candidate.taskId
        && current?.displayStatus === "waiting_for_confirmation"
        && current.pendingAction?.id === actionId
      ) {
        latest.failOperation(
          operationKey,
          operationRequest,
          operationError("workflows.operationError.detail", error),
        );
      }
    }).finally(() => {
      waitingHydrationRef.current.delete(inFlightKey);
      useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
    });
    waitingHydrationRef.current.set(inFlightKey, hydration);
  }, [project.projectId, request]);

  const refresh = useCallback(async (requestedMode?: "init" | "reconcile") => {
    if (!enabled || !hasTauri()) return;
    historyRequestRef.current += 1;
    const state = useWorkflowStore.getState();
    const refreshRequest = ++refreshRequestRef.current;
    const requestGuard = captureWorkflowRequestGuard(state);
    const authorityIdentityGuard = workflowAuthorityIdentity(project);
    const operationKey = requestedMode === "reconcile" || state.overview
      ? "overview:reconcile"
      : "overview:init";
    const operationRequest = state.beginOperation(operationKey);
    refreshInFlightRef.current.set(
      projectKey,
      (refreshInFlightRef.current.get(projectKey) ?? 0) + 1,
    );
    if (!state.overview) state.setOverviewStatus("loading");
    try {
      const overviewResultPromise = settle(getWorkflowsOverview(request()));
      const historyResultPromise = project.projectId && project.rootPath
        ? settle(listWorkflowRuns({
            ...request(),
            workflowKind: null,
            displayStatus: null,
            cursor: null,
            limit: 100,
          }))
        : Promise.resolve({
            ok: true as const,
            value: { runs: [] as WorkflowRun[], nextCursor: null },
          });
      const [overviewResult, historyResult] = await Promise.all([
        overviewResultPromise,
        historyResultPromise,
      ]);
      const latest = useWorkflowStore.getState();
      if (
        !workflowRequestScopeMatches(requestGuard)
        || workflowAuthorityIdentity(project) !== authorityIdentityGuard
        || refreshRequestRef.current !== refreshRequest
      ) return;
      if (!overviewResult.ok) {
        latest.setOverviewStatus("error");
        latest.failOperation(
          operationKey,
          operationRequest,
          operationError("workflows.operationError.overview", overviewResult.error),
        );
        return;
      }
      const overview = overviewResult.value;
      if (!historyResult.ok) {
        if (!latest.overview) latest.setOverviewStatus("error");
        latest.failOperation(
          operationKey,
          operationRequest,
          operationError("workflows.operationError.history", historyResult.error),
        );
        return;
      }
      const overviewAccess = overview.projectAccess;
      const currentAuthority = useProjectStore.getState().authority;
      const authorityMatchesOverview = !overviewAccess
        ? !currentAuthority || currentAuthority.projectId !== project.projectId
        : !currentAuthority
          || currentAuthority.projectId !== project.projectId
          || (currentAuthority.canonicalIdentityKey === overviewAccess.canonicalIdentityKey
            && currentAuthority.identityRevision === overviewAccess.identityRevision);
      const historyMatchesOverview = overviewAccess
        ? historyResult.value.runs.every((run) =>
            workflowRunMatchesAccess(run, project.projectId, overviewAccess),
          )
        : historyResult.value.runs.length === 0;
      if (!authorityMatchesOverview || !historyMatchesOverview) {
        latest.failOperation(operationKey, operationRequest, {
          summary: i18next.t("workflows.error.historyIdentityMismatch"),
          technicalDetails: authorityMatchesOverview
            ? "WORKFLOW_HISTORY_IDENTITY_MISMATCH"
            : "WORKFLOW_AUTHORITY_IDENTITY_MISMATCH",
        });
        if (!latest.overview) latest.setOverviewStatus("error");
        return;
      }
      latest.setProjectSnapshot(overview, historyResult.value.runs, historyResult.value.nextCursor);
      if (overviewAccess) {
        const pendingEvents = [...pendingEventsRef.current.values()];
        pendingEventsRef.current.clear();
        for (const pendingEvent of pendingEvents) {
          if (workflowEventMatchesAccess(pendingEvent, project.projectId, overviewAccess)) {
            latest.upsertRun(pendingEvent.run);
            hydrateSelectedWaitingRun(pendingEvent.run);
          }
        }
        pendingRefreshVersionRef.current.delete(projectKey);
        attemptedPendingRefreshVersionRef.current.delete(projectKey);
      }
    } catch (error) {
      const latest = useWorkflowStore.getState();
      if (
        workflowRequestScopeMatches(requestGuard)
        && workflowAuthorityIdentity(project) === authorityIdentityGuard
        && refreshRequestRef.current === refreshRequest
      ) {
        if (!latest.overview) latest.setOverviewStatus("error");
        latest.failOperation(
          operationKey,
          operationRequest,
          operationError("workflows.operationError.overview", error),
        );
      }
    } finally {
      const remainingRefreshes = Math.max(
        0,
        (refreshInFlightRef.current.get(projectKey) ?? 1) - 1,
      );
      if (remainingRefreshes === 0) refreshInFlightRef.current.delete(projectKey);
      else refreshInFlightRef.current.set(projectKey, remainingRefreshes);
      const latest = useWorkflowStore.getState();
      latest.finishOperation(operationKey, operationRequest);
      if (remainingRefreshes === 0 && activeKeyRef.current === projectKey) {
        const pendingVersion = pendingRefreshVersionRef.current.get(projectKey) ?? 0;
        const attemptedVersion = attemptedPendingRefreshVersionRef.current.get(projectKey) ?? 0;
        if (pendingVersion > attemptedVersion) {
          // A buffered event gets one coalesced follow-up after the current
          // refresh wave. Keeping the version until a successful identity
          // check prevents a failed or superseded request from losing the
          // event, without creating an unbounded retry loop.
          attemptedPendingRefreshVersionRef.current.set(projectKey, pendingVersion);
          void refresh("reconcile");
        }
      }
    }
  }, [enabled, hydrateSelectedWaitingRun, project.projectId, project.rootPath, projectKey, request]);

  useEffect(() => {
    activeKeyRef.current = projectKey;
    pendingEventsRef.current.clear();
    pendingRefreshVersionRef.current.clear();
    attemptedPendingRefreshVersionRef.current.clear();
    waitingHydrationRef.current.clear();
    useWorkflowStore.getState().activateProject(projectKey);
  }, [authorityIdentity, projectKey]);

  useEffect(() => {
    if (enabled) void refresh();
  }, [authorityIdentity, enabled, refresh]);

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
          if ((refreshInFlightRef.current.get(projectKey) ?? 0) === 0) void refresh("reconcile");
          return;
        }
        if (
          run.canonicalIdentityKey !== access.canonicalIdentityKey ||
          run.identityRevision !== access.identityRevision
        ) {
          return;
        }
        state.upsertRun(run);
        hydrateSelectedWaitingRun(run);
        void refresh("reconcile");
      }),
    [hydrateSelectedWaitingRun, project.projectId, projectKey, refresh],
  );

  const perform = useCallback(
    async (
      operationKey: string,
      summaryKey: string,
      operation: () => Promise<WorkflowRun | WorkflowStartOutcome | { runs: WorkflowRun[] }>,
    ) => {
      const state = useWorkflowStore.getState();
      const guard = captureWorkflowRequestGuard(state);
      const operationRequest = state.beginOperation(operationKey);
      try {
        const result = await operation();
        const latest = useWorkflowStore.getState();
        if (!workflowRequestGuardMatchesAuthority(guard, project)) return;
        if ("kind" in result && (result.kind === "created" || result.kind === "existing")) {
          if (!workflowRunMatchesGuard(result.run, project.projectId, guard)) return;
          commitOutcome(result);
        } else if ("runs" in result) {
          if (!result.runs.every((run) => workflowRunMatchesGuard(run, project.projectId, guard))) return;
          latest.replaceRuns(result.runs);
        } else {
          if (!workflowRunMatchesGuard(result, project.projectId, guard)) return;
          latest.upsertRun(result);
          useWorkflowStore.getState().selectRun(result.taskId);
        }
        await refresh("reconcile");
      } catch (error) {
        if (workflowRequestGuardMatchesAuthority(guard, project)) {
          useWorkflowStore.getState().failOperation(
            operationKey,
            operationRequest,
            operationError(summaryKey, error),
          );
        }
      } finally {
        useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
      }
    },
    [commitOutcome, project.projectId, refresh],
  );

  const prepareKind = useCallback(
    async (kind: WorkflowKind, scope: WorkflowScope | null = null, routeSelection: WorkflowRouteSelection | null = null) => {
      if (
        project.projectId
        && project.rootPath
        && !useWorkflowStore.getState().identityGuard.canonicalIdentityKey
      ) {
        await refresh("init");
      }
      const state = useWorkflowStore.getState();
      const prepareRequest = ++prepareRequestRef.current;
      const guard = captureWorkflowRequestGuard(state);
      const operationKey = `prepare:${kind}`;
      const operationRequest = state.beginOperation(operationKey);
      try {
        const preparation = await prepareWorkflow({
          ...request(),
          kind,
          scope,
          routeSelection,
        });
        const latest = useWorkflowStore.getState();
        if (
          !workflowRequestGuardMatchesAuthority(guard, project)
          || prepareRequestRef.current !== prepareRequest
          || preparation.projectAccess.canonicalIdentityKey !== guard.canonicalIdentityKey
          || preparation.projectAccess.identityRevision !== guard.identityRevision
        ) return;
        latest.setPreparation(preparation);
      } catch (error) {
        if (workflowRequestGuardMatchesAuthority(guard, project) && prepareRequestRef.current === prepareRequest) {
          useWorkflowStore.getState().failOperation(
            operationKey,
            operationRequest,
            operationError("workflows.operationError.prepare", error),
          );
        }
      } finally {
        useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
      }
    },
    [project.projectId, project.rootPath, refresh, request],
  );

  const loadHistoryMore = useCallback(async () => {
    const state = useWorkflowStore.getState();
    const cursor = state.historyCursor;
    if (!enabled || !cursor || !hasTauri()) return;
    const historyRequest = ++historyRequestRef.current;
    const guard = captureWorkflowRequestGuard(state);
    const expectedAccess = state.overview?.projectAccess;
    if (!expectedAccess) return;
    const operationKey = "history:page";
    const operationRequest = state.beginOperation(operationKey);
    try {
      const page = await listWorkflowRuns({
        ...request(),
        workflowKind: null,
        displayStatus: null,
        cursor,
        limit: 100,
      });
      const latest = useWorkflowStore.getState();
      const latestAccess = latest.overview?.projectAccess;
      if (
        !workflowRequestGuardMatchesAuthority(guard, project)
        || latest.historyCursor !== cursor
        || historyRequestRef.current !== historyRequest
        || !latestAccess
        || latestAccess.canonicalIdentityKey !== expectedAccess.canonicalIdentityKey
        || latestAccess.identityRevision !== expectedAccess.identityRevision
      ) return;
      if (
        !page.runs.every((run) =>
          workflowRunMatchesAccess(run, project.projectId, latestAccess),
        )
      ) {
        latest.failOperation(operationKey, operationRequest, {
          summary: i18next.t("workflows.error.historyIdentityMismatch"),
          technicalDetails: "WORKFLOW_HISTORY_IDENTITY_MISMATCH",
        });
        return;
      }
      latest.replaceRuns(page.runs);
      latest.setHistoryCursor(page.nextCursor);
    } catch (error) {
      if (
        workflowRequestGuardMatchesAuthority(guard, project)
        && historyRequestRef.current === historyRequest
      ) {
        useWorkflowStore.getState().failOperation(
          operationKey,
          operationRequest,
          operationError("workflows.operationError.history", error),
        );
      }
    } finally {
      useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
    }
  }, [enabled, request]);

  return useMemo(
    () => ({
      refresh,
      prepare: prepareKind,
      startPrepared: async (acknowledgeRestrictedContent, acknowledgeRemoteProvider) => {
        const state = useWorkflowStore.getState();
        const preparation = state.preparation;
        if (!preparation) return;
        const guard = captureWorkflowRequestGuard(state);
        if (
          preparation.projectAccess.canonicalIdentityKey !== guard.canonicalIdentityKey
          || preparation.projectAccess.identityRevision !== guard.identityRevision
        ) return;
        await perform(
          `start:${preparation.preparationId}`,
          "workflows.operationError.start",
          () =>
          startWorkflow({
            ...request(),
            preparationId: preparation.preparationId,
            preparationRevision: preparation.preparationRevision,
            acknowledgeRestrictedContent,
            acknowledgeRemoteProvider,
          }),
        );
      },
      cancel: (taskId) => perform(
        `task:${taskId}:cancel`,
        "workflows.operationError.task",
        () => cancelWorkflowRun({ ...request(), taskId }),
      ),
      undoCancel: (taskId) => perform(
        `task:${taskId}:undo-cancel`,
        "workflows.operationError.task",
        () => undoCancelQueuedWorkflow({ ...request(), taskId }),
      ),
      reorder: (taskId, beforeTaskId) =>
        perform(
          `task:${taskId}:reorder`,
          "workflows.operationError.task",
          () => reorderQueuedWorkflow({ ...request(), taskId, beforeTaskId }),
        ),
      retry: (taskId) => perform(
        `task:${taskId}:retry`,
        "workflows.operationError.task",
        () => retryWorkflow({ ...request(), taskId }),
      ),
      adjustAndPrepare: async (run, openSettingsAfter = false) => {
        const routeSelection = routeSelectionOf(run.route);
        if (openSettingsAfter) {
          openSettings("ai", {
            projectId: project.projectId,
            projectRootPath: project.rootPath,
            kind: run.kind,
            scope: run.scope,
            routeSelection,
            source: "adjust",
            expectedSurface: "detail",
            expectedCanonicalIdentityKey: run.canonicalIdentityKey,
            expectedIdentityRevision: run.identityRevision,
            expectedPreparationId: null,
            expectedPreparationRevision: null,
            expectedTaskId: run.taskId,
          });
          return;
        }
        await prepareKind(run.kind, run.scope, routeSelection);
      },
      openRun: async (taskId) => {
        const state = useWorkflowStore.getState();
        const guard = captureWorkflowRequestGuard(state);
        const operationKey = `task:${taskId}:open`;
        const operationRequest = state.beginOperation(operationKey);
        try {
          await hydrateAndSelectWorkflowRun(
            { projectId: project.projectId, rootPath: project.rootPath },
            taskId,
          );
        } catch (error) {
          if (workflowRequestGuardMatchesAuthority(guard, project)) {
            useWorkflowStore.getState().failOperation(
              operationKey,
              operationRequest,
              operationError("workflows.operationError.detail", error),
            );
          }
        } finally {
          useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
        }
      },
      openResult: async (run) => {
        const state = useWorkflowStore.getState();
        const guard = captureWorkflowRequestGuard(state);
        const operationKey = `task:${run.taskId}:open-result`;
        const operationRequest = state.beginOperation(operationKey);
        try {
          await openWorkflowResult(
          { projectId: project.projectId, rootPath: project.rootPath },
          run,
          );
        } catch (error) {
          if (workflowRequestGuardMatchesAuthority(guard, project)) {
            useWorkflowStore.getState().failOperation(
              operationKey,
              operationRequest,
              operationError("workflows.operationError.navigation", error),
            );
          }
        } finally {
          useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
        }
      },
      confirm: (taskId, actionId) =>
        perform(
          `task:${taskId}:confirm:${actionId}`,
          "workflows.operationError.task",
          () => confirmWorkflowAction({ ...request(), taskId, actionId }),
        ),
      discard: (taskId) => perform(
        `task:${taskId}:discard`,
        "workflows.operationError.task",
        () => discardWorkflowResult({ ...request(), taskId }),
      ),
      continueQueue: () => perform(
        "queue:continue",
        "workflows.operationError.task",
        () => continueQueuedWorkflows(request()),
      ),
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
          const preparation = useWorkflowStore.getState().preparation;
          if (!preparation) {
            void refresh();
            return;
          }
          openSettings("ai", {
            projectId: project.projectId,
            projectRootPath: project.rootPath,
            kind: preparation.kind,
            scope: preparation.scope,
            routeSelection: routeSelectionOf(preparation.route),
            source: "prerequisite",
            expectedSurface: "preparation",
            expectedCanonicalIdentityKey:
              preparation.projectAccess.canonicalIdentityKey,
            expectedIdentityRevision: preparation.projectAccess.identityRevision,
            expectedPreparationId: preparation.preparationId,
            expectedPreparationRevision: preparation.preparationRevision,
            expectedTaskId: null,
          });
          return;
        }
        if (action === "prepare_again") {
          const preparation = useWorkflowStore.getState().preparation;
          if (preparation) {
            void prepareKind(
              preparation.kind,
              preparation.scope,
              routeSelectionOf(preparation.route),
            );
          } else {
            void refresh();
          }
          return;
        }
        if (PROJECT_PREREQUISITE_ACTIONS.has(action)) {
          const state = useWorkflowStore.getState();
          const preparation = state.preparation;
          const projectAction = action as WorkflowProjectPrerequisiteAction;
          const operationKey = `prerequisite:project:${projectAction}`;
          if (workflowOperationPending(state.operations, operationKey)) return;
          const guard = captureWorkflowRequestGuard(state);
          const operationRequest = state.beginOperation(operationKey);
          if (onProjectPrerequisite) {
            void Promise.resolve(
              onProjectPrerequisite(projectAction, {
                project,
                preparation,
                prepareAgain: async () => {
                  if (preparation) {
                    await prepareKind(
                      preparation.kind,
                      preparation.scope,
                      routeSelectionOf(preparation.route),
                    );
                  } else {
                    await refresh();
                  }
                },
              }),
            ).catch((error) => {
              if (workflowRequestGuardMatchesAuthority(guard, project)) {
                useWorkflowStore.getState().failOperation(
                  operationKey,
                  operationRequest,
                  operationError("workflows.operationError.prerequisite", error),
                );
              }
            }).finally(() => {
              useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
            });
          } else {
            state.failOperation(operationKey, operationRequest, {
              summary: i18next.t("workflows.prerequisite.projectActionUnavailable"),
              technicalDetails: "WORKFLOW_PROJECT_ACTION_UNAVAILABLE",
            });
          }
          return;
        }
        void refresh();
      },
      backToOverview: () => {
        const state = useWorkflowStore.getState();
        state.setSurface("overview");
      },
    }),
    [
      loadHistoryMore,
      openSettings,
      onProjectPrerequisite,
      perform,
      prepareKind,
      project,
      refresh,
      request,
      setActiveView,
    ],
  );
}

import { useCallback, useEffect, useMemo, useRef } from "react";

import { i18next } from "../../i18n";
import {
  normalizeBackendError,
} from "../../lib/backendError";

import { registerTaskEventListener } from "../../services/taskEventDispatcher";
import {
  cancelWorkflowRun,
  confirmWorkflowAction,
  continueQueuedWorkflows,
  discardWorkflowResult,
  getWorkflowRun,
  getWorkflowsOverview,
  prepareWorkflow,
  reorderQueuedWorkflow,
  retryWorkflow,
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
import { recordWorkflowFacts, useTaskStore } from "../../stores/taskStore";
import { compareWorkflowRevision, workflowRunSummary } from "../../services/workflowTaskSnapshot";
import { useProjectStore } from "../../stores/projectStore";
import {
  cancelWorkflowNavigation,
  hydrateAndSelectWorkflowRun,
  openWorkflowResult,
} from "../../services/workflowNavigation";
import type { ProjectSummary } from "../../types/project";
import type {
  WorkflowDisplayStatus,
  WorkflowKind,
  WorkflowPrerequisiteAction,
  WorkflowRouteSelection,
  WorkflowPreparation,
  WorkflowPreparationDraft,
  WorkflowRun,
  WorkflowRunSummary,
  WorkflowScope,
  WorkflowStartOutcome,
} from "../../types/workflow";

const loadWorkflowExecution = () => import("./workflowExecution");
const loadWorkflowHistoryModule = () => import("./workflowHistory");

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function operationError(summaryKey: string, error: unknown): WorkflowOperationError {
  const normalized = normalizeBackendError(error);
  return {
    summary: i18next.t(summaryKey),
    technicalDetails: normalized.technicalDetails,
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


export interface WorkflowsController {
  refresh: () => Promise<void>;
  prepare: (kind: WorkflowKind, scope?: WorkflowScope | null, routeSelection?: WorkflowRouteSelection | null) => Promise<void>;
  startPrepared: (acknowledgeRestrictedContent: boolean, acknowledgeRemoteProvider: boolean, draft?: WorkflowPreparationDraft) => Promise<void>;
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
  filterHistory: (kind: WorkflowKind | null, status: WorkflowDisplayStatus | null) => Promise<void>;
  loadHistoryMore: () => Promise<void>;
  handlePrerequisite: (action: WorkflowPrerequisiteAction, draft?: WorkflowPreparationDraft) => void;
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
  const enabledRef = useRef(enabled);
  const historyRequestRef = useRef(0);
  const overviewRequestRef = useRef<{ projectKey: string; epoch: number; dirty: boolean; promise: Promise<void> } | null>(null);
  const prepareRequestRef = useRef(0);
  const waitingHydrationRef = useRef<Map<string, { promise: Promise<void>; dirty: boolean; expected: WorkflowRunSummary }>>(new Map());
  enabledRef.current = enabled;

  const request = useCallback(
    () => ({ projectId: project.projectId, projectRootPath: project.rootPath }),
    [project.projectId, project.rootPath],
  );

  const commitOutcome = useCallback((outcome: WorkflowStartOutcome) => {
    const state = useWorkflowStore.getState();
    state.upsertRun(outcome.run);
    useWorkflowStore.getState().selectRun(outcome.run.taskId);
  }, []);

  const hydrateSelectedRun = useCallback((summary: WorkflowRunSummary) => {
    const state = useWorkflowStore.getState();
    if (!enabledRef.current || state.selectedTaskId !== summary.taskId
      || summary.displayStatus === "running" || summary.displayStatus === "queued") return;
    const guard = captureWorkflowRequestGuard(state);
    if (!workflowRunMatchesGuard(summary, project.projectId, guard)) return;
    const key = `${guard.projectKey}\0${summary.taskId}`;
    const existing = waitingHydrationRef.current.get(key);
    if (existing) {
      if (summary.sessionId !== existing.expected.sessionId || compareWorkflowRevision(summary, existing.expected) > 0) existing.dirty = true;
      return;
    }
    if (summary.revision !== undefined && state.detailRevisionById[summary.taskId] === summary.revision) return;
    const slot = { promise: Promise.resolve(), dirty: false, expected: summary };
    waitingHydrationRef.current.set(key, slot);
    const operationKey = `task:${summary.taskId}:hydrate:boundary`;
    const operation = state.beginOperation(operationKey);
    slot.promise = (async () => {
      do {
        slot.dirty = false;
        const expected = useTaskStore.getState().workflowById[summary.taskId] ?? summary;
        slot.expected = expected;
        try {
          const run = await getWorkflowRun({ ...request(), taskId: summary.taskId });
          recordWorkflowFacts([run]);
          const latest = useWorkflowStore.getState();
          if (!workflowRequestGuardMatchesAuthority(guard, project) || latest.selectedTaskId !== summary.taskId) return;
          if (workflowRunMatchesGuard(run, project.projectId, guard)) latest.upsertRun(run);
          const current = useTaskStore.getState().workflowById[summary.taskId];
          if (current && compareWorkflowRevision(current, run) > 0) {
            if (current.sessionId !== expected.sessionId || compareWorkflowRevision(current, expected) > 0) slot.dirty = true;
            else {
              latest.failOperation(operationKey, operation, operationError("workflows.operationError.detail", new Error("WORKFLOW_DETAIL_STALE")));
              return;
            }
          }
        } catch (error) {
          const latest = useWorkflowStore.getState();
          const current = useTaskStore.getState().workflowById[summary.taskId] ?? summary;
          if (!workflowRequestGuardMatchesAuthority(guard, project) || latest.selectedTaskId !== summary.taskId) return;
          if (current.sessionId === expected.sessionId && compareWorkflowRevision(current, expected) === 0) {
            latest.failOperation(operationKey, operation, operationError("workflows.operationError.detail", error));
            return;
          }
          slot.dirty = true;
        }
      } while (slot.dirty && enabledRef.current && workflowRequestGuardMatchesAuthority(guard, project)
        && useWorkflowStore.getState().selectedTaskId === summary.taskId);
    })().finally(() => {
      if (waitingHydrationRef.current.get(key) === slot) waitingHydrationRef.current.delete(key);
      useWorkflowStore.getState().finishOperation(operationKey, operation);
    });
  }, [project, request]);

  const refresh = useCallback(async (): Promise<void> => {
    if (!enabledRef.current || !hasTauri()) return;
    const initial = useWorkflowStore.getState();
    const existing = overviewRequestRef.current;
    if (existing?.projectKey === projectKey && existing.epoch === initial.requestEpoch) {
      existing.dirty = true;
      return existing.promise;
    }
    const slot = { projectKey, epoch: initial.requestEpoch, dirty: false, promise: Promise.resolve() };
    overviewRequestRef.current = slot;
    slot.promise = (async () => {
      do {
        slot.dirty = false;
        const state = useWorkflowStore.getState();
        const guard = captureWorkflowRequestGuard(state);
        const identity = workflowAuthorityIdentity(project);
        const key = state.overview ? "overview:reconcile" : "overview:init";
        const operation = state.beginOperation(key);
        if (!state.overview) state.setOverviewStatus("loading");
        try {
          const overview = await getWorkflowsOverview(request());
          if (!workflowRequestScopeMatches(guard)
            || workflowAuthorityIdentity(project) !== identity) return;
          const access = overview.projectAccess;
          if (access && identity && identity !== `${access.canonicalIdentityKey}\0${access.identityRevision}`) {
            throw new Error("WORKFLOW_AUTHORITY_IDENTITY_MISMATCH");
          }
          const latest = useWorkflowStore.getState();
          latest.setOverviewSnapshot(overview);
          latest.applySummaries(Object.values(useTaskStore.getState().workflowById));
          const selected = useTaskStore.getState().workflowById[latest.selectedTaskId ?? ""];
          if (selected) hydrateSelectedRun(selected);
        } catch (error) {
          if (workflowRequestScopeMatches(guard) && workflowAuthorityIdentity(project) === identity) {
            const latest = useWorkflowStore.getState();
            latest.setOverviewStatus("error");
            latest.failOperation(key, operation, operationError("workflows.operationError.overview", error));
          }
        } finally {
          useWorkflowStore.getState().finishOperation(key, operation);
        }
      } while (slot.dirty && enabledRef.current && activeKeyRef.current === projectKey
        && useWorkflowStore.getState().requestEpoch === slot.epoch);
    })().finally(() => {
      if (overviewRequestRef.current === slot) overviewRequestRef.current = null;
    });
    return slot.promise;
  }, [hydrateSelectedRun, project, projectKey, request]);

  const reconcileOverview = refresh;

  useEffect(() => {
    activeKeyRef.current = projectKey;
    waitingHydrationRef.current.clear();
    useWorkflowStore.getState().activateProject(projectKey);

  }, [authorityIdentity, projectKey]);

  useEffect(() => {
    if (enabled) void refresh();
  }, [authorityIdentity, enabled, refresh]);

  useEffect(() => registerTaskEventListener((event) => {
    if (event.eventType !== "workflow_updated") return;
    const run = workflowRunSummary(event.payload as WorkflowRun | WorkflowRunSummary);
    if (run.projectId !== event.projectId) return;
    // The app dispatcher coalesces progress; facts survive hidden pages and project switches.
    recordWorkflowFacts([run], false);
    const accepted = useTaskStore.getState().workflowById[run.taskId];
    if (!accepted || compareWorkflowRevision(run, accepted) < 0) return;
    if (activeKeyRef.current !== projectKey || run.projectId !== project.projectId) return;
    const row = useWorkflowStore.getState().overview?.rows.find((row) => row.kind === run.kind);
    const boundary = run.displayStatus !== "running" || (row?.activeTaskId !== run.taskId || row.state !== "running");
    useWorkflowStore.getState().applySummaries([accepted]);
    hydrateSelectedRun(accepted);
    if (boundary && enabledRef.current) void reconcileOverview();
  }), [hydrateSelectedRun, project.projectId, projectKey, reconcileOverview]);

  const perform = useCallback(
    async (
      operationKey: string,
      summaryKey: string,
      operation: () => Promise<WorkflowRun | WorkflowStartOutcome | { runs: WorkflowRun[] } | null>,
    ) => {
      const state = useWorkflowStore.getState();
      const guard = captureWorkflowRequestGuard(state);
      const operationRequest = state.beginOperation(operationKey);
      try {
        const result = await operation();
        if (!result) return;
        recordWorkflowFacts("run" in result ? [result.run] : "runs" in result ? result.runs : [result]);
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
        await reconcileOverview();
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
    [commitOutcome, project.projectId, reconcileOverview],
  );

  const prepareKind = useCallback(
    async (kind: WorkflowKind, scope: WorkflowScope | null = null, routeSelection: WorkflowRouteSelection | null = null) => {
      cancelWorkflowNavigation();
      if (
        project.projectId
        && project.rootPath
        && !useWorkflowStore.getState().identityGuard.canonicalIdentityKey
      ) {
        await refresh();
      }
      const state = useWorkflowStore.getState();
      const savedDraft = state.drafts[kind];
      const selectedScope = scope ?? savedDraft?.scope ?? null;
      const selectedRoute = routeSelection ?? savedDraft?.routeSelection ?? null;
      const prepareRequest = ++prepareRequestRef.current;
      const guard = captureWorkflowRequestGuard(state);
      const operationKey = `prepare:${kind}`;
      const operationRequest = state.beginOperation(operationKey);
      try {
        const preparation = await prepareWorkflow({
          ...request(),
          kind,
          scope: selectedScope,
          routeSelection: selectedRoute,
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
      const { loadWorkflowHistory } = await loadWorkflowHistoryModule();
      await loadWorkflowHistory({ ...request(), workflowKind: state.historyKind, displayStatus: state.historyStatus, cursor, limit: 50 },
        guard, expectedAccess, operationRequest, () => enabledRef.current && historyRequestRef.current === historyRequest);
    } catch (error) {
      if (workflowRequestGuardMatchesAuthority(guard, project) && historyRequestRef.current === historyRequest) {
        useWorkflowStore.getState().failOperation(operationKey, operationRequest, operationError("workflows.operationError.history", error));
      }
    } finally {
      useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
    }
  }, [enabled, request]);

  const filterHistory = useCallback(async (
    kind: WorkflowKind | null,
    status: WorkflowDisplayStatus | null,
  ) => {
    if (!enabledRef.current || !hasTauri()) return;
    const state = useWorkflowStore.getState();
    const expectedAccess = state.overview?.projectAccess;
    if (!expectedAccess) return;
    state.setHistoryFilters(kind, status);
    state.clearOperationError("history:page");
    const historyRequest = ++historyRequestRef.current;
    const guard = captureWorkflowRequestGuard(useWorkflowStore.getState());
    const operationKey = "history:filter";
    const operationRequest = useWorkflowStore.getState().beginOperation(operationKey);
    try {
      const { loadWorkflowHistory } = await loadWorkflowHistoryModule();
      await loadWorkflowHistory({ ...request(), workflowKind: kind, displayStatus: status, cursor: null, limit: 50 },
        guard, expectedAccess, operationRequest, () => enabledRef.current && historyRequestRef.current === historyRequest);
    } catch (error) {
      if (workflowRequestGuardMatchesAuthority(guard, project) && historyRequestRef.current === historyRequest) {
        useWorkflowStore.getState().failOperation(operationKey, operationRequest, operationError("workflows.operationError.history", error));
      }
    } finally {
      useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
    }
  }, [project, request]);

  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const selectedSummary = useTaskStore((state) => state.workflowById[selectedTaskId ?? ""]);
  useEffect(() => {
    if (!enabled || !selectedSummary) return;
    const revision = useWorkflowStore.getState().detailRevisionById[selectedSummary.taskId];
    if (revision !== selectedSummary.revision) hydrateSelectedRun(selectedSummary);
  }, [enabled, selectedSummary, hydrateSelectedRun]);

  const surface = useWorkflowStore((state) => state.surface);
  const overviewIdentity = useWorkflowStore((state) => state.identityGuard.identityRevision);
  useEffect(() => {
    if (enabled && surface === "history" && overviewIdentity) {
      const state = useWorkflowStore.getState();
      void filterHistory(state.historyKind, state.historyStatus);
    }
  }, [enabled, surface, overviewIdentity, filterHistory]);

  return useMemo(
    () => ({
      refresh,
      prepare: prepareKind,
      startPrepared: async (acknowledgeRestrictedContent, acknowledgeRemoteProvider, draft) => {
        const state = useWorkflowStore.getState();
        const preparation = state.preparation;
        const retryOfTaskId = state.retryOfTaskId;
        if (!preparation) return;
        const operationKey = `start:${preparation.preparationId}`;
        if (workflowOperationPending(state.operations, operationKey)) return;
        const guard = captureWorkflowRequestGuard(state);
        if (
          preparation.projectAccess.canonicalIdentityKey !== guard.canonicalIdentityKey
          || preparation.projectAccess.identityRevision !== guard.identityRevision
        ) return;
        await perform(
          operationKey,
          "workflows.operationError.start",
          async () => {
            const { startPreparedWorkflow } = await loadWorkflowExecution();
            return startPreparedWorkflow(request(), preparation, {
              acknowledgeRestrictedContent, acknowledgeRemoteProvider, draft, retryOfTaskId,
            }, () => enabledRef.current && workflowRequestGuardMatchesAuthority(guard, project)
              && useWorkflowStore.getState().preparation === preparation);
          }
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
          const state = useWorkflowStore.getState();
          const guard = captureWorkflowRequestGuard(state);
          const operationKey = `task:${run.taskId}:settings`;
          if (workflowOperationPending(state.operations, operationKey)) return;
          const operation = state.beginOperation(operationKey);
          try {
            const { openWorkflowAdjustmentSettings } = await loadWorkflowExecution();
            const latest = useWorkflowStore.getState();
            if (enabledRef.current && workflowRequestGuardMatchesAuthority(guard, project)
              && latest.selectedTaskId === state.selectedTaskId && latest.surface === state.surface) {
              openWorkflowAdjustmentSettings(project, run);
            }
          } catch (error) {
            if (workflowRequestGuardMatchesAuthority(guard, project)) {
              useWorkflowStore.getState().failOperation(operationKey, operation, operationError("workflows.operationError.prepare", error));
            }
          } finally {
            useWorkflowStore.getState().finishOperation(operationKey, operation);
          }
          return;
        }
        if (run.pendingAction?.actionType === "review_scope" && (run.scope.kind === "update_wiki" || run.scope.kind === "health_check")) {
          const state = useWorkflowStore.getState();
          const { selectedTaskId, surface } = state;
          const guard = captureWorkflowRequestGuard(state);
          const operationKey = `task:${run.taskId}:review-scope`;
          if (workflowOperationPending(state.operations, operationKey)) return;
          const operation = useWorkflowStore.getState().beginOperation(operationKey);
          try {
            const { reviewWorkflowScope } = await loadWorkflowExecution();
            await reviewWorkflowScope(request(), run, routeSelection, () => enabledRef.current
              && workflowRequestGuardMatchesAuthority(guard, project)
              && useWorkflowStore.getState().selectedTaskId === selectedTaskId
              && useWorkflowStore.getState().surface === surface);
          } catch (error) {
            if (workflowRequestGuardMatchesAuthority(guard, project)) {
              useWorkflowStore.getState().failOperation(operationKey, operation,
                operationError("workflows.operationError.prepare", error));
            }
          } finally {
            useWorkflowStore.getState().finishOperation(operationKey, operation);
          }
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
          if (error instanceof Error && error.message === "WORKFLOW_NAVIGATION_SUPERSEDED") return;
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
          if (error instanceof Error && error.message === "WORKFLOW_NAVIGATION_SUPERSEDED") return;
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
      filterHistory,
      loadHistoryMore,
      handlePrerequisite: async (action, draft) => {
        const state = useWorkflowStore.getState();
        const guard = captureWorkflowRequestGuard(state);
        const projectAction = PROJECT_PREREQUISITE_ACTIONS.has(action);
        const operationKey = projectAction ? `prerequisite:project:${action}` : `prerequisite:${action}`;
        if (workflowOperationPending(state.operations, operationKey)) return;
        const operation = state.beginOperation(operationKey);
        try {
          if (projectAction && !onProjectPrerequisite) {
            state.failOperation(operationKey, operation, {
              summary: i18next.t("workflows.prerequisite.projectActionUnavailable"),
              technicalDetails: "WORKFLOW_PROJECT_ACTION_UNAVAILABLE",
            });
            return;
          }
          const { handleWorkflowPrerequisite } = await loadWorkflowExecution();
          const latest = useWorkflowStore.getState();
          if (!enabledRef.current || !workflowRequestGuardMatchesAuthority(guard, project)
            || latest.preparation !== state.preparation || latest.selectedTaskId !== state.selectedTaskId
            || latest.surface !== state.surface) return;
          await handleWorkflowPrerequisite(action, draft, project, prepareKind, refresh, onProjectPrerequisite);
        } catch (error) {
          if (workflowRequestGuardMatchesAuthority(guard, project)) {
            useWorkflowStore.getState().failOperation(operationKey, operation, operationError("workflows.operationError.prerequisite", error));
          }
        } finally {
          useWorkflowStore.getState().finishOperation(operationKey, operation);
        }
      },
      backToOverview: () => {
        cancelWorkflowNavigation();
        const state = useWorkflowStore.getState();
        state.setSurface("overview");
      },
    }),
    [
      filterHistory,
      loadHistoryMore,
      onProjectPrerequisite,
      perform,
      prepareKind,
      project,
      refresh,
      request,
    ],
  );
}

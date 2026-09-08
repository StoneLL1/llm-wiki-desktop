import { compareWorkflowRevision, mergeWorkflowOverview } from "../services/workflowTaskSnapshot";
import { recordWorkflowFacts, useTaskStore } from "./taskStore";
import { create } from "zustand";
import { registerProjectScopeResetHandler } from "./projectScopeResetRegistry";

import type {
  UpdateWikiDraft,
  WorkflowDecisionReview,
  WorkflowDisplayStatus,
  WorkflowKind,
  WorkflowPreparation,
  WorkflowPreparationDraft,
  WorkflowRun,
  WorkflowRunSummary,
  WorkflowRouteSelection,
  WorkflowsOverview,
} from "../types/workflow";

export type WorkflowsSurface = "overview" | "preparation" | "detail" | "history";
export type WorkflowOverviewStatus = "idle" | "loading" | "ready" | "error";

export interface WorkflowIdentityGuard {
  canonicalIdentityKey: string | null;
  identityRevision: string | null;
}

export interface WorkflowRequestGuard extends WorkflowIdentityGuard {
  projectKey: string;
  requestEpoch: number;
}

export interface WorkflowOperationError {
  summary: string;
  technicalDetails: string | null;
}

export interface WorkflowOperationState {
  requestId: number;
  pending: boolean;
  error: WorkflowOperationError | null;
}

export interface WorkflowState {
  updateDraft: UpdateWikiDraft;
  setUpdateDraft: (draft: UpdateWikiDraft) => void;
  projectKey: string;
  identityGuard: WorkflowIdentityGuard;
  overview: WorkflowsOverview | null;
  overviewStatus: WorkflowOverviewStatus;
  runs: WorkflowRun[];
  detailRevisionById: Record<string, string | undefined>;
  historyRuns: WorkflowRunSummary[];
  retryOfTaskId: string | null;
  preparation: WorkflowPreparation | null;
  preparingKind: WorkflowKind | null;
  preparations: Partial<Record<WorkflowKind, WorkflowPreparation>>;
  preparedRouteSelections: Partial<Record<WorkflowKind, WorkflowRouteSelection | null>>;
  beginPreparation: (kind: WorkflowKind) => void;
  drafts: Partial<Record<WorkflowKind, WorkflowPreparationDraft & { preparationId: string }>>;
  setDraft: (kind: WorkflowKind, draft: WorkflowPreparationDraft & { preparationId: string }) => void;
  selectedTaskId: string | null;
  surface: WorkflowsSurface;
  historyKind: WorkflowKind | null;
  historyStatus: WorkflowDisplayStatus | null;
  historyCursor: string | null;
  operations: Record<string, WorkflowOperationState>;
  operationSequence: number;
  requestEpoch: number;
  activateProject: (projectKey: string) => number;
  reset: () => void;
  setOverviewSnapshot: (overview: WorkflowsOverview) => void;
  applySummaries: (summaries: readonly WorkflowRunSummary[]) => void;
  setOverviewStatus: (status: WorkflowOverviewStatus) => void;
  replaceRuns: (runs: WorkflowRun[]) => void;
  replaceHistoryPage: (runs: WorkflowRunSummary[], cursor: string | null) => void;
  appendHistoryPage: (runs: WorkflowRunSummary[], cursor: string | null) => void;
  upsertRun: (run: WorkflowRun) => void;
  upsertRuns: (runs: readonly WorkflowRun[]) => void;
  hydrateDecisionReview: (taskId: string, actionId: string, review: WorkflowDecisionReview) => void;
  setPreparation: (preparation: WorkflowPreparation | null, routeSelection?: WorkflowRouteSelection | null) => void;
  selectRun: (taskId: string | null) => void;
  setSurface: (surface: WorkflowsSurface) => void;
  setHistoryFilters: (kind: WorkflowKind | null, status: WorkflowDisplayStatus | null) => void;
  setHistoryCursor: (cursor: string | null) => void;
  beginOperation: (key: string) => number;
  finishOperation: (key: string, requestId: number) => void;
  failOperation: (key: string, requestId: number, error: WorkflowOperationError) => void;
  clearOperationError: (key: string) => void;
}

const initialState = {
  updateDraft: { mode: "changed_sources", selection: { kind: "automatic" }, routeSelection: null } as UpdateWikiDraft,
  projectKey: "",
  identityGuard: {
    canonicalIdentityKey: null,
    identityRevision: null,
  } as WorkflowIdentityGuard,
  overview: null,
  overviewStatus: "idle" as WorkflowOverviewStatus,
  runs: [] as WorkflowRun[],
  detailRevisionById: {} as Record<string, string | undefined>,
  historyRuns: [] as WorkflowRunSummary[],
  retryOfTaskId: null,
  preparation: null,
  preparingKind: null,
  preparations: {} as WorkflowState["preparations"],
  preparedRouteSelections: {} as WorkflowState["preparedRouteSelections"],
  drafts: {} as WorkflowState["drafts"],
  selectedTaskId: null,
  surface: "overview" as WorkflowsSurface,
  historyKind: null as WorkflowKind | null,
  historyStatus: null as WorkflowDisplayStatus | null,
  historyCursor: null as string | null,
  operations: {} as Record<string, WorkflowOperationState>,
  operationSequence: 0,
  requestEpoch: 0,
};

let workflowOperationSequence = 0;

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  ...initialState,
  setUpdateDraft: (updateDraft) => set({ updateDraft }),
  activateProject: (projectKey) => {
    const requestEpoch = get().requestEpoch + 1;
    set({ ...initialState, projectKey, requestEpoch });
    return requestEpoch;
  },
  reset: () => set((state) => ({ ...initialState, requestEpoch: state.requestEpoch + 1 })),
  setOverviewSnapshot: (snapshot) =>
    set((state) => {
      recordWorkflowFacts([...(snapshot.recentRuns ?? []), ...(snapshot.activeRuns ?? [])], true, snapshot.sessionId);
      const overview = mergeWorkflowOverview(snapshot, Object.values(useTaskStore.getState().workflowById));
      const identityChanged = workflowIdentityChanged(state.overview, overview)
        || Boolean(state.overview?.sessionId && overview.sessionId && state.overview.sessionId !== overview.sessionId);
      const identityGuard = identityGuardOf(overview);
      return {
        overview,
        identityGuard,
        overviewStatus: "ready" as WorkflowOverviewStatus,
        ...(identityChanged
          ? {
              runs: [],
              detailRevisionById: {},
              historyRuns: [],
              historyCursor: null,
              retryOfTaskId: null,
              preparation: null,
              preparingKind: null,
              preparations: {},
              preparedRouteSelections: {},
              drafts: {},
              updateDraft: initialState.updateDraft,
              selectedTaskId: null,
              surface: "overview" as WorkflowsSurface,
              operations: {},
            }
          : {}),
      };
    }),
  applySummaries: (summaries) => set((state) => {
    const accepted = summaries.filter((run) => run.projectId === state.projectKey.split("\0")[0]
      && run.canonicalIdentityKey === state.identityGuard.canonicalIdentityKey
      && run.identityRevision === state.identityGuard.identityRevision);
    if (accepted.length === 0) return state;
    let changed = false;
    const runs = state.runs.map((run) => {
      const summary = accepted.find((value) => value.taskId === run.taskId);
      if (!summary || compareWorkflowRevision(summary, run) <= 0) return run;
      changed = true;
      return projectSummaryOntoDetail(run, summary);
    });
    const boundary = accepted.some((run) => run.displayStatus !== "running"
      || state.overview?.rows.some((row) => row.kind === run.kind && (row.activeTaskId !== run.taskId || row.state !== "running")));
    const historyRuns = state.historyRuns.map((run) => {
      const summary = accepted.find((value) => value.taskId === run.taskId);
      return summary && compareWorkflowRevision(summary, run) > 0 ? summary : run;
    });
    return { ...(changed ? { runs } : {}),
      ...(boundary && state.overview ? { overview: mergeWorkflowOverview(state.overview, Object.values(useTaskStore.getState().workflowById)), historyRuns } : {}),
    };
  }),
  setOverviewStatus: (overviewStatus) => set({ overviewStatus }),
  replaceRuns: (runs) =>
    set((state) => {
      let cached = state.runs;
      for (const run of runs) if (cached.some((detail) => detail.taskId === run.taskId)) cached = upsertSortedRun(cached, run);
      return { runs: cached };
    }),
  replaceHistoryPage: (runs, historyCursor) =>
    set(() => ({
      historyRuns: sortHistoryRuns(runs.map(currentSummary)),
      historyCursor,
    })),
  appendHistoryPage: (runs, historyCursor) =>
    set((state) => ({
      historyRuns: sortHistoryRuns(mergeHistorySnapshots(state.historyRuns, runs.map(currentSummary))),
      historyCursor,
    })),
  upsertRun: (run) =>
    set((state) => {
      const runs = upsertSortedRun(state.runs, run);
      if (runs === state.runs) return state;
      return { runs, detailRevisionById: Object.fromEntries(runs.map((detail) => [detail.taskId, detail.taskId === run.taskId ? run.revision : state.detailRevisionById[detail.taskId]])) };
    }),
  upsertRuns: (incoming) =>
    set((state) => {
      if (incoming.length === 0) return state;
      let runs = state.runs;
      const detailRevisionById = { ...state.detailRevisionById };
      for (const run of incoming) {
        const next = upsertSortedRun(runs, run);
        if (next !== runs) detailRevisionById[run.taskId] = run.revision;
        runs = next;
      }
      return runs === state.runs ? state : { runs, detailRevisionById: Object.fromEntries(runs.map((detail) => [detail.taskId, detailRevisionById[detail.taskId]])) };
    }),
  hydrateDecisionReview: (taskId, actionId, decisionReview) =>
    set((state) => {
      const current = state.runs.find((run) => run.taskId === taskId);
      if (
        !current
        || current.displayStatus !== "waiting_for_confirmation"
        || current.pendingAction?.id !== actionId
      ) return state;
      return {
        runs: state.runs.map((run) =>
          run.taskId === taskId ? { ...run, decisionReview } : run,
        ),
      };
    }),
  setDraft: (kind, draft) => set((state) => ({ drafts: { ...state.drafts, [kind]: draft } })),
  beginPreparation: (kind) => set((state) => ({
    preparation: state.preparations[kind] ?? null,
    preparingKind: kind,
    selectedTaskId: null,
    surface: "preparation",
  })),
  setPreparation: (preparation, routeSelection = null) => set((state) => preparation
    ? {
        preparation,
        preparingKind: null,
        preparations: { ...state.preparations, [preparation.kind]: preparation },
        preparedRouteSelections: { ...state.preparedRouteSelections, [preparation.kind]: routeSelection },
        retryOfTaskId: null,
        selectedTaskId: null,
        surface: "preparation",
      }
    : { preparation: null, preparingKind: null }),
  selectRun: (selectedTaskId) =>
    set((state) => {
      if (selectedTaskId && !state.runs.some((run) => run.taskId === selectedTaskId)) {
        return state;
      }
      return selectedTaskId
        ? { selectedTaskId, preparation: null, preparingKind: null, surface: "detail" }
        : { selectedTaskId: null, surface: "overview" };
    }),
  setSurface: (surface) => set((state) => {
    if (surface === "detail") {
      return state.selectedTaskId && state.runs.some((run) => run.taskId === state.selectedTaskId)
        ? { surface }
        : state;
    }
    if (surface === "preparation") {
      return state.preparation
        ? { surface, selectedTaskId: null }
        : state;
    }
    return {
      surface,
      selectedTaskId: null,
      preparation: null,
      preparingKind: null,
    };
  }),
  setHistoryFilters: (historyKind, historyStatus) =>
    set({ historyKind, historyStatus, historyRuns: [], historyCursor: null }),
  setHistoryCursor: (historyCursor) => set({ historyCursor }),
  beginOperation: (key) => {
    const requestId = ++workflowOperationSequence;
    set((state) => ({
      operationSequence: requestId,
      operations: {
        ...state.operations,
        [key]: { requestId, pending: true, error: null },
      },
    }));
    return requestId;
  },
  finishOperation: (key, requestId) => set((state) => {
    const operation = state.operations[key];
    if (!operation || operation.requestId !== requestId || !operation.pending) return state;
    return {
      operations: {
        ...state.operations,
        [key]: { ...operation, pending: false },
      },
    };
  }),
  failOperation: (key, requestId, error) => set((state) => {
    const operation = state.operations[key];
    if (!operation || operation.requestId !== requestId) return state;
    return {
      operations: {
        ...state.operations,
        [key]: { ...operation, pending: false, error },
      },
    };
  }),
  clearOperationError: (key) => set((state) => {
    const operation = state.operations[key];
    if (!operation?.error) return state;
    return {
      operations: {
        ...state.operations,
        [key]: { ...operation, error: null },
      },
    };
  }),
}));

function identityGuardOf(overview: WorkflowsOverview): WorkflowIdentityGuard {
  return {
    canonicalIdentityKey: overview.projectAccess?.canonicalIdentityKey ?? null,
    identityRevision: overview.projectAccess?.identityRevision ?? null,
  };
}

function workflowIdentityChanged(
  previous: WorkflowsOverview | null,
  next: WorkflowsOverview,
): boolean {
  if (!previous) return false;
  return previous.projectAccess?.canonicalIdentityKey !== next.projectAccess?.canonicalIdentityKey
    || previous.projectAccess?.identityRevision !== next.projectAccess?.identityRevision;
}

function sortHistoryRuns(runs: WorkflowRunSummary[]): WorkflowRunSummary[] {
  return [...runs].sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt));
}

function currentSummary(run: WorkflowRunSummary): WorkflowRunSummary {
  const current = useTaskStore.getState().workflowById[run.taskId];
  if (!current || current.canonicalIdentityKey !== run.canonicalIdentityKey
    || current.identityRevision !== run.identityRevision) return run;
  return current.sessionId !== run.sessionId || compareWorkflowRevision(current, run) >= 0 ? current : run;
}

function projectSummaryOntoDetail(run: WorkflowRun, summary: WorkflowRunSummary): WorkflowRun {
  return { ...run, revision: summary.revision, sessionId: summary.sessionId,
    displayStatus: summary.displayStatus, updatedAt: summary.updatedAt, completedAt: summary.completedAt,
    currentStageId: summary.currentStageId === undefined ? run.currentStageId : summary.currentStageId,
    stages: summary.stages ?? (summary.currentStage ? run.stages.map((stage) => stage.id === summary.currentStage?.id ? summary.currentStage : stage) : run.stages),
    queuePosition: summary.queuePosition ?? null, continuationRequired: summary.continuationRequired ?? run.continuationRequired,
    cancellable: summary.cancellable ?? run.cancellable,
    pendingAction: null, decisionReview: null, result: null, error: null,
  };
}

function mergeHistorySnapshots(
  current: WorkflowRunSummary[],
  incoming: WorkflowRunSummary[],
): WorkflowRunSummary[] {
  const merged = new Map(current.map((run) => [run.taskId, run]));
  for (const run of incoming) merged.set(run.taskId, run);
  return [...merged.values()];
}

function upsertSortedRun(current: WorkflowRun[], incoming: WorkflowRun): WorkflowRun[] {
  const previousIndex = current.findIndex((candidate) => candidate.taskId === incoming.taskId);
  const previous = previousIndex >= 0 ? current[previousIndex] : undefined;
  if (previous && !shouldAcceptWorkflowRun(previous, incoming)) return current;
  const fact = useTaskStore.getState().workflowById[incoming.taskId];
  const retired = useTaskStore.getState().retiredWorkflowSessions;
  if (incoming.sessionId && retired.includes(incoming.sessionId)) return current;
  const accepted = fact && compareWorkflowRevision(fact, incoming) > 0
    ? projectSummaryOntoDetail(incoming, fact)
    : preserveHydratedDecisionReview(previous, incoming);
  // Detail cache eviction follows access, not task time. An old history item
  // must survive the very request that opens it, alongside the selected task.
  const selected = useWorkflowStore.getState().selectedTaskId;
  const retained = current.filter((run) => run.taskId !== accepted.taskId);
  const pinned = selected ? retained.find((run) => run.taskId === selected) : undefined;
  return [accepted, ...(pinned ? [pinned] : []), ...retained.filter((run) => run !== pinned)].slice(0, 16);
}

const TERMINAL_WORKFLOW_STATUSES = new Set<WorkflowRun["displayStatus"]>([
  "completed",
  "failed",
  "cancelled",
  "interrupted",
]);

function shouldAcceptWorkflowRun(previous: WorkflowRun, incoming: WorkflowRun): boolean {
  const taskState = useTaskStore.getState();
  if (incoming.sessionId && taskState.retiredWorkflowSessions.includes(incoming.sessionId)) return false;
  if (incoming.sessionId && previous.sessionId && incoming.sessionId !== previous.sessionId) {
    return incoming.sessionId === taskState.workflowSessionId;
  }
  if (
    incoming.revision === undefined
    && TERMINAL_WORKFLOW_STATUSES.has(previous.displayStatus)
    && !TERMINAL_WORKFLOW_STATUSES.has(incoming.displayStatus)
  ) return false;
  return compareWorkflowRevision(incoming, previous) >= 0;
}

function preserveHydratedDecisionReview(
  previous: WorkflowRun | undefined,
  incoming: WorkflowRun,
): WorkflowRun {
  if (incoming.decisionReview || !previous?.decisionReview) return incoming;
  if (
    incoming.displayStatus !== "waiting_for_confirmation"
    || !incoming.pendingAction
    || previous.pendingAction?.id !== incoming.pendingAction.id
  ) {
    return incoming;
  }
  return { ...incoming, decisionReview: previous.decisionReview };
}

export function captureWorkflowRequestGuard(
  state: Pick<WorkflowState, "projectKey" | "requestEpoch" | "identityGuard"> = useWorkflowStore.getState(),
): WorkflowRequestGuard {
  return {
    projectKey: state.projectKey,
    requestEpoch: state.requestEpoch,
    canonicalIdentityKey: state.identityGuard.canonicalIdentityKey,
    identityRevision: state.identityGuard.identityRevision,
  };
}

export function workflowRequestGuardMatches(
  guard: WorkflowRequestGuard,
  state: Pick<WorkflowState, "projectKey" | "requestEpoch" | "identityGuard"> = useWorkflowStore.getState(),
): boolean {
  return state.projectKey === guard.projectKey
    && state.requestEpoch === guard.requestEpoch
    && state.identityGuard.canonicalIdentityKey === guard.canonicalIdentityKey
    && state.identityGuard.identityRevision === guard.identityRevision;
}

export function workflowRunMatchesGuard(
  run: Pick<WorkflowRun, "projectId" | "canonicalIdentityKey" | "identityRevision">,
  projectId: string,
  guard: WorkflowRequestGuard,
): boolean {
  return run.projectId === projectId
    && run.canonicalIdentityKey === guard.canonicalIdentityKey
    && run.identityRevision === guard.identityRevision;
}

export function workflowOperationPending(
  operations: Record<string, WorkflowOperationState>,
  keyOrPrefix: string,
): boolean {
  return Object.entries(operations).some(([key, operation]) =>
    operation.pending && (key === keyOrPrefix || key.startsWith(keyOrPrefix)),
  );
}

export function selectWorkflowRun(taskId: string | null): WorkflowRun | null {
  if (!taskId) return null;
  return useWorkflowStore.getState().runs.find((run) => run.taskId === taskId) ?? null;
}

export function recommendedWorkflowKind(overview: WorkflowsOverview | null): WorkflowKind | null {
  return overview?.rows.find((row) => row.recommended)?.kind ?? null;
}

registerProjectScopeResetHandler("workflows", () => useWorkflowStore.getState().reset());

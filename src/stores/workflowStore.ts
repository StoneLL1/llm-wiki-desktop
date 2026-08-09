import { create } from "zustand";

import type {
  WorkflowDecisionReview,
  WorkflowDisplayStatus,
  WorkflowKind,
  WorkflowPreparation,
  WorkflowRun,
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
  projectKey: string;
  identityGuard: WorkflowIdentityGuard;
  overview: WorkflowsOverview | null;
  overviewStatus: WorkflowOverviewStatus;
  runs: WorkflowRun[];
  preparation: WorkflowPreparation | null;
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
  setProjectSnapshot: (
    overview: WorkflowsOverview,
    runs: WorkflowRun[],
    historyCursor: string | null,
  ) => void;
  setOverviewSnapshot: (overview: WorkflowsOverview) => void;
  setOverviewStatus: (status: WorkflowOverviewStatus) => void;
  replaceRuns: (runs: WorkflowRun[]) => void;
  upsertRun: (run: WorkflowRun) => void;
  upsertRuns: (runs: readonly WorkflowRun[]) => void;
  hydrateDecisionReview: (taskId: string, actionId: string, review: WorkflowDecisionReview) => void;
  setPreparation: (preparation: WorkflowPreparation | null) => void;
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
  projectKey: "",
  identityGuard: {
    canonicalIdentityKey: null,
    identityRevision: null,
  } as WorkflowIdentityGuard,
  overview: null,
  overviewStatus: "idle" as WorkflowOverviewStatus,
  runs: [] as WorkflowRun[],
  preparation: null,
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
  activateProject: (projectKey) => {
    const requestEpoch = get().requestEpoch + 1;
    set({ ...initialState, projectKey, requestEpoch });
    return requestEpoch;
  },
  reset: () => set((state) => ({ ...initialState, requestEpoch: state.requestEpoch + 1 })),
  setProjectSnapshot: (overview, runs, historyCursor) =>
    set((state) => {
      const identityChanged = workflowIdentityChanged(state.overview, overview);
      const identityGuard = identityGuardOf(overview);
      return {
        overview,
        identityGuard,
        overviewStatus: "ready" as WorkflowOverviewStatus,
        runs: sortRuns(mergeRunSnapshots(identityChanged ? [] : state.runs, runs)),
        historyCursor,
        ...(identityChanged
          ? {
              preparation: null,
              selectedTaskId: null,
              surface: "overview" as WorkflowsSurface,
              operations: {},
            }
          : {}),
      };
    }),
  setOverviewSnapshot: (overview) =>
    set((state) => {
      const identityChanged = workflowIdentityChanged(state.overview, overview);
      const identityGuard = identityGuardOf(overview);
      return {
        overview,
        identityGuard,
        overviewStatus: "ready" as WorkflowOverviewStatus,
        ...(identityChanged
          ? {
              runs: [],
              historyCursor: null,
              preparation: null,
              selectedTaskId: null,
              surface: "overview" as WorkflowsSurface,
              operations: {},
            }
          : {}),
      };
    }),
  setOverviewStatus: (overviewStatus) => set({ overviewStatus }),
  replaceRuns: (runs) =>
    set((state) => ({ runs: sortRuns(mergeRunSnapshots(state.runs, runs)) })),
  upsertRun: (run) =>
    set((state) => {
      const previous = state.runs.find((candidate) => candidate.taskId === run.taskId);
      if (previous && !shouldAcceptWorkflowRun(previous, run)) return state;
      return {
        runs: sortRuns([
          ...state.runs.filter((candidate) => candidate.taskId !== run.taskId),
          preserveHydratedDecisionReview(previous, run),
        ]),
      };
    }),
  upsertRuns: (incoming) =>
    set((state) => {
      if (incoming.length === 0) return state;
      const runs = mergeRunSnapshots(state.runs, [...incoming]);
      return { runs: sortRuns(runs) };
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
  setPreparation: (preparation) => set(preparation
    ? { preparation, selectedTaskId: null, surface: "preparation" }
    : { preparation: null }),
  selectRun: (selectedTaskId) =>
    set((state) => {
      if (selectedTaskId && !state.runs.some((run) => run.taskId === selectedTaskId)) {
        return state;
      }
      return selectedTaskId
        ? { selectedTaskId, preparation: null, surface: "detail" }
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
    };
  }),
  setHistoryFilters: (historyKind, historyStatus) =>
    set({ historyKind, historyStatus }),
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
    if (!operation || operation.requestId !== requestId) return state;
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

function sortRuns(runs: WorkflowRun[]): WorkflowRun[] {
  return [...runs].sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt));
}

function mergeRunSnapshots(current: WorkflowRun[], incoming: WorkflowRun[]): WorkflowRun[] {
  const merged = new Map(current.map((run) => [run.taskId, run]));
  for (const run of incoming) {
    const previous = merged.get(run.taskId);
    if (!previous || shouldAcceptWorkflowRun(previous, run)) {
      merged.set(run.taskId, preserveHydratedDecisionReview(previous, run));
    }
  }
  return [...merged.values()];
}

const TERMINAL_WORKFLOW_STATUSES = new Set<WorkflowRun["displayStatus"]>([
  "completed",
  "failed",
  "cancelled",
  "interrupted",
]);

function shouldAcceptWorkflowRun(previous: WorkflowRun, incoming: WorkflowRun): boolean {
  if (
    TERMINAL_WORKFLOW_STATUSES.has(previous.displayStatus)
    && !TERMINAL_WORKFLOW_STATUSES.has(incoming.displayStatus)
  ) return false;
  return Date.parse(incoming.updatedAt) >= Date.parse(previous.updatedAt);
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
  run: WorkflowRun,
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

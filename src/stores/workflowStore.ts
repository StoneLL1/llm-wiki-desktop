import { create } from "zustand";

import type {
  WorkflowDisplayStatus,
  WorkflowKind,
  WorkflowPreparation,
  WorkflowRun,
  WorkflowsOverview,
} from "../types/workflow";

export type WorkflowsSurface = "overview" | "preparation" | "detail" | "history";

interface WorkflowState {
  projectKey: string;
  overview: WorkflowsOverview | null;
  runs: WorkflowRun[];
  preparation: WorkflowPreparation | null;
  selectedTaskId: string | null;
  surface: WorkflowsSurface;
  historyKind: WorkflowKind | null;
  historyStatus: WorkflowDisplayStatus | null;
  historyCursor: string | null;
  loading: boolean;
  error: string | null;
  requestEpoch: number;
  activateProject: (projectKey: string) => number;
  reset: () => void;
  setProjectSnapshot: (
    overview: WorkflowsOverview,
    runs: WorkflowRun[],
    historyCursor: string | null,
  ) => void;
  setOverview: (overview: WorkflowsOverview | null) => void;
  replaceRuns: (runs: WorkflowRun[]) => void;
  upsertRun: (run: WorkflowRun) => void;
  setPreparation: (preparation: WorkflowPreparation | null) => void;
  selectRun: (taskId: string | null) => void;
  setSurface: (surface: WorkflowsSurface) => void;
  setHistoryFilters: (kind: WorkflowKind | null, status: WorkflowDisplayStatus | null) => void;
  setHistoryCursor: (cursor: string | null) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
}

const initialState = {
  projectKey: "",
  overview: null,
  runs: [] as WorkflowRun[],
  preparation: null,
  selectedTaskId: null,
  surface: "overview" as WorkflowsSurface,
  historyKind: null as WorkflowKind | null,
  historyStatus: null as WorkflowDisplayStatus | null,
  historyCursor: null as string | null,
  loading: false,
  error: null as string | null,
  requestEpoch: 0,
};

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
      const previousAccess = state.overview?.projectAccess;
      const nextAccess = overview.projectAccess;
      const identityChanged = Boolean(
        previousAccess
          && nextAccess
          && (previousAccess.canonicalIdentityKey !== nextAccess.canonicalIdentityKey
            || previousAccess.identityRevision !== nextAccess.identityRevision),
      );
      return {
        overview,
        runs: sortRuns(mergeRunSnapshots(identityChanged ? [] : state.runs, runs)),
        historyCursor,
        ...(identityChanged
          ? {
              preparation: null,
              selectedTaskId: null,
              surface: "overview" as WorkflowsSurface,
            }
          : {}),
      };
    }),
  setOverview: (overview) => set({ overview }),
  replaceRuns: (runs) =>
    set((state) => ({ runs: sortRuns(mergeRunSnapshots(state.runs, runs)) })),
  upsertRun: (run) =>
    set((state) => {
      const previous = state.runs.find((candidate) => candidate.taskId === run.taskId);
      if (previous && Date.parse(previous.updatedAt) > Date.parse(run.updatedAt)) return state;
      return {
        runs: sortRuns([
          ...state.runs.filter((candidate) => candidate.taskId !== run.taskId),
          preserveHydratedDecisionReview(previous, run),
        ]),
      };
    }),
  setPreparation: (preparation) => set({ preparation }),
  selectRun: (selectedTaskId) =>
    set({ selectedTaskId, surface: selectedTaskId ? "detail" : "overview" }),
  setSurface: (surface) => set({ surface }),
  setHistoryFilters: (historyKind, historyStatus) =>
    set({ historyKind, historyStatus }),
  setHistoryCursor: (historyCursor) => set({ historyCursor }),
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
}));

function sortRuns(runs: WorkflowRun[]): WorkflowRun[] {
  return [...runs].sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt));
}

function mergeRunSnapshots(current: WorkflowRun[], incoming: WorkflowRun[]): WorkflowRun[] {
  const merged = new Map(current.map((run) => [run.taskId, run]));
  for (const run of incoming) {
    const previous = merged.get(run.taskId);
    if (!previous || Date.parse(run.updatedAt) >= Date.parse(previous.updatedAt)) {
      merged.set(run.taskId, preserveHydratedDecisionReview(previous, run));
    }
  }
  return [...merged.values()];
}

function preserveHydratedDecisionReview(
  previous: WorkflowRun | undefined,
  incoming: WorkflowRun,
): WorkflowRun {
  if (incoming.decisionReview || !previous?.decisionReview) return incoming;
  return { ...incoming, decisionReview: previous.decisionReview };
}

export function selectWorkflowRun(taskId: string | null): WorkflowRun | null {
  if (!taskId) return null;
  return useWorkflowStore.getState().runs.find((run) => run.taskId === taskId) ?? null;
}

export function recommendedWorkflowKind(overview: WorkflowsOverview | null): WorkflowKind | null {
  return overview?.rows.find((row) => row.recommended)?.kind ?? null;
}

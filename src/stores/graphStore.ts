import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  GraphBuildResult,
  GraphColorMode,
  GraphData,
  GraphStatus,
  SaveGraphLayoutRequest,
} from "../types/graph";
import { waitForTaskTerminal } from "../lib/waitForTaskTerminal";
import type { WikiPageType } from "../types/wiki";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";
import { useTaskStore } from "./taskStore";
import type { BackendTask } from "../types/task";

interface GraphState {
  data: GraphData | null;
  cached: boolean;
  layoutStale: boolean;
  status: GraphStatus;
  error: string | null;
  colorMode: GraphColorMode;
  selectedNodeId: string | null;
  focusedNodeId: string | null;
  search: string;
  /** Page types hidden from the canvas. Unchecked types are omitted here. */
  typeFilter: Set<WikiPageType>;
  /** Nodes with degree <= this threshold are hidden (0 hides nothing). */
  degreeThreshold: number;
  /**
   * Live action hooks registered by GraphView once its sigma renderer is ready.
   * The inspector (rendered in RightContextPanel) needs to trigger PNG export
   * and layout recompute, which both require the graphology instance that
   * lives in GraphView's refs — so GraphView publishes closures here.
   */
  exportPng: (() => void) | null;
  recomputeLayout: (() => void) | null;
  load: (projectId: string, rootPath: string) => Promise<void>;
  rebuild: (projectId: string, rootPath: string) => Promise<void>;
  saveLayout: (
    projectId: string,
    rootPath: string,
    positions: Record<string, [number, number]>,
    communities: Record<string, number>,
  ) => Promise<void>;
  setColorMode: (mode: GraphColorMode) => void;
  setSelectedNode: (id: string | null) => void;
  setFocusedNodeId: (nodeId: string | null) => void;
  clearFocus: () => void;
  setSearch: (query: string) => void;
  toggleTypeFilter: (type: WikiPageType) => void;
  setDegreeThreshold: (value: number) => void;
  registerActions: (actions: { exportPng: (() => void) | null; recomputeLayout: (() => void) | null }) => void;
  reset: () => void;
  projectKey: string | null;
}

const initial = {
  data: null as GraphData | null,
  cached: false,
  layoutStale: false,
  status: "idle" as GraphStatus,
  error: null as string | null,
  colorMode: "type" as GraphColorMode,
  selectedNodeId: null as string | null,
  focusedNodeId: null as string | null,
  search: "",
  typeFilter: new Set<WikiPageType>(),
  degreeThreshold: 0,
  exportPng: null,
  recomputeLayout: null,
  projectKey: null as string | null,
};

export const useGraphStore = create<GraphState>((set, get) => ({
  ...initial,
  load: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    const projectKey = createProjectKey(projectId, rootPath);
    const state = get();
    const sameProject = state.projectKey === projectKey;
    if (!sameProject) {
      set({
        data: null,
        cached: false,
        layoutStale: false,
        status: "loading",
        error: null,
        selectedNodeId: null,
        focusedNodeId: null,
        search: "",
        typeFilter: new Set<WikiPageType>(),
        degreeThreshold: 0,
        projectKey,
      });
    } else if (!state.data) {
      set({ status: "loading", error: null, projectKey });
    } else {
      set({ error: null, projectKey });
    }
    try {
      const result = await invoke<GraphBuildResult>("get_graph", {
        request: { projectId, projectRootPath: rootPath },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({
        data: result.data,
        cached: result.cached,
        layoutStale: result.layoutStale,
        status: graphStatusForData(result.data),
        error: null,
        projectKey,
      });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      if (errorCode(error) === "GRAPH_BUILD_REQUIRED") {
        try {
          await runGraphBuild(projectId, rootPath, scope, projectKey);
        } catch (buildError) {
          if (!isProjectScopeCurrent(scope)) return;
          set((state) => ({
            status: state.data ? graphStatusForData(state.data) : "error",
            error: errorMessage(buildError),
          }));
        }
      } else {
        set((state) => ({
          status: state.data ? graphStatusForData(state.data) : "error",
          error: errorMessage(error),
        }));
      }
    }
  },
  rebuild: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    const projectKey = createProjectKey(projectId, rootPath);
    set((state) => ({
      status: state.data ? "rebuilding" : "loading",
      error: null,
      projectKey,
    }));
    try {
      await runGraphBuild(projectId, rootPath, scope, projectKey);
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set((state) => ({
        status: state.data ? graphStatusForData(state.data) : "error",
        error: errorMessage(error),
      }));
    }
  },
  saveLayout: async (projectId, rootPath, positions, communities) => {
    const scope = captureProjectScope();
    const data = get().data;
    if (!data) return;
    const request: SaveGraphLayoutRequest = {
      projectId,
      projectRootPath: rootPath,
      contentHash: data.contentHash,
      positions,
      communities,
    };
    try {
      const updated = await invoke<GraphData | null>("save_graph_layout", { request });
      if (!isProjectScopeCurrent(scope)) return;
      if (updated) {
        set({ data: updated, layoutStale: false });
      }
    } catch {
      // Layout persistence is best-effort; a failure must not block the UI.
    }
  },
  setColorMode: (colorMode) => set({ colorMode }),
  setSelectedNode: (selectedNodeId) =>
    set((state) => ({
      selectedNodeId,
      focusedNodeId: selectedNodeId && selectedNodeId === state.selectedNodeId ? state.focusedNodeId : null,
    })),
  setFocusedNodeId: (focusedNodeId) => set({ focusedNodeId }),
  clearFocus: () => set({ focusedNodeId: null }),
  setSearch: (search) => set({ search }),
  toggleTypeFilter: (type) =>
    set((state) => {
      const next = new Set(state.typeFilter);
      if (next.has(type)) next.delete(type);
      else next.add(type);
      return { typeFilter: next };
    }),
  setDegreeThreshold: (degreeThreshold) => set({ degreeThreshold: Math.max(0, Math.floor(degreeThreshold)) }),
  registerActions: (actions) => set({ exportPng: actions.exportPng, recomputeLayout: actions.recomputeLayout }),
  reset: () => set({ ...initial, typeFilter: new Set<WikiPageType>() }),
}));

async function runGraphBuild(
  projectId: string,
  rootPath: string,
  scope: ReturnType<typeof captureProjectScope>,
  projectKey: string,
): Promise<void> {
  const task = await invoke<BackendTask>("build_graph", {
    request: { projectId, projectRootPath: rootPath },
  });
  useTaskStore.getState().upsertTask(task);
  useTaskStore.getState().openDrawer(task.id);
  const terminalTask = await waitForTaskTerminal(task);
  useTaskStore.getState().upsertTask(terminalTask);
  if (!isProjectScopeCurrent(scope)) return;
  if (terminalTask.status !== "succeeded") {
    useGraphStore.setState((state) => ({
      status: state.data ? graphStatusForData(state.data) : "error",
      error:
        terminalTask.status === "cancelled"
          ? "Graph build was cancelled."
          : terminalTask.error?.message ?? "Graph build failed.",
    }));
    return;
  }
  const result = await invoke<GraphBuildResult>("get_graph", {
    request: { projectId, projectRootPath: rootPath },
  });
  if (!isProjectScopeCurrent(scope)) return;
  useGraphStore.setState({
    data: result.data,
    cached: result.cached,
    layoutStale: result.layoutStale,
    status: graphStatusForData(result.data),
    error: null,
    projectKey,
  });
}

function graphStatusForData(data: GraphData): GraphStatus {
  return data.nodes.length === 0 ? "ready-empty" : "ready";
}

function createProjectKey(projectId: string, rootPath: string): string {
  return `${projectId}\u0000${rootPath}`;
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function errorCode(error: unknown): string | null {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code: unknown }).code;
    return typeof code === "string" ? code : null;
  }
  return null;
}

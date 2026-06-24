import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  GraphBuildResult,
  GraphColorMode,
  GraphData,
  GraphStatus,
  SaveGraphLayoutRequest,
} from "../types/graph";
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
  setSearch: (query: string) => void;
  toggleTypeFilter: (type: WikiPageType) => void;
  setDegreeThreshold: (value: number) => void;
  registerActions: (actions: { exportPng: (() => void) | null; recomputeLayout: (() => void) | null }) => void;
  reset: () => void;
}

const initial = {
  data: null as GraphData | null,
  cached: false,
  layoutStale: false,
  status: "idle" as GraphStatus,
  error: null as string | null,
  colorMode: "type" as GraphColorMode,
  selectedNodeId: null as string | null,
  search: "",
  typeFilter: new Set<WikiPageType>(),
  degreeThreshold: 0,
  exportPng: null,
  recomputeLayout: null,
};

export const useGraphStore = create<GraphState>((set, get) => ({
  ...initial,
  load: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    set({ status: "loading", error: null });
    try {
      const result = await invoke<GraphBuildResult>("get_graph", {
        request: { projectId, projectRootPath: rootPath },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({
        data: result.data,
        cached: result.cached,
        layoutStale: result.layoutStale,
        status: "ready",
      });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      if (errorCode(error) === "GRAPH_BUILD_REQUIRED") {
        try {
          await runGraphBuild(projectId, rootPath, scope, set);
        } catch (buildError) {
          if (!isProjectScopeCurrent(scope)) return;
          set({ status: "error", error: errorMessage(buildError) });
        }
      } else {
        set({ status: "error", error: errorMessage(error) });
      }
    }
  },
  rebuild: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    set({ status: "loading", error: null });
    try {
      await runGraphBuild(projectId, rootPath, scope, set);
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ status: "error", error: errorMessage(error) });
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
  setSelectedNode: (selectedNodeId) => set({ selectedNodeId }),
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
  set: (partial: Partial<GraphState>) => void,
): Promise<void> {
  let task = await invoke<BackendTask>("build_graph", {
    request: { projectId, projectRootPath: rootPath },
  });
  useTaskStore.getState().upsertTask(task);
  useTaskStore.getState().openDrawer(task.id);
  while (!isTerminalTask(task)) {
    await new Promise((resolve) => setTimeout(resolve, 250));
    task = await invoke<BackendTask>("get_task", { request: { taskId: task.id } });
    useTaskStore.getState().upsertTask(task);
  }
  if (!isProjectScopeCurrent(scope)) return;
  if (task.status !== "succeeded") {
    throw new Error(task.error?.message ?? `Graph build ${task.status}.`);
  }
  const result = await invoke<GraphBuildResult>("get_graph", {
    request: { projectId, projectRootPath: rootPath },
  });
  if (!isProjectScopeCurrent(scope)) return;
  set({
    data: result.data,
    cached: result.cached,
    layoutStale: result.layoutStale,
    status: "ready",
    error: null,
  });
}

function isTerminalTask(task: BackendTask): boolean {
  return task.status === "succeeded" || task.status === "failed" || task.status === "cancelled";
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

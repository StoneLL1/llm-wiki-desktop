import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  GraphBuildResult,
  GraphColorMode,
  GraphData,
  GraphStatus,
  SaveGraphLayoutRequest,
} from "../types/graph";

interface GraphState {
  data: GraphData | null;
  cached: boolean;
  layoutStale: boolean;
  status: GraphStatus;
  error: string | null;
  colorMode: GraphColorMode;
  selectedNodeId: string | null;
  search: string;
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
};

export const useGraphStore = create<GraphState>((set, get) => ({
  ...initial,
  load: async (projectId, rootPath) => {
    set({ status: "loading", error: null });
    try {
      const result = await invoke<GraphBuildResult>("get_graph", {
        request: { projectId, projectRootPath: rootPath },
      });
      set({
        data: result.data,
        cached: result.cached,
        layoutStale: result.layoutStale,
        status: "ready",
      });
    } catch (error) {
      set({ status: "error", error: errorMessage(error) });
    }
  },
  rebuild: async (projectId, rootPath) => {
    set({ status: "loading", error: null });
    try {
      const result = await invoke<GraphBuildResult>("build_graph", {
        request: { projectId, projectRootPath: rootPath },
      });
      set({
        data: result.data,
        cached: false,
        layoutStale: true,
        status: "ready",
      });
    } catch (error) {
      set({ status: "error", error: errorMessage(error) });
    }
  },
  saveLayout: async (projectId, rootPath, positions, communities) => {
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
  reset: () => set({ ...initial }),
}));

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

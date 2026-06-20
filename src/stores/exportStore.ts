import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  ExportRecord,
  ExportRoutePreference,
  ExportType,
  ListExportsRequest,
  OpenExportFolderRequest,
  ReadExportPreviewRequest,
  RegenerateExportRequest,
  StartExportRequest,
} from "../types/export";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

const ROUTE_PREFERENCE: ExportRoutePreference = "auto";

export interface ExportState {
  records: ExportRecord[];
  loading: boolean;
  /** taskId of the in-flight export, if any. */
  runningTaskId: string | null;
  previewHtml: string | null;
  previewId: string | null;
  selectedType: ExportType;
  sourcePath: string;
  error: string | null;

  loadExports: (projectId: string, rootPath: string) => Promise<void>;
  setSelectedType: (type: ExportType) => void;
  setSourcePath: (path: string) => void;
  startExport: (
    projectId: string,
    rootPath: string,
    type: ExportType,
    sourcePath: string,
  ) => Promise<string | null>;
  regenerateExport: (
    projectId: string,
    rootPath: string,
    record: ExportRecord,
  ) => Promise<string | null>;
  clearRunningTask: () => void;
  loadPreview: (request: ReadExportPreviewRequest, id: string) => Promise<void>;
  clearPreview: () => void;
  openFolder: (request: OpenExportFolderRequest) => Promise<void>;
  reset: () => void;
}

const initial = {
  records: [] as ExportRecord[],
  loading: false,
  runningTaskId: null as string | null,
  previewHtml: null as string | null,
  previewId: null as string | null,
  selectedType: "beautiful_read" as ExportType,
  sourcePath: "",
  error: null as string | null,
};

export const useExportStore = create<ExportState>((set) => ({
  ...initial,

  loadExports: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    set({ loading: true, error: null });
    try {
      const request: ListExportsRequest = { projectId, projectRootPath: rootPath };
      const records = await invoke<ExportRecord[]>("list_exports", { request });
      set({ records, loading: false });
    } catch (error) {
      set({ loading: false, error: errorMessage(error) });
    }
  },

  setSelectedType: (selectedType) => set({ selectedType }),
  setSourcePath: (sourcePath) => set({ sourcePath }),

  startExport: async (projectId, rootPath, type, sourcePath) => {
    if (!hasTauri()) return null;
    set({ error: null });
    const request: StartExportRequest = {
      projectId,
      projectRootPath: rootPath,
      exportType: type,
      sourcePath: sourcePath.trim() ? sourcePath.trim() : null,
      route: ROUTE_PREFERENCE,
      agent: null,
      provider: null,
    };
    try {
      const task = await invoke<{ id: string }>("start_export", { request });
      set({ runningTaskId: task.id });
      return task.id;
    } catch (error) {
      set({ error: errorMessage(error) });
      return null;
    }
  },

  regenerateExport: async (projectId, rootPath, record) => {
    if (!hasTauri()) return null;
    set({ error: null });
    const request: RegenerateExportRequest = {
      projectId,
      projectRootPath: rootPath,
      exportType: record.exportType,
      sourcePath: record.sourcePath ?? null,
      route: ROUTE_PREFERENCE,
      agent: null,
      provider: null,
    };
    try {
      const task = await invoke<{ id: string }>("regenerate_export", { request });
      set({ runningTaskId: task.id });
      return task.id;
    } catch (error) {
      set({ error: errorMessage(error) });
      return null;
    }
  },

  clearRunningTask: () => set({ runningTaskId: null }),

  loadPreview: async (request, id) => {
    if (!hasTauri()) return;
    set({ error: null });
    try {
      const html = await invoke<string>("read_export_preview", { request });
      set({ previewHtml: html, previewId: id });
    } catch (error) {
      set({ previewHtml: null, previewId: null, error: errorMessage(error) });
    }
  },

  clearPreview: () => set({ previewHtml: null, previewId: null }),

  openFolder: async (request) => {
    if (!hasTauri()) return;
    try {
      await invoke("open_export_folder", { request });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  reset: () => set({ ...initial }),
}));

// Re-exported for tests; mirrors lintStore's selectAllIssues helper shape.
export function selectHasRecords(state: ExportState): boolean {
  return state.records.length > 0;
}

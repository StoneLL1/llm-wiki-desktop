import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  ExportContentOptions,
  ExportRecord,
  ExportRoutePreference,
  ExportType,
  ListExportsRequest,
  OpenExportFolderRequest,
  ReadExportPreviewRequest,
  RegenerateExportRequest,
  StartExportRequest,
  ToggleExportBookmarkRequest,
  ToggleExportBookmarkResponse,
} from "../types/export";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";

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

/**
 * Caller-supplied export preferences. All optional so the bare 4-arg call
 * (WikiView quick export) keeps working with the prior "auto + no template"
 * behaviour.
 */
export interface ExportPrefs {
  route?: ExportRoutePreference;
  template?: string | null;
  options?: ExportContentOptions;
}

export interface ExportState {
  records: ExportRecord[];
  loading: boolean;
  /** taskId of the in-flight export, if any. */
  runningTaskId: string | null;
  previewHtml: string | null;
  previewId: string | null;
  error: string | null;

  loadExports: (projectId: string, rootPath: string) => Promise<void>;
  startExport: (
    projectId: string,
    rootPath: string,
    type: ExportType,
    sourcePath: string,
    prefs?: ExportPrefs,
  ) => Promise<string | null>;
  regenerateExport: (
    projectId: string,
    rootPath: string,
    record: ExportRecord,
    prefs?: ExportPrefs,
  ) => Promise<string | null>;
  clearRunningTask: () => void;
  loadPreview: (request: ReadExportPreviewRequest, id: string) => Promise<void>;
  clearPreview: () => void;
  toggleBookmark: (projectId: string, rootPath: string, recordId: string) => Promise<void>;
  openFolder: (request: OpenExportFolderRequest) => Promise<void>;
  reset: () => void;
}

const initial = {
  records: [] as ExportRecord[],
  loading: false,
  runningTaskId: null as string | null,
  previewHtml: null as string | null,
  previewId: null as string | null,
  error: null as string | null,
};

export const useExportStore = create<ExportState>((set) => ({
  ...initial,

  loadExports: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    set({ loading: true, error: null });
    try {
      const request: ListExportsRequest = { projectId, projectRootPath: rootPath };
      const records = await invoke<ExportRecord[]>("list_exports", { request });
      if (!isProjectScopeCurrent(scope)) return;
      set({ records, loading: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ loading: false, error: errorMessage(error) });
    }
  },

  startExport: async (projectId, rootPath, type, sourcePath, prefs) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
    set({ error: null });
    const request: StartExportRequest = {
      projectId,
      projectRootPath: rootPath,
      exportType: type,
      sourcePath: sourcePath.trim() ? sourcePath.trim() : null,
      route: prefs?.route ?? ROUTE_PREFERENCE,
      agent: null,
      provider: null,
      template: prefs?.template ?? null,
      options: prefs?.options,
    };
    try {
      const task = await invoke<{ id: string }>("start_export", { request });
      if (!isProjectScopeCurrent(scope)) return null;
      set({ runningTaskId: task.id });
      return task.id;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ error: errorMessage(error) });
      return null;
    }
  },

  regenerateExport: async (projectId, rootPath, record, prefs) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
    set({ error: null });
    const request: RegenerateExportRequest = {
      projectId,
      projectRootPath: rootPath,
      exportType: record.exportType,
      sourcePath: record.sourcePath ?? null,
      route: prefs?.route ?? ROUTE_PREFERENCE,
      agent: null,
      provider: null,
      template: prefs?.template ?? null,
      options: prefs?.options,
    };
    try {
      const task = await invoke<{ id: string }>("regenerate_export", { request });
      if (!isProjectScopeCurrent(scope)) return null;
      set({ runningTaskId: task.id });
      return task.id;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ error: errorMessage(error) });
      return null;
    }
  },

  clearRunningTask: () => set({ runningTaskId: null }),

  loadPreview: async (request, id) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    set({ error: null });
    try {
      const html = await invoke<string>("read_export_preview", { request });
      if (!isProjectScopeCurrent(scope)) return;
      set({ previewHtml: html, previewId: id });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ previewHtml: null, previewId: null, error: errorMessage(error) });
    }
  },

  clearPreview: () => set({ previewHtml: null, previewId: null }),

  toggleBookmark: async (projectId, rootPath, recordId) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    set({ error: null });
    const request: ToggleExportBookmarkRequest = {
      projectId,
      projectRootPath: rootPath,
      exportRecordId: recordId,
    };
    try {
      const response = await invoke<ToggleExportBookmarkResponse>(
        "toggle_export_bookmark",
        { request },
      );
      if (!isProjectScopeCurrent(scope)) return;
      set((state) => ({
        records: state.records.map((record) =>
          record.id === response.exportRecordId
            ? { ...record, bookmarked: response.bookmarked }
            : record,
        ),
      }));
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },

  openFolder: async (request) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    try {
      await invoke("open_export_folder", { request });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },

  reset: () => set({ ...initial }),
}));

// Re-exported for tests; mirrors lintStore's selectAllIssues helper shape.
export function selectHasRecords(state: ExportState): boolean {
  return state.records.length > 0;
}

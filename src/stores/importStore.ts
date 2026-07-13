import { create } from "zustand";

import type { ImportedSource, ImportPreview } from "../types/import";
import type { ImportItem, ImportSession } from "../types/importV2";

export type ImportQueueFilter =
  | "all"
  | "active"
  | "ready"
  | "needs_action"
  | "failed"
  | "completed";

export const importProjectKey = (projectId: string, rootPath: string): string =>
  `${projectId}\0${rootPath}`;

interface ImportState {
  projectKey: string | null;
  session: ImportSession | null;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  isBootstrapping: boolean;
  mutationKeys: ReadonlySet<string>;
  sessionEpoch: number;
  previewItemId: string | null;
  byokItemId: string | null;
  capabilityItemId: string | null;
  loginItemId: string | null;
  migrationDialogOpen: boolean;
  preview: ImportPreview | null;
  importedSources: ImportedSource[];
  isConfirming: boolean;
  selectedSourcePath: string | null;
  urlDialogOpen: boolean;
  folderDialogOpen: boolean;
  createCheckpoint: boolean;
  compileAfterImport: boolean;
  setPreview: (preview: ImportPreview | null) => void;
  setImportedSources: (sources: ImportedSource[]) => void;
  setIsConfirming: (confirming: boolean) => void;
  setSelectedSourcePath: (path: string | null) => void;
  setUrlDialogOpen: (open: boolean) => void;
  setFolderDialogOpen: (open: boolean) => void;
  setCreateCheckpoint: (create: boolean) => void;
  setCompileAfterImport: (compile: boolean) => void;
  attachSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  replaceSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  appendItems: (projectKey: string, items: ImportItem[], epoch?: number) => boolean;
  replaceItem: (projectKey: string, item: ImportItem, epoch?: number) => boolean;
  resetProjectPresentation: (projectKey: string) => void;
  beginSessionEpoch: (projectKey: string) => number;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
  setBootstrapping: (value: boolean) => void;
  beginMutation: (key: string) => void;
  endMutation: (key: string) => void;
  openPreview: (itemId: string) => void;
  closePreview: () => void;
  openByok: (itemId: string) => void;
  closeByok: () => void;
  openCapability: (itemId: string) => void;
  closeCapability: () => void;
  openLogin: (itemId: string) => void;
  closeLogin: () => void;
  setMigrationDialogOpen: (open: boolean) => void;
  reset: () => void;
}

function selectedItemIdFor(session: ImportSession | null, selectedItemId: string | null): string | null {
  if (!session) return null;
  if (selectedItemId && session.items.some((item) => item.itemId === selectedItemId)) {
    return selectedItemId;
  }
  return null;
}

function scopeAccepts(state: ImportState, projectKey: string, epoch?: number): boolean {
  if (state.projectKey === null && epoch === undefined) return true;
  if (state.projectKey !== projectKey) return false;
  return epoch === undefined || epoch === state.sessionEpoch;
}

function clearDialogs() {
  return {
    previewItemId: null,
    byokItemId: null,
    capabilityItemId: null,
    loginItemId: null,
    migrationDialogOpen: false,
  };
}

export const useImportStore = create<ImportState>((set, get) => ({
  projectKey: null,
  session: null,
  selectedItemId: null,
  filter: "all",
  isBootstrapping: false,
  mutationKeys: new Set<string>(),
  sessionEpoch: 0,
  previewItemId: null,
  byokItemId: null,
  capabilityItemId: null,
  loginItemId: null,
  migrationDialogOpen: false,
  preview: null,
  importedSources: [],
  isConfirming: false,
  selectedSourcePath: null,
  urlDialogOpen: false,
  folderDialogOpen: false,
  createCheckpoint: true,
  compileAfterImport: true,
  setPreview: (preview) =>
    set((current) => ({
      preview,
      selectedSourcePath:
        preview && current.selectedSourcePath
          ? (preview.files.find((f) => f.sourcePath === current.selectedSourcePath)
              ?.sourcePath ?? preview.files[0]?.sourcePath ?? null)
          : (preview?.files[0]?.sourcePath ?? null),
    })),
  setImportedSources: (importedSources) => set({ importedSources }),
  setIsConfirming: (isConfirming) => set({ isConfirming }),
  setSelectedSourcePath: (selectedSourcePath) => set({ selectedSourcePath }),
  setUrlDialogOpen: (urlDialogOpen) => set({ urlDialogOpen }),
  setFolderDialogOpen: (folderDialogOpen) => set({ folderDialogOpen }),
  setCreateCheckpoint: (createCheckpoint) => set({ createCheckpoint }),
  setCompileAfterImport: (compileAfterImport) => set({ compileAfterImport }),
  attachSession: (projectKey, session, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch)) return false;
    set({ projectKey, session, selectedItemId: selectedItemIdFor(session, state.selectedItemId) });
    return true;
  },
  replaceSession: (projectKey, session, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch)) return false;
    set({ projectKey, session, selectedItemId: selectedItemIdFor(session, state.selectedItemId) });
    return true;
  },
  appendItems: (projectKey, items, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session) return false;
    const existing = new Set(state.session.items.map((item) => item.itemId));
    const nextItems = items.filter((item) => !existing.has(item.itemId));
    if (nextItems.length === 0) return true;
    const session = { ...state.session, items: [...state.session.items, ...nextItems] };
    set({ session, selectedItemId: selectedItemIdFor(session, state.selectedItemId) });
    return true;
  },
  replaceItem: (projectKey, item, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session) return false;
    if (!state.session.items.some((current) => current.itemId === item.itemId)) return false;
    const session = {
      ...state.session,
      items: state.session.items.map((current) =>
        current.itemId === item.itemId ? item : current,
      ),
    };
    set({ session, selectedItemId: selectedItemIdFor(session, state.selectedItemId) });
    return true;
  },
  resetProjectPresentation: (projectKey) =>
    set((state) => ({
      projectKey,
      session: null,
      selectedItemId: null,
      filter: "all",
      isBootstrapping: false,
      mutationKeys: new Set<string>(),
      sessionEpoch: state.sessionEpoch + 1,
      ...clearDialogs(),
    })),
  beginSessionEpoch: (projectKey) => {
    const state = get();
    if (state.projectKey !== projectKey) return state.sessionEpoch;
    const sessionEpoch = state.sessionEpoch + 1;
    set({ sessionEpoch });
    return sessionEpoch;
  },
  selectItem: (selectedItemId) =>
    set((state) => ({ selectedItemId: selectedItemIdFor(state.session, selectedItemId) })),
  setFilter: (filter) => set({ filter }),
  setBootstrapping: (isBootstrapping) => set({ isBootstrapping }),
  beginMutation: (key) =>
    set((state) => {
      const mutationKeys = new Set(state.mutationKeys);
      mutationKeys.add(key);
      return { mutationKeys };
    }),
  endMutation: (key) =>
    set((state) => {
      const mutationKeys = new Set(state.mutationKeys);
      mutationKeys.delete(key);
      return { mutationKeys };
    }),
  openPreview: (previewItemId) => set({ previewItemId, selectedItemId: previewItemId }),
  closePreview: () => set({ previewItemId: null }),
  openByok: (byokItemId) => set({ byokItemId, selectedItemId: byokItemId }),
  closeByok: () => set({ byokItemId: null }),
  openCapability: (capabilityItemId) => set({ capabilityItemId, selectedItemId: capabilityItemId }),
  closeCapability: () => set({ capabilityItemId: null }),
  openLogin: (loginItemId) => set({ loginItemId, selectedItemId: loginItemId }),
  closeLogin: () => set({ loginItemId: null }),
  setMigrationDialogOpen: (migrationDialogOpen) => set({ migrationDialogOpen }),
  reset: () =>
    set({
      projectKey: null,
      session: null,
      selectedItemId: null,
      filter: "all",
      isBootstrapping: false,
      mutationKeys: new Set<string>(),
      sessionEpoch: 0,
      ...clearDialogs(),
      preview: null,
      selectedSourcePath: null,
      isConfirming: false,
      urlDialogOpen: false,
      folderDialogOpen: false,
    }),
}));

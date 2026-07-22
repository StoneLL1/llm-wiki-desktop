import { create } from "zustand";

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
  mutationKeys: ReadonlySet<string>;
  sessionEpoch: number;
  previewItemId: string | null;
  byokItemId: string | null;
  capabilityItemId: string | null;
  loginItemId: string | null;
  migrationDialogOpen: boolean;
  isConfirming: boolean;
  setIsConfirming: (confirming: boolean) => void;
  attachSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  replaceSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  replaceItem: (projectKey: string, item: ImportItem, epoch?: number) => boolean;
  resetProjectPresentation: (projectKey: string) => void;
  beginSessionEpoch: (projectKey: string) => number;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
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
  mutationKeys: new Set<string>(),
  sessionEpoch: 0,
  previewItemId: null,
  byokItemId: null,
  capabilityItemId: null,
  loginItemId: null,
  migrationDialogOpen: false,
  isConfirming: false,
  setIsConfirming: (isConfirming) => set({ isConfirming }),
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
      isConfirming: false,
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
      mutationKeys: new Set<string>(),
      sessionEpoch: 0,
      ...clearDialogs(),
      isConfirming: false,
    }),
}));

import { create } from "zustand";

import type { ImportCompletion, ImportItem, ImportSession } from "../types/importV2";

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
  completion: ImportCompletion | null;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  mutationKeys: ReadonlySet<string>;
  sessionEpoch: number;
  previewItemId: string | null;
  capabilityItemId: string | null;
  loginItemId: string | null;
  actionRequest: { itemId: string; action: string; requestId: number } | null;
  isConfirming: boolean;
  setIsConfirming: (confirming: boolean) => void;
  setCompletion: (
    projectKey: string,
    completion: ImportCompletion | null,
    epoch?: number,
  ) => boolean;
  attachSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  replaceSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  replaceItem: (projectKey: string, item: ImportItem, epoch?: number) => boolean;
  patchItems: (projectKey: string, items: readonly ImportItem[], epoch?: number) => boolean;
  resetProjectPresentation: (projectKey: string) => void;
  beginSessionEpoch: (projectKey: string) => number;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
  beginMutation: (key: string) => void;
  endMutation: (key: string) => void;
  openPreview: (itemId: string) => void;
  closePreview: () => void;
  openCapability: (itemId: string) => void;
  closeCapability: () => void;
  openLogin: (itemId: string) => void;
  closeLogin: () => void;
  requestAction: (itemId: string, action: string) => void;
  clearActionRequest: (requestId: number) => void;
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
    capabilityItemId: null,
    loginItemId: null,
  };
}

export const useImportStore = create<ImportState>((set, get) => ({
  projectKey: null,
  session: null,
  completion: null,
  selectedItemId: null,
  filter: "all",
  mutationKeys: new Set<string>(),
  sessionEpoch: 0,
  previewItemId: null,
  capabilityItemId: null,
  loginItemId: null,
  actionRequest: null,
  isConfirming: false,
  setIsConfirming: (isConfirming) => set({ isConfirming }),
  setCompletion: (projectKey, completion, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch)) return false;
    set({ completion });
    return true;
  },
  attachSession: (projectKey, session, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch)) return false;
    set({
      projectKey,
      session,
      completion:
        state.session?.sessionId === session.sessionId ? state.completion : null,
      selectedItemId: selectedItemIdFor(session, state.selectedItemId),
    });
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
  patchItems: (projectKey, items, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session) return false;
    if (items.length === 0) return true;
    const patches = new Map(items.map((item) => [item.itemId, item]));
    let changed = false;
    const session = {
      ...state.session,
      items: state.session.items.map((current) => {
        const patch = patches.get(current.itemId);
        if (!patch) return current;
        patches.delete(current.itemId);
        changed = true;
        return patch;
      }),
    };
    if (patches.size > 0) {
      session.items.push(...patches.values());
      changed = true;
    }
    if (!changed) return false;
    set({ session, selectedItemId: selectedItemIdFor(session, state.selectedItemId) });
    return true;
  },
  resetProjectPresentation: (projectKey) =>
    set((state) => ({
      projectKey,
      session: null,
      completion: null,
      selectedItemId: null,
      filter: "all",
      isConfirming: false,
      mutationKeys: new Set<string>(),
      sessionEpoch: state.sessionEpoch + 1,
      ...clearDialogs(),
      actionRequest: null,
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
  openCapability: (capabilityItemId) => set({ capabilityItemId, selectedItemId: capabilityItemId }),
  closeCapability: () => set({ capabilityItemId: null }),
  openLogin: (loginItemId) => set({ loginItemId, selectedItemId: loginItemId }),
  closeLogin: () => set({ loginItemId: null }),
  requestAction: (itemId, action) =>
    set((state) => ({
      actionRequest: {
        itemId,
        action,
        requestId: (state.actionRequest?.requestId ?? 0) + 1,
      },
    })),
  clearActionRequest: (requestId) =>
    set((state) => state.actionRequest?.requestId === requestId ? { actionRequest: null } : {}),
  reset: () =>
    set({
      projectKey: null,
      session: null,
      completion: null,
      selectedItemId: null,
      filter: "all",
      mutationKeys: new Set<string>(),
      sessionEpoch: 0,
      ...clearDialogs(),
      actionRequest: null,
      isConfirming: false,
    }),
}));

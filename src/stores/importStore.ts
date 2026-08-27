import { create } from "zustand";
import { registerProjectScopeResetHandler } from "./projectScopeResetRegistry";

import type {
  ImportCompletion,
  ImportItem,
  ImportItemPage,
  ImportSession,
  ImportSessionOverview,
} from "../types/importV2";

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
  overview: ImportSessionOverview | null;
  nextItemCursor: string | null;
  itemPageTotal: number;
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
  attachSessionWindow: (projectKey: string, overview: ImportSessionOverview, page: ImportItemPage, epoch?: number) => boolean;
  appendItemPage: (projectKey: string, page: ImportItemPage, epoch?: number) => boolean;
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

function acceptsItemRevision(current: ImportItem, incoming: ImportItem): boolean {
  return current.itemRevision === undefined
    || incoming.itemRevision === undefined
    || incoming.itemRevision > current.itemRevision;
}

function sessionFromWindow(overview: ImportSessionOverview, items: ImportItem[]): ImportSession {
  return {
    schemaVersion: overview.schemaVersion,
    sessionId: overview.sessionId,
    projectId: overview.projectId,
    status: overview.status,
    resourceMode: overview.resourceMode,
    createdAt: overview.createdAt,
    updatedAt: overview.updatedAt,
    discoveryTaskId: overview.discoveryTaskId,
    items,
  };
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
  overview: null,
  nextItemCursor: null,
  itemPageTotal: 0,
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
      overview: null,
      nextItemCursor: null,
      itemPageTotal: session.items.length,
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
        current.itemId === item.itemId && acceptsItemRevision(current, item) ? item : current,
      ),
    };
    set({ session, selectedItemId: selectedItemIdFor(session, state.selectedItemId) });
    return true;
  },
  attachSessionWindow: (projectKey, overview, page, epoch) => {
    const state = get();
    if (
      !scopeAccepts(state, projectKey, epoch)
      || overview.sessionId !== page.sessionId
      || overview.semanticRevision !== page.snapshotRevision
    ) return false;
    const session = sessionFromWindow(overview, page.items);
    set({
      projectKey,
      overview,
      session,
      nextItemCursor: page.nextCursor ?? null,
      itemPageTotal: page.total,
      completion: state.session?.sessionId === session.sessionId ? state.completion : null,
      selectedItemId: selectedItemIdFor(session, state.selectedItemId),
    });
    return true;
  },
  appendItemPage: (projectKey, page, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session || !state.overview) return false;
    if (page.sessionId !== state.session.sessionId || page.snapshotRevision !== state.overview.semanticRevision) return false;
    const known = new Set(state.session.items.map((item) => item.itemId));
    const session = {
      ...state.session,
      items: [...state.session.items, ...page.items.filter((item) => !known.has(item.itemId))],
    };
    set({
      session,
      nextItemCursor: page.nextCursor ?? null,
      itemPageTotal: page.total,
      selectedItemId: selectedItemIdFor(session, state.selectedItemId),
    });
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
        if (!acceptsItemRevision(current, patch)) return current;
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
      overview: null,
      nextItemCursor: null,
      itemPageTotal: 0,
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
      overview: null,
      nextItemCursor: null,
      itemPageTotal: 0,
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

registerProjectScopeResetHandler("import", () => {
  useImportStore.getState().resetProjectPresentation("");
  useImportStore.getState().reset();
});

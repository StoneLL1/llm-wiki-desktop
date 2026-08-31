import { create } from "zustand";

import type {
  ImportCompletion,
  ImportItem,
  ImportItemPage,
  ImportSelectionSummary,
  ImportSession,
  ImportSessionCounts,
  ImportSessionOverview,
  ImportSessionPatchCounts,
} from "../types/importV2";
import { registerProjectScopeResetHandler } from "./projectScopeResetRegistry";

export type ImportQueueFilter = "all" | "active" | "ready" | "needs_action" | "failed" | "completed";

export interface ImportQueueCounts {
  all: number;
  active: number;
  ready: number;
  needsAction: number;
  failed: number;
  completed: number;
  waiting?: number;
}

export interface ImportSessionProgress {
  completed: number;
  total: number;
  active: number;
  processed?: number;
  failed?: number;
  cancelled?: number;
  needsAction?: number;
}

const IMPORT_PAGE_LIMIT = 200;
const IMPORT_PAGE_WINDOW = 3;

export const importProjectKey = (projectId: string, rootPath: string): string => `${projectId}\0${rootPath}`;

interface ItemProjection {
  counts: ImportSessionCounts;
  selection: ImportSelectionSummary;
}

interface NormalizedSessionWindow {
  session: ImportSession;
  itemById: Record<string, ImportItem>;
  orderedItemIdsByPage: Record<string, readonly string[]>;
  loadedPages: readonly string[];
  loadedItemStartIndex: number;
  knownItemIds: ReadonlySet<string>;
  itemIdsByTaskId: Record<string, readonly string[]>;
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
}

interface ImportState {
  projectKey: string | null;
  /** Session metadata plus the bounded compatibility item window. */
  session: ImportSession | null;
  overview: ImportSessionOverview | null;
  itemById: Record<string, ImportItem>;
  orderedItemIdsByPage: Record<string, readonly string[]>;
  loadedPages: readonly string[];
  loadedItemStartIndex: number;
  knownItemIds: ReadonlySet<string>;
  itemIdsByTaskId: Record<string, readonly string[]>;
  operationCountsByBatchId: Record<string, ImportSessionPatchCounts>;
  operationFailedItemIdsByBatchId: Record<string, readonly string[]>;
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
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
  setCompletion: (projectKey: string, completion: ImportCompletion | null, epoch?: number) => boolean;
  attachSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  attachSessionWindow: (projectKey: string, overview: ImportSessionOverview, page: ImportItemPage, epoch?: number) => boolean;
  appendItemPage: (projectKey: string, page: ImportItemPage, epoch?: number) => boolean;
  replaceSession: (projectKey: string, session: ImportSession, epoch?: number) => boolean;
  replaceItem: (projectKey: string, item: ImportItem, epoch?: number) => boolean;
  replaceItemLocal: (projectKey: string, item: ImportItem, epoch?: number) => boolean;
  patchItems: (
    projectKey: string,
    items: readonly ImportItem[],
    epoch?: number,
  ) => boolean;
  recordOperationCounts: (projectKey: string, batchId: string, counts: ImportSessionPatchCounts, items?: readonly ImportItem[], epoch?: number) => boolean;
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

const emptyQueueCounts = (): ImportQueueCounts => ({
  all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0, waiting: 0,
});

const emptyProgress = (): ImportSessionProgress => ({
  completed: 0, total: 0, active: 0, processed: 0, failed: 0, cancelled: 0, needsAction: 0,
});

const emptySessionCounts = (): ImportSessionCounts => ({
  all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0, waiting: 0, processed: 0, cancelled: 0,
});

const emptySelection = (): ImportSelectionSummary => ({
  selected: 0, newSources: 0, updates: 0, warnings: 0, pending: 0, restricted: 0,
});

function resolvedMerge(item: ImportItem): boolean {
  return item.status === "needs_merge"
    && item.preview?.resolution?.kind === "needs_three_way_merge"
    && Boolean(item.preview.resolution.defaultResolution);
}

function exactDuplicate(item: ImportItem): boolean {
  return item.preview?.resolution?.kind === "exact_duplicate";
}

function itemIsCommittable(item: ImportItem): boolean {
  if (!item.selected || !item.preview || item.preview.quality.level === "fail") return false;
  return (item.status === "preview_ready" && (!exactDuplicate(item) || Boolean(item.restrictedContent)))
    || resolvedMerge(item);
}

function projectItem(item: ImportItem): ItemProjection {
  const counts = emptySessionCounts();
  const selection = emptySelection();
  const active = ["queued", "inspecting", "extracting", "validating", "committing"].includes(item.status);
  const ready = (item.status === "preview_ready" && (!exactDuplicate(item) || Boolean(item.restrictedContent))) || resolvedMerge(item);
  const needsAction = ["waiting_capability", "waiting_login", "waiting_authorization", "paused"].includes(item.status)
    || (item.status === "needs_merge" && !resolvedMerge(item));
  const completed = item.status === "completed";
  const skipped = item.status === "skipped";
  const failed = item.status === "failed";
  const cancelled = item.status === "cancelled";
  const processed = ["preview_ready", "needs_merge", "completed", "failed", "cancelled", "skipped"].includes(item.status);
  const waiting = ["waiting_capability", "waiting_login", "waiting_authorization"].includes(item.status);
  const committable = itemIsCommittable(item);
  const resolutionKind = item.preview?.resolution?.kind;
  counts.all = Number(!completed && !skipped);
  counts.active = Number(active);
  counts.ready = Number(ready && !completed && !skipped);
  counts.needsAction = Number(needsAction && !completed && !skipped);
  counts.failed = Number(failed);
  counts.completed = Number(completed);
  counts.waiting = Number(waiting);
  counts.processed = Number(processed);
  counts.cancelled = Number(cancelled);
  selection.selected = Number(committable);
  selection.newSources = Number(committable && resolutionKind === "new_source");
  selection.updates = Number(committable && (resolutionKind === "same_source_new_version" || resolutionKind === "needs_three_way_merge"));
  selection.warnings = Number(committable && item.preview?.quality.level === "warning");
  selection.pending = Number(needsAction || failed);
  selection.restricted = Number(committable && Boolean(item.restrictedContent));
  return { counts, selection };
}

function applyNumberDelta(value: number, before: number, after: number): number {
  return Math.max(0, value - before + after);
}

function applyProjectionDelta(
  counts: ImportQueueCounts,
  progress: ImportSessionProgress,
  overview: ImportSessionOverview | null,
  before: ItemProjection,
  after: ItemProjection,
) {
  const countsUnchanged = (Object.keys(before.counts) as Array<keyof ImportSessionCounts>)
    .every((key) => before.counts[key] === after.counts[key]);
  const selectionUnchanged = (Object.keys(before.selection) as Array<keyof ImportSelectionSummary>)
    .every((key) => before.selection[key] === after.selection[key]);
  if (countsUnchanged && selectionUnchanged) return { counts, progress, overview };
  const nextCounts: ImportQueueCounts = {
    all: applyNumberDelta(counts.all, before.counts.all, after.counts.all),
    active: applyNumberDelta(counts.active, before.counts.active, after.counts.active),
    ready: applyNumberDelta(counts.ready, before.counts.ready, after.counts.ready),
    needsAction: applyNumberDelta(counts.needsAction, before.counts.needsAction, after.counts.needsAction),
    failed: applyNumberDelta(counts.failed, before.counts.failed, after.counts.failed),
    completed: applyNumberDelta(counts.completed, before.counts.completed, after.counts.completed),
    waiting: applyNumberDelta(counts.waiting ?? 0, before.counts.waiting, after.counts.waiting),
  };
  const nextProgress: ImportSessionProgress = {
    ...progress,
    total: overview?.itemCount ?? progress.total,
    active: nextCounts.active,
    completed: nextCounts.completed,
    processed: applyNumberDelta(progress.processed ?? 0, before.counts.processed, after.counts.processed),
    failed: nextCounts.failed,
    cancelled: applyNumberDelta(progress.cancelled ?? 0, before.counts.cancelled, after.counts.cancelled),
    needsAction: nextCounts.needsAction,
  };
  if (!overview) return { counts: nextCounts, progress: nextProgress, overview };
  const nextOverview: ImportSessionOverview = {
    ...overview,
    counts: {
      all: nextCounts.all,
      active: nextCounts.active,
      ready: nextCounts.ready,
      needsAction: nextCounts.needsAction,
      failed: nextCounts.failed,
      completed: nextCounts.completed,
      waiting: nextCounts.waiting ?? 0,
      processed: nextProgress.processed ?? 0,
      cancelled: nextProgress.cancelled ?? 0,
    },
    selection: {
      selected: applyNumberDelta(overview.selection.selected, before.selection.selected, after.selection.selected),
      newSources: applyNumberDelta(overview.selection.newSources, before.selection.newSources, after.selection.newSources),
      updates: applyNumberDelta(overview.selection.updates, before.selection.updates, after.selection.updates),
      warnings: applyNumberDelta(overview.selection.warnings, before.selection.warnings, after.selection.warnings),
      pending: applyNumberDelta(overview.selection.pending, before.selection.pending, after.selection.pending),
      restricted: applyNumberDelta(overview.selection.restricted, before.selection.restricted, after.selection.restricted),
    },
  };
  return { counts: nextCounts, progress: nextProgress, overview: nextOverview };
}

function queueCountsFromSessionCounts(counts: ImportSessionCounts): ImportQueueCounts {
  return {
    all: counts.all,
    active: counts.active,
    ready: counts.ready,
    needsAction: counts.needsAction,
    failed: counts.failed,
    completed: counts.completed,
    waiting: counts.waiting,
  };
}

function progressFromSessionCounts(counts: ImportSessionCounts, total: number): ImportSessionProgress {
  return {
    completed: counts.completed,
    total,
    active: counts.active,
    processed: counts.processed,
    failed: counts.failed,
    cancelled: counts.cancelled,
    needsAction: counts.needsAction,
  };
}

function projectionsFor(items: readonly ImportItem[]): { counts: ImportSessionCounts; selection: ImportSelectionSummary } {
  const counts = emptySessionCounts();
  const selection = emptySelection();
  for (const item of items) {
    const projection = projectItem(item);
    for (const key of Object.keys(counts) as Array<keyof ImportSessionCounts>) counts[key] += projection.counts[key];
    for (const key of Object.keys(selection) as Array<keyof ImportSelectionSummary>) selection[key] += projection.selection[key];
  }
  return { counts, selection };
}

function itemMatchesFilter(item: ImportItem, filter: ImportQueueFilter): boolean {
  const projection = projectItem(item).counts;
  if (filter === "all") return projection.all === 1;
  if (filter === "active") return projection.active === 1;
  if (filter === "ready") return projection.ready === 1;
  if (filter === "needs_action") return projection.needsAction === 1;
  if (filter === "failed") return projection.failed === 1;
  return projection.completed === 1;
}

function indexTasks(items: readonly ImportItem[]): Record<string, readonly string[]> {
  const mutable: Record<string, string[]> = {};
  for (const item of items) {
    if (!item.taskId) continue;
    (mutable[item.taskId] ??= []).push(item.itemId);
  }
  return mutable;
}

function loadedIds(state: Pick<ImportState, "loadedPages" | "orderedItemIdsByPage">): string[] {
  return state.loadedPages.flatMap((pageKey) => state.orderedItemIdsByPage[pageKey] ?? []);
}

function sessionWithItems(session: ImportSession, items: ImportItem[]): ImportSession {
  return { ...session, items };
}

function normalizeFullSession(state: ImportState, session: ImportSession): NormalizedSessionWindow {
  const itemById = Object.fromEntries(session.items.map((item) => [item.itemId, item]));
  const knownItemIds = new Set(session.items.map((item) => item.itemId));
  const filteredIds = session.items.filter((item) => itemMatchesFilter(item, state.filter)).map((item) => item.itemId);
  const sameSession = state.session?.sessionId === session.sessionId;
  const currentIds = sameSession ? loadedIds(state).filter((itemId) => itemById[itemId]) : [];
  const newIds = sameSession ? filteredIds.filter((itemId) => !state.knownItemIds.has(itemId)) : [];
  const preferredIds = sameSession ? [...currentIds, ...newIds] : filteredIds;
  const distinct = [...new Set(preferredIds)];
  const boundedIds = sameSession && distinct.length > IMPORT_PAGE_LIMIT * IMPORT_PAGE_WINDOW
    ? distinct.slice(-IMPORT_PAGE_LIMIT * IMPORT_PAGE_WINDOW)
    : distinct.slice(0, IMPORT_PAGE_LIMIT * IMPORT_PAGE_WINDOW);
  const firstIndex = boundedIds.length > 0 ? Math.max(0, filteredIds.indexOf(boundedIds[0]!)) : 0;
  const orderedItemIdsByPage: Record<string, readonly string[]> = {};
  const pages: string[] = [];
  for (let offset = 0; offset < boundedIds.length; offset += IMPORT_PAGE_LIMIT) {
    const pageKey = `${state.filter}:compat:${firstIndex + offset}`;
    pages.push(pageKey);
    orderedItemIdsByPage[pageKey] = boundedIds.slice(offset, offset + IMPORT_PAGE_LIMIT);
  }
  const items = boundedIds.flatMap((itemId) => itemById[itemId] ? [itemById[itemId]!] : []);
  const aggregate = projectionsFor(session.items);
  return {
    session: sessionWithItems(session, items),
    itemById,
    orderedItemIdsByPage,
    loadedPages: pages,
    loadedItemStartIndex: firstIndex,
    knownItemIds,
    itemIdsByTaskId: indexTasks(session.items),
    counts: queueCountsFromSessionCounts(aggregate.counts),
    progress: progressFromSessionCounts(aggregate.counts, session.items.length),
  };
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

function selectedItemIdFor(state: Pick<ImportState, "knownItemIds" | "itemById">, selectedItemId: string | null): string | null {
  if (!selectedItemId) return null;
  return state.knownItemIds.has(selectedItemId) || Boolean(state.itemById[selectedItemId]) ? selectedItemId : null;
}

function acceptsItemRevision(current: ImportItem, incoming: ImportItem): boolean {
  return current.itemRevision === undefined || incoming.itemRevision === undefined || incoming.itemRevision > current.itemRevision;
}

function scopeAccepts(state: ImportState, projectKey: string, epoch?: number): boolean {
  if (state.projectKey === null && epoch === undefined) return true;
  if (state.projectKey !== projectKey) return false;
  return epoch === undefined || epoch === state.sessionEpoch;
}

function clearDialogs() {
  return { previewItemId: null, capabilityItemId: null, loginItemId: null };
}

function clearedNormalizedState() {
  return {
    session: null,
    overview: null,
    itemById: {},
    orderedItemIdsByPage: {},
    loadedPages: [],
    loadedItemStartIndex: 0,
    knownItemIds: new Set<string>(),
    itemIdsByTaskId: {},
    operationCountsByBatchId: {},
    operationFailedItemIdsByBatchId: {},
    counts: emptyQueueCounts(),
    progress: emptyProgress(),
    nextItemCursor: null,
    itemPageTotal: 0,
  };
}

export const useImportStore = create<ImportState>((set, get) => ({
  projectKey: null,
  ...clearedNormalizedState(),
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
    const normalized = normalizeFullSession(state, session);
    set({
      projectKey,
      ...normalized,
      overview: state.session?.sessionId === session.sessionId ? state.overview : null,
      operationCountsByBatchId: state.session?.sessionId === session.sessionId ? state.operationCountsByBatchId : {},
      operationFailedItemIdsByBatchId: state.session?.sessionId === session.sessionId ? state.operationFailedItemIdsByBatchId : {},
      nextItemCursor: null,
      itemPageTotal: normalized.counts.all,
      completion: state.session?.sessionId === session.sessionId ? state.completion : null,
      selectedItemId: selectedItemIdFor(normalized, state.selectedItemId),
    });
    return true;
  },
  replaceSession: (projectKey, session, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch)) return false;
    const normalized = normalizeFullSession(state, session);
    const aggregate = projectionsFor(Object.values(normalized.itemById));
    const overview = state.overview?.sessionId === session.sessionId ? {
      ...state.overview,
      status: session.status,
      updatedAt: session.updatedAt,
      itemCount: Object.keys(normalized.itemById).length,
      counts: aggregate.counts,
      selection: aggregate.selection,
    } : null;
    set({
      projectKey,
      ...normalized,
      overview,
      operationCountsByBatchId: state.session?.sessionId === session.sessionId ? state.operationCountsByBatchId : {},
      operationFailedItemIdsByBatchId: state.session?.sessionId === session.sessionId ? state.operationFailedItemIdsByBatchId : {},
      itemPageTotal: normalized.counts.all,
      selectedItemId: selectedItemIdFor(normalized, state.selectedItemId),
    });
    return true;
  },
  attachSessionWindow: (projectKey, overview, page, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || overview.sessionId !== page.sessionId || overview.semanticRevision !== page.snapshotRevision) return false;
    const pageKey = `${state.filter}:0`;
    const sameSession = state.session?.sessionId === overview.sessionId;
    const itemById = Object.fromEntries(page.items.map((item) => [item.itemId, item]));
    if (sameSession && state.selectedItemId && state.itemById[state.selectedItemId]) {
      itemById[state.selectedItemId] ??= state.itemById[state.selectedItemId]!;
    }
    const normalized = {
      itemById,
      orderedItemIdsByPage: { [pageKey]: page.items.map((item) => item.itemId) },
      loadedPages: [pageKey],
      loadedItemStartIndex: 0,
      knownItemIds: new Set([
        ...page.items.map((item) => item.itemId),
        ...(sameSession && state.selectedItemId && state.itemById[state.selectedItemId]
          ? [state.selectedItemId]
          : []),
      ]),
      itemIdsByTaskId: indexTasks(page.items),
    };
    const session = sessionFromWindow(overview, page.items);
    set({
      projectKey,
      overview,
      session,
      ...normalized,
      operationCountsByBatchId: state.session?.sessionId === session.sessionId ? state.operationCountsByBatchId : {},
      operationFailedItemIdsByBatchId: state.session?.sessionId === session.sessionId ? state.operationFailedItemIdsByBatchId : {},
      counts: queueCountsFromSessionCounts(overview.counts),
      progress: progressFromSessionCounts(overview.counts, overview.itemCount),
      nextItemCursor: page.nextCursor ?? null,
      itemPageTotal: page.total,
      completion: state.session?.sessionId === session.sessionId ? state.completion : null,
      selectedItemId: sameSession ? selectedItemIdFor(normalized, state.selectedItemId) : null,
    });
    return true;
  },
  appendItemPage: (projectKey, page, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session || !state.overview) return false;
    if (page.sessionId !== state.session.sessionId || page.snapshotRevision !== state.overview.semanticRevision) return false;
    const nextStart = state.loadedItemStartIndex + loadedIds(state).length;
    const pageKey = `${state.filter}:${nextStart}`;
    const orderedItemIdsByPage = { ...state.orderedItemIdsByPage, [pageKey]: page.items.map((item) => item.itemId) };
    const loadedPages = [...state.loadedPages.filter((key) => key !== pageKey), pageKey];
    let loadedItemStartIndex = state.loadedItemStartIndex;
    while (loadedPages.length > IMPORT_PAGE_WINDOW) {
      const evicted = loadedPages.shift();
      if (evicted) {
        loadedItemStartIndex += orderedItemIdsByPage[evicted]?.length ?? 0;
        delete orderedItemIdsByPage[evicted];
      }
    }
    const retainedIds = new Set(loadedPages.flatMap((key) => orderedItemIdsByPage[key] ?? []));
    const itemById: Record<string, ImportItem> = {};
    for (const itemId of retainedIds) {
      const existing = state.itemById[itemId];
      if (existing) itemById[itemId] = existing;
    }
    for (const item of page.items) {
      const existing = state.itemById[item.itemId];
      itemById[item.itemId] = existing && !acceptsItemRevision(existing, item) ? existing : item;
    }
    if (state.selectedItemId && state.itemById[state.selectedItemId]) itemById[state.selectedItemId] ??= state.itemById[state.selectedItemId]!;
    const knownItemIds = new Set(state.knownItemIds);
    for (const item of page.items) knownItemIds.add(item.itemId);
    const itemIdsByTaskId: Record<string, readonly string[]> = { ...state.itemIdsByTaskId };
    for (const item of page.items) {
      if (!item.taskId) continue;
      itemIdsByTaskId[item.taskId] = [...new Set([...(itemIdsByTaskId[item.taskId] ?? []), item.itemId])];
    }
    const items = loadedPages.flatMap((key) => orderedItemIdsByPage[key] ?? []).flatMap((itemId) => itemById[itemId] ? [itemById[itemId]!] : []);
    set({
      session: sessionWithItems(state.session, items),
      itemById,
      orderedItemIdsByPage,
      loadedPages,
      loadedItemStartIndex,
      knownItemIds,
      itemIdsByTaskId,
      nextItemCursor: page.nextCursor ?? null,
      itemPageTotal: page.total,
      selectedItemId: selectedItemIdFor({ knownItemIds, itemById }, state.selectedItemId),
    });
    return true;
  },
  replaceItem: (projectKey, item, epoch) => {
    return get().patchItems(projectKey, [item], epoch);
  },
  replaceItemLocal: (projectKey, item, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session || !state.itemById[item.itemId]) return false;
    const currentItem = state.itemById[item.itemId]!;
    const itemById = { ...state.itemById, [item.itemId]: item };
    let itemIdsByTaskId = state.itemIdsByTaskId;
    if (currentItem.taskId !== item.taskId) {
      itemIdsByTaskId = { ...itemIdsByTaskId };
      if (currentItem.taskId) {
        const remaining = (itemIdsByTaskId[currentItem.taskId] ?? []).filter((itemId) => itemId !== item.itemId);
        if (remaining.length === 0) delete itemIdsByTaskId[currentItem.taskId];
        else itemIdsByTaskId[currentItem.taskId] = remaining;
      }
      if (item.taskId) itemIdsByTaskId[item.taskId] = [...new Set([...(itemIdsByTaskId[item.taskId] ?? []), item.itemId])];
    }
    const sessionItems = state.session.items.map((candidate) => candidate.itemId === item.itemId ? item : candidate);
    const aggregate = applyProjectionDelta(state.counts, state.progress, state.overview, projectItem(currentItem), projectItem(item));
    set({ itemById, itemIdsByTaskId, session: sessionWithItems(state.session, sessionItems), ...aggregate });
    return true;
  },
  patchItems: (projectKey, items, epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch) || !state.session) return false;
    if (items.length === 0) return true;
    let itemById = state.itemById;
    let sessionItems = state.session.items;
    let counts = state.counts;
    let progress = state.progress;
    let overview = state.overview;
    let itemIdsByTaskId = state.itemIdsByTaskId;
    let orderedItemIdsByPage = state.orderedItemIdsByPage;
    let changed = false;
    let membershipChanged = false;
    let itemPageTotal = state.itemPageTotal;
    for (const item of items) {
      const current = itemById[item.itemId];
      if (!current || !acceptsItemRevision(current, item)) continue;
      if (!changed) {
        itemById = { ...itemById };
        itemIdsByTaskId = { ...itemIdsByTaskId };
      }
      itemById[item.itemId] = item;
      const matchedBefore = itemMatchesFilter(current, state.filter);
      const matchesNow = itemMatchesFilter(item, state.filter);
      if (matchedBefore !== matchesNow) {
        if (!membershipChanged) orderedItemIdsByPage = { ...orderedItemIdsByPage };
        if (matchedBefore) {
          for (const pageKey of state.loadedPages) {
            const pageIds = orderedItemIdsByPage[pageKey] ?? [];
            if (pageIds.includes(item.itemId)) {
              orderedItemIdsByPage[pageKey] = pageIds.filter((itemId) => itemId !== item.itemId);
              break;
            }
          }
        } else {
          const pageKey = state.loadedPages[state.loadedPages.length - 1];
          if (pageKey) {
            const pageIds = orderedItemIdsByPage[pageKey] ?? [];
            orderedItemIdsByPage[pageKey] = [item.itemId, ...pageIds.filter((itemId) => itemId !== item.itemId)].slice(0, IMPORT_PAGE_LIMIT);
          }
        }
        itemPageTotal = Math.max(0, itemPageTotal - Number(matchedBefore) + Number(matchesNow));
        membershipChanged = true;
      }
      if (current.taskId !== item.taskId) {
        if (current.taskId) {
          const remaining = (itemIdsByTaskId[current.taskId] ?? []).filter((id) => id !== item.itemId);
          if (remaining.length === 0) delete itemIdsByTaskId[current.taskId];
          else itemIdsByTaskId[current.taskId] = remaining;
        }
        if (item.taskId) itemIdsByTaskId[item.taskId] = [...new Set([...(itemIdsByTaskId[item.taskId] ?? []), item.itemId])];
      }
      ({ counts, progress, overview } = applyProjectionDelta(counts, progress, overview, projectItem(current), projectItem(item)));
      changed = true;
    }
    if (!changed) return false;
    if (changed) {
      sessionItems = loadedIds({ loadedPages: state.loadedPages, orderedItemIdsByPage })
        .flatMap((itemId) => itemById[itemId] ? [itemById[itemId]!] : []);
    }
    set({
      itemById,
      itemIdsByTaskId,
      orderedItemIdsByPage,
      itemPageTotal,
      session: changed ? sessionWithItems(state.session, sessionItems) : state.session,
      counts,
      progress,
      overview,
      selectedItemId: selectedItemIdFor({ knownItemIds: state.knownItemIds, itemById }, state.selectedItemId),
    });
    return true;
  },
  recordOperationCounts: (projectKey, batchId, nextCounts, items = [], epoch) => {
    const state = get();
    if (!scopeAccepts(state, projectKey, epoch)) return false;
    const current = state.operationCountsByBatchId[batchId];
    const failedItemIds = new Set(state.operationFailedItemIdsByBatchId[batchId] ?? []);
    for (const item of items) {
      if (item.status === "failed") failedItemIds.add(item.itemId);
      else failedItemIds.delete(item.itemId);
    }
    const nextFailedItemIds = [...failedItemIds];
    const countsSame = current && Object.keys(nextCounts).every((key) => current[key as keyof ImportSessionPatchCounts] === nextCounts[key as keyof ImportSessionPatchCounts]);
    const failedSame = nextFailedItemIds.length === (state.operationFailedItemIdsByBatchId[batchId]?.length ?? 0)
      && nextFailedItemIds.every((itemId, index) => itemId === state.operationFailedItemIdsByBatchId[batchId]?.[index]);
    if (countsSame && failedSame) return false;
    set({
      operationCountsByBatchId: { ...state.operationCountsByBatchId, [batchId]: nextCounts },
      operationFailedItemIdsByBatchId: { ...state.operationFailedItemIdsByBatchId, [batchId]: nextFailedItemIds },
    });
    return true;
  },
  resetProjectPresentation: (projectKey) => set((state) => ({
    projectKey,
    ...clearedNormalizedState(),
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
  selectItem: (selectedItemId) => set((state) => ({ selectedItemId: selectedItemIdFor(state, selectedItemId) })),
  setFilter: (filter) => set({ filter }),
  beginMutation: (key) => set((state) => {
    const mutationKeys = new Set(state.mutationKeys);
    mutationKeys.add(key);
    return { mutationKeys };
  }),
  endMutation: (key) => set((state) => {
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
  requestAction: (itemId, action) => set((state) => ({
    actionRequest: { itemId, action, requestId: (state.actionRequest?.requestId ?? 0) + 1 },
  })),
  clearActionRequest: (requestId) => set((state) => state.actionRequest?.requestId === requestId ? { actionRequest: null } : {}),
  reset: () => set({
    projectKey: null,
    ...clearedNormalizedState(),
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

export function selectLoadedImportItems(state: Pick<ImportState, "loadedPages" | "orderedItemIdsByPage" | "itemById">): ImportItem[] {
  return loadedIds(state).flatMap((itemId) => state.itemById[itemId] ? [state.itemById[itemId]!] : []);
}

registerProjectScopeResetHandler("import", () => {
  useImportStore.getState().resetProjectPresentation("");
  useImportStore.getState().reset();
});

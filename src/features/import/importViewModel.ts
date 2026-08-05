import type { ImportItem, ImportSession, ImportItemStatus } from "../../types/importV2";
import type { ImportQueueFilter } from "../../stores/importStore";
import { presentImportItem } from "./importStatusPresentation";

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

export interface ImportViewModelSnapshot {
  visibleItems: ImportItem[];
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
}

const ACTIVE_STATUSES: ReadonlySet<ImportItemStatus> = new Set([
  "queued",
  "inspecting",
  "extracting",
  "validating",
  "committing",
]);

export const isImportItemActive = (item: ImportItem): boolean => ACTIVE_STATUSES.has(item.status);

export const isImportItemReady = (item: ImportItem): boolean =>
  presentImportItem(item).userState === "ready";

export const isImportItemNeedsAction = (item: ImportItem): boolean =>
  presentImportItem(item).userState === "needs_action";

export const selectVisibleItems = (
  session: ImportSession | null,
  filter: ImportQueueFilter,
): ImportItem[] => {
  if (!session) return [];
  const activeQueue = session.items.filter(
    (item) => item.status !== "completed" && item.status !== "skipped",
  );
  switch (filter) {
    case "active":
      return activeQueue.filter(isImportItemActive);
    case "ready":
      return activeQueue.filter(isImportItemReady);
    case "needs_action":
      return activeQueue.filter(isImportItemNeedsAction);
    case "failed":
      return activeQueue.filter((item) => item.status === "failed");
    case "completed":
      return [];
    case "all":
    default:
      return activeQueue;
  }
};

export const selectImportViewModel = (
  session: ImportSession | null,
  filter: ImportQueueFilter,
): ImportViewModelSnapshot => {
  const items = session?.items ?? [];
  const visibleItems: ImportItem[] = [];
  const counts: ImportQueueCounts = {
    all: 0,
    active: 0,
    ready: 0,
    needsAction: 0,
    failed: 0,
    completed: 0,
    waiting: 0,
  };
  const progress: ImportSessionProgress = {
    completed: 0,
    total: items.length,
    active: 0,
    processed: 0,
    failed: 0,
    cancelled: 0,
    needsAction: 0,
  };
  for (const item of items) {
    const active = isImportItemActive(item);
    const presentation = presentImportItem(item);
    const ready = presentation.userState === "ready";
    const needsAction = presentation.userState === "needs_action";
    const completed = item.status === "completed";
    const skipped = item.status === "skipped";
    const failed = item.status === "failed";
    const cancelled = item.status === "cancelled";
    const processed = [
      "preview_ready",
      "needs_merge",
      "completed",
      "failed",
      "cancelled",
      "skipped",
    ].includes(item.status);
    if (completed) progress.completed += 1;
    if (active) progress.active += 1;
    if (processed) progress.processed = (progress.processed ?? 0) + 1;
    if (failed) progress.failed = (progress.failed ?? 0) + 1;
    if (cancelled) progress.cancelled = (progress.cancelled ?? 0) + 1;
    if (needsAction) progress.needsAction = (progress.needsAction ?? 0) + 1;
    if (completed || skipped) {
      if (completed) counts.completed += 1;
      continue;
    }
    counts.all += 1;
    if (active) counts.active += 1;
    if (ready) counts.ready += 1;
    if (needsAction) counts.needsAction += 1;
    if (failed) counts.failed += 1;
    if (["waiting_capability", "waiting_login", "waiting_authorization"].includes(item.status)) {
      counts.waiting = (counts.waiting ?? 0) + 1;
    }
    const visible = filter === "all"
      || (filter === "active" && active)
      || (filter === "ready" && ready)
      || (filter === "needs_action" && needsAction)
      || (filter === "failed" && failed);
    if (visible) visibleItems.push(item);
  }
  return { visibleItems, counts, progress };
};
export const selectQueueCounts = (session: ImportSession | null): ImportQueueCounts => {
  const items = (session?.items ?? []).filter(
    (item) => item.status !== "completed" && item.status !== "skipped",
  );
  return {
    all: items.length,
    active: items.filter(isImportItemActive).length,
    ready: items.filter(isImportItemReady).length,
    needsAction: items.filter(isImportItemNeedsAction).length,
    failed: items.filter((item) => item.status === "failed").length,
    completed: items.filter((item) => item.status === "completed").length,
    waiting: items.filter((item) => item.status === "waiting_capability" || item.status === "waiting_login" || item.status === "waiting_authorization").length,
  };
};
export const selectCommittableItems = (session: ImportSession | null): ImportItem[] =>
  (session?.items ?? []).filter(
    (item) => item.selected && presentImportItem(item).committable,
  );

export const selectSessionProgress = (session: ImportSession | null): ImportSessionProgress => {
  const items = session?.items ?? [];
  const processed = items.filter((item) => item.status === "preview_ready" || item.status === "needs_merge" || item.status === "completed" || item.status === "failed" || item.status === "cancelled" || item.status === "skipped").length;
  return {
    completed: items.filter((item) => item.status === "completed").length,
    total: items.length,
    active: items.filter(isImportItemActive).length,
    processed,
    failed: items.filter((item) => item.status === "failed").length,
    cancelled: items.filter((item) => item.status === "cancelled").length,
    needsAction: items.filter(isImportItemNeedsAction).length,
  };
};

import type { ImportItem, ImportSession, ImportItemStatus } from "../../types/importV2";
import type { ImportQueueFilter } from "../../stores/importStore";

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

const ACTIVE_STATUSES: ReadonlySet<ImportItemStatus> = new Set([
  "queued",
  "inspecting",
  "extracting",
  "validating",
  "committing",
]);

export const isImportItemActive = (item: ImportItem): boolean => ACTIVE_STATUSES.has(item.status);

export const isImportItemReady = (item: ImportItem): boolean => item.status === "preview_ready";

export const isImportItemNeedsAction = (item: ImportItem): boolean =>
  item.status === "waiting_capability" ||
  item.status === "waiting_login" ||
  item.status === "needs_merge" ||
  item.status === "paused" ||
  item.issue?.userActionRequired === true;

export const selectVisibleItems = (
  session: ImportSession | null,
  filter: ImportQueueFilter,
): ImportItem[] => {
  if (!session) return [];
  switch (filter) {
    case "active":
      return session.items.filter(isImportItemActive);
    case "ready":
      return session.items.filter(isImportItemReady);
    case "needs_action":
      return session.items.filter(isImportItemNeedsAction);
    case "failed":
      return session.items.filter((item) => item.status === "failed");
    case "completed":
      return session.items.filter((item) => item.status === "completed");
    case "all":
    default:
      return session.items;
  }
};
export const selectQueueCounts = (session: ImportSession | null): ImportQueueCounts => {
  const items = session?.items ?? [];
  return {
    all: items.length,
    active: items.filter(isImportItemActive).length,
    ready: items.filter(isImportItemReady).length,
    needsAction: items.filter(isImportItemNeedsAction).length,
    failed: items.filter((item) => item.status === "failed").length,
    completed: items.filter((item) => item.status === "completed").length,
    waiting: items.filter((item) => item.status === "waiting_capability" || item.status === "waiting_login").length,
  };
};
export const selectCommittableItems = (session: ImportSession | null): ImportItem[] =>
  (session?.items ?? []).filter((item) => item.selected && isImportItemReady(item));

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

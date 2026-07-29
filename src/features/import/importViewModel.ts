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

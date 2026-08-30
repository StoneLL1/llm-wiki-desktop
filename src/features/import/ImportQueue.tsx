import { File, Folder, Globe2, LockKeyhole } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { ImportItem } from "../../types/importV2";
import type { BackendTask } from "../../types/task";
import type { ImportQueueCounts, ImportQueueFilter, ImportSessionProgress } from "../../stores/importStore";
import type { ImportItemAction } from "./importStatusPresentation";
import { presentImportItem } from "./importStatusPresentation";
import { ImportItemActions } from "./ImportItemActions";
import { ImportItemStatus } from "./ImportItemStatus";

export interface ImportQueueProps {
  items: readonly ImportItem[];
  totalItems?: number;
  itemIndexOffset?: number;
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  onFilterChange: (filter: ImportQueueFilter) => void;
  onSelectItem: (itemId: string) => void;
  onSetItemSelected: (itemId: string, selected: boolean) => void;
  onAction: (action: ImportItemAction, itemId: string) => void;
  pendingItemIds?: ReadonlySet<string>;
  onCopyLocator?: (locator: string) => void | Promise<void>;
  sessionSyncing?: boolean;
  discoveryTask?: BackendTask | null;
  resetKey?: string | null;
  hasMoreItems?: boolean;
  loadingMoreItems?: boolean;
  onLoadMoreItems?: () => void;
}

const FILTERS: readonly { key: ImportQueueFilter; labelKey: string; count: (counts: ImportQueueCounts) => number }[] = [
  { key: "all", labelKey: "importV2.queue.filter.all", count: (counts) => counts.all },
  { key: "active", labelKey: "importV2.queue.filter.active", count: (counts) => counts.active },
  { key: "ready", labelKey: "importV2.queue.filter.ready", count: (counts) => counts.ready },
  { key: "needs_action", labelKey: "importV2.queue.filter.needsAction", count: (counts) => counts.needsAction },
  { key: "failed", labelKey: "importV2.queue.filter.failed", count: (counts) => counts.failed },
];

const ROW_HEIGHT = 72;
const DEFAULT_VIEWPORT_HEIGHT = 560;
const OVERSCAN_ROWS = 8;
const MAX_MOUNTED_ROWS = 80;
const scrollAnchors = new Map<string, { topItemId: string; intraRowOffset: number; globalIndex: number }>();

function rowDomId(itemId: string): string {
  return `import-queue-row-${encodeURIComponent(itemId)}`;
}

function useBoundedLiveSummary(summary: string): string {
  const [announced, setAnnounced] = useState(summary);
  const timerRef = useRef<number | null>(null);
  const latestRef = useRef(summary);
  latestRef.current = summary;
  useEffect(() => {
    if (summary === announced) return;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      setAnnounced(latestRef.current);
    }, 500);
    return () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [announced, summary]);
  return announced;
}

function itemIcon(item: ImportItem) {
  if (item.input.kind === "url") return Globe2;
  if (item.input.kind === "folder") return Folder;
  return File;
}

interface ImportQueueRowProps {
  item: ImportItem;
  position: number;
  setSize: number;
  selected: boolean;
  active: boolean;
  pending: boolean;
  onSelectItem: (itemId: string) => void;
  onSetItemSelected: (itemId: string, selected: boolean) => void;
  onAction: (action: ImportItemAction, itemId: string) => void;
  onCopyLocator?: (locator: string) => void | Promise<void>;
  onActiveItem: (itemId: string) => void;
}

const ImportQueueRow = memo(function ImportQueueRow({
  item,
  position,
  setSize,
  selected,
  active,
  pending,
  onSelectItem,
  onSetItemSelected,
  onAction,
  onCopyLocator,
  onActiveItem,
}: ImportQueueRowProps) {
  const { t } = useTranslation();
  const presentation = presentImportItem(item);
  const SourceIcon = itemIcon(item);
  return (
    <article
      id={rowDomId(item.itemId)}
      data-testid={`import-item-${item.itemId}`}
      data-virtual-row-id={item.itemId}
      className={`import-v2-queue__row ${selected ? "is-selected" : ""} ${active ? "is-active" : ""}`}
      onClick={() => onSelectItem(item.itemId)}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return;
        if (event.key === "Enter") {
          event.preventDefault();
          onSelectItem(item.itemId);
        } else if (event.key === " " && presentation.selectable && !pending) {
          event.preventDefault();
          onSetItemSelected(item.itemId, !item.selected);
        }
      }}
      tabIndex={-1}
      role="option"
      aria-posinset={position}
      aria-setsize={setSize}
      aria-selected={selected}
      onFocus={(event) => {
        if (event.target === event.currentTarget) onActiveItem(item.itemId);
      }}
    >
      <div className="flex min-w-0 items-start gap-2">
        <input
          type="checkbox"
          aria-label={t("importV2.queue.select", { name: item.input.displayName })}
          checked={presentation.selectable && item.selected}
          disabled={!presentation.selectable || pending}
          aria-busy={pending}
          onClick={(event) => event.stopPropagation()}
          onChange={(event) => onSetItemSelected(item.itemId, event.target.checked)}
        />
        <SourceIcon size={15} className="mt-0.5 shrink-0 text-[var(--text-muted)]" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-[13px] font-medium" title={item.input.locator}>{item.input.displayName}</span>
            {item.restrictedContent ? (
              <span
                className="shrink-0 text-[var(--warning)]"
                title={item.restrictedIdentitySummary ?? t("importV2.restricted.locked")}
                aria-label={item.restrictedIdentitySummary ?? t("importV2.restricted.locked")}
              >
                <LockKeyhole size={12} aria-hidden="true" />
              </span>
            ) : null}
            <span className="shrink-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t(`importV2.queue.kind.${item.input.kind}`)}</span>
          </div>
          {presentation.userIssue ? (
            <p className="m-0 truncate text-[11px] text-[var(--warning-text)]">
              {t(presentation.userIssue.title)}
            </p>
          ) : null}
        </div>
      </div>
      <ImportItemStatus item={item} presentation={presentation} />
      <ImportItemActions item={item} presentation={presentation} pending={pending} onAction={onAction} onCopyLocator={onCopyLocator} />
    </article>
  );
});

function progressPercent(progress: ImportSessionProgress): number {
  if (progress.total === 0) return 0;
  return Math.round(((progress.processed ?? progress.completed) / progress.total) * 100);
}

export function ImportQueue({
  items,
  totalItems,
  itemIndexOffset = 0,
  counts,
  progress,
  selectedItemId,
  filter,
  onFilterChange,
  onSelectItem,
  onSetItemSelected,
  onAction,
  pendingItemIds = new Set<string>(),
  onCopyLocator,
  sessionSyncing = false,
  discoveryTask,
  resetKey,
  hasMoreItems = false,
  loadingMoreItems = false,
  onLoadMoreItems,
}: ImportQueueProps) {
  const { t } = useTranslation();
  const percent = progressPercent(progress);
  const processed = progress.processed ?? progress.completed;
  const failed = progress.failed ?? counts.failed;
  const needsAction = progress.needsAction ?? counts.needsAction;
  const discoveryActive = discoveryTask?.status === "queued" || discoveryTask?.status === "running" || discoveryTask?.status === "cancelling";
  const discoveryCount = discoveryTask?.progress?.current ?? 0;
  const listRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(DEFAULT_VIEWPORT_HEIGHT);
  const [activeItemId, setActiveItemId] = useState<string | null>(selectedItemId);
  const anchorKey = `${resetKey ?? "queue"}\0${filter}`;
  const setSize = Math.max(totalItems ?? progress.total, itemIndexOffset + items.length);
  const windowStart = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
  const desiredRows = Math.ceil(viewportHeight / ROW_HEIGHT) + (OVERSCAN_ROWS * 2);
  const windowEnd = Math.min(items.length, windowStart + Math.min(MAX_MOUNTED_ROWS, desiredRows));
  const mountedItems = items.slice(windowStart, windowEnd);
  const topSpacer = windowStart * ROW_HEIGHT;
  const totalHeight = items.length * ROW_HEIGHT;
  const liveSummary = useMemo(() => t("importV2.queue.liveSummary", {
    total: counts.all,
    active: progress.active,
    ready: counts.ready,
    needsAction,
    failed,
  }), [counts.all, counts.ready, failed, needsAction, progress.active, t]);
  const announcedSummary = useBoundedLiveSummary(liveSummary);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const anchor = scrollAnchors.get(anchorKey);
    if (!anchor) {
      list.scrollTop = 0;
      setScrollTop(0);
      return;
    }
    const localIndex = items.findIndex((item) => item.itemId === anchor.topItemId);
    const fallbackIndex = Math.max(0, Math.min(items.length - 1, anchor.globalIndex - itemIndexOffset));
    const index = localIndex >= 0 ? localIndex : fallbackIndex;
    const nextScrollTop = Math.max(0, (index * ROW_HEIGHT) + anchor.intraRowOffset);
    list.scrollTop = nextScrollTop;
    setScrollTop(nextScrollTop);
  }, [anchorKey, itemIndexOffset]);

  useEffect(() => {
    if (selectedItemId) setActiveItemId(selectedItemId);
  }, [selectedItemId]);

  const handleScroll = (element: HTMLDivElement) => {
    const nextScrollTop = Math.max(0, element.scrollTop);
    const nextViewportHeight = element.clientHeight || DEFAULT_VIEWPORT_HEIGHT;
    const nextWindowStart = Math.max(0, Math.floor(nextScrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
    const nextWindowEnd = Math.min(
      items.length,
      nextWindowStart + Math.min(MAX_MOUNTED_ROWS, Math.ceil(nextViewportHeight / ROW_HEIGHT) + (OVERSCAN_ROWS * 2)),
    );
    const activeIndex = items.findIndex((item) => item.itemId === activeItemId);
    if (activeIndex < nextWindowStart || activeIndex >= nextWindowEnd) {
      const visibleIndex = Math.max(0, Math.min(items.length - 1, Math.floor(nextScrollTop / ROW_HEIGHT)));
      setActiveItemId(items[visibleIndex]?.itemId ?? null);
    }
    const topIndex = Math.max(0, Math.min(items.length - 1, Math.floor(nextScrollTop / ROW_HEIGHT)));
    const topItem = items[topIndex];
    if (topItem) {
      scrollAnchors.set(anchorKey, {
        topItemId: topItem.itemId,
        intraRowOffset: nextScrollTop - (topIndex * ROW_HEIGHT),
        globalIndex: itemIndexOffset + topIndex,
      });
      if (scrollAnchors.size > 24) scrollAnchors.delete(scrollAnchors.keys().next().value as string);
    }
    setViewportHeight(nextViewportHeight);
    setScrollTop(nextScrollTop);
    if (
      hasMoreItems
      && !loadingMoreItems
      && onLoadMoreItems
      && nextScrollTop + nextViewportHeight >= totalHeight - (ROW_HEIGHT * 2)
    ) onLoadMoreItems();
  };

  const handleListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || items.length === 0) return;
    const currentIndex = items.findIndex((item) => item.itemId === activeItemId);
    let nextIndex = currentIndex;
    if (event.key === "ArrowDown") nextIndex = Math.min(items.length - 1, currentIndex + 1);
    else if (event.key === "ArrowUp") nextIndex = currentIndex < 0 ? 0 : Math.max(0, currentIndex - 1);
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = items.length - 1;
    else if (event.key === "Enter" && activeItemId) {
      event.preventDefault();
      onSelectItem(activeItemId);
      return;
    } else if (event.key === " " && activeItemId) {
      event.preventDefault();
      const activeItem = items.find((item) => item.itemId === activeItemId);
      if (activeItem) {
        const presentation = presentImportItem(activeItem);
        if (presentation.selectable && !pendingItemIds.has(activeItemId)) {
          onSetItemSelected(activeItemId, !activeItem.selected);
        }
      }
      return;
    } else return;
    event.preventDefault();
    const nextItem = items[nextIndex];
    if (!nextItem) return;
    setActiveItemId(nextItem.itemId);
    const nextTop = nextIndex * ROW_HEIGHT;
    if (nextTop < scrollTop || nextTop + ROW_HEIGHT > scrollTop + viewportHeight) {
      listRef.current?.scrollTo({ top: Math.max(0, nextTop - OVERSCAN_ROWS * ROW_HEIGHT) });
      setScrollTop(Math.max(0, nextTop - OVERSCAN_ROWS * ROW_HEIGHT));
    }
  };
  return (
    <section className="import-v2-queue" aria-label={t("importV2.queue.label")}>
      <header className="import-v2-queue__header">
        <div className="flex min-w-0 items-baseline gap-2">
          <h2 className="m-0 text-[15px] font-semibold">{t("importV2.queue.title")}</h2>
          <span className="text-[11px] text-[var(--text-muted)]">{t("importV2.queue.items", { count: counts.all })}</span>
        </div>
        <div className="flex items-center gap-2 font-mono text-[11px] text-[var(--text-muted)]">
          <span>{discoveryActive ? t("importV2.queue.discoveryProgress", { count: discoveryCount }) : sessionSyncing ? t("importV2.queue.syncing") : t("importV2.queue.progress", { percent, processed, total: progress.total })}</span>
          {progress.active > 0 || failed > 0 || needsAction > 0 ? <span className="text-[var(--text-secondary)]">{t("importV2.queue.summary", { active: progress.active, failed, needsAction })}</span> : null}
        </div>
      </header>
      <p className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {announcedSummary}
      </p>
      <nav className="import-v2-queue__filters" aria-label={t("importV2.queue.filters")}>
        {FILTERS.map((entry) => (
          <button
            key={entry.key}
            type="button"
            className={`import-v2-queue__filter ${filter === entry.key ? "is-active" : ""}`}
            aria-pressed={filter === entry.key}
            onClick={() => onFilterChange(entry.key)}
          >
            {t(entry.labelKey)} {entry.count(counts)}
          </button>
        ))}
      </nav>
      <div
        ref={listRef}
        className="import-v2-queue__list app-pane-scrollbar"
        role="listbox"
        aria-activedescendant={activeItemId && mountedItems.some((item) => item.itemId === activeItemId) ? rowDomId(activeItemId) : undefined}
        aria-label={t("importV2.queue.title")}
        tabIndex={0}
        onKeyDown={handleListKeyDown}
        onScroll={(event) => handleScroll(event.currentTarget)}
      >
        {items.length === 0 ? (
          <div className="import-v2-queue__empty" role="status">
            <p className="m-0 text-[13px] text-[var(--text-secondary)]">{discoveryActive ? t("importV2.queue.building", { count: discoveryCount }) : t("importV2.queue.empty")}</p>
          </div>
        ) : <div className="import-v2-queue__virtual-spacer" style={{ height: `${totalHeight}px` }}>
          <div className="import-v2-queue__virtual-window" style={{ transform: `translateY(${topSpacer}px)` }}>
          {mountedItems.map((item, mountedIndex) => (
            <ImportQueueRow
              key={item.itemId}
              item={item}
              position={itemIndexOffset + windowStart + mountedIndex + 1}
              setSize={setSize}
              selected={selectedItemId === item.itemId}
              active={activeItemId === item.itemId}
              pending={pendingItemIds.has(item.itemId)}
              onSelectItem={onSelectItem}
              onSetItemSelected={onSetItemSelected}
              onAction={onAction}
              onCopyLocator={onCopyLocator}
              onActiveItem={setActiveItemId}
            />
          ))}
          </div>
        </div>}
      </div>
      {hasMoreItems ? <div className="import-v2-queue__paging" role="status"><span>{t("importV2.queue.showing", { shown: Math.min(setSize, itemIndexOffset + items.length), total: setSize })}</span><button type="button" className="btn btn--sm" disabled={loadingMoreItems} onClick={onLoadMoreItems}>{t("importV2.queue.loadMore")}</button></div> : null}
    </section>
  );
}

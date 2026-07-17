import { Copy, File, Folder, Globe2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ImportItem } from "../../types/importV2";
import type { BackendTask } from "../../types/task";
import type { ImportQueueFilter } from "../../stores/importStore";
import type { ImportItemAction } from "./importStatusPresentation";
import { presentImportItem } from "./importStatusPresentation";
import { ImportItemActions } from "./ImportItemActions";
import { ImportItemStatus } from "./ImportItemStatus";
import type { ImportQueueCounts, ImportSessionProgress } from "./importViewModel";

export interface ImportQueueProps {
  items: readonly ImportItem[];
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
}

const FILTERS: readonly { key: ImportQueueFilter; labelKey: string; count: (counts: ImportQueueCounts) => number }[] = [
  { key: "all", labelKey: "importV2.queue.filter.all", count: (counts) => counts.all },
  { key: "active", labelKey: "importV2.queue.filter.active", count: (counts) => counts.active },
  { key: "ready", labelKey: "importV2.queue.filter.ready", count: (counts) => counts.ready },
  { key: "needs_action", labelKey: "importV2.queue.filter.needsAction", count: (counts) => counts.needsAction },
  { key: "failed", labelKey: "importV2.queue.filter.failed", count: (counts) => counts.failed },
  { key: "completed", labelKey: "importV2.queue.filter.completed", count: (counts) => counts.completed },
];

const QUEUE_PAGE_SIZE = 200;

function itemIcon(item: ImportItem) {
  if (item.input.kind === "url") return Globe2;
  if (item.input.kind === "folder") return Folder;
  return File;
}

function progressPercent(progress: ImportSessionProgress): number {
  if (progress.total === 0) return 0;
  return Math.round(((progress.processed ?? progress.completed) / progress.total) * 100);
}

export function ImportQueue({
  items,
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
}: ImportQueueProps) {
  const { t } = useTranslation();
  const [visibleLimit, setVisibleLimit] = useState(QUEUE_PAGE_SIZE);
  useEffect(() => {
    setVisibleLimit(QUEUE_PAGE_SIZE);
  }, [filter, resetKey]);
  const percent = progressPercent(progress);
  const processed = progress.processed ?? progress.completed;
  const failed = progress.failed ?? counts.failed;
  const needsAction = progress.needsAction ?? counts.needsAction;
  const discoveryActive = discoveryTask?.status === "queued" || discoveryTask?.status === "running" || discoveryTask?.status === "cancelling";
  const discoveryCount = discoveryTask?.progress?.current ?? 0;
  const renderedItems = items.slice(0, visibleLimit);
  const hasMoreItems = renderedItems.length < items.length;
  return (
    <section className="import-v2-queue" aria-label={t("importV2.queue.label")}>
      <header className="import-v2-queue__header">
        <div className="flex min-w-0 items-baseline gap-2">
          <h2 className="m-0 text-[15px] font-semibold">{t("importV2.queue.title")}</h2>
          <span className="text-[11px] text-[var(--text-muted)]">{t("importV2.queue.items", { count: counts.all })}</span>
        </div>
        <div className="flex items-center gap-2 font-mono text-[11px] text-[var(--text-muted)]" role="status" aria-live="polite">
          <span>{discoveryActive ? t("importV2.queue.discoveryProgress", { count: discoveryCount }) : sessionSyncing ? t("importV2.queue.syncing") : t("importV2.queue.progress", { percent, processed, total: progress.total })}</span>
          {progress.active > 0 || failed > 0 || needsAction > 0 ? <span className="text-[var(--text-secondary)]">{t("importV2.queue.summary", { active: progress.active, failed, needsAction })}</span> : null}
        </div>
      </header>
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
      <div className="import-v2-queue__list" role="list" aria-label={t("importV2.queue.title")}>
        {items.length === 0 ? (
          <div className="import-v2-queue__empty" role="status">
            <p className="m-0 text-[13px] text-[var(--text-secondary)]">{discoveryActive ? t("importV2.queue.building", { count: discoveryCount }) : t("importV2.queue.empty")}</p>
          </div>
        ) : renderedItems.map((item) => {
          const presentation = presentImportItem(item);
          const SourceIcon = itemIcon(item);
          const isSelected = selectedItemId === item.itemId;
          return (
            <article
              key={item.itemId}
              data-testid={`import-item-${item.itemId}`}
              className={`import-v2-queue__row ${isSelected ? "is-selected" : ""}`}
              onClick={() => onSelectItem(item.itemId)}
              onKeyDown={(event) => {
                if (event.target !== event.currentTarget) return;
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onSelectItem(item.itemId);
                }
              }}
              tabIndex={0}
              role="listitem"
              aria-current={isSelected ? "true" : undefined}
            >
              <div className="flex min-w-0 items-start gap-2">
                {presentation.selectable ? (
                  <input
                    type="checkbox"
                    aria-label={t("importV2.queue.select", { name: item.input.displayName })}
                    checked={item.selected}
                    disabled={pendingItemIds.has(item.itemId)}
                    aria-busy={pendingItemIds.has(item.itemId)}
                    onClick={(event) => event.stopPropagation()}
                    onChange={(event) => onSetItemSelected(item.itemId, event.target.checked)}
                  />
                ) : null}
                <SourceIcon size={15} className="mt-0.5 shrink-0 text-[var(--text-muted)]" aria-hidden="true" />
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-[13px] font-medium" title={item.input.locator}>{item.input.displayName}</span>
                    <span className="shrink-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t(`importV2.queue.kind.${item.input.kind}`)}</span>
                  </div>
                  <div className="flex min-w-0 items-center gap-1">
                    <p className="m-0 min-w-0 flex-1 truncate font-mono text-[10.5px] text-[var(--text-muted)]" title={item.input.locator}>{item.input.locator}</p>
                    {onCopyLocator ? <button type="button" className="icon-button shrink-0" title={item.input.locator} aria-label={t("importV2.queue.copyLocator", { name: item.input.displayName })} onClick={(event) => { event.stopPropagation(); void onCopyLocator(item.input.locator); }}><Copy size={12} aria-hidden="true" /></button> : null}
                  </div>
                  {item.issue ? <p className="m-0 truncate text-[11px] text-[var(--danger)]" title={item.issue.message}>{item.issue.message}</p> : null}
                </div>
              </div>
              <ImportItemStatus item={item} presentation={presentation} />
              <ImportItemActions item={item} presentation={presentation} pending={pendingItemIds.has(item.itemId)} onAction={onAction} />
            </article>
          );
        })}
      </div>
      {hasMoreItems ? <div className="import-v2-queue__paging" role="status"><span>{t("importV2.queue.showing", { shown: renderedItems.length, total: items.length })}</span><button type="button" className="btn btn--sm" onClick={() => setVisibleLimit((limit) => Math.min(limit + QUEUE_PAGE_SIZE, items.length))}>{t("importV2.queue.loadMore")}</button></div> : null}
    </section>
  );
}

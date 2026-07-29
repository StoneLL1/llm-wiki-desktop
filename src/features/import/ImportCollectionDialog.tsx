import { ListTree, LoaderCircle, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { ImportCollectionPreview } from "../../types/importV2Web";

interface ImportCollectionDialogProps {
  preview: ImportCollectionPreview | null;
  onLoadMore: (loadAll?: boolean) => Promise<void>;
  onConfirm: (itemRefs: readonly string[]) => Promise<void>;
  onCancel: () => void;
}

const COLLECTION_PAGE_SIZE = 25;

function formatDuration(seconds: number | null): string {
  if (seconds === null) return "—";
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.ceil((seconds % 3_600) / 60);
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

export function ImportCollectionDialog({
  preview,
  onLoadMore,
  onConfirm,
  onCancel,
}: ImportCollectionDialogProps) {
  const { t } = useTranslation();
  const open = preview !== null;
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const allRefs = useMemo(
    () => preview?.items.map((item) => item.itemRef) ?? [],
    [preview],
  );
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [visibleCount, setVisibleCount] = useState(COLLECTION_PAGE_SIZE);
  const [busy, setBusy] = useState(false);
  const [loadingPage, setLoadingPage] = useState(false);
  const selectionCollectionRef = useRef<string | null>(null);
  const knownItemRefs = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (selectionCollectionRef.current !== (preview?.collectionRef ?? null)) {
      selectionCollectionRef.current = preview?.collectionRef ?? null;
      knownItemRefs.current = new Set(allRefs);
      setSelected(new Set(allRefs));
      setVisibleCount(COLLECTION_PAGE_SIZE);
      return;
    }
    const previousRefs = knownItemRefs.current;
    knownItemRefs.current = new Set(allRefs);
    setSelected((current) => {
      const next = new Set(current);
      for (const itemRef of allRefs) {
        if (!previousRefs.has(itemRef)) next.add(itemRef);
      }
      return next;
    });
  }, [allRefs, preview?.collectionRef]);

  const activePreview = preview;
  if (!activePreview) return null;
  const allSelected = selected.size === activePreview.items.length;
  const previewItems = activePreview.items;
  const visibleItems = previewItems.slice(0, visibleCount);

  async function confirm() {
    if (selected.size === 0) return;
    setBusy(true);
    try {
      await onConfirm(previewItems
        .filter((item) => selected.has(item.itemRef))
        .map((item) => item.itemRef));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-collection-title"
    >
      <section className="flex max-h-[78vh] w-full max-w-[720px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <ListTree size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h2 id="import-collection-title" className="m-0 truncate text-[15px] font-semibold">
              {t("importV2.collection.title", { title: activePreview.title })}
            </h2>
            <p className="m-0 truncate text-[11px] text-[var(--text-muted)]">
              {t("importV2.collection.summary", { count: activePreview.discoveredTotal, platform: activePreview.platform })}
            </p>
          </div>
          <button type="button" className="icon-button" aria-label={t("common.close")} title={t("common.close")} onClick={onCancel}>
            <X size={16} aria-hidden="true" />
          </button>
        </header>
        <div className="border-b border-[var(--border)] px-4 py-2">
          <dl className="mb-2 grid grid-cols-3 gap-2 text-[11px]">
            <div>
              <dt className="text-[var(--text-muted)]">{t("importV2.collection.duration")}</dt>
              <dd className="m-0 font-mono">{formatDuration(activePreview.totalDurationSeconds)}</dd>
            </div>
            <div>
              <dt className="text-[var(--text-muted)]">{t("importV2.collection.loginEstimate")}</dt>
              <dd className="m-0 font-mono">{activePreview.estimatedLoginCount}</dd>
            </div>
            <div>
              <dt className="text-[var(--text-muted)]">{t("importV2.collection.asrEstimate")}</dt>
              <dd className="m-0 font-mono">{activePreview.estimatedAsrCount}</dd>
            </div>
          </dl>
          <div className="flex items-center gap-3">
            <label className="inline-flex cursor-pointer items-center gap-2 text-[12px]">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={(event) => setSelected(event.target.checked ? new Set(allRefs) : new Set())}
              />
              {t("importV2.collection.selectAll")}
            </label>
            <button
              type="button"
              className="text-[12px] text-[var(--accent)] hover:underline"
              onClick={() => setSelected(new Set(allRefs.filter((itemRef) => !selected.has(itemRef))))}
            >
              {t("importV2.collection.invertSelection")}
            </button>
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto" role="group" aria-label={t("importV2.collection.items")}>
          {visibleItems.map((item, index) => (
            <label
              key={item.itemRef}
              className="grid cursor-pointer grid-cols-[20px_32px_minmax(0,1fr)] items-start gap-2 border-b border-[var(--border-subtle)] px-4 py-2.5 last:border-b-0 hover:bg-[var(--surface-hover)]"
            >
              <input
                type="checkbox"
                className="mt-0.5"
                checked={selected.has(item.itemRef)}
                onChange={(event) => {
                  setSelected((current) => {
                    const next = new Set(current);
                    if (event.target.checked) next.add(item.itemRef);
                    else next.delete(item.itemRef);
                    return next;
                  });
                }}
              />
              <span className="text-right font-mono text-[11px] text-[var(--text-muted)]">{index + 1}</span>
              <span className="min-w-0">
                <span className="block truncate text-[12px]">{item.title}</span>
                <span className="block truncate font-mono text-[10.5px] text-[var(--text-muted)]">{item.publicUrl}</span>
              </span>
            </label>
          ))}
          {visibleCount < previewItems.length || activePreview.hasMore ? (
            <div className="flex justify-center px-4 py-3">
              <button
                type="button"
                className="btn btn--sm"
                disabled={loadingPage}
                onClick={() => {
                  if (visibleCount < previewItems.length) {
                    setVisibleCount((current) => Math.min(current + COLLECTION_PAGE_SIZE, previewItems.length));
                    return;
                  }
                  setLoadingPage(true);
                  void onLoadMore(false).finally(() => setLoadingPage(false));
                }}
              >
                {t("importV2.collection.loadMore", {
                  shown: visibleItems.length,
                  total: activePreview.discoveredTotal,
                })}
              </button>
              {activePreview.hasMore ? (
                <button
                  type="button"
                  className="btn btn--sm ml-2"
                  disabled={loadingPage}
                  onClick={() => {
                    setLoadingPage(true);
                    void onLoadMore(true).finally(() => setLoadingPage(false));
                  }}
                >
                  {t("importV2.collection.loadAll")}
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
        <footer className="flex items-center justify-between gap-3 border-t border-[var(--border)] px-4 py-3">
          <span className="text-[11px] text-[var(--text-muted)]">
            {t("importV2.collection.selected", { count: selected.size })}
          </span>
          <div className="flex items-center gap-2">
            <button type="button" className="btn btn--sm" onClick={onCancel} disabled={busy}>
              {t("common.cancel")}
            </button>
            <button type="button" className="btn btn--sm btn--primary" onClick={() => void confirm()} disabled={busy || selected.size === 0}>
              {busy ? <LoaderCircle size={13} className="mr-1 inline animate-spin" aria-hidden="true" /> : null}
              {t("importV2.collection.addSelected")}
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
}

import { FileClock, History, ScrollText } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportHistoryPage } from "../../types/importV2Presentation";

const HISTORY_STATUS_KEYS: Record<string, string> = {
  queued: "importV2.itemStatus.queued",
  processing: "importV2.itemStatus.extracting",
  completed: "importV2.itemStatus.completed",
  failed: "importV2.itemStatus.failed",
  cancelled: "importV2.itemStatus.cancelled",
};

export interface ImportHistoryPanelProps {
  page: ImportHistoryPage | null;
  onOpenEntry: (entryId: string) => void;
  onLoadMore: (cursor: string) => void;
}

export function ImportHistoryPanel({ page, onOpenEntry, onLoadMore }: ImportHistoryPanelProps) {
  const { t } = useTranslation();
  if (!page || (page.entries.length === 0 && page.legacyReadOnly.length === 0)) return <section aria-label={t("importV2.history.title")} className="px-4 py-3 text-[12px] text-[var(--text-muted)]">{t("importV2.history.empty")}</section>;
  return (
    <section aria-labelledby="import-history-title" className="border-t border-[var(--border)] px-4 py-3">
      <h2 id="import-history-title" className="m-0 flex items-center gap-2 text-[13px] font-semibold"><History size={15} aria-hidden="true" />{t("importV2.history.title")}</h2>
      <div className="mt-2 space-y-2">
        {page.entries.map((entry) => <article key={entry.id} className="border-b border-[var(--border-subtle)] pb-2"><div className="flex items-start gap-2"><FileClock size={14} className="mt-0.5 shrink-0 text-[var(--text-muted)]" aria-hidden="true" /><div className="min-w-0 flex-1"><p className="m-0 truncate text-[12px] font-medium" title={entry.title}>{entry.title}</p><p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.history.v2")} · {t("importV2.history.status", { status: t(HISTORY_STATUS_KEYS[entry.status] ?? "importV2.history.unknownStatus") })}</p><div className="mt-1 flex flex-wrap gap-1.5">{entry.availableActions.includes("open_result") ? <button type="button" className="btn btn--sm" onClick={() => onOpenEntry(entry.id)}><FileClock size={12} className="mr-1 inline" aria-hidden="true" />{t("importV2.history.openResult")}</button> : null}{entry.availableActions.includes("view_logs") ? <button type="button" className="btn btn--sm" onClick={() => onOpenEntry(entry.id)}><ScrollText size={12} className="mr-1 inline" aria-hidden="true" />{t("importV2.history.viewLogs")}</button> : null}</div></div></div></article>)}
        {page.legacyReadOnly.map((entry) => <article key={entry.id} className="border-b border-[var(--border-subtle)] pb-2"><p className="m-0 text-[12px] font-medium">{entry.title}</p><p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.history.legacyBadge")}</p><p className="m-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.history.legacyEvidence", { path: entry.evidencePath })}</p></article>)}
      </div>
      {page.nextCursor ? <button type="button" className="btn btn--sm mt-3" onClick={() => onLoadMore(page.nextCursor!)}>{t("importV2.history.loadMore")}</button> : null}
    </section>
  );
}

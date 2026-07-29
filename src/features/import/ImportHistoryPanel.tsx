import { FileClock, History, ScrollText } from "lucide-react";
import { useTranslation } from "react-i18next";

import { canOpenHistoricalResult, type ImportHistoryAction, type ImportHistoryPage } from "../../types/importV2Presentation";

const HISTORY_STATUS_KEYS: Record<string, string> = {
  queued: "importV2.itemStatus.queued",
  processing: "importV2.itemStatus.extracting",
  completed: "importV2.itemStatus.completed",
  failed: "importV2.itemStatus.failed",
  cancelled: "importV2.itemStatus.cancelled",
  partially_committed: "importV2.history.partiallyCommitted",
};

export interface ImportHistoryPanelProps {
  page: ImportHistoryPage | null;
  onOpenEntry?: (entryId: string, action: ImportHistoryAction) => void;
  openingEntryId?: string | null;
  onLoadMore: (cursor: string) => void;
  loadingMore?: boolean;
  loading?: boolean;
  error?: boolean;
  onRetry?: () => void;
}

export function ImportHistoryPanel({ page, onOpenEntry, openingEntryId = null, onLoadMore, loadingMore = false, loading = false, error = false, onRetry }: ImportHistoryPanelProps) {
  const { t, i18n } = useTranslation();
  if (loading) return <section aria-label={t("importV2.history.title")} className="px-4 py-3 text-[12px] text-[var(--text-muted)]" role="status" aria-busy="true">{t("importV2.history.loading")}</section>;
  if (error) return <section aria-label={t("importV2.history.title")} className="px-4 py-3 text-[12px] text-[var(--danger)]" role="alert"><span>{t("importV2.history.error")}</span>{onRetry ? <button type="button" className="btn btn--sm ml-2" onClick={onRetry}>{t("importV2.history.retry")}</button> : null}</section>;
  if (!page) return <section aria-label={t("importV2.history.title")} className="px-4 py-3 text-[12px] text-[var(--text-muted)]">{t("importV2.history.empty")}</section>;
  const records = [
    ...page.entries.map((entry) => ({ kind: "v2" as const, entry, sortAt: entry.startedAt ?? entry.updatedAt })),
    ...page.legacyReadOnly.map((entry) => ({ kind: "legacy" as const, entry, sortAt: entry.startedAt ?? entry.updatedAt })),
  ].sort((left, right) => timestamp(right.sortAt) - timestamp(left.sortAt));
  return (
    <section aria-labelledby="import-history-title" className="border-t border-[var(--border)] px-4 py-3">
      <h2 id="import-history-title" className="m-0 flex items-center gap-2 text-[13px] font-semibold"><History size={15} aria-hidden="true" />{t("importV2.history.title")}</h2>
      {page.warnings.length > 0 ? <div role="status" className="mt-2 space-y-1 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] text-[var(--warning-text)]">{page.warnings.map((warning) => <p key={`${warning.code}:${warning.evidencePath}`} className="m-0">{warning.message} <span className="font-mono">{warning.evidencePath}</span></p>)}</div> : null}
      {records.length === 0 ? <p className="mt-2 mb-0 text-[12px] text-[var(--text-muted)]">{t("importV2.history.empty")}</p> : null}
      <div className="mt-2 space-y-2">
        {records.map((record) => record.kind === "v2" ? (
          <article key={record.entry.id} aria-labelledby={historyDomId("import-history-entry", record.entry.id)} className="border-b border-[var(--border-subtle)] pb-2">
            <div className="flex items-start gap-2">
              <FileClock size={14} className="mt-0.5 shrink-0 text-[var(--text-muted)]" aria-hidden="true" />
              <div className="min-w-0 flex-1">
                <p id={historyDomId("import-history-entry", record.entry.id)} className="m-0 truncate text-[12px] font-medium" title={record.entry.title}>{record.entry.title}</p>
                <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.history.v2")} · {t("importV2.history.status", { status: t(HISTORY_STATUS_KEYS[record.entry.status] ?? "importV2.history.unknownStatus") })} · {t("importV2.history.itemCount", { count: record.entry.itemIds.length })}</p>
                {record.entry.updatedAt || record.entry.startedAt ? <time className="font-mono text-[10.5px] text-[var(--text-muted)]" dateTime={record.entry.updatedAt ?? record.entry.startedAt ?? undefined}>{t("importV2.history.updatedAt", { time: formatTimestamp(record.entry.updatedAt ?? record.entry.startedAt, i18n.language) })}</time> : null}
                <div className="mt-1 flex flex-wrap gap-1.5">
                  {onOpenEntry && canOpenHistoricalResult(record.entry) ? <button type="button" className="btn btn--sm" disabled={openingEntryId === record.entry.id} aria-busy={openingEntryId === record.entry.id} aria-label={t("importV2.history.openResultAria", { title: record.entry.title })} onClick={() => onOpenEntry(record.entry.id, "open_result")}><FileClock size={12} className="mr-1 inline" aria-hidden="true" />{t(openingEntryId === record.entry.id ? "importV2.history.opening" : "importV2.history.openResult")}</button> : onOpenEntry && record.entry.availableActions.includes("open_detail") ? <button type="button" className="btn btn--sm" disabled={openingEntryId === record.entry.id} aria-busy={openingEntryId === record.entry.id} aria-label={t("importV2.history.openDetailAria", { title: record.entry.title })} onClick={() => onOpenEntry(record.entry.id, "open_detail")}><FileClock size={12} className="mr-1 inline" aria-hidden="true" />{t(openingEntryId === record.entry.id ? "importV2.history.opening" : "importV2.history.openDetail")}</button> : null}
                  {onOpenEntry && record.entry.availableActions.includes("view_logs") ? <button type="button" className="btn btn--sm" disabled={openingEntryId === record.entry.id} aria-busy={openingEntryId === record.entry.id} aria-label={t("importV2.history.viewLogsAria", { title: record.entry.title })} onClick={() => onOpenEntry(record.entry.id, "view_logs")}><ScrollText size={12} className="mr-1 inline" aria-hidden="true" />{t(openingEntryId === record.entry.id ? "importV2.history.opening" : "importV2.history.viewLogs")}</button> : null}
                  {onOpenEntry && record.entry.availableActions.includes("update_wiki") ? <button type="button" className="btn btn--primary btn--sm" disabled={openingEntryId === record.entry.id} aria-busy={openingEntryId === record.entry.id} aria-label={t("importV2.history.updateWikiAria", { title: record.entry.title })} onClick={() => onOpenEntry(record.entry.id, "update_wiki")}>{t(openingEntryId === record.entry.id ? "importV2.history.opening" : "importV2.completion.updateWiki")}</button> : null}
                </div>
              </div>
            </div>
          </article>
        ) : (
          <article key={record.entry.id} aria-labelledby={historyDomId("import-history-entry", record.entry.id)} className="border-b border-[var(--border-subtle)] pb-2"><p id={historyDomId("import-history-entry", record.entry.id)} className="m-0 text-[12px] font-medium">{record.entry.title}</p><p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.history.legacyBadge")}</p>{record.entry.updatedAt || record.entry.startedAt ? <time className="font-mono text-[10.5px] text-[var(--text-muted)]" dateTime={record.entry.updatedAt ?? record.entry.startedAt ?? undefined}>{t("importV2.history.updatedAt", { time: formatTimestamp(record.entry.updatedAt ?? record.entry.startedAt, i18n.language) })}</time> : null}<p className="m-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.history.legacyEvidence", { path: record.entry.evidencePath })}</p></article>
        ))}
      </div>
      {page.nextCursor ? <button type="button" className="btn btn--sm mt-3" disabled={loadingMore} aria-busy={loadingMore} onClick={() => onLoadMore(page.nextCursor!)}>{t(loadingMore ? "importV2.history.loading" : "importV2.history.loadMore")}</button> : null}
    </section>
  );
}

function timestamp(value: string | null): number {
  if (!value) return 0;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function formatTimestamp(value: string | null, language: string): string {
  const parsed = timestamp(value);
  return parsed > 0 ? new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(new Date(parsed)) : "—";
}

function historyDomId(prefix: string, value: string): string {
  return `${prefix}-${encodeURIComponent(value).replace(/%/g, "_")}`;
}

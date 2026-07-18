import { Eye, FileClock, ScrollText, X } from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { AttemptRecord, ImportItem, ImportSession } from "../../types/importV2";
import type { ImportHistoryEntry } from "../../types/importV2Presentation";
import { presentImportItem } from "./importStatusPresentation";

const STATUS_KEYS: Record<string, string> = {
  queued: "importV2.itemStatus.queued",
  inspecting: "importV2.itemStatus.inspecting",
  waiting_capability: "importV2.itemStatus.waitingCapability",
  waiting_login: "importV2.itemStatus.waitingLogin",
  extracting: "importV2.itemStatus.extracting",
  validating: "importV2.itemStatus.validating",
  preview_ready: "importV2.itemStatus.previewReady",
  needs_merge: "importV2.itemStatus.needsMerge",
  committing: "importV2.itemStatus.committing",
  completed: "importV2.itemStatus.completed",
  paused: "importV2.itemStatus.paused",
  cancelled: "importV2.itemStatus.cancelled",
  skipped: "importV2.itemStatus.skipped",
  failed: "importV2.itemStatus.failed",
  partially_committed: "importV2.history.partiallyCommitted",
};

export interface ImportHistoryDetailDialogProps {
  open: boolean;
  entry: ImportHistoryEntry | null;
  session: ImportSession | null;
  onClose: () => void;
  onPreview: (itemId: string) => void;
  canViewLogs: (taskId: string) => boolean;
  onViewLogs: (taskId: string) => void;
  resultUnavailable?: boolean;
}

export function ImportHistoryDetailDialog({ open, entry, session, onClose, onPreview, canViewLogs, onViewLogs, resultUnavailable = false }: ImportHistoryDetailDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose });
  if (!open || !entry || !session) return null;

  const itemIds = new Set(entry.itemIds);
  const items = session.items.filter((item) => itemIds.has(item.itemId));

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-history-detail-title">
      <section className="flex max-h-[76vh] w-full max-w-[720px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <FileClock size={17} className="shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h2 id="import-history-detail-title" className="truncate text-[15px] font-semibold text-[var(--text-primary)]" title={entry.title}>{entry.title}</h2>
            <p className="m-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.history.session", { id: session.sessionId })}</p>
          </div>
          <button type="button" className="icon-button" aria-label={t("importV2.history.closeDetail")} title={t("importV2.history.closeDetail")} onClick={onClose}><X size={16} aria-hidden="true" /></button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          <div className="mb-3 flex items-center gap-2 text-[11px] text-[var(--text-muted)]">
            <span>{t("importV2.history.itemCount", { count: items.length })}</span>
            <span aria-hidden="true">·</span>
            <span>{t("importV2.history.status", { status: t(STATUS_KEYS[entry.status] ?? "importV2.history.unknownStatus") })}</span>
          </div>
          {entry.snapshotAvailable === false ? <p role="status" className="mb-3 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11px] text-[var(--warning-text)]">{t("importV2.history.snapshotFallback")}</p> : null}
          {resultUnavailable ? <p role="alert" className="mb-3 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11px] text-[var(--warning-text)]">{t("importV2.history.resultUnavailableDetail")}</p> : null}
          <div className="space-y-2">
            {items.map((item) => <HistoryItem key={item.itemId} item={item} onPreview={onPreview} canViewLogs={canViewLogs} onViewLogs={onViewLogs} />)}
            {items.length === 0 ? <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("importV2.history.noItems")}</p> : null}
          </div>
        </div>

        <footer className="flex min-h-[52px] items-center justify-end border-t border-[var(--border)] px-4">
          <button type="button" className="rounded-[var(--radius-md)] bg-[var(--text-primary)] px-2.5 py-1.5 text-[11.5px] text-[var(--surface)]" onClick={onClose}>{t("importV2.preview.close")}</button>
        </footer>
      </section>
    </div>
  );
}

function HistoryItem({ item, onPreview, canViewLogs, onViewLogs }: { item: ImportItem; onPreview: (itemId: string) => void; canViewLogs: (taskId: string) => boolean; onViewLogs: (taskId: string) => void }) {
  const { t } = useTranslation();
  const presentation = presentImportItem(item);
  const statusKey = STATUS_KEYS[item.status] ?? "importV2.history.unknownStatus";
  const warnings = Array.from(new Set(item.preview?.quality.warnings ?? []));
  return (
    <article aria-labelledby={historyDomId("import-history-item", item.itemId)} className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2.5">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h3 id={historyDomId("import-history-item", item.itemId)} className="m-0 truncate text-[12.5px] font-medium text-[var(--text-primary)]" title={item.input.displayName}>{item.input.displayName}</h3>
          <p className="m-0 mt-1 text-[11px] text-[var(--text-muted)]">{t(statusKey)}</p>
          {item.issue ? <p role="alert" className="m-0 mt-1 text-[11px] text-[var(--danger-text)]"><strong>{item.issue.code}</strong>: {item.issue.message}</p> : null}
          {item.attempts.length > 0 ? <details className="mt-2 border-t border-[var(--border-subtle)] pt-2">
            <summary className="cursor-pointer text-[11px] font-medium text-[var(--text-secondary)]">{t("importV2.history.attempts", { count: item.attempts.length })}</summary>
            <ol className="mt-2 space-y-2 border-l border-[var(--border-subtle)] pl-3">
              {item.attempts.map((attempt, index) => <AttemptRow key={`${attempt.startedAt}-${attempt.route}-${index}`} attempt={attempt} index={index} />)}
            </ol>
          </details> : null}
          {warnings.length > 0 ? <details className="mt-2 border-t border-[var(--border-subtle)] pt-2">
            <summary className="cursor-pointer text-[11px] font-medium text-[var(--warning-text)]">{t("importV2.history.qualityWarnings", { count: warnings.length })}</summary>
            <ul className="mt-1 list-disc space-y-1 pl-4 text-[11px] text-[var(--warning-text)]">{warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
          </details> : null}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {item.preview && (presentation.actions.includes("preview_markdown") || item.status === "completed") ? <button type="button" className="btn btn--sm" aria-label={t("importV2.history.previewItemAria", { title: item.input.displayName })} onClick={() => onPreview(item.itemId)}><Eye size={12} className="mr-1 inline" aria-hidden="true" />{t("importV2.history.previewItem")}</button> : null}
          {item.taskId && canViewLogs(item.taskId) ? <button type="button" className="btn btn--sm" aria-label={t("importV2.history.viewItemLogsAria", { title: item.input.displayName })} onClick={() => onViewLogs(item.taskId!)}><ScrollText size={12} className="mr-1 inline" aria-hidden="true" />{t("importV2.history.viewItemLogs")}</button> : null}
        </div>
      </div>
    </article>
  );
}

function AttemptRow({ attempt, index }: { attempt: AttemptRecord; index: number }) {
  const { t } = useTranslation();
  const outcomeKey = `importV2.history.outcome.${attempt.outcome}`;
  const duration = formatAttemptDuration(attempt.startedAt, attempt.completedAt, t);
  return (
    <li className="text-[11px]">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-[var(--text-primary)]">{t("importV2.history.attempt", { index: index + 1 })}</span>
        <span className="text-[var(--text-muted)]">{t(outcomeKey, { defaultValue: attempt.outcome })}</span>
      </div>
      <div className="mt-0.5 break-words font-mono text-[10.5px] text-[var(--text-muted)]">
        {attempt.route} · {attempt.engineId} {attempt.engineVersion} · {attempt.stage}
      </div>
      {duration ? <div className="mt-0.5 text-[10.5px] text-[var(--text-muted)]">{t("importV2.history.duration", { duration })}</div> : null}
      {attempt.warnings.length > 0 ? <ul className="mt-1 list-disc space-y-0.5 pl-4 text-[10.5px] text-[var(--warning-text)]">{attempt.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul> : null}
    </li>
  );
}

function formatAttemptDuration(startedAt: string, completedAt: string | null, t: TFunction): string | null {
  if (!completedAt) return null;
  const start = Date.parse(startedAt);
  const end = Date.parse(completedAt);
  if (Number.isNaN(start) || Number.isNaN(end)) return null;
  const milliseconds = Math.max(0, end - start);
  if (milliseconds < 1000) return t("importV2.history.durationMs", { value: milliseconds });
  if (milliseconds < 60_000) return t("importV2.history.durationSeconds", { value: (milliseconds / 1000).toFixed(1) });
  const minutes = Math.floor(milliseconds / 60_000);
  const seconds = Math.round((milliseconds % 60_000) / 1000);
  return seconds === 0 ? t("importV2.history.durationMinutes", { value: minutes }) : t("importV2.history.durationMinutesSeconds", { minutes, seconds });
}

function historyDomId(prefix: string, value: string): string {
  return `${prefix}-${encodeURIComponent(value).replace(/%/g, "_")}`;
}

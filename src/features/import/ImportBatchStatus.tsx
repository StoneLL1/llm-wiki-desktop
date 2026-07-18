import { CircleAlert, CircleCheck, CircleX, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportBatchProgress } from "./useImportWorkflow";

export interface ImportBatchStatusProps {
  batch: ImportBatchProgress | null | undefined;
  isCancelling?: boolean;
  onCancel: (batchId?: string) => void;
  onDismiss: (batchId?: string) => void;
  onRetryFailed?: (batchId: string) => void;
  onViewTask?: (taskId: string) => void;
}

export function ImportBatchStatus({
  batch,
  isCancelling = false,
  onCancel,
  onDismiss,
  onRetryFailed,
  onViewTask,
}: ImportBatchStatusProps) {
  const { t } = useTranslation();
  if (!batch) return null;

  const isActive = batch.active > 0;
  const waitingForConfirmation = batch.waitingForConfirmation ?? 0;
  const reviewReady = batch.reviewReady ?? 0;
  const waitingForAction = Math.max(0, waitingForConfirmation - reviewReady);
  const hasUnknown = batch.unknown > 0;
  const hasCancelled = batch.cancelled > 0;
  const canCancel = batch.tasks.some((task) => task.status !== "unknown" && task.cancellable && !["succeeded", "failed", "cancelled", "waiting_for_confirmation"].includes(task.status));
  const percent = batch.total > 0 ? Math.round((batch.processed / batch.total) * 100) : 0;

  return (
    <section className="mx-4 mb-3 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-muted)] px-3 py-2.5">
      <div className="flex items-start gap-3" role="status" aria-live="polite" aria-busy={isActive}>
        {isActive ? <LoaderCircle size={16} className="mt-0.5 shrink-0 animate-spin text-[var(--accent)]" aria-hidden="true" /> : batch.failed > 0 || hasUnknown ? <CircleAlert size={16} className={`mt-0.5 shrink-0 ${batch.failed > 0 ? "text-[var(--danger)]" : "text-[var(--warning)]"}`} aria-hidden="true" /> : hasCancelled ? <CircleX size={16} className="mt-0.5 shrink-0 text-[var(--warning)]" aria-hidden="true" /> : <CircleCheck size={16} className="mt-0.5 shrink-0 text-[var(--accent)]" aria-hidden="true" />}
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[12px] font-medium text-[var(--text-primary)]">
            <span>{t("importV2.batch.title")}</span>
            <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.batch.progress", { processed: batch.processed, total: batch.total })}</span>
          </div>
          <div className="mt-0.5 text-[11px] text-[var(--text-secondary)]">
            {t("importV2.batch.summary", { completed: batch.completed, reviewReady, waitingForAction, failed: batch.failed, cancelled: batch.cancelled })}
            {batch.unknown > 0 ? <span> · {t("importV2.batch.unknownSummary", { count: batch.unknown })}</span> : null}
            {batch.nonCancellable > 0 ? <span> · {t("importV2.batch.nonCancellable", { count: batch.nonCancellable })}</span> : null}
          </div>
          <div className="mt-1 h-1 overflow-hidden rounded-full bg-[var(--border)]" aria-hidden="true">
            <div className="h-full rounded-full bg-[var(--accent)] transition-[width]" style={{ width: `${percent}%` }} />
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {isActive && canCancel ? (
            <button type="button" className="btn btn--sm" disabled={isCancelling || batch.cancelling > 0} onClick={() => onCancel(batch.id)}>
              {isCancelling || batch.cancelling > 0 ? t("importV2.batch.cancelling") : t("importV2.batch.cancel")}
            </button>
          ) : !isActive ? (
            <>
              {batch.failedItemIds.length > 0 && onRetryFailed ? <button type="button" className="btn btn--sm" onClick={() => onRetryFailed(batch.id)}>{t("importV2.batch.retryFailed")}</button> : null}
              <button type="button" className="icon-button" aria-label={t("importV2.batch.dismiss")} title={t("importV2.batch.dismiss")} onClick={() => onDismiss(batch.id)}>
                <X size={14} aria-hidden="true" />
              </button>
            </>
          ) : null}
        </div>
      </div>
      {batch.tasks.length > 0 ? (
        <details className="mt-2 border-t border-[var(--border)] pt-2">
          <summary className="cursor-pointer select-none text-[11px] text-[var(--text-secondary)] hover:text-[var(--text-primary)]">
            {t("importV2.batch.viewTasks", { count: batch.tasks.length })}
          </summary>
          <ul className="m-0 mt-1.5 max-h-48 list-none space-y-1 overflow-y-auto p-0 pr-1" aria-label={t("importV2.batch.taskList")}>
            {batch.tasks.map((task) => (
              <li key={task.id} className="flex min-w-0 items-center gap-2 text-[11px]">
                <span className="min-w-0 flex-1 truncate text-[var(--text-secondary)]" title={task.title}>{task.title}</span>
                <span className="shrink-0 text-[var(--text-muted)]">{task.status === "unknown" ? t("importV2.batch.unknownStatus") : t(`task.status.${task.status}`)}</span>
                {onViewTask && task.status !== "unknown" ? <button type="button" className="btn btn--sm shrink-0" aria-label={t("importV2.batch.viewTaskAria", { title: task.title })} onClick={() => onViewTask(task.id)}>{t("importV2.batch.viewTask")}</button> : null}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      {batch.unknown > 0 ? <p className="m-0 mt-2 border-t border-[var(--border)] pt-2 text-[11px] text-[var(--text-muted)]">{t("importV2.batch.unknownTasks", { count: batch.unknown })}</p> : null}
    </section>
  );
}

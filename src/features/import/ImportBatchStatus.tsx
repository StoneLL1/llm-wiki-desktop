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

  const summary = [
    t("importV2.batch.title"),
    t("importV2.batch.progress", { processed: batch.processed, total: batch.total }),
    waitingForAction > 0 ? t("importV2.commit.pending", { count: waitingForAction }) : null,
    reviewReady > 0 ? t("importV2.batch.readyCount", { count: reviewReady }) : null,
    batch.failed > 0 ? t("importV2.completion.failedCount", { count: batch.failed }) : null,
    hasCancelled ? t("importV2.batch.cancelledCount", { count: batch.cancelled }) : null,
    hasUnknown ? t("importV2.batch.unknownSummary", { count: batch.unknown }) : null,
    batch.nonCancellable > 0 ? t("importV2.batch.nonCancellable", { count: batch.nonCancellable }) : null,
  ].filter(Boolean).join(" · ");

  return (
    <section className="import-v2-status-row import-v2-batch">
      <details className="import-v2-status-details">
        <summary aria-label={`${t("importV2.batch.viewTasks", { count: batch.tasks.length })}: ${summary}`}>
          {isActive ? <LoaderCircle size={15} className="animate-spin text-[var(--accent)]" aria-hidden="true" /> : batch.failed > 0 || hasUnknown ? <CircleAlert size={15} className="text-[var(--warning)]" aria-hidden="true" /> : hasCancelled ? <CircleX size={15} className="text-[var(--text-muted)]" aria-hidden="true" /> : <CircleCheck size={15} className="text-[var(--accent)]" aria-hidden="true" />}
          <span role="status" aria-live="polite" aria-busy={isActive}>{summary}</span>
        </summary>
        <div className="import-v2-status-details__body">
          <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("importV2.batch.summary", { completed: batch.completed, reviewReady, waitingForAction, failed: batch.failed, cancelled: batch.cancelled })}</p>
          <ul className="m-0 mt-2 grid max-h-48 list-none gap-1 overflow-y-auto p-0" aria-label={t("importV2.batch.taskList")}>
            {batch.tasks.map((task) => (
              <li key={task.id} className="flex min-w-0 items-center gap-2 text-[12px]">
                <span className="min-w-0 flex-1 truncate" title={task.title}>{task.title}</span>
                <span className="shrink-0 text-[var(--text-muted)]">{task.status === "unknown" ? t("importV2.batch.unknownStatus") : t(`task.status.${task.status}`)}</span>
                {onViewTask && task.status !== "unknown" ? <button type="button" className="btn btn--sm shrink-0" aria-label={t("importV2.batch.viewTaskAria", { title: task.title })} onClick={() => onViewTask(task.id)}>{t("importV2.batch.viewTask")}</button> : null}
              </li>
            ))}
          </ul>
          {hasUnknown ? <p className="mb-0 text-[12px] text-[var(--text-muted)]">{t("importV2.batch.unknownTasks", { count: batch.unknown })}</p> : null}
        </div>
      </details>
      <div className="import-v2-status-row__actions">
        {isActive && canCancel ? (
          <button type="button" className="btn btn--sm" disabled={isCancelling || batch.cancelling > 0} onClick={() => onCancel(batch.id)}>
            {isCancelling || batch.cancelling > 0 ? t("importV2.batch.cancelling") : t("importV2.batch.cancel")}
          </button>
        ) : !isActive ? <>
          {batch.failedItemIds.length > 0 && onRetryFailed ? <button type="button" className="btn btn--sm" onClick={() => onRetryFailed(batch.id)}>{t("importV2.batch.retryFailed")}</button> : null}
          <button type="button" className="icon-button" aria-label={t("importV2.batch.dismiss")} title={t("importV2.batch.dismiss")} onClick={() => onDismiss(batch.id)}><X size={14} aria-hidden="true" /></button>
        </> : null}
      </div>
      {isActive ? <progress className="import-v2-batch__progress" value={percent} max={100} aria-label={t("importV2.batch.progress", { processed: batch.processed, total: batch.total })} /> : null}
    </section>
  );
}

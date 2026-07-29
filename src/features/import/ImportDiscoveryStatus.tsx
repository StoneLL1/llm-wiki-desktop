import { CircleAlert, CircleCheck, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { FileScanResult } from "../../types/importV2File";
import type { BackendTask } from "../../types/task";

export interface ImportDiscoveryStatusProps {
  task: BackendTask | null;
  scan?: FileScanResult | null;
  unavailable?: boolean;
  onCancel: () => void;
  onDismiss: () => void;
  onConfirmLargeData?: (paths: string[]) => void | Promise<unknown>;
  cancelling?: boolean;
  confirmingLargeData?: boolean;
}

function parseDiscoverySummary(summary: string | undefined): { added: number; skipped: number } | null {
  if (!summary) return null;
  const match = summary.match(/added\s+(\d+)\s+files?\s*;\s*skipped\s+(\d+)/i);
  return match ? { added: Number(match[1]), skipped: Number(match[2]) } : null;
}

export function ImportDiscoveryStatus({
  task,
  scan = null,
  unavailable = false,
  onCancel,
  onDismiss,
  onConfirmLargeData,
  cancelling = false,
  confirmingLargeData = false,
}: ImportDiscoveryStatusProps) {
  const { t, i18n } = useTranslation();
  if (!task) {
    if (!unavailable) return null;
    return (
      <section className="mx-4 mb-3 flex items-center gap-2 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11px] text-[var(--warning-text)]" role="alert">
        <CircleAlert size={15} className="shrink-0" aria-hidden="true" />
        <span className="min-w-0 flex-1">{t("importV2.discovery.interrupted")}</span>
        <button type="button" className="icon-button" aria-label={t("importV2.discovery.dismiss")} title={t("importV2.discovery.dismiss")} onClick={onDismiss}><X size={14} aria-hidden="true" /></button>
      </section>
    );
  }

  const isActive = task.status === "queued" || task.status === "running" || task.status === "cancelling";
  const cancelRequested = cancelling || task.status === "cancelling";
  const discovered = task.progress?.current ?? 0;
  const resultSummary = parseDiscoverySummary(task.result?.summary);
  const liveSummary = resultSummary ?? { added: discovered, skipped: 0 };
  const skipped = scan?.skipped ?? [];
  const pendingLargePaths = skipped
    .filter((entry) => entry.reason === "large_data_confirmation_required")
    .map((entry) => entry.sourcePath);
  const pendingLarge = (scan?.files ?? []).filter((file) => pendingLargePaths.includes(file.sourcePath));
  const mismatches = (scan?.files ?? []).filter((file) => file.identity.extensionMismatch);
  const number = new Intl.NumberFormat(i18n.language);

  const skippedDetails = skipped.length > 0 ? (
    <details className="basis-full min-w-0 border-t border-[var(--border-subtle)] pt-2 text-[11px]">
      <summary className="cursor-pointer text-[var(--text-secondary)]">
        {t("importV2.discovery.skippedDetails", { count: skipped.length })}
      </summary>
      <ul className="mt-2 max-h-40 space-y-1 overflow-y-auto pl-4">
        {skipped.slice(0, 50).map((entry, index) => (
          <li key={`${entry.sourcePath}-${index}`} className="min-w-0" title={entry.detail ?? undefined}>
            <div className="truncate font-mono text-[10.5px] text-[var(--text-primary)]">
              {entry.relativePath ?? entry.sourcePath}
            </div>
            <div className="text-[10.5px] text-[var(--text-muted)]">
              {t(`importV2.discovery.reason.${entry.reason}`, { defaultValue: entry.detail ?? entry.reason })}
            </div>
          </li>
        ))}
      </ul>
      {skipped.length > 50 ? <p className="m-0 mt-1 text-[10.5px] text-[var(--text-muted)]">{t("importV2.discovery.skippedMore", { count: skipped.length - 50 })}</p> : null}
    </details>
  ) : null;
  const formatDetails = mismatches.length > 0 ? (
    <details className="basis-full min-w-0 border-t border-[var(--border-subtle)] pt-2 text-[11px]">
      <summary className="cursor-pointer text-[var(--text-secondary)]">
        {t("importV2.discovery.detectedFormats", { count: mismatches.length })}
      </summary>
      <ul className="mt-2 max-h-32 space-y-1 overflow-y-auto pl-4">
        {mismatches.map((file) => (
          <li key={file.sourcePath} className="flex min-w-0 gap-2">
            <span className="min-w-0 flex-1 truncate font-mono text-[10.5px]" title={file.relativePath}>{file.relativePath}</span>
            <span className="shrink-0 text-[var(--text-muted)]">
              {t("importV2.discovery.detectedFormat", {
                extension: file.identity.extension || t("importV2.discovery.noExtension"),
                format: file.format.toUpperCase(),
              })}
            </span>
          </li>
        ))}
      </ul>
    </details>
  ) : null;
  const largeDataConfirmation = pendingLarge.length > 0 ? (
    <section className="basis-full min-w-0 border-t border-[var(--border-subtle)] pt-2" aria-label={t("importV2.discovery.largeDataTitle")}>
      <div className="flex min-w-0 flex-wrap items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="m-0 font-medium text-[var(--text-primary)]">{t("importV2.discovery.largeDataTitle")}</p>
          {pendingLarge.map((file) => (
            <p key={file.sourcePath} className="m-0 mt-1 break-words text-[10.5px] text-[var(--text-muted)]">
              <span className="font-mono text-[var(--text-secondary)]">{file.relativePath}</span>
              {" · "}
              {t("importV2.discovery.largeDataEstimate", {
                rows: number.format(file.largeData?.rowCount ?? 0),
                files: number.format(file.largeData?.estimatedOutputFiles ?? 0),
                bytes: number.format(file.largeData?.totalBytes ?? file.sizeBytes),
              })}
            </p>
          ))}
        </div>
        <button
          type="button"
          className="btn btn--sm btn--primary"
          disabled={!onConfirmLargeData || confirmingLargeData}
          aria-busy={confirmingLargeData}
          onClick={() => void onConfirmLargeData?.(pendingLargePaths)}
        >
          {confirmingLargeData ? <LoaderCircle size={14} className="animate-spin" aria-hidden="true" /> : null}
          {t(confirmingLargeData ? "importV2.discovery.largeDataConfirming" : "importV2.discovery.largeDataConfirm")}
        </button>
      </div>
    </section>
  ) : null;

  if (isActive) {
    return (
      <section className="mx-4 mb-3 flex items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-muted)] px-3 py-2.5" role="status" aria-live="polite" aria-busy="true">
        <LoaderCircle size={16} className="shrink-0 animate-spin text-[var(--accent)]" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[12px] font-medium text-[var(--text-primary)]">
            <span>{t("importV2.discovery.scanning")}</span>
            <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.discovery.stage")}</span>
          </div>
          <div className="mt-0.5 flex items-center gap-2 text-[11px] text-[var(--text-secondary)]">
            <span>{t("importV2.discovery.discovered", { count: discovered })}</span>
            <span>{t("importV2.discovery.added", { count: liveSummary.added })}</span>
            <span>{t("importV2.discovery.skipped", { count: liveSummary.skipped })}</span>
          </div>
          <div className="mt-1 h-1 overflow-hidden rounded-full bg-[var(--border)]" aria-hidden="true"><div className="h-full w-full rounded-full bg-[var(--accent)] animate-pulse opacity-60" /></div>
        </div>
        {task.cancellable ? <button type="button" className="btn btn--sm" disabled={cancelRequested} aria-busy={cancelRequested} onClick={onCancel}>{t(cancelRequested ? "importV2.discovery.cancelling" : "importV2.discovery.cancel")}</button> : null}
      </section>
    );
  }

  if (task.status === "succeeded") {
    return (
      <section className="mx-4 mb-3 flex flex-wrap items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-muted)] px-3 py-2 text-[11px] text-[var(--text-secondary)]" role="status" aria-live="polite">
        <CircleCheck size={15} className="shrink-0 text-[var(--accent)]" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate">
          {t("importV2.discovery.complete")}
          {discovered > 0 ? ` · ${t("importV2.discovery.discovered", { count: discovered })}` : ""}
          {resultSummary ? ` · ${t("importV2.discovery.added", { count: resultSummary.added })} · ${t("importV2.discovery.skipped", { count: resultSummary.skipped })}` : ""}
        </span>
        <button type="button" className="icon-button" aria-label={t("importV2.discovery.dismiss")} title={t("importV2.discovery.dismiss")} onClick={onDismiss}><X size={14} aria-hidden="true" /></button>
        {largeDataConfirmation}
        {formatDetails}
        {skippedDetails}
      </section>
    );
  }

  return (
    <section className="mx-4 mb-3 flex items-center gap-2 rounded-[var(--radius-md)] border border-[var(--danger)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger-text)]" role="alert">
      <CircleAlert size={15} className="shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <p className="m-0">{task.status === "cancelled" ? t("importV2.discovery.cancelled") : t("importV2.discovery.failed")}</p>
        {task.status !== "cancelled" && task.error?.message ? <p className="m-0 mt-0.5 break-words text-[10.5px] text-[var(--danger-text)]">{t("importV2.discovery.errorDetail", { message: task.error.message })}</p> : null}
      </div>
      <button type="button" className="icon-button" aria-label={t("importV2.discovery.dismiss")} title={t("importV2.discovery.dismiss")} onClick={onDismiss}><X size={14} aria-hidden="true" /></button>
    </section>
  );
}

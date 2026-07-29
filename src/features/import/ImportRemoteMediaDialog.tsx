import { Download, HardDrive, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { RemoteMediaRetentionPlan } from "../../types/importV2Web";

interface ImportRemoteMediaDialogProps {
  plan: RemoteMediaRetentionPlan | null;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}

function formatBytes(value: number | null): string {
  if (value === null) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && size >= 1024; index += 1) {
    size /= 1024;
    unit = units[index];
  }
  return `${size >= 10 ? size.toFixed(0) : size.toFixed(1)} ${unit}`;
}

export function ImportRemoteMediaDialog({ plan, onConfirm, onCancel }: ImportRemoteMediaDialogProps) {
  const { t } = useTranslation();
  const open = plan !== null;
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setAcknowledged(false);
    setBusy(false);
  }, [plan?.itemId]);

  if (!plan) return null;

  async function confirm() {
    if (!acknowledged) return;
    setBusy(true);
    try {
      await onConfirm();
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
      aria-labelledby="import-remote-media-title"
    >
      <section className="w-full max-w-[520px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <Download size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-remote-media-title" className="m-0 flex-1 text-[15px] font-semibold">
            {t("importV2.remoteMedia.title")}
          </h2>
          <button type="button" className="icon-button" aria-label={t("common.close")} title={t("common.close")} onClick={onCancel}>
            <X size={16} aria-hidden="true" />
          </button>
        </header>
        <div className="space-y-3 px-4 py-4">
          <p className="m-0 text-[12px] leading-5 text-[var(--text-secondary)]">
            {t("importV2.remoteMedia.description")}
          </p>
          <dl className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)] px-3 py-2.5 text-[12px]">
            <dt className="text-[var(--text-muted)]">{t("importV2.remoteMedia.quality")}</dt>
            <dd className="m-0 font-medium">{t("importV2.remoteMedia.bestAvailable")}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.remoteMedia.estimatedSize")}</dt>
            <dd className="m-0 font-mono">{formatBytes(plan.estimatedBytes)}</dd>
            <dt className="flex items-center gap-1.5 text-[var(--text-muted)]">
              <HardDrive size={13} aria-hidden="true" />
              {t("importV2.remoteMedia.availableDisk")}
            </dt>
            <dd className="m-0 font-mono">{formatBytes(plan.availableDiskBytes)}</dd>
          </dl>
          {plan.enoughDisk === false ? (
            <p className="m-0 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-subtle)] px-3 py-2 text-[11px] leading-4 text-[var(--warning-text)]" role="alert">
              {t("importV2.remoteMedia.insufficientDisk")}
            </p>
          ) : null}
          <label className="flex cursor-pointer items-start gap-2 text-[12px] leading-5">
            <input
              type="checkbox"
              className="mt-1"
              checked={acknowledged}
              disabled={plan.enoughDisk === false}
              onChange={(event) => setAcknowledged(event.target.checked)}
            />
            <span>{t("importV2.remoteMedia.acknowledge")}</span>
          </label>
        </div>
        <footer className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={onCancel} disabled={busy}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--sm btn--primary"
            onClick={() => void confirm()}
            disabled={busy || !acknowledged || plan.enoughDisk === false}
          >
            {busy ? <LoaderCircle size={13} className="mr-1 inline animate-spin" aria-hidden="true" /> : null}
            {t("importV2.remoteMedia.confirm")}
          </button>
        </footer>
      </section>
    </div>
  );
}

import { Check, Download, LoaderCircle, Package, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { ImportCapabilityRequirement } from "../../types/importV2Presentation";

export interface ImportCapabilityDialogProps {
  open: boolean;
  requirement: ImportCapabilityRequirement | null;
  onInstall: (capabilityId: string) => Promise<void> | void;
  onCancel: () => void;
}

function bytes(value: number | null): string {
  if (value === null) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

export function ImportCapabilityDialog({ open, requirement, onInstall, onCancel }: ImportCapabilityDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setAcknowledged(false);
    setBusy(false);
  }, [open, requirement?.requirement.capabilityId, requirement?.requirement.minimumVersion]);
  if (!open || !requirement) return null;
  const canInstall = !requirement.available && requirement.installable && acknowledged && !busy;
  async function install() {
    if (!canInstall) return;
    setBusy(true);
    try {
      await onInstall(requirement!.requirement.capabilityId);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-capability-title">
      <section className="flex max-h-[84vh] w-full max-w-[680px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <Package size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-capability-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.capability.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.capability.cancel")} onClick={onCancel}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 text-[12px]">
          <dl className="grid grid-cols-[150px_1fr] gap-x-4 gap-y-1.5">
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.purpose")}</dt><dd className="m-0">{requirement.requirement.capabilityId} · {requirement.route}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.version")}</dt><dd className="m-0">{requirement.requirement.minimumVersion ?? "—"}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.protocol")}</dt><dd className="m-0">{requirement.requirement.protocolVersion}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.platform")}</dt><dd className="m-0 font-mono text-[11px]">{requirement.requirement.targetTriple}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.compressed")}</dt><dd className="m-0">{bytes(requirement.compressedBytes)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.installed")}</dt><dd className="m-0">{bytes(requirement.installedBytes)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.model")}</dt><dd className="m-0">{bytes(requirement.modelBytes)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.license")}</dt><dd className="m-0">{requirement.license ?? requirement.requirement.acceptedLicenseExpressions.join(", ")}</dd>
          </dl>
          {requirement.fallback ? <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--text-muted)]"><strong>{t("importV2.capability.fallback")}:</strong> {requirement.fallback}</p> : null}
          {requirement.available ? <p className="mt-3 flex items-center gap-1.5 text-[var(--success-text)]" role="status"><Check size={14} aria-hidden="true" />{t("importV2.capability.installedState")}</p> : null}
          {!requirement.installable ? <p className="mt-3 text-[11px] text-[var(--warning-text)]" role="alert">{t("importV2.capability.unavailable")}</p> : null}
          {!requirement.available && requirement.installable ? <label className="mt-3 flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2"><input type="checkbox" aria-label={t("importV2.capability.installAck")} checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={busy} /><span>{t("importV2.capability.installAck")}</span></label> : null}
        </div>
        <footer className="flex items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={onCancel} disabled={busy}>{t("importV2.capability.cancel")}</button>
          {!requirement.available ? <button type="button" className="btn btn--sm btn--primary" onClick={() => void install()} disabled={!canInstall} title={!requirement.installable ? t("importV2.capability.unavailable") : undefined}><Download size={13} className="mr-1 inline" aria-hidden="true" />{busy ? <LoaderCircle size={13} className="animate-spin" aria-label={t("importV2.common.loading")} /> : t("importV2.capability.install")}</button> : null}
        </footer>
      </section>
    </div>
  );
}

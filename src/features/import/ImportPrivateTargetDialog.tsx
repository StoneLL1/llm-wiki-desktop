import { KeyRound, LoaderCircle, ShieldAlert, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";

export interface ImportPrivateTargetDialogProps {
  open: boolean;
  itemId: string;
  target: string;
  addressCategory: string;
  reason: string;
  onAuthorize: (itemId: string, target: string) => Promise<void> | void;
  onCancel: () => void;
}

export function ImportPrivateTargetDialog({ open, itemId, target, addressCategory, reason, onAuthorize, onCancel }: ImportPrivateTargetDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const [busy, setBusy] = useState(false);
  if (!open) return null;
  async function authorize() {
    setBusy(true);
    try {
      await onAuthorize(itemId, target);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-private-title">
      <section className="w-full max-w-[620px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <ShieldAlert size={17} className="text-[var(--warning-text)]" aria-hidden="true" />
          <h2 id="import-private-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.private.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.private.cancel")} title={t("importV2.private.cancel")} onClick={onCancel}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="space-y-3 px-4 py-4 text-[12px]">
          <dl className="grid grid-cols-[130px_1fr] gap-x-4 gap-y-1.5">
            <dt className="text-[var(--text-muted)]">{t("importV2.private.target")}</dt><dd className="m-0 break-all font-mono text-[11px]">{target}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.private.category")}</dt><dd className="m-0">{addressCategory}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.private.reason")}</dt><dd className="m-0">{reason}</dd>
          </dl>
          <p className="m-0 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("importV2.private.scope")}</p>
        </div>
        <footer className="flex items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm btn--primary" onClick={onCancel} disabled={busy}>{t("importV2.private.cancel")}</button>
          <button type="button" className="btn btn--sm" onClick={() => void authorize()} disabled={busy}><KeyRound size={13} className="mr-1 inline" aria-hidden="true" />{busy ? <LoaderCircle size={13} className="animate-spin" aria-label={t("importV2.common.loading")} /> : t("importV2.private.authorize")}</button>
        </footer>
      </section>
    </div>
  );
}

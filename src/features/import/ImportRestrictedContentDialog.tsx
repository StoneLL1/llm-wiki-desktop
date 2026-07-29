import { LockKeyhole, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";

interface ImportRestrictedContentDialogProps {
  open: boolean;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}

export function ImportRestrictedContentDialog({
  open,
  onConfirm,
  onCancel,
}: ImportRestrictedContentDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  if (!open) return null;

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-restricted-title"
    >
      <section className="w-full max-w-[520px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <LockKeyhole size={17} className="text-[var(--warning)]" aria-hidden="true" />
          <h2 id="import-restricted-title" className="m-0 flex-1 text-[15px] font-semibold">
            {t("importV2.restricted.title")}
          </h2>
          <button type="button" className="icon-button" aria-label={t("common.close")} title={t("common.close")} onClick={onCancel}>
            <X size={16} aria-hidden="true" />
          </button>
        </header>
        <div className="space-y-3 px-4 py-4 text-[12px] leading-5 text-[var(--text-secondary)]">
          <p className="m-0">{t("importV2.restricted.description")}</p>
          <p className="m-0 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-subtle)] px-3 py-2 text-[var(--warning-text)]">
            {t("importV2.restricted.sharingWarning")}
          </p>
          <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.restricted.once")}</p>
        </div>
        <footer className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={onCancel}>
            {t("common.cancel")}
          </button>
          <button type="button" className="btn btn--sm btn--primary" onClick={() => void onConfirm()}>
            {t("importV2.restricted.confirm")}
          </button>
        </footer>
      </section>
    </div>
  );
}

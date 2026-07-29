import { LockKeyhole, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";

interface ExportRestrictedContentDialogProps {
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ExportRestrictedContentDialog({
  count,
  onConfirm,
  onCancel,
}: ExportRestrictedContentDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open: true, onClose: onCancel });
  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="export-restricted-title"
    >
      <section className="w-full max-w-[520px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <LockKeyhole size={17} className="text-[var(--warning)]" aria-hidden="true" />
          <h2 id="export-restricted-title" className="m-0 flex-1 text-[15px] font-semibold">
            {t("exports.restricted.title")}
          </h2>
          <button type="button" className="icon-button" aria-label={t("common.close")} onClick={onCancel}>
            <X size={16} aria-hidden="true" />
          </button>
        </header>
        <div className="px-4 py-4">
          <p className="m-0 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-subtle)] px-3 py-2.5 text-[12px] leading-5 text-[var(--warning-text)]">
            {t("exports.restricted.warning", { count })}
          </p>
        </div>
        <footer className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={onCancel}>{t("common.cancel")}</button>
          <button type="button" className="btn btn--sm btn--primary" onClick={onConfirm}>
            {t("exports.restricted.confirm")}
          </button>
        </footer>
      </section>
    </div>
  );
}

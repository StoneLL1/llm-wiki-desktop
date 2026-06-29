import { useTranslation } from "react-i18next";
import { GitBranch, ShieldAlert } from "lucide-react";

import { Button } from "../../components/ui/button";
import { useModalDialog } from "../../hooks/useModalDialog";

interface LintBatchConfirmDialogProps {
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
}

export function LintBatchConfirmDialog({
  count,
  onConfirm,
  onCancel,
}: LintBatchConfirmDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open: true, onClose: onCancel });

  return (
    <div
      ref={dialogRef}
      aria-modal="true"
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-labelledby="lint-batch-dialog-title"
      tabIndex={-1}
    >
      <section className="w-full max-w-[520px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <span className="text-[var(--warning)]" aria-hidden="true">
            <ShieldAlert size={18} />
          </span>
          <h2
            id="lint-batch-dialog-title"
            className="text-[16px] font-semibold leading-tight text-[var(--text-primary)]"
          >
            {t("lint.batch.title", { count })}
          </h2>
        </header>

        <div className="space-y-3 px-4 py-4 text-[13px] leading-5 text-[var(--text-secondary)]">
          <p>{t("lint.batch.message")}</p>
          <div className="flex items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface)] px-3 py-2 text-[12px]">
            <GitBranch size={14} aria-hidden="true" />
            <span>{t("lint.batch.checkpoint")}</span>
          </div>
        </div>

        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <Button type="button" variant="secondary" onClick={onCancel}>
            {t("lint.batch.cancel")}
          </Button>
          <Button type="button" variant="primary" onClick={onConfirm}>
            {t("lint.batch.confirm")}
          </Button>
        </footer>
      </section>
    </div>
  );
}

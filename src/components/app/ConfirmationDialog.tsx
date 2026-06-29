import { useTranslation } from "react-i18next";
import { GitBranch, ShieldAlert } from "lucide-react";
import { Button } from "../ui/button";
import { useModalDialog } from "../../hooks/useModalDialog";
import type { PendingAction } from "../../types/backend";

interface ConfirmationDialogProps {
  action: PendingAction;
  checkpointExists: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

const confirmLabelKeys: Record<PendingAction["actionType"], string> = {
  initialize_folder: "confirmation.confirm.initialize_folder",
  delete_file: "confirmation.confirm.delete_file",
  overwrite_file: "confirmation.confirm.overwrite_file",
  batch_rewrite: "confirmation.confirm.batch_rewrite",
  replace_source: "confirmation.confirm.replace_source",
  delete_source: "confirmation.confirm.delete_source",
  merge_conflict: "confirmation.confirm.merge_conflict",
  agent_auto_fix: "confirmation.confirm.agent_auto_fix",
  install_agent: "confirmation.confirm.install_agent",
  run_skill: "confirmation.confirm.run_skill",
};

export function ConfirmationDialog({
  action,
  checkpointExists,
  onCancel,
  onConfirm,
}: ConfirmationDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open: true, onClose: onCancel });
  const isDestructive = action.riskLevel === "destructive";

  return (
    <div
      ref={dialogRef}
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-labelledby="confirmation-dialog-title"
      tabIndex={-1}
    >
      <section className="w-full max-w-[560px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <span
            className={
              isDestructive
                ? "text-[var(--danger)]"
                : "text-[var(--warning)]"
            }
            aria-hidden="true"
          >
            <ShieldAlert size={18} />
          </span>
          <div className="min-w-0">
            <h2
              id="confirmation-dialog-title"
              className="truncate text-[16px] font-semibold leading-tight text-[var(--text-primary)]"
            >
              {action.title}
            </h2>
            <p className="mt-1 text-[12px] text-[var(--text-muted)]">
              {t("confirmation.risk", { risk: action.riskLevel })}
            </p>
          </div>
        </header>

        <div className="max-h-[65vh] space-y-4 overflow-y-auto px-4 py-4 text-[13px] text-[var(--text-secondary)]">
          <p className="leading-5">{action.message}</p>

          <div className="flex items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface)] px-3 py-2 text-[12px]">
            <GitBranch size={14} aria-hidden="true" />
            <span>
              {t(
                checkpointExists
                  ? "confirmation.checkpoint.available"
                  : "confirmation.checkpoint.missing",
              )}
            </span>
          </div>

          {action.preview ? (
            <div className="space-y-2">
              <h3 className="text-[12px] font-medium text-[var(--text-primary)]">
                {t("confirmation.preview")}
              </h3>
              <p className="text-[12px] text-[var(--text-muted)]">
                {action.preview.summary}
              </p>
              {action.preview.diff ? (
                <pre className="max-h-[180px] overflow-auto rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface-muted)] p-3 font-mono text-[11px] leading-5 text-[var(--text-primary)]">
                  {action.preview.diff.split("\n").map((line, index) => (
                    <span key={`${line}-${index}`} className="block">
                      {line}
                    </span>
                  ))}
                </pre>
              ) : null}
            </div>
          ) : null}

          <div className="space-y-2">
            <h3 className="text-[12px] font-medium text-[var(--text-primary)]">
              {t("confirmation.affectedPaths")}
            </h3>
            <ul className="max-h-[144px] overflow-auto rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface)]">
              {action.affectedPaths.map((path) => (
                <li
                  key={path}
                  className="border-b border-[var(--border-subtle)] px-3 py-2 font-mono text-[11px] last:border-b-0"
                >
                  {path}
                </li>
              ))}
            </ul>
          </div>
        </div>

        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <Button type="button" variant="secondary" onClick={onCancel}>
            {t("confirmation.cancel")}
          </Button>
          <Button
            type="button"
            variant={isDestructive ? "danger" : "secondary"}
            onClick={onConfirm}
          >
            {t(confirmLabelKeys[action.actionType])}
          </Button>
        </footer>
      </section>
    </div>
  );
}

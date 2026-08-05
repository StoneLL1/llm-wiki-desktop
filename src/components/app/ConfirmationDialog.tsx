import { useTranslation } from "react-i18next";
import { GitBranch, ShieldAlert } from "lucide-react";
import { Button } from "../ui/button";
import { useModalDialog } from "../../hooks/useModalDialog";
import type { PendingAction } from "../../types/backend";

interface ConfirmationDialogProps {
  action: PendingAction;
  busy?: boolean;
  checkpointExists: boolean;
  error?: string | null;
  onCancel: () => void;
  onConfirm: () => void;
}

const confirmLabelKeys: Record<PendingAction["actionType"], string> = {
  repair_project: "confirmation.confirm.repair_project",
  delete_file: "confirmation.confirm.delete_file",
  overwrite_file: "confirmation.confirm.overwrite_file",
  batch_rewrite: "confirmation.confirm.batch_rewrite",
  merge_conflict: "confirmation.confirm.merge_conflict",
  agent_auto_fix: "confirmation.confirm.agent_auto_fix",
  install_agent: "confirmation.confirm.install_agent",
  run_skill: "confirmation.confirm.run_skill",
  enable_compatible_project: "confirmation.confirm.enable_compatible_project",
  configure_compatible_layout: "confirmation.confirm.configure_compatible_layout",
  trust_compatible_project: "confirmation.confirm.trust_compatible_project",
  initialize_git_repository: "confirmation.confirm.initialize_git_repository",
  create_git_checkpoint: "confirmation.confirm.create_git_checkpoint",
};

const localizedAuthorityActions = new Set<PendingAction["actionType"]>([
  "repair_project",
  "enable_compatible_project",
  "configure_compatible_layout",
  "trust_compatible_project",
  "initialize_git_repository",
  "create_git_checkpoint",
]);

export function ConfirmationDialog({
  action,
  busy = false,
  checkpointExists,
  error = null,
  onCancel,
  onConfirm,
}: ConfirmationDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open: true, onClose: onCancel });
  const isDestructive = action.riskLevel === "destructive";
  const hasLocalizedAuthorityCopy = localizedAuthorityActions.has(action.actionType);
  const title = hasLocalizedAuthorityCopy
    ? t(`confirmation.action.${action.actionType}.title`)
    : action.title;
  const message = hasLocalizedAuthorityCopy
    ? t(`confirmation.action.${action.actionType}.message`)
    : action.message;
  const previewSummary = hasLocalizedAuthorityCopy && action.preview
    ? t(`confirmation.action.${action.actionType}.preview`)
    : action.preview?.summary;

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
              {title}
            </h2>
            <p className="mt-1 text-[12px] text-[var(--text-muted)]">
              {t("confirmation.risk", { risk: action.riskLevel })}
            </p>
          </div>
        </header>

        <div className="max-h-[65vh] space-y-4 overflow-y-auto px-4 py-4 text-[13px] text-[var(--text-secondary)]">
          <p className="leading-5">{message}</p>

          {error ? (
            <p
              className="rounded-[var(--radius-md)] border border-[var(--danger)]/30 bg-[var(--danger)]/5 px-3 py-2 text-[12px] text-[var(--danger)]"
              role="alert"
            >
              {error}
            </p>
          ) : null}

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
                {previewSummary}
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

          {action.affectedPaths.length > 0 ? <div className="space-y-2">
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
          </div> : null}
        </div>

        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <Button type="button" variant="secondary" onClick={onCancel} disabled={busy}>
            {t("confirmation.cancel")}
          </Button>
          <Button
            type="button"
            variant={isDestructive ? "danger" : "secondary"}
            onClick={onConfirm}
            disabled={busy}
          >
            {t(confirmLabelKeys[action.actionType])}
          </Button>
        </footer>
      </section>
    </div>
  );
}

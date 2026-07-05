import { AlertTriangle, Check, FileDiff, RotateCcw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ChatConvenienceEdit } from "../../types/chat";

export interface ChatConveniencePanelProps {
  enabled: boolean;
  pending: boolean;
  edit?: ChatConvenienceEdit | null;
  onSetEnabled: (enabled: boolean) => void;
  onKeep?: () => void;
  onRollback?: () => void;
  onRollbackLast?: () => void;
}

export function ChatConveniencePanel({
  enabled,
  pending,
  edit,
  onSetEnabled,
  onKeep,
  onRollback,
  onRollbackLast,
}: ChatConveniencePanelProps) {
  const { t } = useTranslation();

  if (edit) {
    const softPending = edit.status === "soft_violation_pending";
    const failed = edit.status === "rollback_failed";
    const rolledBack = edit.status === "rolled_back";
    const applied = edit.status === "applied" || edit.status === "kept_after_soft_violation";
    const tone = failed || softPending ? "border-[var(--warning)] bg-[var(--warning-soft)]" : "border-[var(--border)] bg-[var(--surface)]";
    return (
      <div className={`mt-2 grid gap-2 rounded-[var(--radius-md)] border px-3 py-2 text-[12px] ${tone}`}>
        <div className="flex min-w-0 items-center gap-2">
          {applied ? <Check size={14} /> : <AlertTriangle size={14} />}
          <span className="font-medium">{t(`chat.convenience.status.${edit.status}`)}</span>
          <span className="truncate font-mono text-[11px] text-[var(--text-muted)]">{edit.diffSummary}</span>
        </div>
        {edit.violationReason ? (
          <div className="text-[11.5px] text-[var(--text-secondary)]">{edit.violationReason}</div>
        ) : null}
        {edit.affectedPaths.length > 0 ? (
          <div className="flex flex-wrap gap-1">
            {edit.affectedPaths.map((path) => (
              <span
                key={path}
                className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-1.5 py-0.5 font-mono text-[10.5px] text-[var(--text-secondary)]"
              >
                {path}
              </span>
            ))}
          </div>
        ) : null}
        {edit.diffText ? (
          <details className="group">
            <summary className="flex cursor-pointer items-center gap-1 text-[11px] text-[var(--accent-hover)]">
              <FileDiff size={13} />
              {t("chat.convenience.diff")}
            </summary>
            <pre className="mt-2 max-h-[220px] overflow-auto rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--background)] p-2 font-mono text-[10.5px] leading-4 text-[var(--text-secondary)]">
              {edit.diffText}
            </pre>
          </details>
        ) : null}
        {softPending ? (
          <div className="flex gap-2">
            <button type="button" className="settings-button" onClick={onKeep}>
              {t("chat.convenience.keep")}
            </button>
            <button type="button" className="settings-button settings-button--danger" onClick={onRollback}>
              {t("chat.convenience.rollback")}
            </button>
          </div>
        ) : rolledBack || failed ? null : onRollback ? (
          <button type="button" className="settings-button settings-button--danger w-fit" onClick={onRollback}>
            <RotateCcw size={13} />
            {t("chat.convenience.rollback")}
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        aria-pressed={enabled}
        disabled={pending}
        onClick={() => onSetEnabled(!enabled)}
        className={`h-[26px] rounded-[var(--radius-sm)] border px-2 text-[11px] font-medium ${
          enabled
            ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent-hover)]"
            : "border-[var(--border)] bg-[var(--surface-raised)] text-[var(--text-secondary)]"
        } disabled:cursor-not-allowed disabled:opacity-50`}
        title={pending ? t("chat.convenience.pending") : t("chat.convenience.toggle")}
      >
        <span className="inline-flex items-center gap-1">
          <ShieldCheck size={13} />
          {t("chat.convenience.label")}
        </span>
      </button>
      {onRollbackLast ? (
        <button
          type="button"
          className="h-[26px] rounded-[var(--radius-sm)] px-2 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
          onClick={onRollbackLast}
          aria-label={t("chat.convenience.rollbackLast")}
          title={t("chat.convenience.rollbackLast")}
        >
          <RotateCcw size={13} />
        </button>
      ) : null}
    </div>
  );
}

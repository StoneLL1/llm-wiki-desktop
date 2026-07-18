import { Check, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { CommitConflictAction } from "../../types/importV2";

export interface ImportCommitBarProps {
  selectedReadyCount: number;
  unresolvedActionCount: number;
  isConfirming: boolean;
  disabled?: boolean;
  conflictAction: Exclude<CommitConflictAction, "apply_merged_candidate">;
  onConflictActionChange: (action: Exclude<CommitConflictAction, "apply_merged_candidate">) => void;
  onConfirm: () => void;
}

export function ImportCommitBar({ selectedReadyCount, unresolvedActionCount, isConfirming, disabled = false, conflictAction, onConflictActionChange, onConfirm }: ImportCommitBarProps) {
  const { t } = useTranslation();
  const canConfirm = selectedReadyCount > 0 && !isConfirming && !disabled;
  return (
    <footer className="import-v2-commit-bar">
      <div className="min-w-0 text-[11px] text-[var(--text-secondary)]" aria-live="polite">
        <strong>{t("importV2.commit.selected", { count: selectedReadyCount })}</strong>
        <span className="mx-1 text-[var(--text-muted)]">·</span>
        <span>{t("importV2.commit.unresolved", { count: unresolvedActionCount })}</span>
      </div>
      <label className="flex items-center gap-2 text-[11px] text-[var(--text-secondary)]">
        <span>{t("importV2.commit.conflictPolicy")}</span>
        <select
          className="input input--sm"
          value={conflictAction}
          disabled={isConfirming || disabled}
          onChange={(event) => onConflictActionChange(event.target.value as Exclude<CommitConflictAction, "apply_merged_candidate">)}
        >
          <option value="create_new">{t("importV2.commit.createNew")}</option>
          <option value="keep_wiki">{t("importV2.commit.keepWiki")}</option>
        </select>
      </label>
      <button type="button" className="btn btn--sm btn--primary" disabled={!canConfirm} onClick={onConfirm}>
        {isConfirming ? <LoaderCircle size={14} className="mr-1 inline animate-spin" aria-label={t("importV2.commit.confirming")} /> : <Check size={14} className="mr-1 inline" aria-hidden="true" />}
        {isConfirming ? t("importV2.commit.confirming") : t("importV2.commit.confirm")}
      </button>
    </footer>
  );
}

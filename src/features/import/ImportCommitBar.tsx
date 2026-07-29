import { Check, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";

export interface ImportCommitBarCounts {
  newSources: number;
  updates: number;
  warnings: number;
  pending: number;
  selected: number;
}

export interface ImportCommitBarProps {
  counts?: ImportCommitBarCounts;
  /** @deprecated Compatibility for focused component callers. */
  selectedReadyCount?: number;
  /** @deprecated Compatibility for focused component callers. */
  unresolvedActionCount?: number;
  isConfirming: boolean;
  disabled?: boolean;
  onConfirm: () => void;
}

export function ImportCommitBar({
  counts,
  selectedReadyCount = 0,
  unresolvedActionCount = 0,
  isConfirming,
  disabled = false,
  onConfirm,
}: ImportCommitBarProps) {
  const { t } = useTranslation();
  const resolvedCounts = counts ?? {
    newSources: selectedReadyCount,
    updates: 0,
    warnings: 0,
    pending: unresolvedActionCount,
    selected: selectedReadyCount,
  };
  const canConfirm = resolvedCounts.selected > 0 && !isConfirming && !disabled;
  return (
    <footer className="import-v2-commit-bar">
      <div
        className="import-v2-commit-bar__summary"
        aria-live="polite"
        aria-atomic="true"
      >
        <span className="import-v2-commit-bar__selected">{t("importV2.commit.selected", { count: resolvedCounts.selected })}</span>
        <span className="import-v2-commit-bar__breakdown">
          <span>{t("importV2.commit.newSources", { count: resolvedCounts.newSources })}</span>
          <span aria-hidden="true">·</span>
          <span>{t("importV2.commit.updates", { count: resolvedCounts.updates })}</span>
          <span aria-hidden="true">·</span>
          <span>{t("importV2.commit.warnings", { count: resolvedCounts.warnings })}</span>
          <span aria-hidden="true">·</span>
          <span>{t("importV2.commit.pending", { count: resolvedCounts.pending })}</span>
        </span>
      </div>
      <button
        type="button"
        className="btn btn--sm btn--primary"
        disabled={!canConfirm}
        onClick={onConfirm}
      >
        {isConfirming ? (
          <LoaderCircle
            size={14}
            className="mr-1 inline animate-spin"
            aria-label={t("importV2.commit.confirming")}
          />
        ) : (
          <Check size={14} className="mr-1 inline" aria-hidden="true" />
        )}
        {isConfirming
          ? t("importV2.commit.confirming")
          : t("importV2.commit.confirmCount", { count: resolvedCounts.selected })}
      </button>
    </footer>
  );
}

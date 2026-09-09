import { useEffect, useRef, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { LintHistoryEntry, LintIgnoreEntry, LintIssueType } from "../../types/lint";
import { LintHistoryList } from "./LintHistoryList";

interface LintManagementPanelProps {
  view: "history" | "ignores";
  history: LintHistoryEntry[];
  activeHistoryId: string | null;
  historyLoading: boolean;
  historyError: string | null;
  ignores: LintIgnoreEntry[];
  removingIgnoreKey: string | null;
  disabled: boolean;
  onOpenReport: (id: string) => Promise<void>;
  onRetryHistory: () => void;
  onRestore: (path: string, rule: LintIssueType) => void;
  onClose: () => void;
}

export function LintManagementPanel({
  view, history, activeHistoryId, historyLoading, historyError, ignores,
  removingIgnoreKey, disabled, onOpenReport, onRetryHistory, onRestore, onClose,
}: LintManagementPanelProps) {
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  const [openingId, setOpeningId] = useState<string | null>(null);
  useEffect(() => { closeRef.current?.focus(); }, []);
  const title = t(view === "history" ? "lint.history.title" : "lint.ignores.title");

  return (
    <aside className="lint-view__details lint-management" aria-label={title}
      onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } }}>
      <header className="lint-management__header">
        <button ref={closeRef} type="button" className="btn btn--ghost btn--sm" onClick={onClose}
          aria-label={t("lint.management.back")} title={t("lint.management.back")}>
          <ArrowLeft size={15} aria-hidden="true" />
        </button>
        <h2>{title}</h2>
        <span>{view === "history" ? history.length : ignores.length}</span>
      </header>
      <div className="lint-management__body">
        {view === "history" ? (
          <LintHistoryList entries={history} activeId={activeHistoryId} loading={historyLoading}
            openingId={openingId} error={historyError} disabled={disabled}
            onRetry={onRetryHistory} onOpen={(id) => {
              setOpeningId(id);
              void onOpenReport(id).finally(() => setOpeningId(null));
            }} />
        ) : (
          <>
            <p className="lint-management__message">{t(ignores.length ? "lint.ignores.help" : "lint.ignores.empty")}</p>
            {ignores.map((entry) => {
              const key = `${entry.path}:${entry.rule}`;
              return (
                <div key={key} className="lint-ignore-row">
                  <div className="lint-ignore-row__copy">
                    <span>{t(`lint.issueType.${entry.rule}`)}</span>
                    <span className="lint-ignore-row__path" title={entry.path}>{entry.path}</span>
                  </div>
                  <button type="button" className="btn btn--secondary btn--sm"
                    onClick={() => onRestore(entry.path, entry.rule)}
                    disabled={Boolean(removingIgnoreKey) || disabled}>
                    {removingIgnoreKey === key ? t("lint.ignores.restoring") : t("lint.ignores.restore")}
                  </button>
                </div>
              );
            })}
          </>
        )}
      </div>
    </aside>
  );
}

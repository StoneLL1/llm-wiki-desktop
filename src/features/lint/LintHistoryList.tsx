import { Clock3, FileSearch, ShieldCheck, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { LintHistoryEntry } from "../../types/lint";

interface LintHistoryListProps {
  entries: LintHistoryEntry[];
  activeId: string | null;
  loading: boolean;
  error: string | null;
  onOpen: (id: string) => void;
}

function formatHistoryTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function LintHistoryList({
  entries,
  activeId,
  loading,
  error,
  onOpen,
}: LintHistoryListProps) {
  const { t } = useTranslation();
  const displayError = error === "lint.history.waitForFix" ? t(error) : error;

  return (
    <section className="lint-history" aria-label={t("lint.history.title")}>
      <header className="lint-history__head">
        <span>{t("lint.history.title")}</span>
        {loading ? (
          <span className="text-[11px] text-[var(--text-muted)]">
            {t("lint.history.loading")}
          </span>
        ) : null}
      </header>
      {displayError ? (
        <div className="lint-history__error" role="status">
          <TriangleAlert size={13} aria-hidden />
          <span>{displayError}</span>
        </div>
      ) : null}
      {entries.length === 0 && !loading ? (
        <div className="lint-history__empty">{t("lint.history.empty")}</div>
      ) : (
        <div className="lint-history__list">
          {entries.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={`lint-history__row ${activeId === entry.id ? "is-active" : ""}`}
              onClick={() => onOpen(entry.id)}
            >
              {entry.kind === "local" ? (
                <ShieldCheck size={14} aria-hidden />
              ) : (
                <FileSearch size={14} aria-hidden />
              )}
              <span className="lint-history__copy">
                <span className="lint-history__main">
                  {t(`lint.history.kind.${entry.kind}`)}
                  <span className="lint-history__count">{entry.issueCount}</span>
                </span>
                <span className="lint-history__meta">
                  <Clock3 size={11} aria-hidden />
                  {formatHistoryTime(entry.createdAt)}
                </span>
              </span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

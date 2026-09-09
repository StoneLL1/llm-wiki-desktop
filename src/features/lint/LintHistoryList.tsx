import { Check, LoaderCircle, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { LintHistoryEntry } from "../../types/lint";

interface LintHistoryListProps {
  entries: LintHistoryEntry[];
  activeId: string | null;
  loading: boolean;
  openingId?: string | null;
  error: string | null;
  onOpen: (id: string) => void;
  onRetry: () => void;
  disabled?: boolean;
}

export function LintHistoryList({
  entries, activeId, loading, openingId, error, onOpen, onRetry, disabled = false,
}: LintHistoryListProps) {
  const { t, i18n } = useTranslation();
  const displayError = error === "lint.history.waitForFix" ? t(error) : error;

  return (
    <div className="lint-history">
      {displayError ? (
        <div className="lint-history__error" role="status">
          <TriangleAlert size={14} aria-hidden="true" />
          <span>{displayError}</span>
          <button type="button" className="btn btn--secondary btn--sm" onClick={onRetry} disabled={loading}>
            {t("workflows.action.retry")}
          </button>
        </div>
      ) : null}
      {loading ? <p className="lint-management__message" role="status">{t("lint.history.loading")}</p> : null}
      {entries.length === 0 && !loading ? (
        <p className="lint-management__message">{t("lint.history.empty")}</p>
      ) : (
        <div className="lint-history__list">
          {entries.map((entry) => {
            const date = new Date(entry.createdAt);
            const current = activeId === entry.id;
            return (
              <button key={entry.id} type="button"
                className={`lint-history__row ${current ? "is-active" : ""}`}
                onClick={() => onOpen(entry.id)} disabled={disabled || Boolean(openingId)} aria-pressed={current}>
                <span className="lint-history__copy">
                  <time className="lint-history__time" dateTime={entry.createdAt}>
                    {Number.isNaN(date.getTime()) ? entry.createdAt : date.toLocaleString(i18n.resolvedLanguage, {
                      year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
                    })}
                  </time>
                  <span className="lint-history__meta">
                    {t(`lint.history.kind.${entry.kind}`)} · {t("lint.history.issueCount", { count: entry.issueCount })}
                  </span>
                  {entry.persistent === false ? <span className="lint-history__meta">{t("lint.history.nonPersistent")}</span> : null}
                </span>
                {openingId === entry.id ? <LoaderCircle size={14} className="animate-spin" aria-label={t("lint.history.loading")} />
                  : current ? <span className="lint-history__current"><Check size={13} aria-hidden="true" />{t("lint.history.current")}</span> : null}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

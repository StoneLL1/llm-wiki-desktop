import { useTranslation } from "react-i18next";

import type { LintIssue } from "../../types/lint";

interface LintSummaryCardsProps {
  /** Mode-filtered issues driving the error/warning/info counts. */
  issues: LintIssue[];
  /** Number of local deterministic rules that passed this scan. */
  passedCount: number;
}

export function LintSummaryCards({ issues, passedCount }: LintSummaryCardsProps) {
  const { t } = useTranslation();

  const counts = {
    error: issues.filter((issue) => issue.severity === "error").length,
    warning: issues.filter((issue) => issue.severity === "warning").length,
    info: issues.filter((issue) => issue.severity === "info").length,
  };

  const cards = [
    {
      label: t("lint.summary.errors"),
      value: counts.error,
      hint: t("lint.summary.errorsHint"),
      color: "var(--danger)",
    },
    {
      label: t("lint.summary.warnings"),
      value: counts.warning,
      hint: t("lint.summary.warningsHint"),
      color: "var(--warning)",
    },
    {
      label: t("lint.summary.info"),
      value: counts.info,
      hint: t("lint.summary.infoHint"),
      color: "var(--info)",
    },
    {
      label: t("lint.summary.passed"),
      value: passedCount,
      hint: t("lint.summary.passedHint"),
      color: "var(--accent-hover)",
    },
  ] as const;

  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(140px,1fr))] gap-3 border-b border-[var(--border-subtle)] bg-[var(--surface)] px-5 py-4">
      {cards.map((card) => (
        <div key={card.label} className="sumcard sumcard--lint">
          <div className="sumcard__label">{card.label}</div>
          <div className="sumcard__value" style={{ color: card.color }}>
            {card.value}
          </div>
          <div className="sumcard__hint">{card.hint}</div>
        </div>
      ))}
    </div>
  );
}

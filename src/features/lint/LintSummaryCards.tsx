import { useTranslation } from "react-i18next";
import type { LintIssue } from "../../types/lint";

export function LintSummaryCards({ issues, passedCount }: { issues: LintIssue[]; passedCount: number }) {
  const { t } = useTranslation();
  return (
    <div className="lint-summary" aria-label={t("lint.healthReport.title")}>
      {(["error", "warning", "info"] as const).map((severity) => (
        <span key={severity}>
          <span>{t(`lint.summary.${severity === "error" ? "errors" : severity === "warning" ? "warnings" : "info"}`)}</span>
          <strong>{issues.filter((issue) => issue.severity === severity).length}</strong>
        </span>
      ))}
      <span><span>{t("lint.summary.passed")}</span><strong>{passedCount}</strong></span>
    </div>
  );
}

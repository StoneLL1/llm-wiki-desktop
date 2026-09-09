import { useTranslation } from "react-i18next";

import type { HealthCheckReport } from "../../types/lint";

export function HealthCheckReportSummary({ report }: { report: HealthCheckReport }) {
  const { t, i18n } = useTranslation();
  const scannedAt = report.execution?.scannedAt ?? report.generatedAt;
  const date = new Date(scannedAt);
  const freshness = report.execution?.freshness ?? "unknown";
  const deepStatus = report.execution?.deepStatus ?? (report.mode === "local_quick" ? "not_requested" : "unknown");
  return (
    <section aria-label={t("lint.healthReport.title")} className="lint-report">
      <details className="lint-disclosure">
      <summary>{t("lint.healthReport.title")} · <time dateTime={scannedAt}>{Number.isNaN(date.getTime()) ? scannedAt : date.toLocaleString(i18n.resolvedLanguage)}</time></summary>
      <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
        <div><dt className="text-[var(--text-muted)]">{t("lint.healthReport.scannedAt")}</dt><dd><time dateTime={scannedAt}>{Number.isNaN(date.getTime()) ? scannedAt : date.toLocaleString(i18n.resolvedLanguage)}</time></dd></div>
        <div><dt className="text-[var(--text-muted)]">{t("lint.healthReport.freshness")}</dt><dd>{t(`lint.healthReport.freshness.${freshness}`)}</dd></div>
        <div><dt className="text-[var(--text-muted)]">{t("workflows.result.coverage")}</dt><dd>{t("lint.healthReport.localCoverage", { sources: report.coverage.sourcePages, wiki: report.coverage.wikiPages, count: report.coverage.scannedPages })}</dd></div>
        <div><dt className="text-[var(--text-muted)]">{t("lint.healthReport.deepStatus")}</dt><dd>{t(`lint.healthReport.deep.${deepStatus}`)}{report.coverage.deepCoveredPages != null ? ` · ${t("lint.healthReport.deepPages", { count: report.coverage.deepCoveredPages })}` : ""}</dd></div>
      </dl>
      </details>
      {report.coverage.deepTruncated ? <p className="mt-2">{t("lint.healthReport.truncated")}</p> : null}
      {deepStatus === "failed" || deepStatus === "pending" ? <p className="mt-2" role="status">{t("workflows.result.health_check.localRetained")}</p> : null}
      {freshness !== "current" ? <p className="mt-2">{t(`lint.healthReport.freshnessHelp.${freshness}`)}</p> : null}
      <p className="mt-2 text-[11px] text-[var(--text-muted)]">{t(`lint.healthReport.storage.${report.persistent ? "persistent" : "memory"}`)}</p>
    </section>
  );
}

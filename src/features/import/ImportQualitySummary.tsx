import { CircleAlert, CircleCheck, CircleX } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { QualityReport } from "../../types/importV2";

interface ImportQualitySummaryProps {
  quality: QualityReport | null;
}

function percentage(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function ImportQualitySummary({ quality }: ImportQualitySummaryProps) {
  const { t } = useTranslation();

  if (!quality) {
    return (
      <section className="border-b border-[var(--border-subtle)] px-4 py-3" aria-labelledby="import-quality-title">
        <h3 id="import-quality-title" className="import-v2-inspector-heading">{t("importV2.inspector.quality")}</h3>
        <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("importV2.inspector.qualityUnavailable")}</p>
      </section>
    );
  }

  const Icon = quality.level === "pass" ? CircleCheck : quality.level === "warning" ? CircleAlert : CircleX;
  const tone = quality.level === "pass" ? "text-[var(--accent)]" : quality.level === "warning" ? "text-[var(--warning)]" : "text-[var(--danger)]";

  return (
    <section className="border-b border-[var(--border-subtle)] px-4 py-3" aria-labelledby="import-quality-title">
      <div className="mb-2 flex items-center justify-between gap-2">
        <h3 id="import-quality-title" className="import-v2-inspector-heading">{t("importV2.inspector.quality")}</h3>
        <span className={`flex items-center gap-1 text-[11px] font-medium uppercase ${tone}`}>
          <Icon size={14} aria-hidden="true" />
          {t(`importV2.quality.${quality.level}`)}
        </span>
      </div>

      {quality.metrics.length > 0 ? (
        <dl className="m-0 space-y-1.5 text-[11.5px]">
          {quality.metrics.map((metric) => (
            <div key={metric.code} className="flex items-center justify-between gap-3">
              <dt className="font-mono text-[var(--text-muted)]">{metric.code}</dt>
              <dd className={metric.passed ? "m-0 text-[var(--text-secondary)]" : "m-0 text-[var(--danger)]"}>
                {percentage(metric.actual)} / {percentage(metric.minimum)}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}

      {quality.warnings.length > 0 ? (
        <ul className="mt-2 space-y-1 pl-4 text-[11.5px] text-[var(--warning-text)]">
          {quality.warnings.map((warning) => <li key={warning} className="font-mono">{warning}</li>)}
        </ul>
      ) : null}
    </section>
  );
}

import { CircleCheck, CircleX, Clock3, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { AttemptRecord } from "../../types/importV2";

interface ImportAttemptTimelineProps {
  attempts: AttemptRecord[];
}

function formatTime(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function ImportAttemptTimeline({ attempts }: ImportAttemptTimelineProps) {
  const { t } = useTranslation();
  return (
    <section className="border-b border-[var(--border-subtle)] px-4 py-3" aria-labelledby="import-attempts-title">
      <div className="mb-2 flex items-center justify-between gap-2">
        <h3 id="import-attempts-title" className="import-v2-inspector-heading">{t("importV2.inspector.attempts")}</h3>
        <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{attempts.length}</span>
      </div>
      {attempts.length === 0 ? (
        <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("importV2.inspector.attemptsEmpty")}</p>
      ) : (
        <ol className="m-0 space-y-2 border-l border-[var(--border-subtle)] pl-3">
          {attempts.map((attempt, index) => {
            const succeeded = attempt.outcome === "succeeded";
            const cancelled = attempt.outcome === "cancelled";
            const Icon = succeeded ? CircleCheck : cancelled ? X : CircleX;
            const tone = succeeded ? "text-[var(--accent)]" : cancelled ? "text-[var(--text-muted)]" : "text-[var(--danger)]";
            return (
              <li key={`${attempt.startedAt}-${attempt.route}-${index}`} className="relative text-[11.5px]">
                <span className={`absolute -left-[21px] top-0.5 bg-[var(--surface)] ${tone}`}><Icon size={14} aria-hidden="true" /></span>
                <div className="flex items-center justify-between gap-2">
                  <span className="font-mono text-[var(--text-primary)]">{attempt.route}</span>
                  <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{formatTime(attempt.completedAt ?? attempt.startedAt)}</span>
                </div>
                <div className="mt-0.5 flex items-center gap-1 text-[var(--text-muted)]">
                  <Clock3 size={11} aria-hidden="true" />
                  <span>{attempt.engineId} {attempt.engineVersion}</span>
                  <span>·</span>
                  <span>{attempt.stage}</span>
                  <span>·</span>
                  <span>{attempt.outcome}</span>
                </div>
                {attempt.errorCode ? <div className="mt-0.5 break-all font-mono text-[10.5px] text-[var(--danger)]">{attempt.errorCode}</div> : null}
                {attempt.warnings.length > 0 ? (
                  <ul className="mt-1 list-disc pl-4 font-mono text-[10.5px] text-[var(--warning-text)]">
                    {attempt.warnings.map((warning) => <li key={warning}>{warning}</li>)}
                  </ul>
                ) : null}
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}

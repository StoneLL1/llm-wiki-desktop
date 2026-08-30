import { CheckCircle2, FileText, RefreshCw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportCompletion } from "../../types/importV2";

export interface ImportCompletionSummaryProps {
  completion: ImportCompletion;
  remainingCount?: number;
  onContinueRemaining?: () => void;
  onViewSources: () => void;
  onViewSource: (wikiPath: string) => void;
  onUpdateWiki: () => void;
  onRetryFailure: (itemId: string) => void;
}

export function completionCountRows(completion: ImportCompletion) {
  return [
    { key: "new", count: completion.newSources.length },
    { key: "updated", count: completion.updatedSources.length },
    { key: "duplicate", count: completion.duplicateSkips.length },
    { key: "warning", count: completion.warnings.length },
    { key: "failure", count: completion.failures.length },
  ] as const;
}

function fileName(path: string): string {
  return path.replaceAll("\\", "/").split("/").filter(Boolean).at(-1) ?? path;
}

export function ImportCompletionSummary({
  completion,
  remainingCount = 0,
  onContinueRemaining = () => undefined,
  onViewSources,
  onViewSource,
  onUpdateWiki,
  onRetryFailure,
}: ImportCompletionSummaryProps) {
  const { t } = useTranslation();
  const changes = [...completion.newSources, ...completion.updatedSources];

  return (
    <section
      className="mx-3 mt-3 border border-[var(--border)] bg-[var(--surface)]"
      aria-labelledby="import-completion-title"
    >
      <div className="flex min-h-11 items-center gap-2 border-b border-[var(--border)] px-3">
        <CheckCircle2 aria-hidden="true" size={16} className="text-[var(--accent)]" />
        <h2 id="import-completion-title" className="m-0 text-[13px] font-medium">
          {t("importV2.completion.title")}
        </h2>
      </div>
      <div className="p-3">
        {remainingCount > 0 ? (
          <div className="mb-3 flex items-center justify-between gap-3 border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[12px]">
            <span>{t("importV2.completion.remaining", { count: remainingCount })}</span>
            <button type="button" className="btn btn--sm" onClick={onContinueRemaining}>
              {t("importV2.completion.continueRemaining", { count: remainingCount })}
            </button>
          </div>
        ) : null}
        <dl className="m-0 grid grid-cols-5 gap-2" aria-label={t("importV2.completion.counts")}>
          {completionCountRows(completion).map((row) => (
            <div key={row.key} className="min-w-0">
              <dt className="truncate text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                {t(`importV2.completion.${row.key}`)}
              </dt>
              <dd className="m-0 mt-1 font-mono text-[13px]">{row.count}</dd>
            </div>
          ))}
        </dl>

        {changes.length > 0 ? (
          <ul className="m-0 mt-3 grid list-none gap-1 p-0">
            {changes.map((change) => {
              const name = fileName(change.wikiPath);
              return (
                <li
                  key={`${change.sourceId}:${change.versionId}`}
                  className="flex min-w-0 items-center gap-2 text-[12px]"
                  title={name}
                >
                  <FileText aria-hidden="true" size={14} className="shrink-0 text-[var(--text-muted)]" />
                  <a
                    href={`#source-${encodeURIComponent(change.wikiPath)}`}
                    className="truncate text-[var(--text)] hover:underline"
                    onClick={(event) => {
                      event.preventDefault();
                      onViewSource(change.wikiPath);
                    }}
                  >
                    {name}
                  </a>
                </li>
              );
            })}
          </ul>
        ) : null}

        {completion.failures.length > 0 ? (
          <ul className="m-0 mt-3 grid list-none gap-1 p-0">
            {completion.failures.map((failure) => (
              <li key={failure.itemId} className="flex items-center gap-2 text-[12px]">
                <TriangleAlert aria-hidden="true" size={14} className="shrink-0 text-[var(--warning)]" />
                <span className="min-w-0 flex-1 truncate" title={failure.inputLabel}>
                  {failure.inputLabel}
                </span>
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() => onRetryFailure(failure.itemId)}
                >
                  <RefreshCw aria-hidden="true" size={13} />
                  {t("importV2.completion.retry")}
                </button>
              </li>
            ))}
          </ul>
        ) : null}

        {changes.length > 0 ? (
          <div className="mt-3 flex flex-wrap gap-2">
            <button type="button" className="btn btn--sm" onClick={onViewSources}>
              {t("importV2.completion.viewSources")}
            </button>
            <button type="button" className="btn btn--primary btn--sm" onClick={onUpdateWiki}>
              {t("importV2.completion.updateWiki")}
            </button>
          </div>
        ) : null}
      </div>
    </section>
  );
}

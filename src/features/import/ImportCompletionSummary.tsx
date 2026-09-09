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
    <section className="import-v2-status-row import-v2-completion" aria-labelledby="import-completion-title">
      <details className="import-v2-status-details">
        <summary>
          <CheckCircle2 aria-hidden="true" size={15} className="text-[var(--accent)]" />
          <h2 id="import-completion-title">{t(remainingCount > 0 ? "importV2.completion.partialTitle" : "importV2.completion.title")}</h2>
          <span>{t("importV2.completion.importedCount", { count: changes.length })}</span>
          {completion.warnings.length > 0 ? <span className="text-[var(--warning-text)]">{t("importV2.commit.warnings", { count: completion.warnings.length })}</span> : null}
          {completion.failures.length > 0 ? <span className="text-[var(--danger)]">{t("importV2.completion.failedCount", { count: completion.failures.length })}</span> : null}
          {remainingCount > 0 ? <span>{t("importV2.completion.remaining", { count: remainingCount })}</span> : null}
        </summary>
        <div className="import-v2-status-details__body">
          <dl className="import-v2-result-counts" aria-label={t("importV2.completion.counts")}>
            {completionCountRows(completion).map((row) => (
              <div key={row.key}><dt>{t(`importV2.completion.${row.key}`)}</dt><dd>{row.count}</dd></div>
            ))}
          </dl>
          {changes.length > 0 ? (
            <ul className="m-0 mt-2 grid list-none gap-1 p-0">
              {changes.map((change) => (
                <li key={`${change.sourceId}:${change.versionId}`} className="flex min-w-0 items-center gap-2 text-[12px]">
                  <FileText aria-hidden="true" size={14} className="shrink-0 text-[var(--text-muted)]" />
                  <a href={`#source-${encodeURIComponent(change.wikiPath)}`} className="truncate hover:underline" title={change.wikiPath}
                    onClick={(event) => { event.preventDefault(); onViewSource(change.wikiPath); }}>
                    {fileName(change.wikiPath)}
                  </a>
                </li>
              ))}
            </ul>
          ) : null}
          {completion.failures.length > 0 ? (
            <ul className="m-0 mt-2 grid list-none gap-1 p-0">
              {completion.failures.map((failure) => (
                <li key={failure.itemId} className="flex items-center gap-2 text-[12px]">
                  <TriangleAlert aria-hidden="true" size={14} className="shrink-0 text-[var(--warning)]" />
                  <span className="min-w-0 flex-1 truncate" title={failure.inputLabel}>{failure.inputLabel}</span>
                  <button type="button" className="btn btn--sm" onClick={() => onRetryFailure(failure.itemId)}>
                    <RefreshCw aria-hidden="true" size={13} />{t("importV2.completion.retry")}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </details>
      <div className="import-v2-status-row__actions">
        {remainingCount > 0 ? <button type="button" className="btn btn--sm" onClick={onContinueRemaining}>{t("importV2.completion.continueRemaining", { count: remainingCount })}</button> : null}
        {changes.length > 0 ? <>
          <button type="button" className="btn btn--sm" onClick={onViewSources}>{t("importV2.completion.viewSources")}</button>
          <button type="button" className="btn btn--sm" onClick={onUpdateWiki}>{t("importV2.completion.updateWiki")}</button>
        </> : null}
      </div>
    </section>
  );
}

import { CircleAlert, Clock3, LoaderCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  useWorkflowStore,
  workflowOperationPending,
  type WorkflowOperationError,
  type WorkflowOverviewStatus,
} from "../../stores/workflowStore";
import type { WorkflowKind, WorkflowRunSummary, WorkflowsOverview } from "../../types/workflow";
import { WorkflowRow } from "./WorkflowRow";
import {
  attentionWorkflowRow,
  isQueueOwningStatus,
  WORKFLOW_KINDS,
  workflowDateTimeLabel,
  workflowKindDescriptionKey,
  workflowKindKey,
  workflowStatusKey,
} from "./workflowPresentation";

export function WorkflowsOverviewView({
  overview,
  overviewStatus,
  error,
  onRetry,
  onPrepare,
  onPrerequisite,
  onOpenRun,
  onContinueQueue,
}: {
  overview: WorkflowsOverview | null;
  overviewStatus: WorkflowOverviewStatus;
  error: WorkflowOperationError | string | null;
  onRetry: () => void;
  onPrepare: (kind: WorkflowKind) => void;
  onPrerequisite: (action: NonNullable<WorkflowsOverview["rows"][number]["prerequisite"]>["action"]) => void;
  onOpenRun: (taskId: string) => void;
  onContinueQueue: () => void;
}) {
  const { t, i18n } = useTranslation();
  const operations = useWorkflowStore((state) => state.operations);
  const errorSummary = typeof error === "string" ? error : error?.summary ?? null;
  const technicalDetails = typeof error === "string" ? null : error?.technicalDetails ?? null;
  if (!overview) {
    const failed = overviewStatus === "error";
    return (
      <div
        className="workflow-empty"
        role={failed ? "alert" : "status"}
        aria-live="polite"
        aria-atomic="true"
        aria-busy={!failed}
      >
        {failed ? (
          <CircleAlert className="workflow-empty__icon is-error" size={20} aria-hidden="true" />
        ) : (
          <LoaderCircle className="workflow-empty__icon animate-spin" size={20} aria-hidden="true" />
        )}
        <h2 data-workflow-surface-title tabIndex={-1}>{t(failed ? "workflows.loadError.title" : "workflows.loading.title")}</h2>
        <p>{t(failed ? "workflows.loadError.description" : "workflows.loading.description")}</p>
        {failed && errorSummary ? <code className="workflow-empty__detail">{errorSummary}</code> : null}
        {failed && technicalDetails ? <details><summary>{t("workflows.error.technicalDetails")}</summary><pre>{technicalDetails}</pre></details> : null}
        {failed ? (
          <button className="btn btn--secondary workflow-empty__retry" type="button" onClick={onRetry}>
            <RefreshCw size={14} aria-hidden="true" />
            {t("workflows.action.retry")}
          </button>
        ) : null}
      </div>
    );
  }
  const leadingRow = attentionWorkflowRow(overview.rows);
  const leadingStatus = leadingRow?.state ?? null;
  const recentRuns = (overview.recentRuns ?? []).slice(0, 5);
  const hasActiveRun = overview.rows.some((row) => row.activeTaskId && isQueueOwningStatus(row.state));
  const recommendedKind = !leadingRow
    ? WORKFLOW_KINDS.find((kind) => overview.rows.find((row) => row.kind === kind)?.recommended) ?? null
    : null;
  const attentionActionKey = leadingStatus === "queued" && leadingRow?.activeContinuationRequired
    ? "workflows.action.continueQueue"
    : leadingStatus === "running" || leadingStatus === "queued"
      ? "workflows.action.viewProgress"
      : "workflows.action.view";
  const attentionActionPending = leadingRow?.activeTaskId
    ? leadingStatus === "queued" && leadingRow.activeContinuationRequired
      ? workflowOperationPending(operations, "queue:continue")
      : workflowOperationPending(operations, `task:${leadingRow.activeTaskId}:open`)
    : false;
  return (
    <div className="workflows-overview">
      <div className="workflows-intro">
        <h2 data-workflow-surface-title tabIndex={-1}>{t("workflows.overview.title")}</h2>
        <p>{t("workflows.overview.description")}</p>
      </div>
      {leadingRow && leadingRow.activeTaskId && leadingStatus ? (
        <section className="workflow-overview-section" aria-labelledby="workflow-overview-attention">
          <h2 className="workflow-overview-section__title" id="workflow-overview-attention">
            {t("workflows.overview.attention")}
          </h2>
          <div className={`workflow-attention-run is-${leadingStatus.replaceAll("_", "-")}`}>
            <div className="workflow-attention-run__icon">
              {leadingStatus === "running" ? (
                <LoaderCircle className="animate-spin" size={16} aria-hidden="true" />
              ) : leadingStatus === "queued" ? (
                <Clock3 size={16} aria-hidden="true" />
              ) : (
                <CircleAlert size={16} aria-hidden="true" />
              )}
            </div>
            <div className="min-w-0">
              <div className="workflow-attention-run__heading">
                <h3>{t(workflowKindKey(leadingRow.kind))}</h3>
                <span className={`workflow-badge is-${leadingStatus.replaceAll("_", "-")}`}>
                  {t(workflowStatusKey(leadingStatus))}
                </span>
              </div>
              <p>{t(workflowKindDescriptionKey(leadingRow.kind))}</p>
            </div>
            <button
              aria-label={`${t(attentionActionKey)}: ${t(workflowKindKey(leadingRow.kind))}`}
              className="btn btn--primary btn--sm"
              data-workflow-return-key={`attention:${leadingRow.activeTaskId}`}
              disabled={attentionActionPending}
              type="button"
              onClick={() => leadingStatus === "queued" && leadingRow.activeContinuationRequired
                ? onContinueQueue()
                : onOpenRun(leadingRow.activeTaskId!)}
            >
              {t(attentionActionKey)}
            </button>
          </div>
        </section>
      ) : null}
      <section className="workflow-overview-section" aria-labelledby="workflow-overview-available">
        <h2 className="workflow-overview-section__title" id="workflow-overview-available">
          {t("workflows.overview.available")}
        </h2>
        <div className="workflow-list" role="list">
          {WORKFLOW_KINDS.map((kind) => {
            const row = overview.rows.find((candidate) => candidate.kind === kind);
            if (!row) return null;
            return (
              <div key={kind} role="listitem">
                <WorkflowRow
                  row={row}
                  highlighted={kind === recommendedKind}
                  hasOtherActiveRun={hasActiveRun && !row.activeTaskId}
                  pending={row.activeTaskId
                    ? workflowOperationPending(operations, `task:${row.activeTaskId}:open`)
                    : row.state === "up_to_date" && row.lastCompletedTaskId
                      ? workflowOperationPending(operations, `task:${row.lastCompletedTaskId}:open`)
                      : workflowOperationPending(operations, `prepare:${kind}`)
                        || workflowOperationPending(operations, "prerequisite:project:")}
                  onPrepare={() => onPrepare(kind)}
                  onPrerequisite={onPrerequisite}
                  onOpenRun={onOpenRun}
                />
              </div>
            );
          })}
        </div>
      </section>
      <section className="workflow-overview-section" aria-labelledby="workflow-overview-recent">
        <h2 className="workflow-overview-section__title" id="workflow-overview-recent">
          {t("workflows.overview.recent")}
        </h2>
        {recentRuns.length > 0 ? (
          <div className="workflow-recent-list" role="list">
            {recentRuns.map((run) => (
              <RecentRunRow
                key={run.taskId}
                run={run}
                language={i18n.resolvedLanguage ?? i18n.language}
                pending={workflowOperationPending(operations, `task:${run.taskId}:open`)}
                onOpen={() => onOpenRun(run.taskId)}
              />
            ))}
          </div>
        ) : (
          <p className="workflow-overview-section__empty">{t("workflows.overview.noRecentRuns")}</p>
        )}
      </section>
    </div>
  );
}

function RecentRunRow({ run, language, pending, onOpen }: {
  run: WorkflowRunSummary;
  language: string;
  pending: boolean;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  const dateTimeLabel = workflowDateTimeLabel(run.updatedAt, language);
  return (
    <div className="workflow-recent-row" role="listitem">
      <div className="workflow-recent-row__icon">
        <Clock3 size={15} aria-hidden="true" />
      </div>
      <div className="min-w-0">
        <div className="workflow-recent-row__heading">
          <h3>{t(workflowKindKey(run.kind))}</h3>
          <span className="workflow-recent-row__status">{t(workflowStatusKey(run.displayStatus))}</span>
        </div>
        <time dateTime={run.updatedAt}>{dateTimeLabel}</time>
      </div>
      <button
        aria-label={`${t("workflows.action.view")}: ${t(workflowKindKey(run.kind))}, ${t(workflowStatusKey(run.displayStatus))}, ${dateTimeLabel}`}
        className="btn btn--ghost btn--sm"
        data-workflow-return-key={`recent:${run.taskId}`}
        disabled={pending}
        type="button"
        onClick={onOpen}
      >
        {t("workflows.action.view")}
      </button>
    </div>
  );
}

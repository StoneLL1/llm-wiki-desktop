import { Activity, ChevronRight, FileOutput, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
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
import { WorkflowStatus } from "./WorkflowStatus";

const workflowIcons = {
  update_wiki: RefreshCw,
  health_check: Activity,
  generate_content: FileOutput,
} satisfies Record<WorkflowKind, typeof Activity>;

export function WorkflowsOverviewView({
  overview,
  overviewStatus,
  error,
  onRetry,
  onPrepare,
  onPrerequisite,
  onOpenRun,
  onContinueQueue,
  selectedKind = null,
  children,
}: {
  selectedKind?: WorkflowKind | null;
  children?: ReactNode;
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
  const waitingForOverview = !overview;
  if (!overview) overview = {
    schemaVersion: 1, projectAccess: null,
    rows: WORKFLOW_KINDS.map((kind) => ({ kind, state: "ready", recommended: false,
      activeTaskId: null, activeContinuationRequired: false, lastCompletedTaskId: null, lastCompletedAt: null, prerequisite: null })),
  };
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
  const AttentionIcon = leadingRow ? workflowIcons[leadingRow.kind] : Activity;
  return (
    <div className={`workflows-overview${children ? " has-panel" : ""}`}>
      {waitingForOverview ? <div role={overviewStatus === "error" ? "alert" : "status"} className={overviewStatus === "error" ? "workflow-error-banner" : "workflow-overview-section__empty"}>
        <span>{t(overviewStatus === "error" ? "workflows.loadError.description" : "workflows.loading.description")}</span>
        {errorSummary ? <span>{errorSummary}</span> : null}
        {technicalDetails ? <details><summary>{t("workflows.error.technicalDetails")}</summary><pre>{technicalDetails}</pre></details> : null}
        {overviewStatus === "error" ? <button className="btn btn--secondary btn--sm" onClick={onRetry} type="button">{t("workflows.action.retry")}</button> : null}
      </div> : null}
      <section className="workflow-overview-section" aria-label={t("workflows.overview.available")}>
        <h2 className="workflow-overview-section__title" id="workflow-overview-available">
          <span data-workflow-surface-title={children ? undefined : true} tabIndex={-1}>{t("workflows.design.choose")}</span>
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
                  selected={kind === (selectedKind ?? leadingRow?.kind ?? recommendedKind)}
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
      {children ?? (<>
      {leadingRow && leadingRow.activeTaskId && leadingStatus ? (
        <section className="workflow-overview-section" aria-labelledby="workflow-overview-attention">
          <h2 className="workflow-overview-section__title" id="workflow-overview-attention">
            {t("workflows.overview.attention")}
          </h2>
          <div className={`workflow-attention-run is-${leadingStatus.replaceAll("_", "-")}`}>
            <div className="workflow-attention-run__icon">
              <AttentionIcon size={16} aria-hidden="true" />
            </div>
            <div className="min-w-0">
              <div className="workflow-attention-run__heading">
                <h3>{t(workflowKindKey(leadingRow.kind))}</h3>
                <WorkflowStatus status={leadingStatus} />
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
      </>)}
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
          <p className="workflow-overview-section__empty">{waitingForOverview ? "—" : t("workflows.overview.noRecentRuns")}</p>
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
  const RunIcon = workflowIcons[run.kind];
  const dateTimeLabel = workflowDateTimeLabel(run.updatedAt, language);
  return (
    <div className="workflow-recent-row" role="listitem">
      <div className="workflow-recent-row__icon">
        <RunIcon size={15} aria-hidden="true" />
      </div>
      <div className="min-w-0">
        <div className="workflow-recent-row__heading">
          <h3>{t(workflowKindKey(run.kind))}</h3>
          <WorkflowStatus className="workflow-recent-row__status" status={run.displayStatus} />
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
        <ChevronRight size={15} aria-hidden="true" />
      </button>
    </div>
  );
}

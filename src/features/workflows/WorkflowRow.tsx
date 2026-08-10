import { Activity, CircleAlert, FileOutput, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowKind, WorkflowOverviewRow } from "../../types/workflow";
import {
  workflowDateTimeLabel,
  workflowKindDescriptionKey,
  workflowKindKey,
} from "./workflowPresentation";
import { WorkflowStatus } from "./WorkflowStatus";

const icons = {
  update_wiki: RefreshCw,
  health_check: Activity,
  generate_content: FileOutput,
} satisfies Record<WorkflowKind, typeof Activity>;

export function WorkflowRow({
  row,
  highlighted,
  hasOtherActiveRun,
  pending,
  onPrepare,
  onPrerequisite,
  onOpenRun,
}: {
  row: WorkflowOverviewRow;
  highlighted: boolean;
  hasOtherActiveRun: boolean;
  pending: boolean;
  onPrepare: () => void;
  onPrerequisite: (action: NonNullable<WorkflowOverviewRow["prerequisite"]>["action"]) => void;
  onOpenRun: (taskId: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const Icon = icons[row.kind];
  const state = row.state;
  const activeTaskId = row.activeTaskId;
  const prerequisite = activeTaskId ? null : row.prerequisite;
  const opensProjectWorkbench = prerequisite?.action === "open_or_create_project";
  const actionKey = activeTaskId
    ? state === "running" || state === "queued"
      ? "workflows.action.viewProgress"
      : "workflows.action.view"
    : state === "up_to_date"
      ? row.lastCompletedTaskId ? "workflows.action.view" : "workflows.status.up_to_date"
      : prerequisite
        ? "workflows.action.run"
        : hasOtherActiveRun
        ? "workflows.action.queue"
        : "workflows.action.run";
  const actionLabel = t(actionKey);
  const kindLabel = t(workflowKindKey(row.kind));
  return (
    <div className="workflow-row">
      <div className="workflow-row__icon"><Icon aria-hidden="true" size={16} /></div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h3 className="m-0 text-[13px] font-semibold">{kindLabel}</h3>
          {highlighted ? <span className="workflow-badge is-accent">{t("workflows.recommended")}</span> : null}
        </div>
        <p className="m-0 mt-1 text-[12px] leading-5 text-[var(--text-muted)]">
          {t(workflowKindDescriptionKey(row.kind))}
        </p>
        {prerequisite && highlighted ? (
          <p className={`workflow-row__prerequisite${prerequisite.blocking ? " is-blocking" : ""}`}>
            <CircleAlert size={12} aria-hidden="true" />
            <span>{t(prerequisite.messageKey)}</span>
          </p>
        ) : null}
        {!activeTaskId && row.lastCompletedAt ? <time className="workflow-row__last" dateTime={row.lastCompletedAt}>{t("workflows.overview.lastCompleted", { time: workflowDateTimeLabel(row.lastCompletedAt, i18n.resolvedLanguage ?? i18n.language) })}</time> : null}
      </div>
      <div className="workflow-row__state">
        <WorkflowStatus status={state} />
      </div>
      {activeTaskId ? (
        <button aria-label={`${actionLabel}: ${kindLabel}`} className="btn btn--secondary btn--sm" data-workflow-return-key={`row:${row.kind}:${activeTaskId}`} disabled={pending} onClick={() => onOpenRun(activeTaskId)} type="button">
          {actionLabel}
        </button>
      ) : (
        <button
          aria-label={`${actionLabel}: ${kindLabel}`}
          className={`btn ${highlighted ? "btn--primary" : "btn--secondary"} btn--sm`}
          data-workflow-return-key={`row:${row.kind}:${row.lastCompletedTaskId ?? "prepare"}`}
          disabled={pending || (state === "up_to_date" && !row.lastCompletedTaskId)}
          onClick={() => state === "up_to_date" && row.lastCompletedTaskId
            ? onOpenRun(row.lastCompletedTaskId)
            : opensProjectWorkbench && prerequisite
              ? onPrerequisite(prerequisite.action)
              : onPrepare()}
          type="button"
        >
          {actionLabel}
        </button>
      )}
    </div>
  );
}

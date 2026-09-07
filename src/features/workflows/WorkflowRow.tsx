import { ChevronRight, CircleAlert, FileOutput, RefreshCw, ShieldCheck } from "lucide-react";
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
  health_check: ShieldCheck,
  generate_content: FileOutput,
} satisfies Record<WorkflowKind, typeof RefreshCw>;

export function WorkflowRow({
  row,
  highlighted,
  selected = false,
  hasOtherActiveRun,
  pending,
  onPrepare,
  onPrerequisite,
  onOpenRun,
}: {
  row: WorkflowOverviewRow;
  highlighted: boolean;
  selected?: boolean;
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
    <button
      aria-label={`${actionLabel}: ${kindLabel}`}
      aria-pressed={selected}
      className={`workflow-row${selected ? " is-selected" : ""}`}
      data-workflow-return-key={`row:${row.kind}:${activeTaskId ?? row.lastCompletedTaskId ?? "prepare"}`}
      disabled={pending || (state === "up_to_date" && !row.lastCompletedTaskId)}
      onClick={() => activeTaskId
        ? onOpenRun(activeTaskId)
        : state === "up_to_date" && row.lastCompletedTaskId
          ? onOpenRun(row.lastCompletedTaskId)
          : opensProjectWorkbench && prerequisite
            ? onPrerequisite(prerequisite.action)
            : onPrepare()}
      type="button"
    >
      <span className="workflow-row__head">
        <span className="workflow-row__icon"><Icon aria-hidden="true" size={17} /></span>
        <span className="workflow-row__name" role="heading" aria-level={3}>{kindLabel}</span>
        <WorkflowStatus status={state} />
      </span>
      <span className="workflow-row__description">
          {t(workflowKindDescriptionKey(row.kind))}
      </span>
        {prerequisite && highlighted ? (
          <span className={`workflow-row__prerequisite${prerequisite.blocking ? " is-blocking" : ""}`}>
            <CircleAlert size={12} aria-hidden="true" />
            <span>{t(prerequisite.messageKey)}</span>
          </span>
        ) : null}
      <span className="workflow-row__foot">
        <span>
          {!activeTaskId && row.lastCompletedAt
            ? <time dateTime={row.lastCompletedAt}>{workflowDateTimeLabel(row.lastCompletedAt, i18n.resolvedLanguage ?? i18n.language)}</time>
            : actionLabel}
          {highlighted ? <span className="workflow-badge is-accent">{t("workflows.recommended")}</span> : null}
        </span>
        <ChevronRight aria-hidden="true" size={14} />
      </span>
    </button>
  );
}

import { Activity, CircleAlert, FileOutput, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowKind, WorkflowOverviewRow, WorkflowRun } from "../../types/workflow";
import { workflowKindDescriptionKey, workflowKindKey, workflowStatusKey } from "./workflowPresentation";

const icons = {
  update_wiki: RefreshCw,
  health_check: Activity,
  generate_content: FileOutput,
} satisfies Record<WorkflowKind, typeof Activity>;

export function WorkflowRow({
  row,
  activeRun,
  pending,
  onPrepare,
  onPrerequisite,
  onOpenRun,
}: {
  row: WorkflowOverviewRow;
  activeRun: WorkflowRun | null;
  pending: boolean;
  onPrepare: () => void;
  onPrerequisite: (action: NonNullable<WorkflowOverviewRow["prerequisite"]>["action"]) => void;
  onOpenRun: (taskId: string) => void;
}) {
  const { t } = useTranslation();
  const Icon = icons[row.kind];
  const state = activeRun?.displayStatus ?? row.state;
  const prerequisite = activeRun ? null : row.prerequisite;
  const opensProjectWorkbench = prerequisite?.action === "open_or_create_project";
  return (
    <div className="workflow-row">
      <div className="workflow-row__icon"><Icon aria-hidden="true" size={16} /></div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h3 className="m-0 text-[13px] font-semibold">{t(workflowKindKey(row.kind))}</h3>
          {row.recommended ? <span className="workflow-badge is-accent">{t("workflows.recommended")}</span> : null}
        </div>
        <p className="m-0 mt-1 text-[12px] leading-5 text-[var(--text-muted)]">
          {t(workflowKindDescriptionKey(row.kind))}
        </p>
        {prerequisite && row.recommended ? (
          <p className={`workflow-row__prerequisite${prerequisite.blocking ? " is-blocking" : ""}`}>
            <CircleAlert size={12} aria-hidden="true" />
            <span>{t(prerequisite.messageKey)}</span>
          </p>
        ) : null}
        {!activeRun && row.lastCompletedAt ? <time className="workflow-row__last" dateTime={row.lastCompletedAt}>{t("workflows.overview.lastCompleted", { time: new Date(row.lastCompletedAt).toLocaleString() })}</time> : null}
      </div>
      <div className="workflow-row__state">
        <span className={`workflow-status-dot is-${String(state).replaceAll("_", "-")}`} aria-hidden="true" />
        <span>{t(workflowStatusKey(state))}</span>
      </div>
      {activeRun ? (
        <button className="btn btn--secondary btn--sm" disabled={pending} onClick={() => onOpenRun(activeRun.taskId)} type="button">
          {t("workflows.action.open")}
        </button>
      ) : (
        <button
          className="btn btn--secondary btn--sm"
          disabled={pending}
          onClick={() => opensProjectWorkbench ? onPrerequisite(prerequisite.action) : onPrepare()}
          type="button"
        >
          {t(opensProjectWorkbench
            ? "workflows.action.openOrCreateProject"
            : "workflows.action.prepare")}
        </button>
      )}
    </div>
  );
}

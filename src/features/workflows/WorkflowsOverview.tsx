import { CircleAlert, LoaderCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowOverviewStatus } from "../../stores/workflowStore";
import type { WorkflowKind, WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { WorkflowRow } from "./WorkflowRow";
import { WORKFLOW_KINDS } from "./workflowPresentation";

export function WorkflowsOverviewView({
  overview,
  overviewStatus,
  error,
  runs,
  onRetry,
  onPrepare,
  onPrerequisite,
  onOpenRun,
  onContinueQueue,
}: {
  overview: WorkflowsOverview | null;
  overviewStatus: WorkflowOverviewStatus;
  error: string | null;
  runs: WorkflowRun[];
  onRetry: () => void;
  onPrepare: (kind: WorkflowKind) => void;
  onPrerequisite: (action: NonNullable<WorkflowsOverview["rows"][number]["prerequisite"]>["action"]) => void;
  onOpenRun: (taskId: string) => void;
  onContinueQueue: () => void;
}) {
  const { t } = useTranslation();
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
        <h2>{t(failed ? "workflows.loadError.title" : "workflows.loading.title")}</h2>
        <p>{t(failed ? "workflows.loadError.description" : "workflows.loading.description")}</p>
        {failed && error ? <code className="workflow-empty__detail">{error}</code> : null}
        {failed ? (
          <button className="btn btn--secondary workflow-empty__retry" type="button" onClick={onRetry}>
            <RefreshCw size={14} aria-hidden="true" />
            {t("workflows.action.retry")}
          </button>
        ) : null}
      </div>
    );
  }
  return (
    <div className="workflows-overview">
      <div className="workflows-intro">
        <h2>{t("workflows.overview.title")}</h2>
        <p>{t("workflows.overview.description")}</p>
      </div>
      {runs.some((run) => run.displayStatus === "queued" && run.continuationRequired) ? (
        <div className="workflow-attention">
          <p>{t("workflows.recovery.queuedDescription")}</p>
          <button className="btn btn--primary" type="button" onClick={onContinueQueue}>
            {t("workflows.action.continueQueue")}
          </button>
        </div>
      ) : null}
      <div className="workflow-list" role="list">
        {WORKFLOW_KINDS.map((kind) => {
          const row = overview.rows.find((candidate) => candidate.kind === kind);
          if (!row) return null;
          const activeRun = row.activeTaskId
            ? runs.find((run) => run.taskId === row.activeTaskId) ?? null
            : null;
          return (
            <div key={kind} role="listitem">
              <WorkflowRow row={row} activeRun={activeRun} onPrepare={() => onPrepare(kind)} onPrerequisite={onPrerequisite} onOpenRun={onOpenRun} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

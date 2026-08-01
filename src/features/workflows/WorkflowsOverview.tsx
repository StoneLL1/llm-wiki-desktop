import { useTranslation } from "react-i18next";

import type { WorkflowKind, WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { WorkflowRow } from "./WorkflowRow";
import { WORKFLOW_KINDS } from "./workflowPresentation";

export function WorkflowsOverviewView({
  overview,
  runs,
  onPrepare,
  onOpenRun,
}: {
  overview: WorkflowsOverview | null;
  runs: WorkflowRun[];
  onPrepare: (kind: WorkflowKind) => void;
  onOpenRun: (taskId: string) => void;
}) {
  const { t } = useTranslation();
  if (!overview) {
    return (
      <div className="workflow-empty">
        <h2>{t("workflows.noProject.title")}</h2>
        <p>{t("workflows.noProject.description")}</p>
      </div>
    );
  }
  return (
    <div className="workflows-overview">
      <div className="workflows-intro">
        <h2>{t("workflows.overview.title")}</h2>
        <p>{t("workflows.overview.description")}</p>
      </div>
      <div className="workflow-list" role="list">
        {WORKFLOW_KINDS.map((kind) => {
          const row = overview.rows.find((candidate) => candidate.kind === kind);
          if (!row) return null;
          const activeRun = row.activeTaskId
            ? runs.find((run) => run.taskId === row.activeTaskId) ?? null
            : null;
          return (
            <div key={kind} role="listitem">
              <WorkflowRow row={row} activeRun={activeRun} onPrepare={() => onPrepare(kind)} onOpenRun={onOpenRun} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

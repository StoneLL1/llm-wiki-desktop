import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { cancelWorkflowRun } from "../../services/workflowApi";
import { hydrateAndSelectWorkflowRun } from "../../services/workflowNavigation";
import { useNavigationStore } from "../../stores/navigationStore";
import { captureProjectScope, isProjectScopeCurrent } from "../../stores/projectScope";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";

/** Workflows owns execution; Lint only presents the same authoritative task facts. */
export function LintTaskStatus() {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const authority = useProjectStore((state) => state.authority);
  const workflows = useTaskStore((state) => state.workflowById);
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = useMemo(() => Object.values(workflows)
    .filter((candidate) => candidate.projectId === project.projectId
      && authority?.projectId === project.projectId
      && candidate.canonicalIdentityKey === authority.canonicalIdentityKey
      && candidate.identityRevision === authority.identityRevision
      && candidate.kind === "health_check" && candidate.operation.kind === "built_in"
      && ["running", "queued", "waiting_for_confirmation"].includes(candidate.displayStatus))
    .sort((a, b) => Number(a.displayStatus === "queued") - Number(b.displayStatus === "queued")
      || a.startedAt.localeCompare(b.startedAt))[0], [workflows, project.projectId, authority]);

  if (!run) return null;
  const stage = run.currentStage;
  const pending = cancellingId === run.taskId;
  const open = async () => {
    const scope = captureProjectScope();
    setOpening(true);
    setError(null);
    try {
      await hydrateAndSelectWorkflowRun(project, run.taskId);
      if (isProjectScopeCurrent(scope)) useNavigationStore.getState().setActiveView("workflows");
    } catch (cause) {
      if (isProjectScopeCurrent(scope)) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (isProjectScopeCurrent(scope)) setOpening(false);
    }
  };
  const cancel = async () => {
    if (pending) return;
    const scope = captureProjectScope();
    setCancellingId(run.taskId);
    setError(null);
    try {
      await cancelWorkflowRun({ projectId: project.projectId, projectRootPath: project.rootPath, taskId: run.taskId });
    } catch (cause) {
      if (isProjectScopeCurrent(scope)) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (isProjectScopeCurrent(scope)) setCancellingId(null);
    }
  };

  return <div className="border-b border-[var(--border)] px-4 py-2 text-[12px]">
    <div className="flex items-center gap-2" role="status">
      <span className="min-w-0 flex-1 truncate">
        {t("workflows.kind.health_check")} · {t(`workflows.status.${run.displayStatus}`)}
        {stage ? ` · ${t(stage.labelKey)}` : ""}
        {stage?.progress?.total != null ? ` · ${t("workflows.progress.count", { current: stage.progress.current, total: stage.progress.total })}` : ""}
      </span>
      <button type="button" className="btn btn--secondary btn--sm" disabled={opening} onClick={() => void open()}>{t("workflows.action.viewProgress")}</button>
      {run.cancellable ? <button type="button" className="btn btn--secondary btn--sm" disabled={pending} onClick={() => void cancel()}>
        {pending ? "…" : t("lint.actions.cancel")}
      </button> : null}
    </div>
    {error ? <p role="alert" className="mt-1 text-[var(--danger)]">{error}</p> : null}
  </div>;
}

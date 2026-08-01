import { ArrowDown, ArrowLeft, ArrowUp, RotateCcw, Square } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowRun } from "../../types/workflow";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowPipeline } from "./WorkflowPipeline";
import { workflowKindKey, workflowStatusKey } from "./workflowPresentation";

export function WorkflowTaskDetail({ run, controller, queuedRuns, onOpenLogs }: { run: WorkflowRun; controller: WorkflowsController; queuedRuns: WorkflowRun[]; onOpenLogs: (taskId: string) => void }) {
  const { t } = useTranslation();
  const queueIndex = queuedRuns.findIndex((candidate) => candidate.taskId === run.taskId);
  const retryable = run.displayStatus === "failed" || run.displayStatus === "interrupted" || (run.displayStatus === "completed" && run.kind === "generate_content");
  const undoAvailable = run.displayStatus === "cancelled" && Boolean(run.undoCancelUntil) && Date.parse(run.undoCancelUntil ?? "") > Date.now();
  const cancelRun = () => {
    if (run.displayStatus === "running" && !window.confirm(t("workflows.cancel.confirm"))) return;
    void controller.cancel(run.taskId);
  };
  return <div className="workflow-detail">
    <button className="workflow-back" onClick={controller.backToOverview} type="button"><ArrowLeft aria-hidden="true" size={14} />{t("workflows.action.back")}</button>
    <div className="workflow-detail__heading"><div><h2>{t(workflowKindKey(run.kind))}</h2><p>{t(workflowStatusKey(run.displayStatus))} · <span className="font-mono">{run.taskId.slice(0, 8)}</span></p></div><span className={`workflow-badge is-${run.displayStatus.replaceAll("_", "-")}`}>{t(workflowStatusKey(run.displayStatus))}</span></div>
    {run.pendingAction ? <section className="workflow-attention"><h3>{t("workflows.attention.title")}</h3><p>{t("workflows.attention.description", { count: run.pendingAction.affectedPaths.length })}</p><dl className="workflow-decision-facts"><div><dt>{t("workflows.attention.risk")}</dt><dd>{run.pendingAction.riskLevel}</dd></div><div><dt>{t("workflows.attention.actionType")}</dt><dd>{run.pendingAction.actionType}</dd></div><div><dt>{t("workflows.attention.checkpoint")}</dt><dd className="font-mono">{run.pendingAction.checkpointHash ?? t("workflows.attention.noCheckpoint")}</dd></div></dl><h4>{t("workflows.context.paths")}</h4><ul className="workflow-affected-paths">{run.pendingAction.affectedPaths.map((path) => <li key={path}><code>{path}</code></li>)}</ul><div className="workflow-actions"><button className="btn btn--primary" onClick={() => void controller.confirm(run.taskId, run.pendingAction!.id)} type="button">{t("workflows.action.applyChanges", { count: run.pendingAction.affectedPaths.length })}</button><button className="btn btn--secondary" onClick={() => void controller.discard(run.taskId)} type="button">{t("workflows.action.discard")}</button></div></section> : null}
    {run.error ? <section className="workflow-error"><h3>{t("workflows.error.title")}</h3><p>{t(run.error.messageKey)}</p><code>{run.error.code}</code></section> : null}
    <section><h3 className="workflow-section-title">{t("workflows.pipeline.title")}</h3><WorkflowPipeline stages={run.stages} /></section>
    {run.result ? <section><h3 className="workflow-section-title">{t("workflows.result.title")}</h3><dl className="workflow-result">{Object.entries(run.result).filter(([key]) => key !== "kind").map(([key, value]) => <div key={key}><dt>{t(`workflows.result.${key}`)}</dt><dd>{Array.isArray(value) ? value.join(", ") : typeof value === "boolean" ? (value ? t("workflows.result.yes") : t("workflows.result.no")) : value ?? "–"}</dd></div>)}</dl></section> : null}
    <div className="workflow-actions">
      {(run.displayStatus === "running" || run.displayStatus === "queued") && run.cancellable !== false ? <button className="btn btn--secondary" onClick={cancelRun} type="button"><Square aria-hidden="true" size={13} />{t("workflows.action.cancel")}</button> : null}
      {undoAvailable ? <button className="btn btn--secondary" onClick={() => void controller.undoCancel(run.taskId)} type="button"><RotateCcw aria-hidden="true" size={13} />{t("workflows.action.undoCancel")}</button> : null}
      {retryable ? <button className="btn btn--secondary" onClick={() => void controller.retry(run.taskId)} type="button"><RotateCcw aria-hidden="true" size={13} />{t("workflows.action.retry")}</button> : null}
      {run.continuationRequired ? <button className="btn btn--primary" onClick={() => void controller.continueQueue()} type="button">{t("workflows.action.continueQueue")}</button> : null}
      <button className="btn btn--ghost" onClick={() => onOpenLogs(run.taskId)} type="button">{t("workflows.action.viewLogs")}</button>
      {run.displayStatus === "queued" && queueIndex > 0 ? <button aria-label={t("workflows.action.moveUp")} className="btn btn--ghost btn--icon" onClick={() => void controller.reorder(run.taskId, queuedRuns[queueIndex - 1]?.taskId ?? null)} title={t("workflows.action.moveUp")} type="button"><ArrowUp aria-hidden="true" size={14} /></button> : null}
      {run.displayStatus === "queued" && queueIndex >= 0 && queueIndex < queuedRuns.length - 1 ? <button aria-label={t("workflows.action.moveDown")} className="btn btn--ghost btn--icon" onClick={() => void controller.reorder(run.taskId, queuedRuns[queueIndex + 2]?.taskId ?? null)} title={t("workflows.action.moveDown")} type="button"><ArrowDown aria-hidden="true" size={14} /></button> : null}
    </div>
  </div>;
}

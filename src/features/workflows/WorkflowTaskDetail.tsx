import { ArrowDown, ArrowLeft, ArrowUp, RotateCcw, Square } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkflowRun } from "../../types/workflow";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowPipeline } from "./WorkflowPipeline";
import { workflowKindKey, workflowStatusKey } from "./workflowPresentation";

export function WorkflowTaskDetail({
  run,
  controller,
  queuedRuns,
  onOpenLogs,
}: {
  run: WorkflowRun;
  controller: WorkflowsController;
  queuedRuns: WorkflowRun[];
  onOpenLogs: (taskId: string) => void;
}) {
  const { t } = useTranslation();
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  const [retryMenuOpen, setRetryMenuOpen] = useState(false);
  const retryOptionsId = useId();
  const [undoClock, setUndoClock] = useState(() => Date.now());
  const queueIndex = queuedRuns.findIndex((candidate) => candidate.taskId === run.taskId);
  const retryable = run.displayStatus === "failed" || run.displayStatus === "interrupted";
  const rerunnable = run.displayStatus === "completed";
  const undoAvailable =
    run.displayStatus === "cancelled" &&
    Boolean(run.undoCancelUntil) &&
    Date.parse(run.undoCancelUntil ?? "") > undoClock;
  const counts = run.decisionReview?.counts;
  const recommendedNext =
    run.result?.kind === "update_wiki"
      ? "health_check"
      : run.result?.kind === "health_check" && run.result.errorCount === 0
        ? "generate_content"
        : null;

  useEffect(() => {
    setUndoClock(Date.now());
    if (!run.undoCancelUntil) return;
    const remaining = Date.parse(run.undoCancelUntil) - Date.now();
    if (remaining <= 0) return;
    const timeout = window.setTimeout(
      () => setUndoClock(Date.now()),
      Math.min(remaining + 25, 2_147_483_647),
    );
    return () => window.clearTimeout(timeout);
  }, [run.taskId, run.undoCancelUntil]);

  const requestCancel = () => {
    if (run.displayStatus === "running") {
      setConfirmingCancel(true);
      return;
    }
    void controller.cancel(run.taskId);
  };

  return (
    <div className="workflow-detail">
      <button className="workflow-back" onClick={controller.backToOverview} type="button">
        <ArrowLeft aria-hidden="true" size={14} />
        {t("workflows.action.back")}
      </button>
      <div className="workflow-detail__heading">
        <div>
          <h2>{t(workflowKindKey(run.kind))}</h2>
          <p>
            {t(workflowStatusKey(run.displayStatus))} · <span className="font-mono">{run.taskId.slice(0, 8)}</span>
          </p>
        </div>
        <span className={`workflow-badge is-${run.displayStatus.replaceAll("_", "-")}`}>
          {t(workflowStatusKey(run.displayStatus))}
        </span>
      </div>

      {run.pendingAction ? (
        <section className="workflow-attention">
          <h3>{t("workflows.attention.title")}</h3>
          <p>{run.decisionReview?.reason ?? t("workflows.attention.description", { count: run.pendingAction.affectedPaths.length })}</p>
          {counts ? (
            <dl className="workflow-decision-facts">
              <div><dt>{t("workflows.attention.created")}</dt><dd>{counts.created}</dd></div>
              <div><dt>{t("workflows.attention.modified")}</dt><dd>{counts.modified}</dd></div>
              <div><dt>{t("workflows.attention.overwritten")}</dt><dd>{counts.overwritten}</dd></div>
              <div><dt>{t("workflows.attention.deleted")}</dt><dd>{counts.deleted}</dd></div>
              <div><dt>{t("workflows.attention.userEdits")}</dt><dd>{run.decisionReview?.userEditsDetected ? t("workflows.result.yes") : t("workflows.result.no")}</dd></div>
            </dl>
          ) : null}
          <dl className="workflow-decision-facts">
            <div><dt>{t("workflows.attention.risk")}</dt><dd>{run.pendingAction.riskLevel}</dd></div>
            <div><dt>{t("workflows.attention.actionType")}</dt><dd>{run.pendingAction.actionType}</dd></div>
            <div><dt>{t("workflows.attention.checkpoint")}</dt><dd className="font-mono">{run.pendingAction.checkpointHash ?? t("workflows.attention.noCheckpoint")}</dd></div>
          </dl>
          <h4>{t("workflows.context.paths")}</h4>
          <ul className="workflow-affected-paths">
            {run.pendingAction.affectedPaths.map((path) => <li key={path}><code>{path}</code></li>)}
          </ul>
          {run.decisionReview?.fileDiffs.map((file) => (
            <details key={`${file.path}:${file.diff.length}`} className="workflow-file-diff">
              <summary><code>{file.path}</code></summary>
              <pre className="terminal mt-2 overflow-auto whitespace-pre-wrap p-3 text-[11px]">{file.diff}</pre>
            </details>
          ))}
          <div className="workflow-actions">
            <button className="btn btn--primary" onClick={() => void controller.confirm(run.taskId, run.pendingAction!.id)} type="button">
              {t("workflows.action.applyChanges", { count: run.pendingAction.affectedPaths.length })}
            </button>
            <button className="btn btn--secondary" onClick={() => void controller.discard(run.taskId)} type="button">
              {t("workflows.action.discard")}
            </button>
          </div>
        </section>
      ) : null}

      {run.error ? <section className="workflow-error"><h3>{t("workflows.error.title")}</h3><p>{t(run.error.messageKey)}</p><code>{run.error.code}</code></section> : null}
      {run.displayStatus === "interrupted" ? (
        <section className="workflow-attention">
          <h3>{t("workflows.recovery.interruptedTitle")}</h3>
          <p>{t("workflows.recovery.interruptedDescription", { stage: run.currentStageId ?? t("workflows.recovery.noStage") })}</p>
          <p>{t("workflows.recovery.mutationState", { state: run.result ? t("workflows.recovery.committed") : t("workflows.recovery.notCommitted") })}</p>
        </section>
      ) : null}
      <section><h3 className="workflow-section-title">{t("workflows.pipeline.title")}</h3><WorkflowPipeline stages={run.stages} /></section>
      {run.result ? (
        <section>
          <h3 className="workflow-section-title">{t("workflows.result.title")}</h3>
          <dl className="workflow-result">{Object.entries(run.result).filter(([key]) => key !== "kind").map(([key, value]) => <div key={key}><dt>{t(`workflows.result.${key}`)}</dt><dd>{Array.isArray(value) ? value.join(", ") : typeof value === "boolean" ? (value ? t("workflows.result.yes") : t("workflows.result.no")) : value ?? "—"}</dd></div>)}</dl>
          <div className="workflow-actions mt-3">
            <button className="btn btn--primary" type="button" onClick={() => void controller.openResult(run)}>{t("workflows.action.openResult")}</button>
            {recommendedNext ? <button className="btn btn--secondary" type="button" onClick={() => void controller.prepare(recommendedNext)}>{t("workflows.action.prepareNext", { workflow: t(workflowKindKey(recommendedNext)) })}</button> : null}
          </div>
        </section>
      ) : null}

      {confirmingCancel ? (
        <section className="workflow-attention" role="alert">
          <p>{t("workflows.cancel.confirmDescription")}</p>
          <div className="workflow-actions">
            <button className="btn btn--danger" type="button" onClick={() => { setConfirmingCancel(false); void controller.cancel(run.taskId); }}>{t("workflows.action.confirmCancel")}</button>
            <button className="btn btn--secondary" type="button" onClick={() => setConfirmingCancel(false)}>{t("workflows.action.keepRunning")}</button>
          </div>
        </section>
      ) : null}

      <div className="workflow-actions">
        {(run.displayStatus === "running" || run.displayStatus === "queued") && run.cancellable !== false ? <button className="btn btn--secondary" onClick={requestCancel} type="button"><Square aria-hidden="true" size={13} />{t("workflows.action.cancel")}</button> : null}
        {undoAvailable ? <button className="btn btn--secondary" onClick={() => void controller.undoCancel(run.taskId)} type="button"><RotateCcw aria-hidden="true" size={13} />{t("workflows.action.undoCancel")}</button> : null}
        {retryable || rerunnable ? (
          <div>
            <button
              aria-controls={retryOptionsId}
              aria-expanded={retryMenuOpen}
              className="btn btn--secondary"
              onClick={() => setRetryMenuOpen((open) => !open)}
              type="button"
            >
              <RotateCcw aria-hidden="true" size={13} />
              {t(rerunnable ? "workflows.action.runAgain" : "workflows.action.retry")}
            </button>
            {retryMenuOpen ? (
              <div
                aria-label={t("workflows.retry.options")}
                className="workflow-retry-menu"
                id={retryOptionsId}
                role="group"
              >
                {retryable ? <button type="button" onClick={() => { setRetryMenuOpen(false); void controller.retry(run.taskId); }}>{t("workflows.retry.sameSettings")}</button> : null}
                <button type="button" onClick={() => { setRetryMenuOpen(false); void controller.adjustAndPrepare(run); }}>{t("workflows.retry.adjustSettings")}</button>
                <button type="button" onClick={() => { setRetryMenuOpen(false); void controller.adjustAndPrepare(run, true); }}>{t("workflows.retry.openSettings")}</button>
              </div>
            ) : null}
          </div>
        ) : null}
        <button className="btn btn--ghost" onClick={() => onOpenLogs(run.taskId)} type="button">{t("workflows.action.viewLogs")}</button>
        {run.displayStatus === "queued" && queueIndex > 0 ? <button aria-label={t("workflows.action.moveUp")} className="btn btn--ghost btn--icon" onClick={() => void controller.reorder(run.taskId, queuedRuns[queueIndex - 1]?.taskId ?? null)} title={t("workflows.action.moveUp")} type="button"><ArrowUp aria-hidden="true" size={14} /></button> : null}
        {run.displayStatus === "queued" && queueIndex >= 0 && queueIndex < queuedRuns.length - 1 ? <button aria-label={t("workflows.action.moveDown")} className="btn btn--ghost btn--icon" onClick={() => void controller.reorder(run.taskId, queuedRuns[queueIndex + 2]?.taskId ?? null)} title={t("workflows.action.moveDown")} type="button"><ArrowDown aria-hidden="true" size={14} /></button> : null}
      </div>
    </div>
  );
}

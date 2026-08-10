import { AlertTriangle, ArrowDown, ArrowLeft, ArrowUp, Ban, FileText, RotateCcw, Square } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useWorkflowStore, workflowOperationPending } from "../../stores/workflowStore";
import { useProjectStore } from "../../stores/projectStore";
import { getWorkflowFileDiff } from "../../services/workflowApi";
import type { WorkflowRun } from "../../types/workflow";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowPipeline } from "./WorkflowPipeline";
import {
  presentWorkflowResult,
  workflowActionTypeKey,
  workflowKindKey,
  workflowPrerequisiteActionKey,
  workflowRiskKey,
  workflowStatusKey,
  type WorkflowResultValue,
} from "./workflowPresentation";

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
  const { t, i18n } = useTranslation();
  const language = i18n?.resolvedLanguage ?? i18n?.language ?? "en";
  const operations = useWorkflowStore((state) => state.operations);
  const taskMutationPending = Object.entries(operations).some(([key, operation]) =>
    operation.pending
    && key.startsWith(`task:${run.taskId}:`)
    && !key.includes(":hydrate:")
    && !key.endsWith(":open")
    && !key.endsWith(":open-result"),
  );
  const openResultPending = workflowOperationPending(
    operations,
    `task:${run.taskId}:open-result`,
  );
  const prepareCurrentPending = workflowOperationPending(operations, `prepare:${run.kind}`);
  const prepareNextPending = recommendedWorkflowOperationPending(
    operations,
    run.result?.kind === "update_wiki"
      ? "health_check"
      : run.result?.kind === "health_check" && run.result.errorCount === 0
        ? "generate_content"
        : null,
  );
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  const [retryMenuOpen, setRetryMenuOpen] = useState(false);
  const [decisionReviewStale, setDecisionReviewStale] = useState(false);
  const retryOptionsId = useId();
  const [undoClock, setUndoClock] = useState(() => Date.now());
  const queueIndex = queuedRuns.findIndex((candidate) => candidate.taskId === run.taskId);
  const retryable = run.displayStatus === "failed" || run.displayStatus === "interrupted";
  const undoAvailable =
    run.displayStatus === "cancelled" &&
    Boolean(run.undoCancelUntil) &&
    Date.parse(run.undoCancelUntil ?? "") > undoClock;
  const counts = run.decisionReview?.counts;
  const decisionReviewReady = Boolean(run.decisionReview) && !decisionReviewStale;
  const recommendedNext =
    run.result?.kind === "update_wiki"
      ? "health_check"
      : run.result?.kind === "health_check" && run.result.errorCount === 0
        ? "generate_content"
        : null;
  const resultPresentation = presentWorkflowResult(run);
  const currentStage = run.stages.find((stage) => stage.id === run.currentStageId);
  const currentStageLabel = currentStage ? t(currentStage.labelKey) : t("workflows.recovery.noStage");

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

  useEffect(() => {
    setDecisionReviewStale(false);
  }, [run.canonicalIdentityKey, run.identityRevision, run.pendingAction?.id, run.taskId]);

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
          <h2 data-workflow-surface-title tabIndex={-1}>{t(workflowKindKey(run.kind))}</h2>
          <p>
            {t(workflowStatusKey(run.displayStatus))} · <span className="font-mono">{run.taskId.slice(0, 8)}</span>
          </p>
        </div>
        <span className={`workflow-badge is-${run.displayStatus.replaceAll("_", "-")}`}>
          {t(workflowStatusKey(run.displayStatus))}
        </span>
      </div>

      {run.pendingAction ? (
        <section aria-labelledby={`workflow-review-${run.taskId}`} className="workflow-attention workflow-decision-review">
          <div className="workflow-attention__title">
            <AlertTriangle aria-hidden="true" size={15} />
            <h3 id={`workflow-review-${run.taskId}`}>{t("workflows.attention.title")}</h3>
          </div>
          <p className="workflow-decision-reason">{run.decisionReview?.reason ?? t("workflows.attention.description", { count: run.pendingAction.affectedPaths.length })}</p>
          <dl className="workflow-decision-facts workflow-decision-classification">
            <div><dt>{t("workflows.attention.risk")}</dt><dd>{t(workflowRiskKey(run.pendingAction.riskLevel))}</dd></div>
            <div><dt>{t("workflows.attention.actionType")}</dt><dd>{t(workflowActionTypeKey(run.pendingAction.actionType))}</dd></div>
          </dl>
          {counts ? (
            <dl className="workflow-decision-facts workflow-decision-counts">
              <div><dt>{t("workflows.attention.created")}</dt><dd>{counts.created}</dd></div>
              <div><dt>{t("workflows.attention.modified")}</dt><dd>{counts.modified}</dd></div>
              <div><dt>{t("workflows.attention.overwritten")}</dt><dd>{counts.overwritten}</dd></div>
              <div><dt>{t("workflows.attention.deleted")}</dt><dd>{counts.deleted}</dd></div>
            </dl>
          ) : null}
          <div aria-label={t("workflows.attention.paths")} className="workflow-decision-paths" role="region">
            <h4>{t("workflows.attention.paths")}</h4>
            <ul className="workflow-affected-paths">
              {run.pendingAction.affectedPaths.map((path) => <li key={path}><code title={path}>{path}</code></li>)}
            </ul>
          </div>
          {!run.decisionReview ? (
            <p className="workflow-user-edits-clear" role="status">{t("workflows.attention.reviewLoading")}</p>
          ) : run.decisionReview.userEditsDetected ? (
            <p className="workflow-conflict-notice" role="alert">
              <AlertTriangle aria-hidden="true" size={14} />
              {t("workflows.attention.userEditsConflict")}
            </p>
          ) : (
            <p className="workflow-user-edits-clear"><span>{t("workflows.attention.userEdits")}</span>{t("workflows.result.no")}</p>
          )}
          <div className="workflow-decision-checkpoint">
            <span>{t("workflows.attention.checkpoint")}</span>
            <code>{run.pendingAction.checkpointHash ?? t("workflows.attention.noCheckpoint")}</code>
          </div>
          {run.decisionReview?.fileDiffs.map((file) => (
            <LazyWorkflowFileDiff
              diff={file.diff}
              diffKind={file.kind ?? "two_way"}
              fileId={file.fileId ?? null}
              key={`${run.canonicalIdentityKey}:${run.identityRevision}:${run.taskId}:${run.pendingAction!.id}:${file.fileId ?? file.path}`}
              path={file.path}
              pendingActionId={run.pendingAction!.id}
              run={run}
              onStale={() => setDecisionReviewStale(true)}
            />
          ))}
          {decisionReviewStale ? (
            <p className="workflow-conflict-notice" role="alert">
              <AlertTriangle aria-hidden="true" size={14} />
              {t("workflows.diff.stale")}
            </p>
          ) : null}
          {taskMutationPending ? <p className="workflow-atomic-note" role="status">{t("workflows.attention.atomicApply")}</p> : null}
          <div className="workflow-actions">
            <button className="btn btn--primary" disabled={taskMutationPending || !decisionReviewReady} onClick={() => void controller.confirm(run.taskId, run.pendingAction!.id)} type="button">
              {t("workflows.action.applyChanges", { count: run.pendingAction.affectedPaths.length })}
            </button>
            {decisionReviewStale ? (
              <button className="btn btn--secondary" disabled={taskMutationPending || prepareCurrentPending} onClick={() => void controller.adjustAndPrepare(run)} type="button">
                {t("workflows.action.prepareAgain")}
              </button>
            ) : null}
            <button className="btn btn--secondary" disabled={taskMutationPending} onClick={() => void controller.discard(run.taskId)} type="button">
              {t("workflows.action.discard")}
            </button>
          </div>
        </section>
      ) : null}

      <section aria-labelledby={`workflow-pipeline-${run.taskId}`}>
        <h3 className="workflow-section-title" id={`workflow-pipeline-${run.taskId}`}>{t("workflows.pipeline.title")}</h3>
        <WorkflowPipeline currentStageId={run.currentStageId} displayStatus={run.displayStatus} stages={run.stages} />
      </section>
      {run.displayStatus === "failed" && run.error ? (
        <section aria-label={t("workflows.failure.title")} className="workflow-error workflow-failure" role="region">
          <div className="workflow-attention__title">
            <AlertTriangle aria-hidden="true" size={15} />
            <h3>{t("workflows.failure.title")}</h3>
          </div>
          <p>{t(run.error.messageKey)}</p>
          <dl className="workflow-failure__facts">
            <div><dt>{t("workflows.failure.failedStage")}</dt><dd>{currentStageLabel}</dd></div>
            <div><dt>{t("workflows.failure.completedStages")}</dt><dd>{run.stages.filter((stage) => stage.status === "completed").length}</dd></div>
            <div><dt>{t("workflows.failure.projectState")}</dt><dd>{t(`workflows.failure.mutation.${run.error.projectMutationState ?? "unknown"}`)}</dd></div>
            {run.error.suggestedAction ? <div><dt>{t("workflows.failure.suggestedAction")}</dt><dd>{t(workflowPrerequisiteActionKey(run.error.suggestedAction))}</dd></div> : null}
          </dl>
          <details className="workflow-technical-error">
            <summary>{t("workflows.error.technicalDetails")}</summary>
            <code>{run.error.code}</code>
          </details>
        </section>
      ) : null}
      {run.displayStatus === "interrupted" ? (
        <section className="workflow-attention" role="status">
          <h3>{t("workflows.interrupted.title")}</h3>
          <p>{t("workflows.recovery.interruptedDescription", { stage: currentStageLabel })}</p>
          {run.error ? <p>{t(run.error.messageKey)}</p> : null}
          <p>{t("workflows.recovery.mutationState", { state: t(`workflows.failure.mutation.${run.error?.projectMutationState ?? "unknown"}`) })}</p>
        </section>
      ) : null}
      {run.displayStatus === "cancelled" ? (
        <section className="workflow-cancelled" role="status">
          <Ban aria-hidden="true" size={15} />
          <div><h3>{t("workflows.cancelled.title")}</h3><p>{t("workflows.cancelled.description")}</p></div>
        </section>
      ) : null}
      {resultPresentation ? (
        <section aria-label={t(resultPresentation.titleKey)} className={`workflow-typed-result is-${run.result?.kind}`} role="region">
          <h3 className="workflow-section-title">{t(resultPresentation.titleKey)}</h3>
          <p className="workflow-result-summary">{t(resultPresentation.summaryKey)}</p>
          <dl className="workflow-result">
            {resultPresentation.rows.map((row) => (
              <div key={row.labelKey}>
                <dt>{t(row.labelKey)}</dt>
                <dd className={row.value.kind === "text" && row.value.mono ? "font-mono" : undefined}>{renderResultValue(row.value, language, t)}</dd>
              </div>
            ))}
          </dl>
          {resultPresentation.paths.length > 0 ? (
            <div aria-label={t("workflows.result.paths")} className="workflow-result-paths" role="region">
              <h4>{t("workflows.result.paths")}</h4>
              <ul>{resultPresentation.paths.map((path) => <li key={path}><code title={path}>{path}</code></li>)}</ul>
            </div>
          ) : null}
          <div className="workflow-actions mt-3">
            <button className="btn btn--primary" disabled={openResultPending} type="button" onClick={() => void controller.openResult(run)}>{t(resultPresentation.primaryActionKey)}</button>
            <button className="btn btn--secondary" disabled={taskMutationPending || prepareCurrentPending} type="button" onClick={() => void controller.adjustAndPrepare(run)}>{t("workflows.action.runAgain")}</button>
            {recommendedNext ? <button className="btn btn--secondary" disabled={taskMutationPending || prepareNextPending} type="button" onClick={() => void controller.prepare(recommendedNext)}>{t("workflows.action.prepareNext", { workflow: t(workflowKindKey(recommendedNext)) })}</button> : null}
          </div>
        </section>
      ) : null}

      {confirmingCancel ? (
        <section className="workflow-attention" role="alert">
          <p>{t("workflows.cancel.confirmDescription")}</p>
          <div className="workflow-actions">
            <button className="btn btn--danger" disabled={taskMutationPending} type="button" onClick={() => { setConfirmingCancel(false); void controller.cancel(run.taskId); }}>{t("workflows.action.confirmCancel")}</button>
            <button className="btn btn--secondary" type="button" onClick={() => setConfirmingCancel(false)}>{t("workflows.action.keepRunning")}</button>
          </div>
        </section>
      ) : null}

      <div className="workflow-actions">
        {(run.displayStatus === "running" || run.displayStatus === "queued") && run.cancellable !== false ? <button className="btn btn--secondary" disabled={taskMutationPending} onClick={requestCancel} type="button"><Square aria-hidden="true" size={13} />{t("workflows.action.cancel")}</button> : null}
        {undoAvailable ? <button className="btn btn--secondary" disabled={taskMutationPending} onClick={() => void controller.undoCancel(run.taskId)} type="button"><RotateCcw aria-hidden="true" size={13} />{t("workflows.action.undoCancel")}</button> : null}
        {retryable ? (
          <div>
            <button
              aria-controls={retryOptionsId}
              aria-expanded={retryMenuOpen}
              className="btn btn--secondary"
              disabled={taskMutationPending || prepareCurrentPending}
              onClick={() => setRetryMenuOpen((open) => !open)}
              type="button"
            >
              <RotateCcw aria-hidden="true" size={13} />
              {t("workflows.action.retry")}
            </button>
            {retryMenuOpen ? (
              <div
                aria-label={t("workflows.retry.options")}
                className="workflow-retry-menu"
                id={retryOptionsId}
                role="group"
              >
                <button disabled={taskMutationPending} type="button" onClick={() => { setRetryMenuOpen(false); void controller.retry(run.taskId); }}>{t("workflows.retry.sameSettings")}</button>
                <button disabled={taskMutationPending || prepareCurrentPending} type="button" onClick={() => { setRetryMenuOpen(false); void controller.adjustAndPrepare(run); }}>{t("workflows.retry.adjustSettings")}</button>
                <button disabled={taskMutationPending || prepareCurrentPending} type="button" onClick={() => { setRetryMenuOpen(false); void controller.adjustAndPrepare(run, true); }}>{t("workflows.retry.openSettings")}</button>
              </div>
            ) : null}
          </div>
        ) : null}
        {run.displayStatus === "queued" && queueIndex > 0 ? <button aria-label={t("workflows.action.moveUp")} className="btn btn--ghost btn--icon" disabled={taskMutationPending} onClick={() => void controller.reorder(run.taskId, queuedRuns[queueIndex - 1]?.taskId ?? null)} title={t("workflows.action.moveUp")} type="button"><ArrowUp aria-hidden="true" size={14} /></button> : null}
        {run.displayStatus === "queued" && queueIndex >= 0 && queueIndex < queuedRuns.length - 1 ? <button aria-label={t("workflows.action.moveDown")} className="btn btn--ghost btn--icon" disabled={taskMutationPending} onClick={() => void controller.reorder(run.taskId, queuedRuns[queueIndex + 2]?.taskId ?? null)} title={t("workflows.action.moveDown")} type="button"><ArrowDown aria-hidden="true" size={14} /></button> : null}
      </div>
      <details className="workflow-logs-disclosure">
        <summary><FileText aria-hidden="true" size={14} />{t("workflows.logs.title")}</summary>
        <p>{t("workflows.logs.description")}</p>
        <button className="btn btn--ghost btn--sm" onClick={() => onOpenLogs(run.taskId)} type="button">{t("workflows.action.viewLogs")}</button>
      </details>
    </div>
  );
}

function LazyWorkflowFileDiff({ path, diff, diffKind, fileId, pendingActionId, run, onStale }: {
  path: string;
  diff: string | null;
  diffKind: "two_way" | "three_way";
  fileId: string | null;
  pendingActionId: string;
  run: WorkflowRun;
  onStale: () => void;
}) {
  const { t } = useTranslation();
  const projectRootPath = useProjectStore((state) => state.currentProject.rootPath);
  const [open, setOpen] = useState(false);
  const [content, setContent] = useState<string | null>(diff);
  const [nextCursor, setNextCursor] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<"retry" | "stale" | null>(null);
  const [effectiveKind, setEffectiveKind] = useState(diffKind);
  const requestEpoch = useRef(0);

  useEffect(() => () => {
    requestEpoch.current += 1;
  }, [fileId, path, pendingActionId, run.canonicalIdentityKey, run.identityRevision, run.taskId]);

  const loadChunk = async (cursor: number | null) => {
    if (!fileId || loading) return;
    const epoch = requestEpoch.current + 1;
    requestEpoch.current = epoch;
    setLoading(true);
    setError(null);
    try {
      const page = await getWorkflowFileDiff({
        projectId: run.projectId,
        projectRootPath,
        taskId: run.taskId,
        pendingActionId,
        fileId,
        cursor,
        limitBytes: 64 * 1024,
      });
      if (requestEpoch.current !== epoch) return;
      setContent((current) => cursor === null ? page.diff : `${current ?? ""}${page.diff}`);
      setNextCursor(page.nextCursor);
      setEffectiveKind(page.kind ?? diffKind);
    } catch (loadError) {
      if (requestEpoch.current === epoch) {
        const stale = backendErrorCode(loadError) === "WORKFLOW_OUTPUT_BASELINE_CHANGED";
        setError(stale ? "stale" : "retry");
        if (stale) {
          setNextCursor(null);
          onStale();
        }
      }
    } finally {
      if (requestEpoch.current === epoch) setLoading(false);
    }
  };
  const toggle = () => {
    const nextOpen = !open;
    setOpen(nextOpen);
    if (nextOpen && content === null && fileId) void loadChunk(null);
  };
  const diffLabelKey = effectiveKind === "three_way" ? "workflows.diff.threeWay" : "workflows.diff.file";
  return <details aria-label={t(diffLabelKey, { path })} className="workflow-file-diff" open={open}>
    <summary onClick={(event) => { event.preventDefault(); toggle(); }}><span>{t(diffLabelKey)}</span><code title={path}>{path}</code></summary>
    {open && content !== null ? <pre className="terminal mt-2 overflow-auto whitespace-pre-wrap p-3 text-[11px]">{content}</pre> : null}
    {open && loading ? <p className="workflow-muted" role="status">{t("workflows.diff.loading")}</p> : null}
    {open && error === "stale" ? <p className="workflow-conflict-notice" role="alert">{t("workflows.diff.stale")}</p> : null}
    {open && error === "retry" ? <button className="btn btn--secondary btn--sm" onClick={() => void loadChunk(nextCursor)} type="button">{t("workflows.action.retry")}</button> : null}
    {open && nextCursor !== null && !loading && !error ? <button className="btn btn--ghost btn--sm" onClick={() => void loadChunk(nextCursor)} type="button">{t("workflows.diff.loadMore")}</button> : null}
  </details>;
}

function backendErrorCode(error: unknown): string | null {
  if (!error || typeof error !== "object" || !("code" in error)) return null;
  return typeof error.code === "string" ? error.code : null;
}

function renderResultValue(
  value: WorkflowResultValue,
  language: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  switch (value.kind) {
    case "count":
      return new Intl.NumberFormat(language).format(value.value);
    case "boolean":
      return t(value.value ? "workflows.result.yes" : "workflows.result.no");
    case "text":
      return value.value ?? t("workflows.result.unavailable");
    case "translation":
      return t(value.key);
    case "duration": {
      const seconds = Math.max(0, Math.round(value.milliseconds / 1_000));
      const formatter = new Intl.NumberFormat(language);
      if (seconds < 60) return t("workflows.duration.seconds", { count: formatter.format(seconds) });
      const minutes = Math.floor(seconds / 60);
      const remainingSeconds = seconds % 60;
      return remainingSeconds === 0
        ? t("workflows.duration.minutes", { count: formatter.format(minutes) })
        : t("workflows.duration.minutesSeconds", {
            minutes: formatter.format(minutes),
            seconds: formatter.format(remainingSeconds),
          });
    }
  }
}

function recommendedWorkflowOperationPending(
  operations: Parameters<typeof workflowOperationPending>[0],
  kind: WorkflowRun["kind"] | null,
): boolean {
  return kind ? workflowOperationPending(operations, `prepare:${kind}`) : false;
}

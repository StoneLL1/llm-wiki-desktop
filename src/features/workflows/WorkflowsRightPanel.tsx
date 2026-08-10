import {
  AlertTriangle,
  Ban,
  CheckCircle2,
  CircleDashed,
  Clipboard,
  Clock3,
  GitBranch,
  History,
  Layers3,
  LoaderCircle,
  RefreshCw,
  XCircle,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { RightPanelHeader } from "../../components/app/RightPanelHeader";
import { hydrateAndSelectWorkflowRun } from "../../services/workflowNavigation";
import { useProjectStore } from "../../stores/projectStore";
import {
  captureWorkflowRequestGuard,
  useWorkflowStore,
  workflowOperationPending,
  workflowRequestGuardMatches,
} from "../../stores/workflowStore";
import type {
  WorkflowDisplayStatus,
  WorkflowRun,
  WorkflowScope,
  WorkflowStageStatus,
} from "../../types/workflow";
import {
  workflowArtifactTypeKey,
  workflowDateTimeLabel,
  workflowKindKey,
  workflowRouteKey,
  workflowStatusKey,
} from "./workflowPresentation";

const EMPTY_VALUE = "—";

function statusPresentation(status: WorkflowDisplayStatus): {
  icon: ReactNode;
  tone: string;
} {
  const iconProps = { "aria-hidden": true, size: 13 } as const;
  switch (status) {
    case "completed":
      return { icon: <CheckCircle2 {...iconProps} />, tone: "is-success" };
    case "running":
      return { icon: <LoaderCircle {...iconProps} />, tone: "is-running" };
    case "queued":
      return { icon: <Clock3 {...iconProps} />, tone: "is-neutral" };
    case "waiting_for_confirmation":
      return { icon: <AlertTriangle {...iconProps} />, tone: "is-warning" };
    case "failed":
      return { icon: <XCircle {...iconProps} />, tone: "is-danger" };
    case "cancelled":
      return { icon: <Ban {...iconProps} />, tone: "is-neutral" };
    case "interrupted":
      return { icon: <CircleDashed {...iconProps} />, tone: "is-warning" };
  }
}

function scopeDetailKey(scope: WorkflowScope): string {
  if (scope.kind === "update_wiki") {
    return scope.mode === "changed_sources"
      ? "workflows.mode.changedSources"
      : "workflows.mode.fullRecompile";
  }
  if (scope.kind === "health_check") {
    return scope.mode === "local_quick"
      ? "workflows.mode.localQuick"
      : "workflows.mode.complete";
  }
  return workflowArtifactTypeKey(scope.artifactType);
}

function runOutputPaths(run: WorkflowRun): string[] {
  if (run.result?.kind === "generate_content") return run.result.outputPaths;
  if (run.scope.kind === "generate_content" && run.scope.outputPath) {
    return [run.scope.outputPath];
  }
  return [];
}

function runAffectedPaths(run: WorkflowRun): string[] {
  const paths = [
    ...(run.result?.kind === "update_wiki" ? run.result.affectedPaths : []),
    ...(run.result?.kind === "generate_content" ? run.result.outputPaths : []),
    ...(run.pendingAction?.affectedPaths ?? []),
  ];
  return [...new Set(paths)];
}

function CopyablePath({ path }: { path: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const copy = () => {
    if (!navigator.clipboard) return;
    void navigator.clipboard.writeText(path).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    }).catch(() => setCopied(false));
  };

  return (
    <div className="workflow-context-path">
      <code aria-label={path} title={path}>{path}</code>
      <button
        aria-label={copied ? t("workflows.context.pathCopied") : t("workflows.context.copyPath")}
        className="icon-button"
        onClick={copy}
        title={copied ? t("workflows.context.pathCopied") : t("workflows.context.copyPath")}
        type="button"
      >
        <Clipboard aria-hidden="true" size={13} />
      </button>
    </div>
  );
}

function StatusLabel({ status, live = false }: { status: WorkflowDisplayStatus; live?: boolean }) {
  const { t } = useTranslation();
  const presentation = statusPresentation(status);
  return (
    <span
      aria-live={live ? "polite" : undefined}
      className={`workflow-context-status ${presentation.tone}`}
      role={live ? "status" : undefined}
    >
      {presentation.icon}
      {t(workflowStatusKey(status))}
    </span>
  );
}

function StageStatusLabel({ status }: { status: WorkflowStageStatus }) {
  const { t } = useTranslation();
  const iconProps = { "aria-hidden": true, size: 13 } as const;
  const presentation = status === "completed"
    ? { icon: <CheckCircle2 {...iconProps} />, tone: "is-success" }
    : status === "running"
      ? { icon: <LoaderCircle {...iconProps} />, tone: "is-running" }
      : status === "waiting"
        ? { icon: <AlertTriangle {...iconProps} />, tone: "is-warning" }
        : status === "failed"
          ? { icon: <XCircle {...iconProps} />, tone: "is-danger" }
          : { icon: <CircleDashed {...iconProps} />, tone: "is-neutral" };
  return <span className={`workflow-context-status ${presentation.tone}`}>{presentation.icon}{t(`workflows.stageStatus.${status}`)}</span>;
}

export function WorkflowsRightPanel() {
  const { t, i18n } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const overview = useWorkflowStore((state) => state.overview);
  const runs = useWorkflowStore((state) => state.runs);
  const historyRuns = useWorkflowStore((state) => state.historyRuns);
  const historyKind = useWorkflowStore((state) => state.historyKind);
  const historyStatus = useWorkflowStore((state) => state.historyStatus);
  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const preparation = useWorkflowStore((state) => state.preparation);
  const surface = useWorkflowStore((state) => state.surface);
  const operations = useWorkflowStore((state) => state.operations);
  const setSurface = useWorkflowStore((state) => state.setSurface);
  const contextSummary = overview?.contextSummary ?? null;
  const selectedRun = selectedTaskId
    ? runs.find((candidate) => candidate.taskId === selectedTaskId) ?? null
    : null;
  const selectedOutputPaths = selectedRun ? runOutputPaths(selectedRun) : [];
  const selectedAffectedPaths = selectedRun ? runAffectedPaths(selectedRun) : [];
  const additionalAffectedPaths = selectedAffectedPaths.filter(
    (path) => !selectedOutputPaths.includes(path),
  );
  const queued = contextSummary?.queuedRuns ?? [];
  const currentStage = selectedRun?.stages.find((stage) => stage.id === selectedRun.currentStageId) ?? null;
  const number = new Intl.NumberFormat(i18n.language);

  const openRun = async (taskId: string) => {
    const state = useWorkflowStore.getState();
    const requestAuthority = useProjectStore.getState().authority;
    const requestAuthorityIdentity = requestAuthority?.projectId === project.projectId
      ? `${requestAuthority.canonicalIdentityKey}\0${requestAuthority.identityRevision}`
      : null;
    const operationKey = `task:${taskId}:open`;
    const operationRequest = state.beginOperation(operationKey);
    const guard = captureWorkflowRequestGuard(state);
    try {
      await hydrateAndSelectWorkflowRun(
        { projectId: project.projectId, rootPath: project.rootPath },
        taskId,
      );
    } catch (error) {
      const currentProject = useProjectStore.getState().currentProject;
      const currentAuthority = useProjectStore.getState().authority;
      const currentAuthorityIdentity = currentAuthority?.projectId === project.projectId
        ? `${currentAuthority.canonicalIdentityKey}\0${currentAuthority.identityRevision}`
        : null;
      if (
        workflowRequestGuardMatches(guard)
        && currentProject.projectId === project.projectId
        && currentProject.rootPath === project.rootPath
        && currentAuthorityIdentity === requestAuthorityIdentity
      ) {
        useWorkflowStore.getState().failOperation(operationKey, operationRequest, {
          summary: t("workflows.operationError.detail"),
          technicalDetails: String(error),
        });
      }
    } finally {
      useWorkflowStore.getState().finishOperation(operationKey, operationRequest);
    }
  };

  return (
    <aside id="right-context-panel" aria-label={t("workflows.context.title")} className="right-panel">
      <RightPanelHeader title={t("workflows.context.title")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {surface === "preparation" && preparation ? (
          <>
            <section className="workflow-context-section">
              <div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.preparation")}</div>
              <h3>{t(workflowKindKey(preparation.kind))}</h3>
              <dl className="workflow-context-facts">
                <div><dt>{t("workflows.context.scope")}</dt><dd>{t(`workflows.scope.${preparation.scope.kind}`)}</dd></div>
                <div><dt>{t("workflows.preparation.structuredOptions")}</dt><dd>{t(scopeDetailKey(preparation.scope))}</dd></div>
                <div><dt>{t("workflows.preparation.count")}</dt><dd>{number.format(preparation.baseline.itemCount)}</dd></div>
                <div><dt>{t("workflows.context.route")}</dt><dd>{t(workflowRouteKey(preparation.route))}</dd></div>
                <div><dt>{t("workflows.context.git")}</dt><dd>{t(`workflows.git.${preparation.gitPolicy}`)}</dd></div>
                <div><dt>{t("workflows.context.output")}</dt><dd>{t(preparation.output.labelKey)}</dd></div>
              </dl>
              {preparation.output.location ? <CopyablePath path={preparation.output.location} /> : null}
            </section>
            <section className="workflow-context-section">
              <h3><AlertTriangle aria-hidden="true" size={13} />{t("workflows.context.prerequisites")}</h3>
              {preparation.prerequisites.length === 0 ? (
                <p>{t("workflows.context.prerequisitesReady")}</p>
              ) : (
                <ul className="workflow-context-list">
                  {preparation.prerequisites.map((item) => (
                    <li className={item.blocking ? "is-blocking" : ""} key={item.code}>{t(item.messageKey)}</li>
                  ))}
                </ul>
              )}
            </section>
          </>
        ) : surface === "detail" && selectedRun ? (
          <>
            <section className="workflow-context-section">
              <div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.selection")}</div>
              <h3>{t(workflowKindKey(selectedRun.kind))}</h3>
              <code className="workflow-context-task-id">{selectedRun.taskId.slice(0, 8)}</code>
              <dl className="workflow-context-facts">
                <div>
                  <dt>{t("workflows.context.currentStage")}</dt>
                  <dd aria-live="polite">{currentStage ? t(currentStage.labelKey) : t("workflows.recovery.noStage")}</dd>
                </div>
                <div><dt>{t("workflows.context.stageState")}</dt><dd>{currentStage ? <StageStatusLabel status={currentStage.status} /> : EMPTY_VALUE}</dd></div>
                <div><dt>{t("workflows.context.taskState")}</dt><dd><StatusLabel status={selectedRun.displayStatus} /></dd></div>
                <div><dt>{t("workflows.context.scope")}</dt><dd>{t(`workflows.scope.${selectedRun.scope.kind}`)} · {t(scopeDetailKey(selectedRun.scope))}</dd></div>
                <div><dt>{t("workflows.context.route")}</dt><dd>{t(workflowRouteKey(selectedRun.route))}</dd></div>
                <div><dt>{t("workflows.context.git")}</dt><dd>{t(`workflows.gitState.${overview?.projectAccess?.gitState ?? "unknown"}`)}</dd></div>
                <div><dt>{t("workflows.context.checkpoint")}</dt><dd className="font-mono">{selectedRun.pendingAction?.checkpointHash ?? (selectedRun.result?.kind === "update_wiki" ? selectedRun.result.checkpointHash : null) ?? t("workflows.attention.noCheckpoint")}</dd></div>
                {selectedRun.result?.kind === "update_wiki" && selectedRun.result.finalCommit ? <div><dt>{t("workflows.result.finalCommit")}</dt><dd className="font-mono">{selectedRun.result.finalCommit}</dd></div> : null}
              </dl>
            </section>
            <section className="workflow-context-section">
              <h3>{t("workflows.context.outputLocation")}</h3>
              {selectedOutputPaths.length === 0 ? <p>{EMPTY_VALUE}</p> : selectedOutputPaths.map((path) => <CopyablePath key={path} path={path} />)}
              {selectedAffectedPaths.length ? (
                <>
                  <h3>{t("workflows.context.paths")}</h3>
                  {additionalAffectedPaths.length
                    ? additionalAffectedPaths.map((path) => <CopyablePath key={`affected:${path}`} path={path} />)
                    : <p>{t("workflows.context.sameAsOutput")}</p>}
                </>
              ) : null}
            </section>
            <section className="workflow-context-section">
              <h3>{t("workflows.context.actions")}</h3>
              <div className="workflow-actions">
                <button className="btn btn--secondary btn--sm" disabled={workflowOperationPending(operations, `task:${selectedRun.taskId}:open`)} onClick={() => void openRun(selectedRun.taskId)} type="button">
                  <RefreshCw aria-hidden="true" size={13} />{t("workflows.context.refreshDetails")}
                </button>
                <button className="btn btn--secondary btn--sm" onClick={() => setSurface("overview")} type="button">{t("workflows.action.back")}</button>
              </div>
            </section>
          </>
        ) : surface === "history" ? (
          <section className="workflow-context-section">
            <div className="workflow-context-kicker"><History aria-hidden="true" size={13} />{t("workflows.context.history")}</div>
            <dl className="workflow-context-facts">
              <div><dt>{t("workflows.context.historyWorkflow")}</dt><dd>{t("workflows.context.historyWorkflowValue", { workflow: historyKind ? t(workflowKindKey(historyKind)) : t("workflows.filter.all") })}</dd></div>
              <div><dt>{t("workflows.context.historyStatus")}</dt><dd>{t("workflows.context.historyStatusValue", { status: historyStatus ? t(workflowStatusKey(historyStatus)) : t("workflows.filter.all") })}</dd></div>
              <div><dt>{t("workflows.context.historyCount")}</dt><dd>{number.format(historyRuns.length)}</dd></div>
            </dl>
            <ul className="workflow-context-list">
              {historyRuns.slice(0, 5).map((item) => (
                <li key={item.taskId}>
                  <span>{t(workflowKindKey(item.kind))}</span>
                  <StatusLabel status={item.displayStatus} />
                  <time dateTime={item.updatedAt}>{workflowDateTimeLabel(item.updatedAt, i18n.language)}</time>
                </li>
              ))}
            </ul>
          </section>
        ) : (
          <>
            <section className="workflow-context-section">
              <div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.project")}</div>
              <h3>{project.name}</h3>
              <CopyablePath path={project.rootPath} />
              <dl className="workflow-context-facts">
                <div><dt>{t("workflows.context.pendingSources")}</dt><dd>{contextSummary ? number.format(contextSummary.pendingSourceCount) : t("workflows.context.summaryUnavailable")}</dd></div>
                <div><dt>{t("workflows.context.lastHealth")}</dt><dd>{contextSummary?.lastHealth ? t("workflows.context.healthSummary", { errors: number.format(contextSummary.lastHealth.errorCount), warnings: number.format(contextSummary.lastHealth.warningCount), info: number.format(contextSummary.lastHealth.infoCount) }) : contextSummary ? EMPTY_VALUE : t("workflows.context.summaryUnavailable")}</dd></div>
                <div><dt>{t("workflows.context.recentArtifact")}</dt><dd>{contextSummary?.recentArtifact ? t(workflowArtifactTypeKey(contextSummary.recentArtifact.artifactType)) : contextSummary ? EMPTY_VALUE : t("workflows.context.summaryUnavailable")}</dd></div>
                <div><dt>{t("workflows.context.queued")}</dt><dd>{contextSummary ? number.format(contextSummary.queueCount) : t("workflows.context.summaryUnavailable")}</dd></div>
              </dl>
            </section>
            <section className="workflow-context-section">
              <h3><GitBranch aria-hidden="true" size={13} />{t("workflows.context.queue")}</h3>
              {!contextSummary ? <p>{t("workflows.context.summaryUnavailable")}</p> : queued.length === 0 ? <p>{t("workflows.context.queueEmpty")}</p> : queued.map((item) => (
                <button aria-label={t("workflows.context.openQueuedRun", { workflow: t(workflowKindKey(item.kind)), taskId: item.taskId })} className="workflow-context-queue" data-workflow-return-key={`context-queue:${item.taskId}`} disabled={workflowOperationPending(operations, `task:${item.taskId}:open`)} key={item.taskId} onClick={() => void openRun(item.taskId)} type="button">
                  <span>{item.queuePosition === null ? EMPTY_VALUE : number.format(item.queuePosition)}</span>
                  <span>{t(workflowKindKey(item.kind))}</span>
                </button>
              ))}
            </section>
          </>
        )}
      </div>
    </aside>
  );
}

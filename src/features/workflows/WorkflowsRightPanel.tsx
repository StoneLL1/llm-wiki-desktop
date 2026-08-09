import { AlertTriangle, GitBranch, Layers3 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useProjectStore } from "../../stores/projectStore";
import {
  captureWorkflowRequestGuard,
  useWorkflowStore,
  workflowOperationPending,
  workflowRequestGuardMatches,
} from "../../stores/workflowStore";
import { hydrateAndSelectWorkflowRun } from "../../services/workflowNavigation";
import { RightPanelHeader } from "../../components/app/RightPanelHeader";
import { attentionRun, workflowKindKey, workflowStatusKey } from "./workflowPresentation";

export function WorkflowsRightPanel() {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const runs = useWorkflowStore((state) => state.runs);
  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const preparation = useWorkflowStore((state) => state.preparation);
  const surface = useWorkflowStore((state) => state.surface);
  const operations = useWorkflowStore((state) => state.operations);
  const attention = attentionRun(runs);
  const explicit = selectedTaskId ? runs.find((run) => run.taskId === selectedTaskId) ?? null : null;
  const run = surface === "detail" ? explicit : surface === "overview" ? attention : null;
  const visiblePreparation = surface === "preparation" ? preparation : null;
  const queued = runs.filter((candidate) => candidate.displayStatus === "queued");
  const lastHealth = runs.find((candidate) => candidate.kind === "health_check" && candidate.displayStatus === "completed");
  const recentArtifact = runs.find((candidate) => candidate.kind === "generate_content" && candidate.displayStatus === "completed");
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
  return <aside id="right-context-panel" aria-label={t("workflows.context.title")} className="right-panel">
    <RightPanelHeader title={t("workflows.context.title")} />
    <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-3">
      {run ? <>
        <section className="workflow-context-section"><div className="workflow-context-kicker"><AlertTriangle aria-hidden="true" size={13} />{explicit ? t("workflows.context.selection") : t("workflows.context.attention")}</div><button className="workflow-context-run" disabled={workflowOperationPending(operations, `task:${run.taskId}:open`)} onClick={() => void openRun(run.taskId)} type="button"><strong>{t(workflowKindKey(run.kind))}</strong><span>{t(workflowStatusKey(run.displayStatus))}</span><code>{run.taskId.slice(0, 8)}</code></button></section>
        <section className="workflow-context-section"><h3>{t("workflows.context.scope")}</h3><p>{t(`workflows.scope.${run.scope.kind}`)}</p><h3>{t("workflows.context.route")}</h3><p>{run.route ? t(`workflows.route.${run.route.kind}`) : t("workflows.route.none")}</p>{run.pendingAction ? <><h3>{t("workflows.attention.checkpoint")}</h3><p className="font-mono">{run.pendingAction.checkpointHash ?? t("workflows.attention.noCheckpoint")}</p><h3>{t("workflows.context.paths")}</h3><ul>{run.pendingAction.affectedPaths.map((path) => <li className="font-mono" key={path}>{path}</li>)}</ul></> : null}</section>
      </> : visiblePreparation ? <section className="workflow-context-section"><div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.preparation")}</div><h3>{t(workflowKindKey(visiblePreparation.kind))}</h3><p>{t(`workflows.scope.${visiblePreparation.scope.kind}`)}</p><dl className="workflow-context-facts"><div><dt>{t("workflows.preparation.count")}</dt><dd>{visiblePreparation.baseline.itemCount}</dd></div><div><dt>{t("workflows.context.route")}</dt><dd>{visiblePreparation.route ? t(`workflows.route.${visiblePreparation.route.kind}`) : t("workflows.route.none")}</dd></div></dl></section> : <section className="workflow-context-section"><div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.project")}</div><h3>{project.name}</h3><p className="font-mono">{project.rootPath}</p><dl className="workflow-context-facts"><div><dt>{t("workflows.context.sources")}</dt><dd>{project.sourceCount}</dd></div><div><dt>{t("workflows.context.lastHealth")}</dt><dd>{lastHealth ? t(workflowStatusKey(lastHealth.displayStatus)) : "–"}</dd></div><div><dt>{t("workflows.context.recentArtifact")}</dt><dd>{recentArtifact?.result?.kind === "generate_content" ? t(`workflows.artifact.${recentArtifact.result.artifactType === "beautiful_read" ? "beautifulRead" : recentArtifact.result.artifactType === "knowledge_card" ? "knowledgeCard" : recentArtifact.result.artifactType === "concept_map" ? "conceptMap" : "projectReport"}`) : "–"}</dd></div><div><dt>{t("workflows.context.queued")}</dt><dd>{queued.length}</dd></div></dl></section>}
      <section className="workflow-context-section"><h3><GitBranch aria-hidden="true" size={13} />{t("workflows.context.queue")}</h3>{queued.length === 0 ? <p>{t("workflows.context.queueEmpty")}</p> : queued.map((item) => <button className="workflow-context-queue" disabled={workflowOperationPending(operations, `task:${item.taskId}:open`)} key={item.taskId} onClick={() => void openRun(item.taskId)} type="button"><span>{item.queuePosition ?? "—"}</span><span>{t(workflowKindKey(item.kind))}</span></button>)}</section>
    </div>
  </aside>;
}

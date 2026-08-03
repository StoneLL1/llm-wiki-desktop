import { AlertTriangle, GitBranch, Layers3 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { hydrateAndSelectWorkflowRun } from "../../services/workflowNavigation";
import { RightPanelHeader } from "../../components/app/RightPanelHeader";
import { attentionRun, workflowKindKey, workflowStatusKey } from "./workflowPresentation";

export function WorkflowsRightPanel() {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const runs = useWorkflowStore((state) => state.runs);
  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const preparation = useWorkflowStore((state) => state.preparation);
  const attention = attentionRun(runs);
  const explicit = selectedTaskId ? runs.find((run) => run.taskId === selectedTaskId) ?? null : null;
  const run = explicit ?? attention;
  const queued = runs.filter((candidate) => candidate.displayStatus === "queued");
  const lastHealth = runs.find((candidate) => candidate.kind === "health_check" && candidate.displayStatus === "completed");
  const recentArtifact = runs.find((candidate) => candidate.kind === "generate_content" && candidate.displayStatus === "completed");
  const openRun = async (taskId: string) => {
    try {
      await hydrateAndSelectWorkflowRun(
        { projectId: project.projectId, rootPath: project.rootPath },
        taskId,
      );
    } catch (error) {
      useWorkflowStore.getState().setError(String(error));
    }
  };
  return <aside id="right-context-panel" aria-label={t("workflows.context.title")} className="right-panel">
    <RightPanelHeader title={t("workflows.context.title")} />
    <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-3">
      {run ? <>
        <section className="workflow-context-section"><div className="workflow-context-kicker"><AlertTriangle aria-hidden="true" size={13} />{explicit ? t("workflows.context.selection") : t("workflows.context.attention")}</div><button className="workflow-context-run" onClick={() => void openRun(run.taskId)} type="button"><strong>{t(workflowKindKey(run.kind))}</strong><span>{t(workflowStatusKey(run.displayStatus))}</span><code>{run.taskId.slice(0, 8)}</code></button></section>
        <section className="workflow-context-section"><h3>{t("workflows.context.scope")}</h3><p>{t(`workflows.scope.${run.scope.kind}`)}</p><h3>{t("workflows.context.route")}</h3><p>{run.route ? t(`workflows.route.${run.route.kind}`) : t("workflows.route.none")}</p>{run.pendingAction ? <><h3>{t("workflows.attention.checkpoint")}</h3><p className="font-mono">{run.pendingAction.checkpointHash ?? t("workflows.attention.noCheckpoint")}</p><h3>{t("workflows.context.paths")}</h3><ul>{run.pendingAction.affectedPaths.map((path) => <li className="font-mono" key={path}>{path}</li>)}</ul></> : null}</section>
      </> : preparation ? <section className="workflow-context-section"><div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.preparation")}</div><h3>{t(workflowKindKey(preparation.kind))}</h3><p>{t(`workflows.scope.${preparation.scope.kind}`)}</p><dl className="workflow-context-facts"><div><dt>{t("workflows.preparation.count")}</dt><dd>{preparation.baseline.itemCount}</dd></div><div><dt>{t("workflows.context.route")}</dt><dd>{preparation.route ? t(`workflows.route.${preparation.route.kind}`) : t("workflows.route.none")}</dd></div></dl></section> : <section className="workflow-context-section"><div className="workflow-context-kicker"><Layers3 aria-hidden="true" size={13} />{t("workflows.context.project")}</div><h3>{project.name}</h3><p className="font-mono">{project.rootPath}</p><dl className="workflow-context-facts"><div><dt>{t("workflows.context.sources")}</dt><dd>{project.sourceCount}</dd></div><div><dt>{t("workflows.context.lastHealth")}</dt><dd>{lastHealth ? t(workflowStatusKey(lastHealth.displayStatus)) : "–"}</dd></div><div><dt>{t("workflows.context.recentArtifact")}</dt><dd>{recentArtifact?.result?.kind === "generate_content" ? t(`workflows.artifact.${recentArtifact.result.artifactType === "beautiful_read" ? "beautifulRead" : recentArtifact.result.artifactType === "knowledge_card" ? "knowledgeCard" : recentArtifact.result.artifactType === "concept_map" ? "conceptMap" : "projectReport"}`) : "–"}</dd></div><div><dt>{t("workflows.context.queued")}</dt><dd>{queued.length}</dd></div></dl></section>}
      <section className="workflow-context-section"><h3><GitBranch aria-hidden="true" size={13} />{t("workflows.context.queue")}</h3>{queued.length === 0 ? <p>{t("workflows.context.queueEmpty")}</p> : queued.map((item) => <button className="workflow-context-queue" key={item.taskId} onClick={() => void openRun(item.taskId)} type="button"><span>{item.queuePosition ?? "—"}</span><span>{t(workflowKindKey(item.kind))}</span></button>)}</section>
    </div>
  </aside>;
}

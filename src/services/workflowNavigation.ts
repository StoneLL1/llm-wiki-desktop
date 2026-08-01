import { useWikiStore } from "../features/wiki/wikiStore";
import { useExportStore } from "../stores/exportStore";
import { useLintStore } from "../stores/lintStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkflowStore } from "../stores/workflowStore";
import type { WorkflowRun } from "../types/workflow";
import { getWorkflowRun } from "./workflowApi";

interface WorkflowProjectRef {
  projectId: string;
  rootPath: string;
}

function assertActiveProject(project: WorkflowProjectRef): void {
  const current = useProjectStore.getState().currentProject;
  if (current.projectId !== project.projectId || current.rootPath !== project.rootPath) {
    throw new Error("WORKFLOW_PROJECT_CHANGED");
  }
}

export async function hydrateAndSelectWorkflowRun(
  project: WorkflowProjectRef,
  taskId: string,
): Promise<WorkflowRun> {
  assertActiveProject(project);
  const run = await getWorkflowRun({
    projectId: project.projectId,
    projectRootPath: project.rootPath,
    taskId,
  });
  assertActiveProject(project);
  if (run.projectId !== project.projectId) throw new Error("WORKFLOW_PROJECT_MISMATCH");
  const expectedKey = `${project.projectId}\0${project.rootPath}`;
  const state = useWorkflowStore.getState();
  if (state.projectKey !== expectedKey) state.activateProject(expectedKey);
  const latest = useWorkflowStore.getState();
  latest.upsertRun(run);
  latest.selectRun(run.taskId);
  return run;
}

export async function openWorkflowResult(
  project: WorkflowProjectRef,
  run: WorkflowRun,
): Promise<void> {
  assertActiveProject(project);
  const result = run.result;
  if (!result) return;

  if (result.kind === "update_wiki") {
    await useWikiStore.getState().scan(project.projectId, project.rootPath);
    assertActiveProject(project);
    const existingPaths = new Set(
      useWikiStore.getState().tree?.pages.map((page) => page.path) ?? [],
    );
    const existingAffectedPath = result.affectedPaths.find((path) => existingPaths.has(path));
    if (existingAffectedPath) {
      await useWikiStore
        .getState()
        .openPage(project.projectId, project.rootPath, existingAffectedPath);
      assertActiveProject(project);
    }
    useNavigationStore.getState().setActiveView("wiki");
    return;
  }

  if (result.kind === "health_check") {
    if (result.reportId) {
      await useLintStore.getState().openHistoryReport({
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        id: result.reportId,
      });
      assertActiveProject(project);
    }
    useNavigationStore.getState().setActiveView("lint");
    return;
  }

  await useExportStore.getState().loadExports(project.projectId, project.rootPath);
  assertActiveProject(project);
  const record = useExportStore
    .getState()
    .records.find((candidate) => candidate.id === result.recordId);
  if (record) {
    await useExportStore.getState().loadPreview(
      {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        outputPath: record.outputPath,
      },
      record.id,
    );
    assertActiveProject(project);
  }
  useNavigationStore.getState().setActiveView("exports");
}

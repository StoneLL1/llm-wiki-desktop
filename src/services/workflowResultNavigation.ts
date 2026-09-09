import { useWikiStore } from "../features/wiki/wikiStore";
import { useExportStore } from "../stores/exportStore";
import { useLintStore } from "../stores/lintStore";
import { useNavigationStore } from "../stores/navigationStore";
import type { WorkflowRun } from "../types/workflow";
import type { WorkflowNavigation, WorkflowProjectRef } from "./workflowNavigation";

/** Result stores are needed only after the user chooses to open a completed result. */
export async function openWorkflowResultDetails(
  project: WorkflowProjectRef,
  result: NonNullable<WorkflowRun["result"]>,
  navigation: WorkflowNavigation,
  taskId: string,
): Promise<void> {
  if (result.kind === "update_wiki") {
    const commitGuard = navigation.matches;
    await useWikiStore.getState().scan(project.projectId, project.rootPath, commitGuard);
    navigation.assertCurrent();
    const existingPaths = new Set(
      useWikiStore.getState().tree?.pages.map((page) => page.path) ?? [],
    );
    const existingAffectedPath = result.affectedPaths.find((path) => existingPaths.has(path));
    if (existingAffectedPath) {
      await useWikiStore
        .getState()
        .openPage(project.projectId, project.rootPath, existingAffectedPath, commitGuard);
      navigation.assertCurrent();
    }
    useNavigationStore.getState().setActiveView("wiki");
    return;
  }

  if (result.kind === "health_check") {
    if (result.reportId) {
      const opened = await useLintStore.getState().openHistoryReport({
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        id: result.reportId,
      }, navigation.matches, true);
      navigation.assertCurrent();
      if (!opened) throw new Error("WORKFLOW_LINT_CONFIRMATION_ACTIVE");
    }
    useNavigationStore.getState().setActiveView("lint");
    return;
  }

  if (result.kind === "agent_lint_repair") {
    useNavigationStore.getState().setActiveView("lint");
    return;
  }

  const commitGuard = navigation.matches;
  await useExportStore.getState().loadExports(project.projectId, project.rootPath, commitGuard);
  navigation.assertCurrent();
  const exports = useExportStore.getState();
  if (exports.error) throw new Error(exports.error);
  const record = exports.records.find((candidate) => candidate.id === result.recordId);
  if (!record || (record.taskId && record.taskId !== taskId)) {
    throw new Error("WORKFLOW_EXPORT_RESULT_UNAVAILABLE");
  }
  await exports.loadPreview(
    {
      projectId: project.projectId,
      projectRootPath: project.rootPath,
      outputPath: record.outputPath,
    },
    record.id,
    commitGuard,
  );
  navigation.assertCurrent();
  const preview = useExportStore.getState();
  if (preview.error) throw new Error(preview.error);
  if (preview.previewId !== record.id) {
    throw new Error("WORKFLOW_EXPORT_PREVIEW_SUPERSEDED");
  }
  useNavigationStore.getState().setActiveView("exports");
}

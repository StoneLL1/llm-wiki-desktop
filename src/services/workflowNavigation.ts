import { useWikiStore } from "../features/wiki/wikiStore";
import { useExportStore } from "../stores/exportStore";
import { useLintStore } from "../stores/lintStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import {
  captureWorkflowRequestGuard,
  useWorkflowStore,
  workflowRunMatchesGuard,
  type WorkflowRequestGuard,
} from "../stores/workflowStore";
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

function captureNavigationGuard(project: WorkflowProjectRef): WorkflowRequestGuard {
  assertActiveProject(project);
  const expectedKey = `${project.projectId}\0${project.rootPath}`;
  if (useWorkflowStore.getState().projectKey !== expectedKey) {
    useWorkflowStore.getState().activateProject(expectedKey);
  }
  const state = useWorkflowStore.getState();
  const authority = useProjectStore.getState().authority;
  const storeIdentity = state.identityGuard;
  const authorityMatchesProject = authority?.projectId === project.projectId;
  if (
    authorityMatchesProject
    && storeIdentity.canonicalIdentityKey
    && (storeIdentity.canonicalIdentityKey !== authority.canonicalIdentityKey
      || storeIdentity.identityRevision !== authority.identityRevision)
  ) {
    throw new Error("WORKFLOW_PROJECT_CHANGED");
  }
  const canonicalIdentityKey = storeIdentity.canonicalIdentityKey
    ?? (authorityMatchesProject ? authority.canonicalIdentityKey : null);
  const identityRevision = storeIdentity.identityRevision
    ?? (authorityMatchesProject ? authority.identityRevision : null);
  if (!canonicalIdentityKey || !identityRevision) {
    throw new Error("WORKFLOW_IDENTITY_UNAVAILABLE");
  }
  return {
    ...captureWorkflowRequestGuard(state),
    canonicalIdentityKey,
    identityRevision,
  };
}

function assertNavigationGuard(
  project: WorkflowProjectRef,
  guard: WorkflowRequestGuard,
): void {
  assertActiveProject(project);
  const state = useWorkflowStore.getState();
  if (state.projectKey !== guard.projectKey || state.requestEpoch !== guard.requestEpoch) {
    throw new Error("WORKFLOW_PROJECT_CHANGED");
  }
  if (
    state.identityGuard.canonicalIdentityKey
    && (state.identityGuard.canonicalIdentityKey !== guard.canonicalIdentityKey
      || state.identityGuard.identityRevision !== guard.identityRevision)
  ) {
    throw new Error("WORKFLOW_PROJECT_CHANGED");
  }
  const authority = useProjectStore.getState().authority;
  if (
    authority?.projectId === project.projectId
    && (authority.canonicalIdentityKey !== guard.canonicalIdentityKey
      || authority.identityRevision !== guard.identityRevision)
  ) {
    throw new Error("WORKFLOW_PROJECT_CHANGED");
  }
}

function navigationGuardMatches(
  project: WorkflowProjectRef,
  guard: WorkflowRequestGuard,
): boolean {
  try {
    assertNavigationGuard(project, guard);
    return true;
  } catch {
    return false;
  }
}

export async function hydrateAndSelectWorkflowRun(
  project: WorkflowProjectRef,
  taskId: string,
): Promise<WorkflowRun> {
  const guard = captureNavigationGuard(project);
  const run = await getWorkflowRun({
    projectId: project.projectId,
    projectRootPath: project.rootPath,
    taskId,
  });
  assertNavigationGuard(project, guard);
  if (!workflowRunMatchesGuard(run, project.projectId, guard)) {
    throw new Error("WORKFLOW_PROJECT_MISMATCH");
  }
  const latest = useWorkflowStore.getState();
  latest.upsertRun(run);
  latest.selectRun(run.taskId);
  return run;
}

export async function openWorkflowResult(
  project: WorkflowProjectRef,
  run: WorkflowRun,
): Promise<void> {
  const guard = captureNavigationGuard(project);
  if (!workflowRunMatchesGuard(run, project.projectId, guard)) {
    throw new Error("WORKFLOW_PROJECT_MISMATCH");
  }
  const result = run.result;
  if (!result) return;

  if (result.kind === "update_wiki") {
    const commitGuard = () => navigationGuardMatches(project, guard);
    await useWikiStore.getState().scan(project.projectId, project.rootPath, commitGuard);
    assertNavigationGuard(project, guard);
    const existingPaths = new Set(
      useWikiStore.getState().tree?.pages.map((page) => page.path) ?? [],
    );
    const existingAffectedPath = result.affectedPaths.find((path) => existingPaths.has(path));
    if (existingAffectedPath) {
      await useWikiStore
        .getState()
        .openPage(project.projectId, project.rootPath, existingAffectedPath, commitGuard);
      assertNavigationGuard(project, guard);
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
      }, () => navigationGuardMatches(project, guard), true);
      assertNavigationGuard(project, guard);
      if (!opened) throw new Error("WORKFLOW_LINT_CONFIRMATION_ACTIVE");
    }
    useNavigationStore.getState().setActiveView("lint");
    return;
  }

  const commitGuard = () => navigationGuardMatches(project, guard);
  await useExportStore.getState().loadExports(project.projectId, project.rootPath, commitGuard);
  assertNavigationGuard(project, guard);
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
      commitGuard,
    );
    assertNavigationGuard(project, guard);
  }
  useNavigationStore.getState().setActiveView("exports");
}

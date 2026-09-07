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

export interface WorkflowProjectRef {
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

let navigationSequence = 0;

export function cancelWorkflowNavigation(): void {
  navigationSequence += 1;
}

/** Navigation intent is separate from task facts, which may keep arriving in the background. */
function beginNavigation(project: WorkflowProjectRef) {
  const guard = captureNavigationGuard(project);
  const sequence = ++navigationSequence;
  let superseded = false;
  const stopWorkflow = useWorkflowStore.subscribe((state, previous) => {
    if (state.surface !== previous.surface || state.preparation !== previous.preparation
      || state.selectedTaskId !== previous.selectedTaskId) superseded = true;
  });
  const stopView = useNavigationStore.subscribe((state, previous) => {
    if (state.activeView !== previous.activeView) superseded = true;
  });
  const assertCurrent = () => {
    assertNavigationGuard(project, guard);
    if (superseded || sequence !== navigationSequence) {
      throw new Error("WORKFLOW_NAVIGATION_SUPERSEDED");
    }
  };
  return {
    guard,
    assertCurrent,
    matches: () => {
      try { assertCurrent(); return true; } catch { return false; }
    },
    dispose: () => { stopWorkflow(); stopView(); },
  };
}

export async function hydrateAndSelectWorkflowRun(
  project: WorkflowProjectRef,
  taskId: string,
): Promise<WorkflowRun> {
  const navigation = beginNavigation(project);
  try {
    const run = await getWorkflowRun({
      projectId: project.projectId,
      projectRootPath: project.rootPath,
      taskId,
    });
    navigation.assertCurrent();
    if (!workflowRunMatchesGuard(run, project.projectId, navigation.guard)) {
      throw new Error("WORKFLOW_PROJECT_MISMATCH");
    }
    navigation.dispose();
    const latest = useWorkflowStore.getState();
    latest.upsertRun(run);
    latest.selectRun(run.taskId);
    return run;
  } finally {
    navigation.dispose();
  }
}

export async function openWorkflowResult(
  project: WorkflowProjectRef,
  run: WorkflowRun,
): Promise<void> {
  const navigation = beginNavigation(project);
  try {
    if (!workflowRunMatchesGuard(run, project.projectId, navigation.guard)) {
      throw new Error("WORKFLOW_PROJECT_MISMATCH");
    }
    const result = run.result;
    if (!result) return;

    const { openWorkflowResultDetails } = await import("./workflowResultNavigation");
    navigation.assertCurrent();
    await openWorkflowResultDetails(project, result, navigation);
  } finally {
    navigation.dispose();
  }
}

export type WorkflowNavigation = ReturnType<typeof beginNavigation>;

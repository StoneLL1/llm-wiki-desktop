import { prepareWorkflow, startWorkflow, cancelWorkflowRun } from "../../services/workflowApi";
import { workflowScopeEqual } from "../../services/workflowDraft";
import { recordWorkflowFacts } from "../../stores/taskStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { ProjectSummary } from "../../types/project";
import type { WorkflowsController, WorkflowsControllerOptions, WorkflowProjectPrerequisiteAction } from "./useWorkflowsController";
import { useWorkflowStore } from "../../stores/workflowStore";
import type {
  WorkflowPreparation, WorkflowPreparationDraft, WorkflowProjectRequest,
  WorkflowRouteSelection, WorkflowRun, WorkflowStartOutcome, WorkflowPrerequisiteAction,
} from "../../types/workflow";

interface StartPreparedOptions {
  acknowledgeRestrictedContent: boolean;
  acknowledgeRemoteProvider: boolean;
  draft?: WorkflowPreparationDraft;
  retryOfTaskId: string | null;
}

/** Loaded only after the controller captures the approval and takes its operation lock. */
export async function startPreparedWorkflow(
  request: WorkflowProjectRequest,
  preparation: WorkflowPreparation,
  options: StartPreparedOptions,
  isCurrent: () => boolean,
): Promise<WorkflowStartOutcome | null> {
  if (!isCurrent()) return null;
  const { acknowledgeRestrictedContent, acknowledgeRemoteProvider, draft, retryOfTaskId } = options;
  const latestDraft = draft ?? { scope: preparation.scope, routeSelection: routeSelectionOf(preparation.route) };
  const fresh = await prepareWorkflow({ ...request, kind: preparation.kind, ...latestDraft });
  if (!isCurrent()
    || fresh.projectAccess.canonicalIdentityKey !== preparation.projectAccess.canonicalIdentityKey
    || fresh.projectAccess.identityRevision !== preparation.projectAccess.identityRevision) return null;
  const acknowledgementActions = ["acknowledge_restricted_content", "acknowledge_remote_provider"];
  const sameApproval = workflowScopeEqual(preparation.scope, fresh.scope)
    && JSON.stringify(preparation.route) === JSON.stringify(fresh.route)
    && preparation.baseline.fingerprint === fresh.baseline.fingerprint;
  const requiresReview = !workflowScopeEqual(latestDraft.scope, fresh.scope)
    || (!latestDraft.routeSelection && JSON.stringify(preparation.route) !== JSON.stringify(fresh.route))
    || fresh.prerequisites.some((item) => item.blocking && !acknowledgementActions.includes(item.action))
    || fresh.prerequisites.some((item) => item.action === "acknowledge_remote_provider" && (!acknowledgeRemoteProvider || !sameApproval))
    || fresh.prerequisites.some((item) => item.action === "acknowledge_restricted_content" && (!acknowledgeRestrictedContent || !sameApproval))
    || (fresh.kind === "update_wiki" && fresh.scope.kind === "update_wiki" && fresh.scope.sourceVersions.length === 0);
  if (requiresReview) {
    useWorkflowStore.getState().setPreparation(fresh);
    useWorkflowStore.setState({ retryOfTaskId });
    return null;
  }
  return startWorkflow({
    ...request, preparationId: fresh.preparationId,
    preparationRevision: fresh.preparationRevision,
    acknowledgeRestrictedContent, acknowledgeRemoteProvider,
    ...(retryOfTaskId ? { retryOfTaskId } : {}),
  });
}

export async function reviewWorkflowScope(
  request: WorkflowProjectRequest,
  run: WorkflowRun,
  routeSelection: WorkflowRouteSelection | null,
  isCurrent: () => boolean,
): Promise<void> {
  if (!isCurrent() || (run.scope.kind !== "update_wiki" && run.scope.kind !== "health_check")) return;
  let scope = run.scope;
  if (scope.kind === "update_wiki") {
    const available = await prepareWorkflow({ ...request, kind: run.kind, scope: null, routeSelection });
    if (!isCurrent()) return;
    const selectedIds = new Set(scope.sourceVersions.map((source) => source.sourceId));
    const versions = (available.availableSourceVersions ?? []).filter((source) => selectedIds.has(source.sourceId));
    if ([...selectedIds].some((id) => !versions.some((source) => source.sourceId === id))) {
      throw new Error("WORKFLOW_SELECTED_SOURCE_UNAVAILABLE");
    }
    scope = { ...scope, sourceVersions: versions };
  }
  const fresh = await prepareWorkflow({ ...request, kind: run.kind, scope, routeSelection });
  if (!isCurrent()) return;
  const cancelled = await cancelWorkflowRun({ ...request, taskId: run.taskId });
  recordWorkflowFacts([cancelled]);
  if (!isCurrent()) return;
  useWorkflowStore.getState().setPreparation(fresh);
  useWorkflowStore.setState({ retryOfTaskId: run.taskId });
}

function routeSelectionOf(route: WorkflowPreparation["route"]): WorkflowRouteSelection | null {
  if (route?.kind === "agent") return { kind: "agent", agent: route.agent };
  if (route?.kind === "byok") return { kind: "byok", provider: route.provider };
  return null;
}

export async function handleWorkflowPrerequisite(
  action: WorkflowPrerequisiteAction,
  draft: WorkflowPreparationDraft | undefined,
  project: ProjectSummary,
  prepareKind: WorkflowsController["prepare"],
  refresh: WorkflowsController["refresh"],
  onProjectPrerequisite: WorkflowsControllerOptions["onProjectPrerequisite"],
): Promise<void> {
  if (action === "import_sources") {
    const preparation = useWorkflowStore.getState().preparation;
    if (preparation) {
      useNavigationStore.setState({
        workflowLaunchIntent: {
          projectId: project.projectId,
          projectRootPath: project.rootPath,
          kind: preparation.kind,
          origin: "workflows",
          scopePreset: draft?.scope ?? preparation.scope,
          routeSelection: draft ? draft.routeSelection : routeSelectionOf(preparation.route),
          expectedCanonicalIdentityKey: preparation.projectAccess.canonicalIdentityKey,
          expectedIdentityRevision: preparation.projectAccess.identityRevision,
        },
      });
    }
    useNavigationStore.getState().setActiveView("import");
    return;
  }
  if (action === "update_wiki") {
    await prepareKind("update_wiki");
    return;
  }
  if (action === "choose_execution_route") return;
  if (action === "configure_execution_route") {
    const preparation = useWorkflowStore.getState().preparation;
    if (!preparation) {
      await refresh();
      return;
    }
    useNavigationStore.getState().openSettings("ai", {
      projectId: project.projectId,
      projectRootPath: project.rootPath,
      kind: preparation.kind,
      scope: draft?.scope ?? preparation.scope,
      routeSelection: draft ? draft.routeSelection : routeSelectionOf(preparation.route),
      source: "prerequisite",
      expectedSurface: "preparation",
      expectedCanonicalIdentityKey:
        preparation.projectAccess.canonicalIdentityKey,
      expectedIdentityRevision: preparation.projectAccess.identityRevision,
      expectedPreparationId: preparation.preparationId,
      expectedPreparationRevision: preparation.preparationRevision,
      expectedTaskId: null,
    });
    return;
  }
  if (action === "prepare_again") {
    const preparation = useWorkflowStore.getState().preparation;
    if (preparation) {
      await prepareKind(
        preparation.kind,
        preparation.scope,
        routeSelectionOf(preparation.route),
      );
    } else {
      await refresh();
    }
    return;
  }
  if (["open_or_create_project", "trust_project", "make_writable", "configure_git", "resolve_dirty_git"].includes(action)) {
    const preparation = useWorkflowStore.getState().preparation;
    await onProjectPrerequisite?.(action as WorkflowProjectPrerequisiteAction, {
      project,
      preparation,
      prepareAgain: async () => {
        if (preparation) await prepareKind(preparation.kind, preparation.scope, routeSelectionOf(preparation.route));
        else await refresh();
      },
    });
    return;
  }
  await refresh();
}

export function openWorkflowAdjustmentSettings(project: ProjectSummary, run: WorkflowRun): void {
  const routeSelection = routeSelectionOf(run.route);
  useNavigationStore.getState().openSettings("ai", {
    projectId: project.projectId,
    projectRootPath: project.rootPath,
    kind: run.kind,
    scope: run.scope,
    routeSelection,
    source: "adjust",
    expectedSurface: "detail",
    expectedCanonicalIdentityKey: run.canonicalIdentityKey,
    expectedIdentityRevision: run.identityRevision,
    expectedPreparationId: null,
    expectedPreparationRevision: null,
    expectedTaskId: run.taskId,
  });
}

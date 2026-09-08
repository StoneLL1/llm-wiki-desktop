import { prepareWorkflow, startWorkflow, cancelWorkflowRun, listUpdateWikiSources } from "../../services/workflowApi";
import { normalizeBackendError } from "../../lib/backendError";
import { workflowScopeEqual } from "../../services/workflowDraft";
import { recordWorkflowFacts } from "../../stores/taskStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { ProjectSummary } from "../../types/project";
import type { WorkflowsController, WorkflowsControllerOptions, WorkflowProjectPrerequisiteAction } from "./useWorkflowsController";
import { type WorkflowPendingStart, useWorkflowStore, captureWorkflowRequestGuard, workflowRequestGuardMatches } from "../../stores/workflowStore";
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

function sameDraft(a: WorkflowPreparationDraft, b: WorkflowPreparationDraft): boolean {
  return workflowScopeEqual(a.scope, b.scope) && JSON.stringify(a.routeSelection) === JSON.stringify(b.routeSelection);
}

function pendingPreparation(kind: WorkflowPreparation["kind"], options: Omit<WorkflowPendingStart, "preparation">): WorkflowPreparation | undefined {
  const pending = useWorkflowStore.getState().pendingStarts[kind];
  return pending && sameDraft(pending.draft, options.draft)
    && pending.acknowledgeRestrictedContent === options.acknowledgeRestrictedContent
    && pending.acknowledgeRemoteProvider === options.acknowledgeRemoteProvider
    && pending.retryOfTaskId === options.retryOfTaskId ? pending.preparation : undefined;
}

/** The real receipt is also the backend idempotency key. Retain it only while a
 * submitted start has an uncertain outcome, separately from editable drafts. */
async function submitWorkflow(request: WorkflowProjectRequest, pending: WorkflowPendingStart): Promise<WorkflowStartOutcome> {
  const guard = captureWorkflowRequestGuard();
  const { preparation, acknowledgeRestrictedContent, acknowledgeRemoteProvider, retryOfTaskId } = pending;
  const kind = preparation.kind;
  useWorkflowStore.setState((state) => ({ pendingStarts: { ...state.pendingStarts, [kind]: pending } }));
  const clear = () => {
    if (!workflowRequestGuardMatches(guard) || useWorkflowStore.getState().pendingStarts[kind] !== pending) return;
    useWorkflowStore.setState((state) => {
      const pendingStarts = { ...state.pendingStarts };
      delete pendingStarts[kind];
      return { pendingStarts };
    });
  };
  try {
    const outcome = await startWorkflow({ ...request, preparationId: preparation.preparationId,
      preparationRevision: preparation.preparationRevision, acknowledgeRestrictedContent,
      acknowledgeRemoteProvider, ...(retryOfTaskId ? { retryOfTaskId } : {}) });
    clear();
    return outcome;
  } catch (error) {
    if (normalizeBackendError(error).code === "WORKFLOW_PREPARATION_STALE") clear();
    throw error;
  }
}

function reviewDraft(preparation: WorkflowPreparation, draft: WorkflowPreparationDraft): WorkflowPreparationDraft {
  // A generated filename is a result, not a request to overwrite it next time.
  const scope = preparation.scope.kind === "generate_content" && draft.scope.kind === "generate_content"
    && draft.scope.outputPath === null && preparation.gitPolicy !== "required_before_overwrite"
    ? { ...preparation.scope, outputPath: null } : preparation.scope;
  return { scope, routeSelection: draft.routeSelection };
}

function presentReview(preparation: WorkflowPreparation, draft: WorkflowPreparationDraft, retryOfTaskId: string | null): void {
  useWorkflowStore.getState().setPreparation(preparation, draft.routeSelection, reviewDraft(preparation, draft).scope);
  useWorkflowStore.setState({ retryOfTaskId });
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
  const preparedDraft = useWorkflowStore.getState().preparedDrafts[preparation.kind]
    ?? { scope: preparation.scope, routeSelection: null };
  const latestDraft = draft ?? preparedDraft;
  const submission = { draft: latestDraft, acknowledgeRestrictedContent, acknowledgeRemoteProvider, retryOfTaskId };
  // Admission validates the prepared baseline and current authority in Rust.
  const fresh = pendingPreparation(preparation.kind, submission)
    ?? (sameDraft(latestDraft, preparedDraft) ? preparation : await prepareWorkflow({ ...request, kind: preparation.kind, ...latestDraft }));
  if (!isCurrent()
    || fresh.projectAccess.canonicalIdentityKey !== preparation.projectAccess.canonicalIdentityKey
    || fresh.projectAccess.identityRevision !== preparation.projectAccess.identityRevision) return null;
  const acknowledgementActions = ["acknowledge_restricted_content", "acknowledge_remote_provider"];
  const sameApproval = workflowScopeEqual(preparation.scope, fresh.scope)
    && JSON.stringify(preparation.route) === JSON.stringify(fresh.route)
    && preparation.baseline.fingerprint === fresh.baseline.fingerprint;
  const requiresReview = !sameDraft(latestDraft, reviewDraft(fresh, latestDraft))
    || (!latestDraft.routeSelection && JSON.stringify(preparation.route) !== JSON.stringify(fresh.route))
    || fresh.prerequisites.some((item) => item.blocking && !acknowledgementActions.includes(item.action))
    || fresh.prerequisites.some((item) => item.action === "acknowledge_remote_provider" && (!acknowledgeRemoteProvider || !sameApproval))
    || fresh.prerequisites.some((item) => item.action === "acknowledge_restricted_content" && (!acknowledgeRestrictedContent || !sameApproval))
    || (fresh.kind === "update_wiki" && fresh.scope.kind === "update_wiki" && fresh.scope.sourceVersions.length === 0);
  if (requiresReview) {
    presentReview(fresh, latestDraft, retryOfTaskId);
    return null;
  }
  return submitWorkflow(request, { preparation: fresh, ...submission });
}

/** Draft navigation never signs a token. Prepare only the submitted choices. */
export async function startDraftWorkflow(
  request: WorkflowProjectRequest,
  kind: "health_check" | "generate_content",
  draft: WorkflowPreparationDraft,
  retryOfTaskId: string | null,
  isCurrent: () => boolean,
): Promise<WorkflowStartOutcome | null> {
  if (!isCurrent()) return null;
  const owner = useWorkflowStore.getState().identityGuard;
  const submission = { draft, acknowledgeRestrictedContent: false, acknowledgeRemoteProvider: false, retryOfTaskId };
  const fresh = pendingPreparation(kind, submission) ?? await prepareWorkflow({ ...request, kind, ...draft });
  if (!isCurrent()) return null;
  if (fresh.projectAccess.projectId !== request.projectId
    || (owner.canonicalIdentityKey !== null && (fresh.projectAccess.canonicalIdentityKey !== owner.canonicalIdentityKey
      || fresh.projectAccess.identityRevision !== owner.identityRevision))) {
    throw new Error("WORKFLOW_IDENTITY_CHANGED");
  }
  // Changed inputs, overwrite, and sharing still need the real backend review.
  if (fresh.prerequisites.length > 0 || fresh.gitPolicy === "required_before_overwrite"
    || !sameDraft(draft, reviewDraft(fresh, draft))) {
    presentReview(fresh, draft, retryOfTaskId);
    return null;
  }
  return submitWorkflow(request, { preparation: fresh, ...submission });
}

export async function reviewWorkflowScope(
  request: WorkflowProjectRequest,
  run: WorkflowRun,
  routeSelection: WorkflowRouteSelection | null,
  isCurrent: () => boolean,
): Promise<void> {
  if (!isCurrent()) return;
  const scope = run.scope;
  if (scope.kind === "update_wiki") {
    const selectedIds = new Set(scope.sourceVersions.map((source) => source.sourceId));
    const versions: typeof scope.sourceVersions = [];
    let offset: number | null = 0;
    do {
      const page = await listUpdateWikiSources({ ...request, query: "", offset });
      if (!isCurrent()) return;
      versions.push(...page.sources.filter((source) => selectedIds.has(source.sourceId)).map(({ sourceId, versionId }) => ({ sourceId, versionId })));
      offset = page.nextOffset;
    } while (offset !== null && versions.length < selectedIds.size);
    if (versions.length !== selectedIds.size) throw new Error("WORKFLOW_SELECTED_SOURCE_UNAVAILABLE");
    const cancelled = await cancelWorkflowRun({ ...request, taskId: run.taskId });
    recordWorkflowFacts([cancelled]);
    if (!isCurrent()) return;
    const state = useWorkflowStore.getState();
    state.setUpdateDraft({ mode: scope.mode, selection: { kind: "selected", sourceVersions: versions }, routeSelection });
    state.beginPreparation("update_wiki");
    useWorkflowStore.setState({ retryOfTaskId: run.taskId });
    return;
  }
  const fresh = await prepareWorkflow({ ...request, kind: run.kind, scope, routeSelection });
  if (!isCurrent()) return;
  const cancelled = await cancelWorkflowRun({ ...request, taskId: run.taskId });
  recordWorkflowFacts([cancelled]);
  if (!isCurrent()) return;
  useWorkflowStore.getState().setPreparation(fresh, routeSelection);
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

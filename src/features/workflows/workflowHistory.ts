import { i18next } from "../../i18n";
import { backendErrorCode, normalizeBackendError } from "../../lib/backendError";
import { listWorkflowRuns } from "../../services/workflowApi";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore, workflowRequestGuardMatches, type WorkflowRequestGuard } from "../../stores/workflowStore";
import type { ListWorkflowRunsRequest, WorkflowProjectAccessSummary } from "../../types/workflow";
import { historyPageErrorRequiresRefresh } from "./workflowHistoryRecovery";

/** History details are loaded only from a history interaction, with scope captured by the caller. */
export async function loadWorkflowHistory(
  request: ListWorkflowRunsRequest,
  guard: WorkflowRequestGuard,
  expectedAccess: WorkflowProjectAccessSummary,
  operationRequest: number,
  isLatestRequest: () => boolean,
): Promise<void> {
  const append = request.cursor !== null && request.cursor !== undefined;
  const operationKey = append ? "history:page" : "history:filter";
  const isCurrent = () => {
    const state = useWorkflowStore.getState();
    const { currentProject, authority } = useProjectStore.getState();
    return isLatestRequest() && workflowRequestGuardMatches(guard)
      && currentProject.projectId === request.projectId && currentProject.rootPath === request.projectRootPath
      && (!authority || authority.projectId !== request.projectId
        || (authority.canonicalIdentityKey === guard.canonicalIdentityKey && authority.identityRevision === guard.identityRevision))
      && state.historyKind === request.workflowKind && state.historyStatus === request.displayStatus
      && (!append || state.historyCursor === request.cursor)
      && state.overview?.projectAccess?.canonicalIdentityKey === expectedAccess.canonicalIdentityKey
      && state.overview.projectAccess.identityRevision === expectedAccess.identityRevision;
  };
  try {
    if (!isCurrent()) return;
    const page = await listWorkflowRuns(request);
    if (!isCurrent()) return;
    const state = useWorkflowStore.getState();
    if (!page.runs.every((run) => run.projectId === request.projectId
      && run.canonicalIdentityKey === expectedAccess.canonicalIdentityKey
      && run.identityRevision === expectedAccess.identityRevision)) {
      if (append) state.setHistoryCursor(null);
      state.failOperation(operationKey, operationRequest, {
        summary: i18next.t("workflows.error.historyIdentityMismatch"),
        technicalDetails: "WORKFLOW_HISTORY_IDENTITY_MISMATCH",
      });
      return;
    }
    if (append) state.appendHistoryPage(page.runs, page.nextCursor);
    else state.replaceHistoryPage(page.runs, page.nextCursor);
  } catch (error) {
    if (!isCurrent()) return;
    const state = useWorkflowStore.getState();
    const errorCode = backendErrorCode(error);
    const staleCursor = append && (errorCode === "WORKFLOW_CURSOR_SCOPE_MISMATCH" || errorCode === "WORKFLOW_CURSOR_INVALID");
    if (append && historyPageErrorRequiresRefresh(errorCode)) state.setHistoryCursor(null);
    state.failOperation(operationKey, operationRequest, {
      summary: i18next.t(staleCursor ? "workflows.error.historyCursorStale" : "workflows.operationError.history"),
      technicalDetails: normalizeBackendError(error).technicalDetails,
    });
  }
}

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { backendErrorCode } from "../../lib/backendError";
import { getWorkflowHistoryState, undoWorkflowUpdate, type WorkflowUpdateHistoryState } from "../../services/workflowApi";
import { captureProjectScope, invalidateProjectResources, isProjectScopeCurrent } from "../../stores/projectScope";
import { useProjectStore } from "../../stores/projectStore";
import type { WorkflowRun } from "../../types/workflow";

async function refreshWikiAfterUndo(projectId: string, rootPath: string, isProjectCurrent: () => boolean) {
  const { useWikiStore } = await import("../wiki/wikiStore");
  if (!isProjectCurrent()) return;
  invalidateProjectResources({ projectId, rootPath }, ["wiki", "graph"]);
  const canRefresh = () => isProjectCurrent() && useWikiStore.getState().mode === "read";
  // A hidden editor may still contain an unsaved draft. Leave it intact and
  // let the invalidated resource refresh when the user returns to reading.
  if (!canRefresh()) return;
  const selectedPath = useWikiStore.getState().selectedPath;
  await useWikiStore.getState().scan(projectId, rootPath, canRefresh);
  const wiki = useWikiStore.getState();
  if (canRefresh() && selectedPath && wiki.selectedPath === selectedPath
    && wiki.tree?.pages.some((page) => page.path === selectedPath)) {
    await wiki.openPage(projectId, rootPath, selectedPath,
      () => canRefresh() && useWikiStore.getState().selectedPath === selectedPath);
  }
}

export function WorkflowUpdateHistory({ run, onChanged }: { run: WorkflowRun; onChanged: () => Promise<void> }) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const authority = useProjectStore((state) => state.authority);
  const [history, setHistory] = useState<WorkflowUpdateHistoryState | null>(null);
  const [errorKey, setErrorKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const currentRequest = useRef<null | { isCurrent: () => boolean; isProjectCurrent: () => boolean; request: { projectId: string; projectRootPath: string; taskId: string } }>(null);
  const undoPending = useRef(false);
  const isRecovery = Boolean(history?.recovery || history?.undoInProgress);

  useEffect(() => {
    let active = true;
    const scope = captureProjectScope();
    const isProjectCurrent = () => {
      const state = useProjectStore.getState();
      return isProjectScopeCurrent(scope)
        && state.currentProject.projectId === run.projectId && state.currentProject.rootPath === project.rootPath
        && (!state.authority || (state.authority.canonicalIdentityKey === run.canonicalIdentityKey
          && state.authority.identityRevision === run.identityRevision));
    };
    const isCurrent = () => active && isProjectCurrent();
    const request = { projectId: run.projectId, projectRootPath: project.rootPath, taskId: run.taskId };
    currentRequest.current = { isCurrent, isProjectCurrent, request };
    setHistory(null);
    setErrorKey(null);
    setBusy(false);
    if (isCurrent()) {
      void getWorkflowHistoryState(request).then((state) => {
        if (isCurrent()) setHistory(state);
      }).catch(() => {
        if (isCurrent()) setErrorKey("workflows.updateHistory.unavailable");
      });
    }
    return () => { active = false; };
  }, [run.taskId, run.revision, run.canonicalIdentityKey, run.identityRevision, run.projectId, project.projectId, project.rootPath, authority?.canonicalIdentityKey, authority?.identityRevision, refresh]);

  const undo = async () => {
    const context = currentRequest.current;
    if (!context?.isCurrent() || !history?.available || history.undone || undoPending.current) return;
    undoPending.current = true;
    setBusy(true);
    setErrorKey(null);
    let undone = false;
    try {
      const next = await undoWorkflowUpdate(context.request);
      if (!context.isProjectCurrent()) return;
      undone = true;
      if (context.isCurrent()) {
        setHistory(next);
        void onChanged();
      }
      await refreshWikiAfterUndo(context.request.projectId, context.request.projectRootPath, context.isProjectCurrent);
    } catch (error) {
      if (!undone && context.isCurrent()) {
        const latest = await getWorkflowHistoryState(context.request).catch(() => null);
        if (latest && context.isCurrent()) setHistory(latest);
      }
      if (context.isCurrent()) setErrorKey(undone ? "workflows.updateHistory.refreshFailed"
        : backendErrorCode(error) === "WORKFLOW_UNDO_CONFLICT"
          ? "workflows.updateHistory.conflict" : isRecovery ? "workflows.updateHistory.recoveryFailed" : "workflows.updateHistory.failed");
    } finally {
      undoPending.current = false;
      if (context.isCurrent()) setBusy(false);
    }
  };

  return <div className="workflow-update-history">
    {history?.undone ? <p className="workflow-scope-state" role="status">{t(history.recovery || run.displayStatus === "failed" || run.displayStatus === "interrupted" ? "workflows.updateHistory.restored" : "workflows.updateHistory.undone")}</p>
      : history?.available ? <>
        {isRecovery ? <p className="workflow-scope-state">{t(history.undoInProgress ? "workflows.updateHistory.resumeNotice" : "workflows.updateHistory.recoveryNotice")}</p> : null}
        <button className="btn btn--secondary" disabled={busy} aria-busy={busy} onClick={() => void undo()} type="button">{t(busy
          ? isRecovery ? "workflows.updateHistory.recovering" : "workflows.updateHistory.undoing"
          : history.undoInProgress ? "workflows.updateHistory.resume" : history.recovery ? "workflows.updateHistory.restore" : "workflows.updateHistory.undo")}</button>
      </> : null}
    {errorKey ? <p className="workflow-conflict-notice" role="alert">{t(errorKey)}
      {errorKey === "workflows.updateHistory.unavailable" ? <button className="btn btn--ghost btn--sm" type="button" onClick={() => setRefresh((value) => value + 1)}>{t("workflows.action.retry")}</button> : null}
    </p> : null}
  </div>;
}

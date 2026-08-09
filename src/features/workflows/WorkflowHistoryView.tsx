import { ArrowLeft } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import type { WorkflowDisplayStatus, WorkflowKind, WorkflowRun } from "../../types/workflow";
import { useWorkflowStore, workflowOperationPending } from "../../stores/workflowStore";
import { groupWorkflowAttempts, WORKFLOW_KINDS, WORKFLOW_STATUSES, workflowKindKey, workflowStatusKey } from "./workflowPresentation";

export function WorkflowHistoryView({ runs, onBack, onOpen, onLoadMore }: { runs: WorkflowRun[]; onBack: () => void; onOpen: (taskId: string) => void; onLoadMore: () => void }) {
  const { t } = useTranslation();
  const historyKind = useWorkflowStore((state) => state.historyKind);
  const historyStatus = useWorkflowStore((state) => state.historyStatus);
  const setHistoryFilters = useWorkflowStore((state) => state.setHistoryFilters);
  const historyCursor = useWorkflowStore((state) => state.historyCursor);
  const operations = useWorkflowStore((state) => state.operations);
  const pagePending = workflowOperationPending(operations, "history:page");
  const kind: WorkflowKind | "all" = historyKind ?? "all";
  const status: WorkflowDisplayStatus | "all" = historyStatus ?? "all";
  const groups = useMemo(() => groupWorkflowAttempts(runs.filter((run) => (kind === "all" || run.kind === kind) && (status === "all" || run.displayStatus === status))), [kind, runs, status]);
  return <div className="workflow-history">
    <button className="workflow-back" onClick={onBack} type="button"><ArrowLeft aria-hidden="true" size={14} />{t("workflows.action.back")}</button>
    <div className="workflows-intro"><h2>{t("workflows.history.title")}</h2><p>{t("workflows.history.description")}</p></div>
    <div className="workflow-filters"><label>{t("workflows.history.kind")}<select onChange={(event) => setHistoryFilters(event.target.value === "all" ? null : event.target.value as WorkflowKind, historyStatus)} value={kind}><option value="all">{t("workflows.filter.all")}</option>{WORKFLOW_KINDS.map((value) => <option key={value} value={value}>{t(workflowKindKey(value))}</option>)}</select></label><label>{t("workflows.history.status")}<select onChange={(event) => setHistoryFilters(historyKind, event.target.value === "all" ? null : event.target.value as WorkflowDisplayStatus)} value={status}><option value="all">{t("workflows.filter.all")}</option>{WORKFLOW_STATUSES.map((value) => <option key={value} value={value}>{t(workflowStatusKey(value))}</option>)}</select></label></div>
    <div className="workflow-history__list">{groups.length === 0 ? <p className="workflow-muted">{t("workflows.history.empty")}</p> : groups.map((group) => <section key={group.key}><h3>{t("workflows.history.attempts", { count: group.runs.length })}</h3>{group.runs.map((run) => <button className="workflow-history__run" disabled={workflowOperationPending(operations, `task:${run.taskId}:open`)} key={run.taskId} onClick={() => onOpen(run.taskId)} type="button"><span>{t(workflowKindKey(run.kind))}</span><span>{t(workflowStatusKey(run.displayStatus))}</span><time dateTime={run.updatedAt}>{new Date(run.updatedAt).toLocaleString()}</time></button>)}</section>)}</div>
    {historyCursor ? <div className="workflow-actions"><button className="btn btn--secondary" disabled={pagePending} onClick={onLoadMore} type="button">{t("workflows.history.loadMore")}</button></div> : null}
  </div>;
}

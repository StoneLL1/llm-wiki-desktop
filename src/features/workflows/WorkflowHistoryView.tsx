import { ArrowLeft } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkflowDisplayStatus, WorkflowKind, WorkflowRunSummary } from "../../types/workflow";
import { useWorkflowStore, workflowOperationPending } from "../../stores/workflowStore";
import { groupWorkflowAttempts, WORKFLOW_KINDS, WORKFLOW_STATUSES, workflowKindKey, workflowStatusKey } from "./workflowPresentation";

const HISTORY_ROW_HEIGHT = 56;
const HISTORY_VIEWPORT_HEIGHT = 448;
const HISTORY_OVERSCAN = 4;

interface HistoryRow {
  groupKey: string;
  groupSize: number;
  firstAttempt: boolean;
  run: WorkflowRunSummary;
}

export function WorkflowHistoryView({ runs, onBack, onOpen, onLoadMore, onFilter }: {
  runs: WorkflowRunSummary[];
  onBack: () => void;
  onOpen: (taskId: string) => void;
  onLoadMore: () => void;
  onFilter?: (kind: WorkflowKind | null, status: WorkflowDisplayStatus | null) => void;
}) {
  const { t } = useTranslation();
  const historyKind = useWorkflowStore((state) => state.historyKind);
  const historyStatus = useWorkflowStore((state) => state.historyStatus);
  const setHistoryFilters = useWorkflowStore((state) => state.setHistoryFilters);
  const historyCursor = useWorkflowStore((state) => state.historyCursor);
  const operations = useWorkflowStore((state) => state.operations);
  const pagePending = workflowOperationPending(operations, "history:");
  const listRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const kind: WorkflowKind | "all" = historyKind ?? "all";
  const status: WorkflowDisplayStatus | "all" = historyStatus ?? "all";
  const rows = useMemo<HistoryRow[]>(() => groupWorkflowAttempts(runs).flatMap((group) =>
    group.runs.map((run, index) => ({
      groupKey: group.key,
      groupSize: group.runs.length,
      firstAttempt: index === 0,
      run,
    })),
  ), [runs]);
  const firstVisible = Math.max(0, Math.floor(scrollTop / HISTORY_ROW_HEIGHT) - HISTORY_OVERSCAN);
  const visibleCount = Math.ceil(HISTORY_VIEWPORT_HEIGHT / HISTORY_ROW_HEIGHT) + HISTORY_OVERSCAN * 2;
  const lastVisible = Math.min(rows.length, firstVisible + visibleCount);
  const visibleRows = rows.slice(firstVisible, lastVisible);

  useEffect(() => {
    setScrollTop(0);
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [historyKind, historyStatus]);

  const updateFilters = (nextKind: WorkflowKind | null, nextStatus: WorkflowDisplayStatus | null) => {
    if (onFilter) onFilter(nextKind, nextStatus);
    else setHistoryFilters(nextKind, nextStatus);
  };

  return <div className="workflow-history">
    <button className="workflow-back" onClick={onBack} type="button"><ArrowLeft aria-hidden="true" size={14} />{t("workflows.action.back")}</button>
    <div className="workflows-intro"><h2 data-workflow-surface-title tabIndex={-1}>{t("workflows.history.title")}</h2><p>{t("workflows.history.description")}</p></div>
    <div className="workflow-filters"><label>{t("workflows.history.kind")}<select onChange={(event) => updateFilters(event.target.value === "all" ? null : event.target.value as WorkflowKind, historyStatus)} value={kind}><option value="all">{t("workflows.filter.all")}</option>{WORKFLOW_KINDS.map((value) => <option key={value} value={value}>{t(workflowKindKey(value))}</option>)}</select></label><label>{t("workflows.history.status")}<select onChange={(event) => updateFilters(historyKind, event.target.value === "all" ? null : event.target.value as WorkflowDisplayStatus)} value={status}><option value="all">{t("workflows.filter.all")}</option>{WORKFLOW_STATUSES.map((value) => <option key={value} value={value}>{t(workflowStatusKey(value))}</option>)}</select></label></div>
    <div
      aria-label={t("workflows.history.title")}
      className="workflow-history__list app-pane-scrollbar"
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      ref={listRef}
      role="list"
    >
      {rows.length === 0 ? <p className="workflow-muted">{t("workflows.history.empty")}</p> : <div style={{ paddingTop: firstVisible * HISTORY_ROW_HEIGHT, paddingBottom: (rows.length - lastVisible) * HISTORY_ROW_HEIGHT }}>
        {visibleRows.map(({ groupKey, groupSize, firstAttempt, run }) => <div className="workflow-history__row" key={run.taskId} role="listitem">
          <button className="workflow-history__run" disabled={workflowOperationPending(operations, `task:${run.taskId}:open`)} onClick={() => onOpen(run.taskId)} type="button">
            <span className="workflow-history__attempt">{firstAttempt ? t("workflows.history.attempts", { count: groupSize }) : t("workflows.history.retryAttempt", { count: run.retry?.attemptNumber ?? 1 })}</span>
            <span>{t(workflowKindKey(run.kind))}</span>
            <span>{t(workflowStatusKey(run.displayStatus))}</span>
            <time dateTime={run.updatedAt}>{new Date(run.updatedAt).toLocaleString()}</time>
          </button>
          <span className="sr-only">{groupKey}</span>
        </div>)}
      </div>}
    </div>
    {historyCursor ? <div className="workflow-actions"><button className="btn btn--secondary" disabled={pagePending} onClick={onLoadMore} type="button">{t("workflows.history.loadMore")}</button></div> : null}
  </div>;
}

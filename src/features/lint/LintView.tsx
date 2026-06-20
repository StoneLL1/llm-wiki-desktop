import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

import { useLintStore } from "../../stores/lintStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { isTerminalStatus } from "../../types/task";
import type { LintRoutePreference } from "../../types/lint";
import { LintIssueDetails } from "./LintIssueDetails";
import { LintIssueList } from "./LintIssueList";

const ROUTE_PREFERENCE: LintRoutePreference = "auto";

export function LintView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);

  const localReport = useLintStore((state) => state.localReport);
  const deepReport = useLintStore((state) => state.deepReport);
  const loadingLocal = useLintStore((state) => state.loadingLocal);
  const runningDeep = useLintStore((state) => state.runningDeep);
  const deepTaskId = useLintStore((state) => state.deepTaskId);
  const selectedIssueId = useLintStore((state) => state.selectedIssueId);
  const fixStatus = useLintStore((state) => state.fixStatus);
  const fixConfirm = useLintStore((state) => state.fixConfirm);
  const error = useLintStore((state) => state.error);

  const runLocalLint = useLintStore((state) => state.runLocalLint);
  const startDeepLint = useLintStore((state) => state.startDeepLint);
  const clearDeepTask = useLintStore((state) => state.clearDeepTask);
  const loadDeepReport = useLintStore((state) => state.loadDeepReport);
  const selectIssue = useLintStore((state) => state.selectIssue);
  const applyFix = useLintStore((state) => state.applyFix);
  const confirmHighRisk = useLintStore((state) => state.confirmHighRisk);
  const cancelHighRisk = useLintStore((state) => state.cancelHighRisk);

  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const { projectId, rootPath } = currentProject;
  const localIssues = localReport?.issues ?? [];
  const deepIssues = deepReport?.issues ?? [];
  const allIssues = useMemo(
    () => [...localIssues, ...deepIssues],
    [localIssues, deepIssues],
  );
  const selectedIssue = selectedIssueId
    ? allIssues.find((issue) => issue.id === selectedIssueId) ?? null
    : null;

  const deepTask = deepTaskId ? tasks.find((task) => task.id === deepTaskId) ?? null : null;

  // Load the persisted deep-lint report when the background task lands.
  useEffect(() => {
    if (deepTask && isTerminalStatus(deepTask.status)) {
      void loadDeepReport({ projectId, projectRootPath: rootPath, taskId: deepTask.id });
      clearDeepTask();
    }
  }, [deepTask, projectId, rootPath, loadDeepReport, clearDeepTask]);

  const handleRunLocal = () => {
    void runLocalLint(projectId, rootPath);
  };

  const handleStartDeep = () => {
    void startDeepLint(projectId, rootPath, ROUTE_PREFERENCE).then((taskId) => {
      if (taskId) {
        // Mirror start_wiki_compile: upsert so the task drawer follows the run.
        void invoke("list_tasks").then((list) => {
          const found = (list as { id: string }[]).find((task) => task.id === taskId);
          if (found) {
            void invoke("get_task", { request: { taskId } }).then((task) => {
              if (task) upsertTask(task as never);
            });
          }
        });
        openTaskDrawer(taskId);
      }
    });
  };

  const handleCancelDeep = () => {
    if (!deepTaskId) return;
    void invoke("cancel_task", { request: { taskId: deepTaskId } }).then((task) => {
      if (task) upsertTask(task as never);
    });
  };

  const handleApplyFix = (issue: Parameters<typeof applyFix>[2]) => {
    void applyFix(projectId, rootPath, issue).then((outcome) => {
      if (outcome?.kind === "applied") {
        void runLocalLint(projectId, rootPath);
      }
    });
  };

  const handleConfirmHighRisk = (expectedHash: string) => {
    void confirmHighRisk(projectId, rootPath, expectedHash).then((outcome) => {
      if (outcome?.kind === "applied") {
        void runLocalLint(projectId, rootPath);
      }
    });
  };

  return (
    <div className="grid h-full grid-cols-[minmax(0,1fr)_360px]">
      <div className="flex min-w-0 flex-col border-r border-[var(--border)]">
        <div className="flex h-[44px] shrink-0 items-center gap-2 border-b border-[var(--border)] px-4">
          <button
            type="button"
            onClick={handleRunLocal}
            disabled={loadingLocal}
            className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[#1a1a1a] disabled:opacity-40"
          >
            {loadingLocal ? "…" : t("lint.actions.runLocal")}
          </button>
          {runningDeep ? (
            <button
              type="button"
              onClick={handleCancelDeep}
              className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
            >
              {t("lint.actions.cancel")}
            </button>
          ) : (
            <button
              type="button"
              onClick={handleStartDeep}
              className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
            >
              {t("lint.actions.deepLint")}
            </button>
          )}
          <div className="ml-auto flex items-center gap-3 text-[11px] text-[var(--text-muted)]">
            <span>{t("lint.summary.localCount", { count: localReport?.issues.length ?? 0 })}</span>
            <span>{t("lint.summary.agentCount", { count: deepReport?.issues.length ?? 0 })}</span>
          </div>
        </div>
        {error ? (
          <div className="border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {error}
          </div>
        ) : null}
        <LintIssueList
          issues={allIssues}
          selectedIssueId={selectedIssueId}
          onSelect={selectIssue}
        />
      </div>

      <LintIssueDetails
        issue={selectedIssue}
        fixStatus={selectedIssue ? fixStatus[selectedIssue.id] ?? "idle" : "idle"}
        fixConfirm={fixConfirm}
        projectId={projectId}
        rootPath={rootPath}
        onApplyFix={handleApplyFix}
        onConfirmHighRisk={handleConfirmHighRisk}
        onCancelHighRisk={cancelHighRisk}
      />
    </div>
  );
}

import { type CSSProperties, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import { useTaskLauncher } from "../../hooks/useTaskLauncher";
import { useLintStore } from "../../stores/lintStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { captureProjectScope, isProjectScopeCurrent } from "../../stores/projectScope";
import type { LintIssue, LintIssueType, LintRoutePreference } from "../../types/lint";
import { LintBatchConfirmDialog } from "./LintBatchConfirmDialog";
import { LintHistoryList } from "./LintHistoryList";
import { LintIssueDetails } from "./LintIssueDetails";
import { LintIssueList } from "./LintIssueList";
import { LintPassedSection } from "./LintPassedSection";
import { LintSummaryCards } from "./LintSummaryCards";

const ROUTE_PREFERENCE: LintRoutePreference = "auto";

/** Local deterministic rules that earn a "passed" badge when absent. */
const PASSED_RULES: LintIssueType[] = [
  "missing_frontmatter",
  "index_drift",
  "duplicate_filename",
  "missing_resource",
  "path_case",
];

export function LintView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const paneSizes = useNavigationStore((state) => state.paneSizes);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);

  const localReport = useLintStore((state) => state.localReport);
  const deepReport = useLintStore((state) => state.deepReport);
  const loadingLocal = useLintStore((state) => state.loadingLocal);
  const runningDeep = useLintStore((state) => state.runningDeep);
  const deepTaskId = useLintStore((state) => state.deepTaskId);
  const selectedIssueId = useLintStore((state) => state.selectedIssueId);
  const fixStatus = useLintStore((state) => state.fixStatus);
  const fixConfirm = useLintStore((state) => state.fixConfirm);
  const error = useLintStore((state) => state.error);
  const mode = useLintStore((state) => state.mode);
  const batchRunning = useLintStore((state) => state.batchRunning);
  const fixApplying = useLintStore((state) =>
    Object.values(state.fixStatus).some((status) => status === "applying"),
  );
  const batchConfirmations = useLintStore((state) => state.batchConfirmations);
  const hasPendingBatchConfirmations = batchConfirmations.length > 0;
  const safetyPrefs = useLintStore((state) => state.safetyPrefs);
  const ignores = useLintStore((state) => state.ignores);
  const history = useLintStore((state) => state.history);
  const historyLoading = useLintStore((state) => state.historyLoading);
  const historyError = useLintStore((state) => state.historyError);
  const activeHistoryId = useLintStore((state) => state.activeHistoryId);

  const runLocalLint = useLintStore((state) => state.runLocalLint);
  const startDeepLint = useLintStore((state) => state.startDeepLint);
  const clearDeepTask = useLintStore((state) => state.clearDeepTask);
  const loadDeepReport = useLintStore((state) => state.loadDeepReport);
  const selectIssue = useLintStore((state) => state.selectIssue);
  const setMode = useLintStore((state) => state.setMode);
  const setSafetyPrefs = useLintStore((state) => state.setSafetyPrefs);
  const loadHistory = useLintStore((state) => state.loadHistory);
  const openHistoryReport = useLintStore((state) => state.openHistoryReport);
  const loadIgnores = useLintStore((state) => state.loadIgnores);
  const addIgnore = useLintStore((state) => state.addIgnore);
  const removeIgnore = useLintStore((state) => state.removeIgnore);
  const applyFix = useLintStore((state) => state.applyFix);
  const applyFixesBatch = useLintStore((state) => state.applyFixesBatch);
  const openBatchConfirmation = useLintStore((state) => state.openBatchConfirmation);
  const confirmHighRisk = useLintStore((state) => state.confirmHighRisk);
  const cancelHighRisk = useLintStore((state) => state.cancelHighRisk);

  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const { projectId, rootPath } = currentProject;
  const taskLauncher = useTaskLauncher(currentProject);
  const layoutStyle = {
    "--lint-details-w-current": `${paneSizes.lintDetails}px`,
  } as CSSProperties;
  const localIssues = localReport?.issues ?? [];
  const deepIssues = deepReport?.issues ?? [];
  const allIssues = useMemo(
    () => [...localIssues, ...deepIssues],
    [localIssues, deepIssues],
  );
  const modeIssues = useMemo(() => {
    if (mode === "local") return localIssues;
    if (mode === "agent") return deepIssues;
    return allIssues;
  }, [mode, localIssues, deepIssues, allIssues]);
  const selectedIssue = selectedIssueId
    ? allIssues.find((issue) => issue.id === selectedIssueId) ?? null
    : null;

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [ignoringId, setIgnoringId] = useState<string | null>(null);
  const [removingIgnoreKey, setRemovingIgnoreKey] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const autoFixable = useMemo(
    () => modeIssues.filter((issue) => issue.fixability !== "none" && issue.scanHash),
    [modeIssues],
  );

  const presentRules = useMemo(
    () => new Set(localIssues.map((issue) => issue.issueType)),
    [localIssues],
  );
  const passedRules = useMemo(
    () => PASSED_RULES.filter((rule) => !presentRules.has(rule)),
    [presentRules],
  );

  const deepTask = deepTaskId ? tasks.find((task) => task.id === deepTaskId) ?? null : null;

  // Load ignored-issue entries + the persisted deep-lint report when the
  // background task lands.
  useEffect(() => {
    void loadIgnores({ projectId, projectRootPath: rootPath });
  }, [projectId, rootPath, loadIgnores]);

  useEffect(() => {
    let cancelled = false;
    void loadHistory({ projectId, projectRootPath: rootPath }).then((entries) => {
      const hasLoadedReport =
        useLintStore.getState().localReport || useLintStore.getState().deepReport;
      if (cancelled || hasLoadedReport) return;
      const latest = entries[0];
      if (latest) {
        void openHistoryReport({ projectId, projectRootPath: rootPath, id: latest.id });
      }
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, rootPath, loadHistory, openHistoryReport]);

  useEffect(() => {
    if (deepTask?.status === "succeeded") {
      void loadDeepReport({ projectId, projectRootPath: rootPath, taskId: deepTask.id });
      clearDeepTask();
    } else if (deepTask && (deepTask.status === "failed" || deepTask.status === "cancelled")) {
      // The task drawer owns the original failure/cancellation details. Do
      // not issue a second report-read request that would mask them with
      // LINT_DEEP_REPORT_MISSING.
      clearDeepTask();
    }
  }, [deepTask, projectId, rootPath, loadDeepReport, clearDeepTask]);

  const triggerRecompile = () => {
    void taskLauncher
      .startCompile({
        route: ROUTE_PREFERENCE,
        agent: null,
        provider: null,
      })
      .catch(() => {
        /* recompile is best-effort; failures surface in the task drawer */
      });
  };

  const refreshAfterFix = (applied: boolean) => {
    void runLocalLint(projectId, rootPath, {
      preserveBatchConfirmations: useLintStore.getState().batchConfirmations.length > 0,
    });
    if (applied && safetyPrefs.recompile) triggerRecompile();
  };

  const handleRunLocal = () => {
    setNotice(null);
    void runLocalLint(projectId, rootPath);
  };

  const handleStartDeep = () => {
    void startDeepLint(projectId, rootPath, ROUTE_PREFERENCE).then((taskId) => {
      if (taskId) {
        void invoke("list_tasks", { request: { statusFilter: null } }).then((list) => {
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

  const handleApplyFix = async (issue: LintIssue) => {
    // Fixes must use the immutable scan snapshot. Reading the live page here
    // would silently replace the report baseline after an external edit.
    const expectedHash = issue.fixability === "safe" ? issue.scanHash ?? null : null;
    const outcome = await applyFix(projectId, rootPath, issue, expectedHash);
    if (outcome?.kind === "applied") refreshAfterFix(true);
  };

  const handleConfirmHighRisk = (expectedHash: string) => {
    void confirmHighRisk(projectId, rootPath, expectedHash).then((outcome) => {
      if (outcome?.kind === "applied") refreshAfterFix(true);
    });
  };

  const handleIgnore = (issue: LintIssue) => {
    if (fixConfirm || batchRunning || fixApplying || hasPendingBatchConfirmations) return;
    setNotice(null);
    setIgnoringId(issue.id);
    void addIgnore({
      projectId,
      projectRootPath: rootPath,
      path: issue.path,
      rule: issue.issueType,
    }).then((ok) => {
      setIgnoringId(null);
      if (ok) {
        setNotice(t("lint.plan.ignored"));
        selectIssue(null);
        void runLocalLint(projectId, rootPath);
      }
    });
  };

  const handleRemoveIgnore = (path: string, rule: LintIssueType) => {
    if (fixConfirm || batchRunning || fixApplying || hasPendingBatchConfirmations) return;
    const key = `${path}:${rule}`;
    setNotice(null);
    setRemovingIgnoreKey(key);
    void removeIgnore({
      projectId,
      projectRootPath: rootPath,
      path,
      rule,
    }).then((ok) => {
      setRemovingIgnoreKey(null);
      if (ok) {
        setNotice(t("lint.ignores.restored"));
        void runLocalLint(projectId, rootPath);
      }
    });
  };

  // Use scan-time hashes for safe-fixable issues, then run the batch under one
  // shared Git checkpoint. Do not reread live pages between report and fix.
  const handleBatchConfirm = () => {
    setConfirmOpen(false);
    const scope = captureProjectScope();
    const expectedHashes = Object.fromEntries(
      autoFixable
        .filter((issue) => issue.fixability === "safe" && issue.scanHash)
        .map((issue) => [issue.path, issue.scanHash as string]),
    );
    void applyFixesBatch({
      projectId,
      projectRootPath: rootPath,
      issues: autoFixable,
      expectedHashes,
    })
      .then((outcome) => {
        if (!outcome || !isProjectScopeCurrent(scope)) return;
        const parts: string[] = [];
        if (outcome.applied.length > 0) {
          parts.push(
            t("lint.batch.applied", {
              count: outcome.applied.length,
              hash: outcome.finalCommit ?? outcome.checkpoint ?? "—",
            }),
          );
        }
        if (outcome.skipped.length > 0) {
          const reasons = [...new Set(outcome.skipped.map((skip) => skip.reason))];
          parts.push(`${t("lint.batch.skipped", { count: outcome.skipped.length })} (${reasons.join("; ")})`);
        }
        if (outcome.needsConfirmation.length > 0) {
          parts.push(t("lint.batch.pending", { count: outcome.needsConfirmation.length }));
        }
        setNotice(parts.join(" · ") || null);
        void runLocalLint(projectId, rootPath, {
          preserveBatchConfirmations: outcome.needsConfirmation.length > 0,
        });
        if (outcome.applied.length > 0 && safetyPrefs.recompile) triggerRecompile();
      });
  };

  const segButton = (key: typeof mode, label: string, count: number) => (
    <button
      type="button"
      aria-pressed={mode === key}
      className={mode === key ? "is-active" : ""}
      onClick={() => {
        setNotice(null);
        setMode(key);
      }}
    >
      {label} {count}
    </button>
  );

  return (
    <div className="lint-view-layout" style={layoutStyle}>
      <div className="lint-view__list-pane">
        <div className="view-toolbar border-b border-[var(--border)] px-4">
          <div className="seg" role="group" aria-label={t("view.lint.paneTitle")}>
            {segButton("all", t("lint.mode.all"), allIssues.length)}
            {segButton("local", t("lint.mode.local"), localIssues.length)}
            {segButton("agent", t("lint.mode.agent"), deepIssues.length)}
          </div>
          <button
            type="button"
            onClick={handleRunLocal}
            disabled={
              loadingLocal ||
              batchRunning ||
              fixApplying ||
              hasPendingBatchConfirmations ||
              Boolean(fixConfirm)
            }
            className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
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
          <button
            type="button"
            onClick={() => setConfirmOpen(true)}
            disabled={
              autoFixable.length === 0 ||
              loadingLocal ||
              batchRunning ||
              fixApplying ||
              hasPendingBatchConfirmations ||
              Boolean(fixConfirm)
            }
            className="ml-auto h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
          >
            {batchRunning ? "…" : t("lint.actions.autoFix", { count: autoFixable.length })}
          </button>
        </div>

        {notice ? (
          <div className="border-b border-[var(--accent-border)] bg-[var(--accent-soft)] px-4 py-2 text-[12px] text-[var(--accent-hover)]">
            {notice}
          </div>
        ) : null}
        {error ? (
          <div className="border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {error}
          </div>
        ) : null}

        <LintHistoryList
          entries={history}
          activeId={activeHistoryId}
          loading={historyLoading}
          error={historyError}
          onOpen={(id) =>
            void openHistoryReport({ projectId, projectRootPath: rootPath, id })
          }
        />

        {ignores.length > 0 ? (
          <section className="border-b border-[var(--border)] px-4 py-3" aria-label={t("lint.ignores.title")}>
            <div className="mb-2 flex items-center justify-between">
              <span className="text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
                {t("lint.ignores.title")}
              </span>
              <span className="font-mono text-[11px] text-[var(--text-muted)]">{ignores.length}</span>
            </div>
            <div className="space-y-1">
              {ignores.map((entry) => {
                const key = `${entry.path}:${entry.rule}`;
                return (
                  <div key={key} className="flex items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-raised)] px-2 py-1.5">
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-[var(--text-secondary)]" title={entry.path}>
                      {entry.path}
                    </span>
                    <span className="shrink-0 text-[11px] text-[var(--text-muted)]">
                      {t(`lint.issueType.${entry.rule}`)}
                    </span>
                    <button
                      type="button"
                      onClick={() => handleRemoveIgnore(entry.path, entry.rule)}
                      disabled={
                        removingIgnoreKey === key ||
                        Boolean(fixConfirm) ||
                        batchRunning ||
                        fixApplying ||
                        hasPendingBatchConfirmations
                      }
                      className="shrink-0 rounded-[var(--radius-sm)] border border-[var(--border)] px-2 py-1 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)] disabled:opacity-40"
                    >
                      {removingIgnoreKey === key ? t("lint.ignores.restoring") : t("lint.ignores.restore")}
                    </button>
                  </div>
                );
              })}
            </div>
          </section>
        ) : null}

        {localReport ? (
          <LintSummaryCards issues={modeIssues} passedCount={passedRules.length} />
        ) : null}

        <LintIssueList
          issues={modeIssues}
          selectedIssueId={selectedIssueId}
          actionsDisabled={
            loadingLocal || batchRunning || fixApplying || hasPendingBatchConfirmations
          }
          onSelect={selectIssue}
          onApplyFix={handleApplyFix}
        />

        {localReport ? <LintPassedSection passedRules={passedRules} /> : null}

        {batchConfirmations.length > 0 ? (
          <div className="flex flex-wrap items-center gap-2 border-t border-[var(--border)] bg-[var(--warning-soft)] px-4 py-2 text-[12px]">
            <span className="text-[var(--text-primary)]">
              {t("lint.batch.pending", { count: batchConfirmations.length })}
            </span>
            {batchConfirmations.map((entry) => (
              <button
                key={entry.issue.id}
                type="button"
                onClick={() => openBatchConfirmation(entry.issue.id)}
                className="h-[24px] rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-2 text-[11px] hover:bg-[var(--surface-muted)]"
              >
                {t("lint.batch.review")} · {entry.issue.path}
              </button>
            ))}
          </div>
        ) : null}
      </div>

      <ResizableSplitter
        paneId="lintDetails"
        label={t("shell.splitter.lintDetails")}
        min={PANE_WIDTH_LIMITS.lintDetails.min}
        max={PANE_WIDTH_LIMITS.lintDetails.max}
        value={paneSizes.lintDetails}
        onChange={(value) => setPaneSize("lintDetails", value)}
        onReset={() => resetPaneSize("lintDetails")}
      />

      <LintIssueDetails
        issue={selectedIssue}
        fixStatus={selectedIssue ? fixStatus[selectedIssue.id] ?? "idle" : "idle"}
        fixConfirm={fixConfirm}
        ignoring={selectedIssue ? ignoringId === selectedIssue.id : false}
        actionsDisabled={
          loadingLocal ||
          batchRunning ||
          fixApplying ||
          (hasPendingBatchConfirmations && !fixConfirm)
        }
        safetyPrefs={safetyPrefs}
        onSafetyPrefsChange={setSafetyPrefs}
        onApplyFix={handleApplyFix}
        onConfirmHighRisk={handleConfirmHighRisk}
        onCancelHighRisk={cancelHighRisk}
        onIgnore={handleIgnore}
      />

      {confirmOpen ? (
        <LintBatchConfirmDialog
          count={autoFixable.length}
          onConfirm={handleBatchConfirm}
          onCancel={() => setConfirmOpen(false)}
        />
      ) : null}
    </div>
  );
}

import { type CSSProperties, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import { useLintStore } from "../../stores/lintStore";
import { observeProjectResources } from "../../stores/projectScope";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { cancelTaskRequest, useTaskStore } from "../../stores/taskStore";
import { captureProjectScope, isProjectScopeCurrent } from "../../stores/projectScope";
import { isAgentLintRepairEligible } from "../../types/lint";
import type { LintIssue, LintIssueType } from "../../types/lint";
import { AgentLintRepairPanel } from "./AgentLintRepairPanel";
import { LintBatchConfirmDialog } from "./LintBatchConfirmDialog";
import { LintHistoryList } from "./LintHistoryList";
import { LintIssueDetails } from "./LintIssueDetails";
import { LintIssueList } from "./LintIssueList";
import { LintPassedSection } from "./LintPassedSection";
import { LintSummaryCards } from "./LintSummaryCards";

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
  const authority = useProjectStore((state) => state.authority);
  const lintDetailsWidth = useNavigationStore((state) => state.paneSizes.lintDetails);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);
  const requestWorkflowLaunch = useNavigationStore((state) => state.requestWorkflowLaunch);

  const localReport = useLintStore((state) => state.localReport);
  const deepReport = useLintStore((state) => state.deepReport);
  const healthReport = useLintStore((state) => state.healthReport);
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
  const agentRepairSelection = useLintStore((state) => state.agentRepairSelection);
  const agentRepairPreparation = useLintStore((state) => state.agentRepairPreparation);
  const agentRepairPending = useLintStore((state) => state.agentRepairPending);
  const agentRepairErrorCode = useLintStore((state) => state.agentRepairErrorCode);
  const invalidateAgentLintRepairIdentity = useLintStore((state) => state.invalidateAgentLintRepairIdentity);

  const runLocalLint = useLintStore((state) => state.runLocalLint);
  const clearDeepTask = useLintStore((state) => state.clearDeepTask);
  const loadDeepReport = useLintStore((state) => state.loadDeepReport);
  const selectIssue = useLintStore((state) => state.selectIssue);
  const setMode = useLintStore((state) => state.setMode);
  const setSafetyPrefs = useLintStore((state) => state.setSafetyPrefs);
  const ensureHistory = useLintStore((state) => state.ensureHistory);
  const openHistoryReport = useLintStore((state) => state.openHistoryReport);
  const ensureIgnores = useLintStore((state) => state.ensureIgnores);
  const addIgnore = useLintStore((state) => state.addIgnore);
  const removeIgnore = useLintStore((state) => state.removeIgnore);
  const applyFix = useLintStore((state) => state.applyFix);
  const applyFixesBatch = useLintStore((state) => state.applyFixesBatch);
  const openBatchConfirmation = useLintStore((state) => state.openBatchConfirmation);
  const confirmHighRisk = useLintStore((state) => state.confirmHighRisk);
  const cancelHighRisk = useLintStore((state) => state.cancelHighRisk);
  const setAgentRepairSelection = useLintStore((state) => state.setAgentRepairSelection);
  const prepareAgentLintRepair = useLintStore((state) => state.prepareAgentLintRepair);
  const cancelAgentLintRepairPreparation = useLintStore((state) => state.cancelAgentLintRepairPreparation);
  const confirmAgentLintRepairStart = useLintStore((state) => state.confirmAgentLintRepairStart);

  const tasks = useTaskStore((state) => state.tasks);

  const { projectId, rootPath } = currentProject;
  const layoutRef = useRef<HTMLDivElement>(null);
  const authorityIdentity = authority?.projectId === projectId
    ? `${authority.canonicalIdentityKey}\0${authority.identityRevision}`
    : null;
  const layoutStyle = {
    "--lint-details-w-current": `${lintDetailsWidth}px`,
  } as CSSProperties;
  const healthIssues = healthReport
    ? healthReport.issues.map((issue) => ({
        ...issue,
        origins: healthReport.findingOrigins[issue.id] ?? [issue.source],
      }))
    : [];
  const localIssues = healthReport
    ? healthIssues.filter((issue) =>
        healthReport.findingOrigins[issue.id]?.includes("local"),
      )
    : localReport?.issues ?? [];
  const deepIssues = healthReport
    ? healthIssues
        .filter((issue) =>
          healthReport.findingOrigins[issue.id]?.includes("agent"),
        )
        .map((issue) => ({ ...issue, source: "agent" as const }))
    : deepReport?.issues ?? [];
  const allIssues = useMemo(
    () => (healthReport ? healthIssues : [...localIssues, ...deepIssues]),
    [healthReport, healthIssues, localIssues, deepIssues],
  );
  const modeIssues = useMemo(() => {
    if (mode === "local") return localIssues;
    if (mode === "agent") return deepIssues;
    return allIssues;
  }, [mode, localIssues, deepIssues, allIssues]);
  const selectedIssue = selectedIssueId
    ? allIssues.find((issue) => issue.id === selectedIssueId) ?? null
    : null;
  const eligibleAgentFindings = useMemo(
    () => healthReport?.issues.filter((issue) => isAgentLintRepairEligible(issue, healthReport)) ?? [],
    [healthReport],
  );
  const eligibleAgentFindingIds = useMemo(
    () => new Set(eligibleAgentFindings.map((issue) => issue.id)),
    [eligibleAgentFindings],
  );
  const repairSelectionSet = useMemo(() => new Set(agentRepairSelection), [agentRepairSelection]);

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
  const passedRules = useMemo(() => {
    const notApplicable = new Set(
      healthReport?.coverage.notApplicableRules as LintIssueType[] | undefined,
    );
    return PASSED_RULES.filter(
      (rule) => !presentRules.has(rule) && !notApplicable.has(rule),
    );
  }, [presentRules, healthReport]);

  const deepTask = deepTaskId ? tasks.find((task) => task.id === deepTaskId) ?? null : null;

  // Load ignored-issue entries + the persisted deep-lint report when the
  // background task lands.
  useEffect(() => {
    const unobserve = observeProjectResources(
      { projectId, rootPath },
      ["lint-ignores", "lint-history"],
    );
    void ensureIgnores({ projectId, projectRootPath: rootPath });
    return unobserve;
  }, [projectId, rootPath, ensureIgnores]);

  useEffect(() => {
    const selectionReportId = useLintStore.getState().agentRepairSelectionReportId;
    if (selectionReportId && selectionReportId !== healthReport?.reportId) {
      useLintStore.getState().clearAgentRepairSelection();
    }
  }, [projectId, rootPath, healthReport?.reportId]);

  useEffect(() => {
    const state = useLintStore.getState();
    if (!state.agentRepairProjectId || state.agentRepairProjectId !== projectId) return;
    const capturedIdentity = state.agentRepairCanonicalIdentityKey && state.agentRepairIdentityRevision
      ? `${state.agentRepairCanonicalIdentityKey}\0${state.agentRepairIdentityRevision}`
      : null;
    if (capturedIdentity !== authorityIdentity) invalidateAgentLintRepairIdentity();
  }, [authorityIdentity, invalidateAgentLintRepairIdentity, projectId, rootPath]);

  useEffect(() => {
    let cancelled = false;
    void ensureHistory({ projectId, projectRootPath: rootPath }).then((entries) => {
      const hasLoadedReport =
        useLintStore.getState().localReport ||
        useLintStore.getState().deepReport ||
        useLintStore.getState().healthReport;
      if (cancelled || hasLoadedReport) return;
      const latest = entries[0];
      if (latest) {
        void openHistoryReport({ projectId, projectRootPath: rootPath, id: latest.id });
      }
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, rootPath, ensureHistory, openHistoryReport]);

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
    requestWorkflowLaunch({
      projectId,
      projectRootPath: rootPath,
      kind: "update_wiki",
      origin: "lint",
      scopePreset: null,
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
    requestWorkflowLaunch({
      projectId,
      projectRootPath: rootPath,
      kind: "health_check",
      origin: "lint",
      scopePreset: { kind: "health_check", mode: "local_quick" },
    });
  };

  const handleStartDeep = () => {
    requestWorkflowLaunch({
      projectId,
      projectRootPath: rootPath,
      kind: "health_check",
      origin: "lint",
      scopePreset: { kind: "health_check", mode: "complete" },
    });
  };

  const handleCancelDeep = () => {
    if (!deepTaskId) return;
    void cancelTaskRequest(deepTaskId);
  };

  const handleToggleRepairSelection = (issueId: string, selected: boolean) => {
    if (!healthReport || agentRepairPreparation || agentRepairPending) return;
    const next = new Set(agentRepairSelection);
    if (selected) next.add(issueId);
    else next.delete(issueId);
    setAgentRepairSelection(healthReport.reportId, [...next]);
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
    if (fixConfirm || batchRunning || fixApplying || hasPendingBatchConfirmations || agentRepairPending || agentRepairPreparation) return;
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
    if (fixConfirm || batchRunning || fixApplying || hasPendingBatchConfirmations || agentRepairPending || agentRepairPreparation) return;
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
    <div ref={layoutRef} className="lint-view-layout" style={layoutStyle}>
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
              Boolean(fixConfirm) ||
              agentRepairPending ||
              Boolean(agentRepairPreparation)
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
              disabled={
                loadingLocal ||
                batchRunning ||
                fixApplying ||
                hasPendingBatchConfirmations ||
                Boolean(fixConfirm) ||
                agentRepairPending ||
                Boolean(agentRepairPreparation)
              }
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
              Boolean(fixConfirm) ||
              agentRepairPending ||
              Boolean(agentRepairPreparation)
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
          <div className="flex items-center justify-between gap-3 border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            <span>{error}</span>
            <button
              type="button"
              className="btn btn--secondary btn--sm"
              onClick={() => void ensureIgnores({ projectId, projectRootPath: rootPath })}
            >
              {t("workflows.action.retry")}
            </button>
          </div>
        ) : null}

        <AgentLintRepairPanel
          report={healthReport}
          agentRouteConfigured={currentProject.agentRoute === "agent"}
          eligibleFindings={eligibleAgentFindings}
          selectedFindingIds={agentRepairSelection}
          preparation={agentRepairPreparation}
          pending={agentRepairPending}
          errorCode={agentRepairErrorCode}
          onPrepare={() => {
            if (healthReport) void prepareAgentLintRepair(projectId, rootPath, healthReport.reportId);
          }}
          onConfirm={() => void confirmAgentLintRepairStart()}
          onCancel={() => void cancelAgentLintRepairPreparation()}
        />

        <LintHistoryList
          entries={history}
          activeId={activeHistoryId}
          loading={historyLoading}
          error={historyError}
          onOpen={(id) =>
            void openHistoryReport({ projectId, projectRootPath: rootPath, id })
          }
          onRetry={() => void ensureHistory({ projectId, projectRootPath: rootPath })}
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
                        hasPendingBatchConfirmations ||
                        agentRepairPending ||
                        Boolean(agentRepairPreparation)
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

        {localReport || healthReport ? (
          <LintSummaryCards issues={modeIssues} passedCount={passedRules.length} />
        ) : null}

        <LintIssueList
          issues={modeIssues}
          selectedIssueId={selectedIssueId}
          actionsDisabled={
            loadingLocal || batchRunning || fixApplying || hasPendingBatchConfirmations
            || agentRepairPending || Boolean(agentRepairPreparation)
          }
          onSelect={selectIssue}
          onApplyFix={handleApplyFix}
          repairSelection={repairSelectionSet}
          repairEligibleIds={eligibleAgentFindingIds}
          onToggleRepairSelection={handleToggleRepairSelection}
        />

        {localReport || healthReport ? <LintPassedSection passedRules={passedRules} /> : null}

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
        value={lintDetailsWidth}
        previewTargetRef={layoutRef}
        previewCssVariable="--lint-details-w-current"
        onCommit={(value) => setPaneSize("lintDetails", value)}
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
            agentRepairPending ||
            Boolean(agentRepairPreparation) ||
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

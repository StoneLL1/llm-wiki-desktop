import { type CSSProperties, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertCircle, History, EyeOff, Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import { useRouteScrollRestoration } from "../../hooks/useRouteScrollRestoration";
import { selectAllIssues, useLintStore } from "../../stores/lintStore";
import { observeProjectResources } from "../../stores/projectScope";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { captureProjectScope, isProjectScopeCurrent } from "../../stores/projectScope";
import { isAgentLintRepairEligible } from "../../types/lint";
import type { LintIssue, LintIssueType } from "../../types/lint";
import { AgentLintRepairPanel } from "./AgentLintRepairPanel";
import { LintBatchConfirmDialog } from "./LintBatchConfirmDialog";
import { HealthCheckReportSummary } from "./HealthCheckReportSummary";
import { LintManagementPanel } from "./LintManagementPanel";
import { LintIssueDetails } from "./LintIssueDetails";
import { LintIssueList } from "./LintIssueList";
import { LintPassedSection } from "./LintPassedSection";
import { LintSummaryCards } from "./LintSummaryCards";
import { LintTaskStatus } from "./LintTaskStatus";
import { LintGitNotice } from "./LintGitNotice";
import { useLintGitPreflight } from "./useLintGitPreflight";

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
  const selectedIssueId = useLintStore((state) => state.selectedIssueId);
  const fixStatus = useLintStore((state) => state.fixStatus);
  const fixConfirm = useLintStore((state) => state.fixConfirm);
  const error = useLintStore((state) => state.error);
  const errorCode = useLintStore((state) => state.errorCode);
  const errorDetails = useLintStore((state) => state.errorDetails);
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


  const { projectId, rootPath } = currentProject;
  const gitPreflight = useLintGitPreflight(projectId, rootPath);
  const resumeIntent = useRef<{ scope: ReturnType<typeof captureProjectScope>; issue?: LintIssue } | null>(null);
  const [lastOperation, setLastOperation] = useState<{ projectKey: string; id: string } | null>(null);
  const onGitConfigured = useCallback(() => {
    gitPreflight.clearError();
    const state = useLintStore.getState();
    if (/^(GIT_|VERSION_)/.test(state.errorCode ?? "")) useLintStore.setState({ error: null, errorCode: null, errorDetails: null });
  }, [gitPreflight.clearError]);
  const gitError = gitPreflight.error ?? (error && /^(GIT_|VERSION_)/.test(errorCode ?? "") ? { code: errorCode, message: error, details: errorDetails } : null);
  useEffect(() => { resumeIntent.current = null; }, [projectId, rootPath]);
  const layoutRef = useRef<HTMLDivElement>(null);
  const issueListScrollRef = useRouteScrollRestoration(projectId, rootPath, "lint:issues");
  const authorityIdentity = authority?.projectId === projectId
    ? `${authority.canonicalIdentityKey}\0${authority.identityRevision}`
    : null;
  const layoutStyle = {
    "--lint-details-w-current": `${lintDetailsWidth}px`,
  } as CSSProperties;
  const allIssues = useMemo(() => selectAllIssues({ healthReport, localReport, deepReport, ignores })
    .map((issue) => ({
      ...issue,
      origins: issue.origins ?? (!localReport ? healthReport?.findingOrigins[issue.id] : undefined) ?? [issue.source],
    })), [healthReport, localReport, deepReport, ignores]);
  const localIssues = useMemo(() => allIssues.filter((issue) => issue.origins.includes("local")), [allIssues]);
  const deepIssues = useMemo(() => allIssues.filter((issue) => issue.origins.includes("agent")), [allIssues]);
  const modeIssues = useMemo(() => {
    if (mode === "local") return localIssues;
    if (mode === "agent") return deepIssues;
    return allIssues;
  }, [mode, localIssues, deepIssues, allIssues]);
  const [query, setQuery] = useState("");
  const filteredIssues = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return needle ? modeIssues.filter((issue) =>
      [issue.path, issue.target, issue.message, t(`lint.issueType.${issue.issueType}`)]
        .some((value) => value?.toLocaleLowerCase().includes(needle))) : modeIssues;
  }, [modeIssues, query, t]);
  const selectedIssue = fixConfirm?.issue ?? (selectedIssueId
    ? filteredIssues.find((issue) => issue.id === selectedIssueId) ?? null
    : null);
  const hasReport = Boolean(localReport || healthReport || deepReport);
  const projectKey = `${projectId}\0${rootPath}`;
  const [management, setManagement] = useState<{ projectKey: string; view: "history" | "ignores" } | null>(null);
  const managementView = !fixConfirm && management?.projectKey === projectKey ? management.view : null;
  const managementEpoch = useRef(0);
  const historyButtonRef = useRef<HTMLButtonElement>(null);
  const ignoresButtonRef = useRef<HTMLButtonElement>(null);
  const openManagement = (view: "history" | "ignores") => {
    managementEpoch.current += 1;
    setManagement({ projectKey, view });
  };
  const closeManagement = () => {
    managementEpoch.current += 1;
    setManagement(null);
    selectIssue(null);
    const trigger = managementView === "history" ? historyButtonRef : ignoresButtonRef;
    requestAnimationFrame(() => trigger.current?.focus());
  };
  const handleOpenReport = async (id: string) => {
    const scope = captureProjectScope();
    const epoch = managementEpoch.current;
    const isCurrent = () => isProjectScopeCurrent(scope) && epoch === managementEpoch.current;
    const report = await openHistoryReport({ projectId, projectRootPath: rootPath, id }, isCurrent);
    if (report && isCurrent()) {
      setQuery("");
      closeManagement();
    }
  };
  const eligibleAgentFindings = useMemo(
    () => !localReport ? healthReport?.issues.filter((issue) => isAgentLintRepairEligible(issue, healthReport)) ?? [] : [],
    [healthReport, localReport],
  );
  const eligibleAgentFindingIds = useMemo(
    () => new Set(eligibleAgentFindings.map((issue) => issue.id)),
    [eligibleAgentFindings],
  );
  const repairSelectionSet = useMemo(() => new Set(agentRepairSelection), [agentRepairSelection]);

  const localOperationPending = loadingLocal || batchRunning || fixApplying;
  const actionsDisabled = localOperationPending || gitPreflight.checking || hasPendingBatchConfirmations || Boolean(fixConfirm)
    || agentRepairPending || Boolean(agentRepairPreparation);

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [ignoringId, setIgnoringId] = useState<string | null>(null);
  const [removingIgnoreKey, setRemovingIgnoreKey] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => { setQuery(""); }, [projectId, rootPath]);
  const selectFinding = (id: string) => {
    if (!fixConfirm) {
      managementEpoch.current += 1;
      setManagement(null);
      selectIssue(id);
    }
  };
  const closeDetails = () => {
    selectIssue(null);
    // Restore keyboard focus to the row that opened the compact details pane.
    requestAnimationFrame(() => {
      const rows = layoutRef.current?.querySelectorAll<HTMLButtonElement>("[data-lint-issue-id]");
      const row = Array.from(rows ?? []).find((entry) => entry.dataset.lintIssueId === selectedIssueId);
      (row ?? layoutRef.current?.querySelector<HTMLInputElement>("input[type=search]"))?.focus();
    });
  };

  const autoFixable = useMemo(
    () => filteredIssues.filter((issue) => issue.fixability !== "none" && issue.scanHash),
    [filteredIssues],
  );

  const presentRules = useMemo(
    () => new Set((localReport?.issues ?? healthReport?.issues ?? [])
      .map((issue) => issue.issueType)),
    [localReport, healthReport],
  );
  const passedRules = useMemo(() => {
    if (!localReport && !healthReport) return [];
    if ((localReport?.scannedPages ?? healthReport?.coverage.scannedPages ?? 0) === 0) return [];
    const notApplicable = new Set(
      healthReport?.coverage.notApplicableRules as LintIssueType[] | undefined,
    );
    const ignoredRules = new Set(ignores.map((entry) => entry.rule));
    return PASSED_RULES.filter(
      (rule) => !presentRules.has(rule) && !notApplicable.has(rule) && !ignoredRules.has(rule),
    );
  }, [presentRules, healthReport, localReport, ignores]);

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

  const handleToggleRepairSelection = (issueId: string, selected: boolean) => {
    if (!healthReport || agentRepairPreparation || agentRepairPending) return;
    const next = new Set(agentRepairSelection);
    if (selected) next.add(issueId);
    else next.delete(issueId);
    setAgentRepairSelection(healthReport.reportId, [...next]);
  };

  const handleApplyFix = async (issue: LintIssue) => {
    resumeIntent.current = { scope: captureProjectScope(), issue };
    if (!await gitPreflight.check()) return;
    resumeIntent.current = null;
    const scope = captureProjectScope();
    // Fixes must use the immutable scan snapshot. Reading the live page here
    // would silently replace the report baseline after an external edit.
    const expectedHash = issue.fixability === "safe" ? issue.scanHash ?? null : null;
    const outcome = await applyFix(projectId, rootPath, issue, expectedHash);
    if (outcome?.kind === "applied" && isProjectScopeCurrent(scope)) {
      if (outcome.operationId) setLastOperation({ projectKey: `${projectId}\0${rootPath}`, id: outcome.operationId });
      refreshAfterFix(true);
    }
  };

  const handleConfirmHighRisk = (expectedHash: string) => {
    const scope = captureProjectScope();
    void confirmHighRisk(projectId, rootPath, expectedHash).then((outcome) => {
      if (outcome?.kind === "applied" && isProjectScopeCurrent(scope)) {
      if (outcome.operationId) setLastOperation({ projectKey: `${projectId}\0${rootPath}`, id: outcome.operationId });
      refreshAfterFix(true);
    }
    });
  };

  const handleIgnore = (issue: LintIssue) => {
    if (actionsDisabled) return;
    setNotice(null);
    setIgnoringId(issue.id);
    const scope = captureProjectScope();
    void addIgnore({
      projectId,
      projectRootPath: rootPath,
      path: issue.path,
      rule: issue.issueType,
    }).then((ok) => {
      if (!isProjectScopeCurrent(scope)) return;
      setIgnoringId(null);
      if (ok) {
        setNotice(t("lint.plan.ignored"));
        selectIssue(null);
        void runLocalLint(projectId, rootPath);
      }
    });
  };

  const handleRemoveIgnore = (path: string, rule: LintIssueType) => {
    if (actionsDisabled) return;
    const key = `${path}:${rule}`;
    setNotice(null);
    setRemovingIgnoreKey(key);
    const scope = captureProjectScope();
    void removeIgnore({
      projectId,
      projectRootPath: rootPath,
      path,
      rule,
    }).then((ok) => {
      if (!isProjectScopeCurrent(scope)) return;
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
        if (outcome.operationId) setLastOperation({ projectKey: `${projectId}\0${rootPath}`, id: outcome.operationId });
        const parts: string[] = [];
        if (outcome.applied.length > 0) {
          parts.push(
            t("lint.batch.applied", {
              count: outcome.applied.length,
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
      disabled={confirmOpen || Boolean(fixConfirm)}
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
    <div ref={layoutRef} className={`lint-view-layout ${selectedIssue || managementView ? "has-selection" : ""}`} style={layoutStyle}>
      <div className="lint-feedback">
        {notice ? (
          <div className="border-b border-[var(--accent-border)] bg-[var(--accent-soft)] px-4 py-2 text-[12px] text-[var(--accent-hover)]">
            {notice}
          </div>
        ) : null}
        {lastOperation?.projectKey === `${projectId}\0${rootPath}` ? (
          <div className="border-b border-[var(--border)] px-4 py-2">
            <button type="button" className="btn btn--ghost btn--sm" onClick={() => useNavigationStore.getState().openVersionHistory(lastOperation.id)}>{t("versions.viewChanges")}</button>
          </div>
        ) : null}
        {gitError ? (
          <LintGitNotice
            error={gitError}
            checking={gitPreflight.checking}
            onConfigure={() => {
              const intent = resumeIntent.current;
              void gitPreflight.enable().then((ready) => {
                if (!ready) return;
                onGitConfigured();
                if (!intent || !isProjectScopeCurrent(intent.scope)) return;
                resumeIntent.current = null;
                if (intent.issue) void handleApplyFix(intent.issue);
                else setConfirmOpen(true);
              });
            }}
            onRefresh={() => {
              void gitPreflight.check().then((ready) => { if (ready) onGitConfigured(); });
            }}
          />
        ) : error ? (
          <div className="flex items-center justify-between gap-3 border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            <span>{error}</span>
            <button
              type="button"
              className="btn btn--secondary btn--sm"
              onClick={() => {
                void ensureIgnores({ projectId, projectRootPath: rootPath });
                void runLocalLint(projectId, rootPath, { preserveBatchConfirmations: hasPendingBatchConfirmations });
              }}
            >
              {t("lint.git.refreshReport")}
            </button>
          </div>
        ) : null}

      </div>
      <div className="lint-view__list-pane">
        <div className="view-toolbar lint-toolbar border-b border-[var(--border)] px-4">
          <button
            type="button"
            onClick={handleRunLocal}
            disabled={actionsDisabled}
            className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
          >
            {loadingLocal ? "…" : t("lint.actions.runLocal")}
          </button>
          <button
            type="button"
            onClick={handleStartDeep}
            disabled={actionsDisabled}
            className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
          >
            {t("lint.actions.deepLint")}
          </button>
          <button
            type="button"
            onClick={() => {
              resumeIntent.current = { scope: captureProjectScope() };
              void gitPreflight.check().then((ready) => { if (ready) { resumeIntent.current = null; setConfirmOpen(true); } });
            }}
            disabled={autoFixable.length === 0 || actionsDisabled}
            className="ml-auto h-[28px] rounded-[var(--radius-md)] lint-primary bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
          >
            {gitPreflight.checking ? t("lint.git.checking") : batchRunning ? "…" : t("lint.actions.autoFix", { count: autoFixable.length })}
          </button>
        </div>

        <LintTaskStatus key={`${projectId}\0${rootPath}\0${authorityIdentity}`} />

        <div className="lint-context">
        {healthReport ? <HealthCheckReportSummary report={healthReport} /> : null}

        <AgentLintRepairPanel
          report={healthReport}
          locallyRefreshed={Boolean(localReport)}
          agentRouteConfigured={currentProject.agentRoute === "agent"}
          eligibleFindings={eligibleAgentFindings}
          selectedFindingIds={agentRepairSelection}
          preparation={agentRepairPreparation}
          pending={agentRepairPending}
          disabled={localOperationPending || hasPendingBatchConfirmations || Boolean(fixConfirm)}
          errorCode={agentRepairErrorCode}
          onPrepare={() => {
            if (healthReport) void prepareAgentLintRepair(projectId, rootPath, healthReport.reportId);
          }}
          onConfirm={() => void confirmAgentLintRepairStart()}
          onCancel={() => void cancelAgentLintRepairPreparation()}
        />


        </div>

        {hasReport ? (
          <LintSummaryCards issues={modeIssues} passedCount={passedRules.length} />
        ) : null}

        {hasReport ? <div className="lint-filters">
          <div className="seg" role="group" aria-label={t("view.lint.paneTitle")}>
            {segButton("all", t("lint.mode.all"), allIssues.length)}
            {segButton("local", t("lint.mode.local"), localIssues.length)}
            {segButton("agent", t("lint.mode.agent"), deepIssues.length)}
          </div>
          <div className="lint-search">
            <Search size={14} aria-hidden="true" />
            <input type="search" disabled={confirmOpen || Boolean(fixConfirm)} value={query} onChange={(event) => setQuery(event.target.value)}
              aria-label={t("lint.search")} placeholder={t("lint.search")} />
            {query ? <button type="button" disabled={confirmOpen || Boolean(fixConfirm)} onClick={() => setQuery("")} aria-label={t("lint.search.clear")} title={t("lint.search.clear")}><X size={13} aria-hidden="true" /></button> : null}
          </div>
        </div> : null}

        <LintIssueList
          scrollRef={issueListScrollRef}
          issues={filteredIssues}
          selectionDisabled={Boolean(fixConfirm)}
          emptyMessage={t(loadingLocal || historyLoading && !hasReport ? "lint.empty.loading" : !hasReport ? "lint.empty.initial" : query.trim() || mode !== "all" ? "lint.empty.filtered" : "lint.empty.clean")}
          selectedIssueId={selectedIssueId}
          actionsDisabled={actionsDisabled}
          onSelect={selectFinding}
          onApplyFix={handleApplyFix}
          repairSelection={repairSelectionSet}
          repairEligibleIds={eligibleAgentFindingIds}
          onToggleRepairSelection={handleToggleRepairSelection}
        />

        <footer className="lint-footer">
          {localReport || healthReport ? <LintPassedSection passedRules={passedRules} /> : null}
          <div className="lint-footer__tools">
            <button ref={historyButtonRef} type="button" className="btn btn--ghost btn--sm"
              aria-pressed={managementView === "history"} aria-label={t("lint.history.title")}
              disabled={Boolean(fixConfirm) || confirmOpen} onClick={() => openManagement("history")}>
              {historyError ? <AlertCircle size={14} className="text-[var(--danger-text)]" aria-hidden="true" /> : <History size={14} aria-hidden="true" />}
              {t("lint.history.title")}
              {historyError ? <span>{t("lint.history.failed")}</span> : null}
            </button>
            <button ref={ignoresButtonRef} type="button" className="btn btn--ghost btn--sm"
              aria-pressed={managementView === "ignores"} aria-label={t("lint.ignores.title")}
              disabled={Boolean(fixConfirm) || confirmOpen} onClick={() => openManagement("ignores")}>
              <EyeOff size={14} aria-hidden="true" />{t("lint.ignores.title")}
              {ignores.length > 0 ? <span>{ignores.length}</span> : null}
            </button>
          </div>
        </footer>

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
        direction={-1}
        previewTargetRef={layoutRef}
        previewCssVariable="--lint-details-w-current"
        onCommit={(value) => setPaneSize("lintDetails", value)}
        onReset={() => resetPaneSize("lintDetails")}
      />

      {managementView ? (
        <LintManagementPanel key={`${projectKey}:${managementView}`} view={managementView}
          history={history} activeHistoryId={activeHistoryId} historyLoading={historyLoading}
          historyError={historyError} ignores={ignores} removingIgnoreKey={removingIgnoreKey}
          disabled={actionsDisabled || confirmOpen} onOpenReport={handleOpenReport}
          onRetryHistory={() => void ensureHistory({ projectId, projectRootPath: rootPath })}
          onRestore={handleRemoveIgnore} onClose={closeManagement} />
      ) : <LintIssueDetails
        onBack={fixConfirm ? undefined : closeDetails}
        issue={selectedIssue}
        fixStatus={selectedIssue ? fixStatus[selectedIssue.id] ?? "idle" : "idle"}
        fixConfirm={fixConfirm}
        ignoring={selectedIssue ? ignoringId === selectedIssue.id : false}
        actionsDisabled={localOperationPending || gitPreflight.checking || agentRepairPending || Boolean(agentRepairPreparation)
          || (hasPendingBatchConfirmations && !fixConfirm)}
        safetyPrefs={safetyPrefs}
        onSafetyPrefsChange={setSafetyPrefs}
        onApplyFix={handleApplyFix}
        onConfirmHighRisk={handleConfirmHighRisk}
        onCancelHighRisk={cancelHighRisk}
        onIgnore={handleIgnore}
      />}

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

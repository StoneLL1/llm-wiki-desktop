import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

import { useLintStore } from "../../stores/lintStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { isTerminalStatus } from "../../types/task";
import type { LintIssue, LintIssueType, LintRoutePreference } from "../../types/lint";
import type { WikiPageContent } from "../../types/wiki";
import { LintBatchConfirmDialog } from "./LintBatchConfirmDialog";
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
  const batchConfirmations = useLintStore((state) => state.batchConfirmations);
  const safetyPrefs = useLintStore((state) => state.safetyPrefs);

  const runLocalLint = useLintStore((state) => state.runLocalLint);
  const startDeepLint = useLintStore((state) => state.startDeepLint);
  const clearDeepTask = useLintStore((state) => state.clearDeepTask);
  const loadDeepReport = useLintStore((state) => state.loadDeepReport);
  const selectIssue = useLintStore((state) => state.selectIssue);
  const setMode = useLintStore((state) => state.setMode);
  const setSafetyPrefs = useLintStore((state) => state.setSafetyPrefs);
  const loadIgnores = useLintStore((state) => state.loadIgnores);
  const addIgnore = useLintStore((state) => state.addIgnore);
  const applyFix = useLintStore((state) => state.applyFix);
  const applyFixesBatch = useLintStore((state) => state.applyFixesBatch);
  const openBatchConfirmation = useLintStore((state) => state.openBatchConfirmation);
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
  const [notice, setNotice] = useState<string | null>(null);

  const autoFixable = useMemo(
    () => modeIssues.filter((issue) => issue.fixability !== "none"),
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
    if (deepTask && isTerminalStatus(deepTask.status)) {
      void loadDeepReport({ projectId, projectRootPath: rootPath, taskId: deepTask.id });
      clearDeepTask();
    }
  }, [deepTask, projectId, rootPath, loadDeepReport, clearDeepTask]);

  const triggerRecompile = () => {
    void invoke<{ id: string }>("start_wiki_compile", {
      request: {
        projectId,
        projectRootPath: rootPath,
        route: ROUTE_PREFERENCE,
        agent: null,
        provider: null,
      },
    })
      .then((task) => {
        if (task) {
          void invoke("get_task", { request: { taskId: task.id } }).then((fetched) => {
            if (fetched) upsertTask(fetched as never);
          });
          openTaskDrawer(task.id);
        }
      })
      .catch(() => {
        /* recompile is best-effort; failures surface in the task drawer */
      });
  };

  const refreshAfterFix = (applied: boolean) => {
    void runLocalLint(projectId, rootPath);
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
    // Safe fixes require the page's current hash as an optimistic-lock
    // baseline (the backend rejects safe fixes without it). High-risk fixes
    // route through the confirm flow, which resolves the hash separately.
    let expectedHash: string | null = null;
    if (issue.fixability === "safe") {
      try {
        const page = await invoke<WikiPageContent>("read_wiki_page", {
          request: { projectId, projectRootPath: rootPath, relativePath: issue.path },
        });
        expectedHash = page.meta.hash;
      } catch {
        expectedHash = null; // backend will reject with LINT_FIX_HASH_REQUIRED
      }
    }
    const outcome = await applyFix(projectId, rootPath, issue, expectedHash);
    if (outcome?.kind === "applied") refreshAfterFix(true);
  };

  const handleConfirmHighRisk = (expectedHash: string) => {
    void confirmHighRisk(projectId, rootPath, expectedHash).then((outcome) => {
      if (outcome?.kind === "applied") refreshAfterFix(true);
    });
  };

  const handleIgnore = (issue: LintIssue) => {
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

  // Resolve page hashes for safe-fixable issues (optimistic-lock baseline),
  // then run the batch under one shared Git checkpoint.
  const handleBatchConfirm = () => {
    setConfirmOpen(false);
    const safePaths = new Set(
      autoFixable.filter((issue) => issue.fixability === "safe").map((issue) => issue.path),
    );
    void Promise.all(
      [...safePaths].map(async (path) => {
        try {
          const page = await invoke<WikiPageContent>("read_wiki_page", {
            request: { projectId, projectRootPath: rootPath, relativePath: path },
          });
          return [path, page.meta.hash] as const;
        } catch {
          return [path, ""] as const;
        }
      }),
    )
      .then((entries) => Object.fromEntries(entries.filter(([, hash]) => hash)))
      .then((expectedHashes) =>
        applyFixesBatch({
          projectId,
          projectRootPath: rootPath,
          issues: autoFixable,
          expectedHashes,
        }),
      )
      .then((outcome) => {
        if (!outcome) return;
        const parts: string[] = [];
        if (outcome.applied.length > 0) {
          parts.push(
            t("lint.batch.applied", {
              count: outcome.applied.length,
              hash: outcome.checkpoint ?? "—",
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
        void runLocalLint(projectId, rootPath);
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
    <div className="lint-view-layout">
      <div className="flex min-w-0 flex-col border-r border-[var(--border)]">
        <div className="view-toolbar border-b border-[var(--border)] px-4">
          <div className="seg" role="group" aria-label={t("view.lint.paneTitle")}>
            {segButton("all", t("lint.mode.all"), allIssues.length)}
            {segButton("local", t("lint.mode.local"), localIssues.length)}
            {segButton("agent", t("lint.mode.agent"), deepIssues.length)}
          </div>
          <button
            type="button"
            onClick={handleRunLocal}
            disabled={loadingLocal}
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
            disabled={autoFixable.length === 0 || batchRunning}
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

        {localReport ? (
          <LintSummaryCards issues={modeIssues} passedCount={passedRules.length} />
        ) : null}

        <LintIssueList
          issues={modeIssues}
          selectedIssueId={selectedIssueId}
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

      <LintIssueDetails
        issue={selectedIssue}
        fixStatus={selectedIssue ? fixStatus[selectedIssue.id] ?? "idle" : "idle"}
        fixConfirm={fixConfirm}
        projectId={projectId}
        rootPath={rootPath}
        ignoring={selectedIssue ? ignoringId === selectedIssue.id : false}
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

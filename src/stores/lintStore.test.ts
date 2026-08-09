import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  LintBatchOutcome,
  LintFixOutcome,
  LintHistoryFile,
  LintIgnoreFile,
  LintIssue,
  LintReport,
  PersistedLintReport,
} from "../types/lint";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import { selectAllIssues, useLintStore } from "./lintStore";

const localIssue = (overrides: Partial<LintIssue> = {}): LintIssue => ({
  id: "dead_link:wiki/a.md:ghost",
  source: "local",
  severity: "warning",
  issueType: "dead_link",
  path: "wiki/a.md",
  message: "Unresolved wikilink",
  evidence: "[[ghost]]",
  target: "ghost",
  fixability: "high_risk",
  ...overrides,
});

const report = (overrides: Partial<LintReport> = {}): LintReport => ({
  issues: [localIssue()],
  generatedAt: "2026-06-20T00:00:00Z",
  scannedPages: 1,
  ...overrides,
});

const PROJECT = { projectId: "p", rootPath: "/x" };

beforeEach(() => {
  invokeMock.mockReset();
  useLintStore.getState().reset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("lintStore", () => {
  it("does not cancel an existing confirmation for guarded workflow-result navigation", async () => {
    useLintStore.setState({
      fixConfirm: {
        pendingAction: { id: "existing-confirmation" },
      } as never,
    });

    const opened = await useLintStore.getState().openHistoryReport(
      { projectId: "p", projectRootPath: "/x", id: "report-a" },
      () => true,
      true,
    );

    expect(opened).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(useLintStore.getState().fixConfirm).not.toBeNull();
  });

  it("loads the local lint report", async () => {
    invokeMock.mockResolvedValueOnce(report());
    await useLintStore.getState().runLocalLint(PROJECT.projectId, PROJECT.rootPath);
    expect(useLintStore.getState().localReport?.issues).toHaveLength(1);
    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("run_local_lint");
  });

  it("clears stale selection and confirmations before a local rerun", async () => {
    const issue = localIssue();
    useLintStore.setState({
      selectedIssueId: issue.id,
      batchConfirmations: [],
    });
    invokeMock.mockResolvedValueOnce(report({ issues: [] }));
    await useLintStore.getState().runLocalLint(PROJECT.projectId, PROJECT.rootPath);
    expect(useLintStore.getState().selectedIssueId).toBeNull();
    expect(useLintStore.getState().fixConfirm).toBeNull();
    expect(useLintStore.getState().batchConfirmations).toHaveLength(0);
  });

  it("blocks an ordinary local rerun while batch confirmations remain pending", async () => {
    const issue = localIssue();
    useLintStore.setState({
      batchConfirmations: [
        {
          issue,
          pendingAction: {
            id: "pending-batch-guard",
            actionType: "agent_auto_fix",
            title: "fix",
            message: "fix",
            riskLevel: "high",
            affectedPaths: [issue.path],
            preview: null,
            expiresAt: null,
          },
        },
      ],
    });

    await useLintStore.getState().runLocalLint(PROJECT.projectId, PROJECT.rootPath);

    expect(invokeMock).not.toHaveBeenCalled();
    expect(useLintStore.getState().batchConfirmations).toHaveLength(1);
  });

  it("preserves pending high-risk state during a protected batch rescan", async () => {
    const issue = localIssue();
    useLintStore.setState({
      fixConfirm: {
        issue,
        pendingAction: {
          id: "pending-batch",
          actionType: "agent_auto_fix",
          title: "fix",
          message: "fix",
          riskLevel: "high",
          affectedPaths: [issue.path],
          preview: null,
          expiresAt: null,
        },
        expectedHash: "hash",
      },
      batchConfirmations: [
        {
          issue,
          pendingAction: {
            id: "pending-batch",
            actionType: "agent_auto_fix",
            title: "fix",
            message: "fix",
            riskLevel: "high",
            affectedPaths: [issue.path],
            preview: null,
            expiresAt: null,
          },
        },
      ],
    });
    invokeMock.mockResolvedValueOnce(report({ issues: [] }));

    await useLintStore.getState().runLocalLint(PROJECT.projectId, PROJECT.rootPath, {
      preserveBatchConfirmations: true,
    });

    expect(useLintStore.getState().fixConfirm?.pendingAction.id).toBe("pending-batch");
    expect(useLintStore.getState().batchConfirmations).toHaveLength(1);
  });

  it("keeps a high-risk confirmation visible when cancellation IPC fails", async () => {
    const issue = localIssue();
    useLintStore.setState({
      fixConfirm: {
        issue,
        pendingAction: {
          id: "pending-cancel",
          actionType: "agent_auto_fix",
          title: "fix",
          message: "fix",
          riskLevel: "high",
          affectedPaths: [issue.path],
          preview: null,
          expiresAt: null,
        },
        expectedHash: "hash",
      },
    });
    invokeMock.mockRejectedValueOnce(new Error("temporary IPC failure"));

    await useLintStore.getState().cancelHighRisk();

    expect(useLintStore.getState().fixConfirm?.pendingAction.id).toBe("pending-cancel");
    expect(useLintStore.getState().error).toContain("temporary IPC failure");
  });

  it("clears a high-risk confirmation only after cancellation succeeds", async () => {
    const issue = localIssue();
    useLintStore.setState({
      fixConfirm: {
        issue,
        pendingAction: {
          id: "pending-cancel-success",
          actionType: "agent_auto_fix",
          title: "fix",
          message: "fix",
          riskLevel: "high",
          affectedPaths: [issue.path],
          preview: null,
          expiresAt: null,
        },
        expectedHash: "hash",
      },
    });
    invokeMock.mockResolvedValueOnce({});

    await useLintStore.getState().cancelHighRisk();

    expect(useLintStore.getState().fixConfirm).toBeNull();
    expect(invokeMock.mock.calls[0][0]).toBe("confirm_pending_action");
  });

  it("startDeepLint stores the returned task id", async () => {
    invokeMock.mockResolvedValueOnce({ id: "task-1" });
    const taskId = await useLintStore.getState().startDeepLint(
      PROJECT.projectId,
      PROJECT.rootPath,
      "auto",
    );
    expect(taskId).toBe("task-1");
    expect(useLintStore.getState().deepTaskId).toBe("task-1");
    expect(useLintStore.getState().runningDeep).toBe(true);
    expect(invokeMock.mock.calls[0][1].request.route).toBe("auto");
  });

  it("selectAllIssues merges local and deep findings", () => {
    useLintStore.setState({
      localReport: report(),
      deepReport: {
        issues: [localIssue({ id: "duplicate_topic:wiki/b.md", source: "agent", issueType: "duplicate_topic" })],
        rawOutput: "",
        generatedAt: "2026-06-20T00:00:00Z",
      },
    });
    expect(selectAllIssues(useLintStore.getState())).toHaveLength(2);
  });

  it("applyFix marks a safe fix as applied and forwards the optimistic-lock hash", async () => {
    const issue = localIssue({ fixability: "safe" });
    const outcome: LintFixOutcome = {
      kind: "applied",
      affectedPaths: ["wiki/a.md"],
      checkpoint: "abc",
    };
    invokeMock.mockResolvedValueOnce(outcome);
    await useLintStore.getState().applyFix(PROJECT.projectId, PROJECT.rootPath, issue, "hash-safe");
    expect(useLintStore.getState().fixStatus[issue.id]).toBe("applied");
    expect(useLintStore.getState().fixConfirm).toBeNull();
    expect(invokeMock.mock.calls[0][1].request.expectedHash).toBe("hash-safe");
  });

  it("applyFix surfaces needs_confirmation as an inline confirm", async () => {
    const issue = localIssue({ fixability: "high_risk" });
    const outcome: LintFixOutcome = {
      kind: "needs_confirmation",
      affectedPaths: [],
      pendingAction: {
        id: "pa-1",
        actionType: "agent_auto_fix",
        title: "Remove dead link",
        message: "Removes an unresolved wikilink",
        riskLevel: "high",
        affectedPaths: ["wiki/a.md"],
        preview: null,
        expiresAt: null,
      },
    };
    invokeMock.mockResolvedValueOnce(outcome);
    await useLintStore.getState().applyFix(PROJECT.projectId, PROJECT.rootPath, issue);
    expect(useLintStore.getState().fixConfirm?.issue.id).toBe(issue.id);
    expect(useLintStore.getState().fixConfirm?.pendingAction.id).toBe("pa-1");
  });

  it("confirmHighRisk re-invokes with confirmHighRisk and the resolved hash", async () => {
    const issue = localIssue({ fixability: "high_risk" });
    useLintStore.setState({
      fixConfirm: {
        issue,
        pendingAction: {
          id: "pa-1",
          actionType: "agent_auto_fix",
          title: "Remove dead link",
          message: "Removes an unresolved wikilink",
          riskLevel: "high",
          affectedPaths: ["wiki/a.md"],
          preview: null,
          expiresAt: null,
        },
        expectedHash: "",
      },
    });
    invokeMock.mockResolvedValueOnce({ kind: "applied", affectedPaths: ["wiki/a.md"], checkpoint: "abc" } as LintFixOutcome);
    await useLintStore.getState().confirmHighRisk(PROJECT.projectId, PROJECT.rootPath, "hash-123");
    const call = invokeMock.mock.calls[0];
    expect(call[1].request.confirmHighRisk).toBe(true);
    expect(call[1].request.expectedHash).toBe("hash-123");
    expect(useLintStore.getState().fixConfirm).toBeNull();
    expect(useLintStore.getState().fixStatus[issue.id]).toBe("applied");
  });

  it("setMode / setSafetyPrefs update state and keep the checkpoint hard boundary on", () => {
    useLintStore.getState().setMode("local");
    expect(useLintStore.getState().mode).toBe("local");
    useLintStore.getState().setSafetyPrefs({ recompile: true });
    expect(useLintStore.getState().safetyPrefs.recompile).toBe(true);
    // checkpoint can never be turned off via the store.
    useLintStore.getState().setSafetyPrefs({ checkpoint: false } as never);
    expect(useLintStore.getState().safetyPrefs.checkpoint).toBe(true);
  });

  it("applyFixesBatch stores the outcome and surfaces high-risk confirmations", async () => {
    const outcome: LintBatchOutcome = {
      checkpoint: "cp-1",
      applied: [{ kind: "applied", affectedPaths: ["wiki/a.md"] }],
      needsConfirmation: [
        {
          issue: localIssue({ id: "dead_link:wiki/b.md:ghost", path: "wiki/b.md" }),
          pendingAction: {
            id: "pa-batch",
            actionType: "agent_auto_fix",
            title: "Remove dead link",
            message: "Removes an unresolved wikilink",
            riskLevel: "high",
            affectedPaths: ["wiki/b.md"],
            preview: null,
            expiresAt: null,
          },
        },
      ],
      skipped: [],
    };
    invokeMock.mockResolvedValueOnce(outcome);
    const result = await useLintStore.getState().applyFixesBatch({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      issues: [],
      expectedHashes: {},
    });
    expect(result).not.toBeNull();
    expect(useLintStore.getState().batchConfirmations).toHaveLength(1);
    expect(useLintStore.getState().batchRunning).toBe(false);
    expect(invokeMock.mock.calls[0][0]).toBe("apply_lint_fixes");
  });

  it("openBatchConfirmation promotes a batched high-risk item into fixConfirm", () => {
    const issue = localIssue({ id: "dead_link:wiki/b.md:ghost", path: "wiki/b.md" });
    useLintStore.setState({
      batchConfirmations: [
        {
          issue,
          pendingAction: {
            id: "pa-batch",
            actionType: "agent_auto_fix",
            title: "Remove dead link",
            message: "Removes an unresolved wikilink",
            riskLevel: "high",
            affectedPaths: ["wiki/b.md"],
            preview: null,
            expiresAt: null,
          },
        },
      ],
    });
    useLintStore.getState().openBatchConfirmation(issue.id);
    expect(useLintStore.getState().selectedIssueId).toBe(issue.id);
    expect(useLintStore.getState().fixConfirm?.pendingAction.id).toBe("pa-batch");
  });

  it("addIgnore updates the ignore list from the backend response", async () => {
    const file: LintIgnoreFile = {
      ignored: [{ path: "wiki/a.md", rule: "dead_link", createdAt: "2026-06-29T00:00:00Z" }],
    };
    invokeMock.mockResolvedValueOnce(file);
    const ok = await useLintStore.getState().addIgnore({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      path: "wiki/a.md",
      rule: "dead_link",
    });
    expect(ok).toBe(true);
    expect(useLintStore.getState().ignores).toHaveLength(1);
    expect(invokeMock.mock.calls[0][0]).toBe("add_lint_ignore");
  });

  it("removeIgnore restores a rule from the backend response", async () => {
    invokeMock.mockResolvedValueOnce({ ignored: [] } satisfies LintIgnoreFile);
    const ok = await useLintStore.getState().removeIgnore({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      path: "wiki/a.md",
      rule: "dead_link",
    });
    expect(ok).toBe(true);
    expect(useLintStore.getState().ignores).toHaveLength(0);
    expect(invokeMock.mock.calls[0][0]).toBe("remove_lint_ignore");
  });

  it("loadHistory stores entries and sends the typed request payload", async () => {
    const file: LintHistoryFile = {
      version: 1,
      entries: [
        {
          id: "local-1",
          kind: "local",
          createdAt: "2026-07-04T00:00:00Z",
          issueCount: 1,
          errorCount: 1,
          warningCount: 0,
          infoCount: 0,
          scannedPages: 3,
        },
      ],
    };
    invokeMock.mockResolvedValueOnce(file);
    const history = await useLintStore.getState().loadHistory({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
    });
    expect(history).toHaveLength(1);
    expect(useLintStore.getState().history[0].id).toBe("local-1");
    expect(invokeMock.mock.calls[0][0]).toBe("list_lint_history");
    expect(invokeMock.mock.calls[0][1].request.projectRootPath).toBe(PROJECT.rootPath);
  });

  it("openHistoryReport restores a persisted deep report", async () => {
    const persisted: PersistedLintReport = {
      entry: {
        id: "task-1",
        kind: "deep",
        createdAt: "2026-07-04T00:00:00Z",
        issueCount: 1,
        errorCount: 0,
        warningCount: 1,
        infoCount: 0,
        taskId: "task-1",
        route: "auto",
      },
      localReport: null,
      deepReport: {
        issues: [
          localIssue({
            id: "duplicate_topic:wiki/b.md",
            source: "agent",
            issueType: "duplicate_topic",
          }),
        ],
        rawOutput: "raw",
        generatedAt: "2026-07-04T00:00:00Z",
      },
    };
    invokeMock.mockResolvedValueOnce(persisted);
    const restored = await useLintStore.getState().openHistoryReport({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      id: "task-1",
    });
    expect(restored?.entry.id).toBe("task-1");
    expect(useLintStore.getState().deepReport?.issues).toHaveLength(1);
    expect(useLintStore.getState().localReport).toBeNull();
    expect(useLintStore.getState().mode).toBe("agent");
    expect(invokeMock.mock.calls[0][0]).toBe("read_lint_history_report");
    expect(invokeMock.mock.calls[0][1].request.id).toBe("task-1");
  });

  it("openHistoryReport exposes one merged Health Check report to Lint", async () => {
    const issue = localIssue({
      id: "schema_mismatch:wiki/主题.md",
      issueType: "schema_mismatch",
      path: "wiki/主题.md",
      severity: "error",
    });
    const persisted: PersistedLintReport = {
      entry: {
        id: "health-1",
        kind: "health_check",
        createdAt: "2026-08-01T00:00:00Z",
        issueCount: 1,
        errorCount: 1,
        warningCount: 0,
        infoCount: 0,
        taskId: "health-1",
        healthCheckMode: "complete",
        persistent: false,
      },
      healthCheckReport: {
        reportId: "health-1",
        taskId: "health-1",
        mode: "complete",
        route: {
          kind: "byok",
          provider: "ollama",
          model: "qwen-health",
          routeRevision: "route-1",
        },
        persistent: false,
        issues: [issue],
        findingOrigins: { [issue.id]: ["local", "agent"] },
        coverage: {
          scannedPages: 4,
          sourcePages: 1,
          wikiPages: 3,
          notApplicableRules: [],
        },
        errorCount: 1,
        warningCount: 0,
        infoCount: 0,
        findingsByType: { schema_mismatch: 1 },
        durationMs: 42,
        generatedAt: "2026-08-01T00:00:00Z",
      },
    };
    invokeMock.mockResolvedValueOnce(persisted);

    await useLintStore.getState().openHistoryReport({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      id: "health-1",
    });

    const state = useLintStore.getState();
    expect(state.healthReport?.reportId).toBe("health-1");
    expect(state.localReport).toBeNull();
    expect(state.deepReport).toBeNull();
    expect(state.mode).toBe("all");
    expect(selectAllIssues(state)).toEqual([issue]);
  });

  it("cancels every pending action before opening history", async () => {
    const issue = localIssue();
    useLintStore.setState({
      fixConfirm: {
        issue,
        pendingAction: {
          id: "history-single",
          actionType: "agent_auto_fix",
          title: "fix",
          message: "fix",
          riskLevel: "high",
          affectedPaths: [issue.path],
          preview: null,
          expiresAt: null,
        },
        expectedHash: "hash",
      },
      batchConfirmations: [
        {
          issue,
          pendingAction: {
            id: "history-batch",
            actionType: "agent_auto_fix",
            title: "fix",
            message: "fix",
            riskLevel: "high",
            affectedPaths: [issue.path],
            preview: null,
            expiresAt: null,
          },
        },
      ],
    });
    const persisted = {
      entry: {
        id: "history-1",
        kind: "local",
        createdAt: "2026-07-04T00:00:00Z",
        issueCount: 0,
        errorCount: 0,
        warningCount: 0,
        infoCount: 0,
        scannedPages: 0,
      },
      localReport: report({ issues: [] }),
      deepReport: null,
    } satisfies PersistedLintReport;
    invokeMock.mockResolvedValueOnce({}).mockResolvedValueOnce({}).mockResolvedValueOnce(persisted);

    await useLintStore.getState().openHistoryReport({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      id: "history-1",
    });

    expect(invokeMock.mock.calls.slice(0, 2).map((call) => call[0])).toEqual([
      "confirm_pending_action",
      "confirm_pending_action",
    ]);
    expect(invokeMock.mock.calls[0][1].request.actionId).toBe("history-single");
    expect(invokeMock.mock.calls[1][1].request.actionId).toBe("history-batch");
    expect(invokeMock.mock.calls[2][0]).toBe("read_lint_history_report");
    expect(useLintStore.getState().fixConfirm).toBeNull();
    expect(useLintStore.getState().batchConfirmations).toHaveLength(0);
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LintBatchOutcome, LintFixOutcome, LintIgnoreFile, LintIssue, LintReport } from "../types/lint";

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
  it("loads the local lint report", async () => {
    invokeMock.mockResolvedValueOnce(report());
    await useLintStore.getState().runLocalLint(PROJECT.projectId, PROJECT.rootPath);
    expect(useLintStore.getState().localReport?.issues).toHaveLength(1);
    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("run_local_lint");
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
});

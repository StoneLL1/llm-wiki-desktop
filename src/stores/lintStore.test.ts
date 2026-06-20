import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LintFixOutcome, LintIssue, LintReport } from "../types/lint";

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

  it("applyFix marks a safe fix as applied", async () => {
    const issue = localIssue({ fixability: "safe" });
    const outcome: LintFixOutcome = {
      kind: "applied",
      affectedPaths: ["wiki/a.md"],
      checkpoint: "abc",
    };
    invokeMock.mockResolvedValueOnce(outcome);
    await useLintStore.getState().applyFix(PROJECT.projectId, PROJECT.rootPath, issue);
    expect(useLintStore.getState().fixStatus[issue.id]).toBe("applied");
    expect(useLintStore.getState().fixConfirm).toBeNull();
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
});

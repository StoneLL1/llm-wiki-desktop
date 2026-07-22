import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { AgentCandidateView } from "../../types/importV2Agent";
import type { ImportFrontendReadiness, ImportHistoryPage } from "../../types/importV2Presentation";
import type { ImportWorkflow } from "./useImportWorkflow";
import { buildCandidateSelectionRequest, ImportView } from "./ImportView";
import type { ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";

function item(itemId: string, status: ImportItem["status"], selected = false): ImportItem {
  return {
    itemId,
    input: { kind: itemId.startsWith("url") ? "url" : "file", displayName: itemId, locator: itemId, normalizedLocator: null },
    status,
    selected,
    taskId: null,
    progress: status === "extracting" ? { current: 2, total: 4, label: null } : null,
    attempts: [],
    preview: status === "preview_ready" ? { title: itemId, markdown: { kind: "markdown", relativePath: "wiki/item.md", sha256: "a", sizeBytes: 10 }, assets: [], sourceSnapshot: { kind: "source_snapshot", relativePath: "raw/item", sha256: "b", sizeBytes: 10 }, quality: { level: "pass", metrics: [], warnings: [] } } : null,
    issue: status === "failed" ? { code: "EXTRACT_FAILED", message: "Needs review", stage: "extract", retryable: true, userActionRequired: true, recoveryActions: ["retry"], availableActions: [] } : null,
  };
}

function session(items: ImportItem[]): ImportSession {
  return { schemaVersion: 2, sessionId: "session-a", projectId: "project-a", status: "draft", resourceMode: "balanced", createdAt: "2026-07-13T00:00:00Z", updatedAt: "2026-07-13T00:00:00Z", items };
}

function workflow(overrides: Partial<ImportWorkflow> = {}): ImportWorkflow {
  const current = session([]);
  return {
    projectKey: "project-a\0D:/wiki/project-a",
    session: current,
    readiness: { backendVersion: "2.0.0", active: true, migrationStatus: "applied", unfinishedSessionId: current.sessionId, legacyHistoryAvailable: false },
    bootstrapState: "ready",
    visibleItems: current.items,
    counts: { all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0 },
    progress: { completed: 0, total: 0, active: 0 },
    selectedItemId: null,
    filter: "all",
    addPaths: vi.fn(),
    addUrl: vi.fn(),
    setItemSelected: vi.fn(),
    startItems: vi.fn(),
    retryItem: vi.fn(),
    cancelItem: vi.fn(),
    skipItem: vi.fn(),
    authorizeLocalAsr: vi.fn(),
    confirm: vi.fn(),
    refreshSession: vi.fn(),
    selectItem: vi.fn(),
    setFilter: vi.fn(),
    loadPreview: vi.fn(),
    loadSession: vi.fn(),
    getAgentPolicy: vi.fn().mockResolvedValue(null),
    setAgentPolicy: vi.fn(),
    invokeLocalAgent: vi.fn(),
    previewByokScope: vi.fn(),
    approveByokAssistance: vi.fn(),
    acceptAgentCandidate: vi.fn(),
    selectAgentCandidate: vi.fn(),
    discardAgentCandidate: vi.fn(),
    beginLogin: vi.fn(),
    completeLogin: vi.fn(),
    revokeLogin: vi.fn(),
    authorizePrivateTarget: vi.fn(),
    getCapabilityRequirement: vi.fn(),
    installCapability: vi.fn(),
    scanMigration: vi.fn(),
    planMigration: vi.fn(),
    applyMigration: vi.fn(),
    getMigrationStatus: vi.fn(),
    resumeMigration: vi.fn(),
    listHistory: vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] }),
    isConfirming: false,
    requestClipboard: vi.fn(),
    ...overrides,
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportView V2 composition", () => {
  it("binds a three-way merge selection to the Wiki version shown in the diff", () => {
    const view = {
      projectId: "project-a",
      sessionId: "session-a",
      itemId: "item-a",
      candidate: { candidateId: "candidate-a" },
      diff: {
        candidateId: "candidate-a",
        baselineMarkdown: "# Baseline",
        currentMarkdown: "# Current",
        currentMarkdownSha256: "current-hash",
        agentMarkdown: "# Agent",
        unifiedDiff: "@@",
        needsThreeWayMerge: true,
      },
    } as unknown as AgentCandidateView;
    const intent: ImportCandidateDiffIntent = { kind: "apply_merged", candidateId: "candidate-a", mergedMarkdown: "# Human merged" };

    expect(buildCandidateSelectionRequest(view, intent)).toEqual({
      itemId: "item-a",
      candidateId: "candidate-a",
      mergedMarkdown: "# Human merged",
      expectedCurrentWikiSha256: "current-hash",
    });
  });

  it("does not send merge-only fields for a clean Agent candidate", () => {
    const view = {
      itemId: "item-a",
      candidate: { candidateId: "candidate-a" },
      diff: {
        candidateId: "candidate-a",
        baselineMarkdown: "# Baseline",
        currentMarkdown: null,
        currentMarkdownSha256: null,
        agentMarkdown: "# Agent",
        unifiedDiff: "@@",
        needsThreeWayMerge: false,
      },
    } as unknown as AgentCandidateView;

    expect(buildCandidateSelectionRequest(view, { kind: "choose_agent", candidateId: "candidate-a" })).toEqual({
      itemId: "item-a",
      candidateId: "candidate-a",
      mergedMarkdown: null,
      expectedCurrentWikiSha256: null,
    });
  });

  it("renders a loading state without exposing legacy controls", () => {
    render(<ImportView workflow={workflow({ bootstrapState: "loading", session: null })} />);
    expect(screen.getByRole("status")).toHaveTextContent(/loading import workspace/i);
    expect(screen.queryByRole("button", { name: /delete|replace source|legacy/i })).not.toBeInTheDocument();
  });

  it("composes methods, empty queue, and a disabled commit bar for an empty draft", () => {
    render(<ImportView workflow={workflow()} />);
    expect(screen.getByRole("region", { name: /import methods/i })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: /import queue/i })).toHaveTextContent(/no sources/i);
    expect(screen.getByRole("button", { name: /confirm import/i })).toBeDisabled();
  });

  it("keeps mixed queue states actionable and selection keyboard accessible", () => {
    const mixed = [item("研究笔记.md", "preview_ready", true), item("processing.pdf", "extracting"), item("failed.docx", "failed")];
    const currentWorkflow = workflow({ session: session(mixed), visibleItems: mixed, counts: { all: 3, active: 1, ready: 1, needsAction: 1, failed: 1, completed: 0 }, progress: { completed: 0, total: 3, active: 1 } });
    const view = render(<ImportView workflow={currentWorkflow} />);
    expect(screen.getAllByText("研究笔记.md").length).toBeGreaterThan(0);
    expect(screen.getByText(/needs review/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /confirm import/i })).toBeEnabled();
    fireEvent.keyDown(view.getByTestId("import-item-研究笔记.md"), { key: "Enter" });
    expect(currentWorkflow.selectItem).toHaveBeenCalledWith("研究笔记.md");
    expect(screen.getAllByRole("status").some((element) => /0\/3 processed/i.test(element.textContent ?? ""))).toBe(true);
  });

  it("shows migration review without blocking V2 and never offers a V1 switch", () => {
    const readiness: ImportFrontendReadiness = { backendVersion: "2.0.0", active: false, migrationStatus: "awaiting_confirmation", unfinishedSessionId: null, legacyHistoryAvailable: true };
    render(<ImportView workflow={workflow({ readiness, bootstrapState: "blocked", session: null })} />);
    expect(screen.getByRole("tooltip")).toHaveTextContent(/migration/i);
    expect(screen.getByRole("button", { name: /review migration/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /switch to v1|legacy write/i })).not.toBeInTheDocument();
  });

  it("keeps the V2 commit action enabled when migration is not active", () => {
    const ready = item("old-project.md", "preview_ready", true);
    const readiness: ImportFrontendReadiness = { backendVersion: "2.0.0", active: false, migrationStatus: "not_scanned", unfinishedSessionId: null, legacyHistoryAvailable: true };
    const currentWorkflow = workflow({
      readiness,
      session: session([ready]),
      visibleItems: [ready],
      counts: { all: 1, active: 0, ready: 1, needsAction: 0, failed: 0, completed: 0 },
      progress: { completed: 0, total: 1, active: 0 },
    });
    render(<ImportView workflow={currentWorkflow} />);

    expect(screen.getByRole("button", { name: /confirm import/i })).toBeEnabled();
  });

  it("ignores a history response from the previous project scope", async () => {
    let resolvePrevious!: (page: ImportHistoryPage) => void;
    const previousHistory = vi.fn(() => new Promise<ImportHistoryPage>((resolve) => {
      resolvePrevious = resolve;
    }));
    const currentHistory = vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] });
    const previousWorkflow = workflow({ projectKey: "project-a\0D:/wiki/a", listHistory: previousHistory });
    const currentWorkflow = workflow({ projectKey: "project-a\0D:/wiki/b", listHistory: currentHistory });
    const { rerender } = render(<ImportView workflow={previousWorkflow} />);

    await waitFor(() => expect(previousHistory).toHaveBeenCalled());
    rerender(<ImportView workflow={currentWorkflow} />);
    await waitFor(() => expect(currentHistory).toHaveBeenCalled());

    await act(async () => {
      resolvePrevious({
        entries: [{
          id: "old-entry",
          title: "Old project history",
          status: "completed",
          sessionId: "old-session",
          batchId: "old-batch",
          taskId: null,
          startedAt: null,
          updatedAt: null,
          completedAt: null,
          legacyReadOnly: false,
          itemIds: [],
          availableActions: [],
        }],
        legacyReadOnly: [],
        nextCursor: null,
        warnings: [],
      });
    });

    expect(screen.queryByText("Old project history")).not.toBeInTheDocument();
  });

  it("renders the same shell with Chinese copy", async () => {
    await i18next.changeLanguage("zh-CN");
    render(<ImportView workflow={workflow()} />);
    expect(screen.getByRole("region", { name: /导入方式/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /确认导入/ })).toBeDisabled();
  });
});

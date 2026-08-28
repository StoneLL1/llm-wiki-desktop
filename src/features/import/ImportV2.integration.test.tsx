import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { ImportWorkflow } from "./useImportWorkflow";
import { ImportView } from "./ImportView";

function makeItem(itemId: string, status: ImportItem["status"], selected = false): ImportItem {
  return {
    itemId,
    input: { kind: itemId.endsWith(".url") ? "url" : "file", displayName: itemId, locator: itemId, normalizedLocator: null },
    status,
    selected,
    taskId: status === "extracting" ? "task-processing" : null,
    progress: status === "extracting" ? { current: 2, total: 4, label: "extracting" } : null,
    attempts: [],
    preview: status === "preview_ready" ? {
      title: itemId,
      markdown: { kind: "markdown", relativePath: "wiki/item.md", sha256: "sha-markdown", sizeBytes: 12 },
      assets: [],
      sourceSnapshot: { kind: "source_snapshot", relativePath: "raw/item", sha256: "sha-source", sizeBytes: 12 },
      quality: { level: "pass", metrics: [], warnings: [] },
    } : null,
    issue: status === "failed" ? {
      code: "EXTRACT_FAILED",
      message: "Retryable extraction failure",
      stage: "extract",
      retryable: true,
      userActionRequired: true,
      recoveryActions: ["retry"],
      availableActions: [],
    } : null,
  };
}

function makeWorkflow(items: ImportItem[]): ImportWorkflow {
  const current: ImportSession = {
    schemaVersion: 2,
    sessionId: "session-integration",
    projectId: "project-integration",
    status: "processing",
    resourceMode: "balanced",
    createdAt: "2026-07-14T00:00:00Z",
    updatedAt: "2026-07-14T00:00:00Z",
    items,
  };
  return {
    projectKey: "project-integration\0D:/wiki/project-integration",
    session: current,
    completion: null,
    readiness: { backendVersion: "2.0.0", active: true, migrationStatus: "applied", unfinishedSessionId: current.sessionId, legacyHistoryAvailable: false },
    bootstrapState: "ready",
    visibleItems: items,
    counts: { all: items.length, active: 1, ready: 1, needsAction: 1, failed: 1, completed: 0 },
    progress: { completed: 0, total: items.length, active: 1 },
    selectedItemId: null,
    filter: "all",
    addPaths: vi.fn(),
    addText: vi.fn(),
    collectionPreview: null,
    loadCollectionPage: vi.fn(),
    confirmCollection: vi.fn(),
    dismissCollection: vi.fn(),
    remoteMediaRetentionPlan: null,
    planRemoteMediaRetention: vi.fn(),
    confirmRemoteMediaRetention: vi.fn(),
    dismissRemoteMediaRetention: vi.fn(),
    addUrl: vi.fn(),
    setItemSelected: vi.fn(),
    startItems: vi.fn(),
    retryItem: vi.fn(),
    cancelItem: vi.fn(),
    skipItem: vi.fn(),
    authorizeLocalAsr: vi.fn(),
    authorizeLocalOcr: vi.fn(),
    selectSubtitle: vi.fn(),
    confirm: vi.fn(),
    restrictedCommitPending: false,
    confirmRestrictedContent: vi.fn(),
    dismissRestrictedContent: vi.fn(),
    viewImportedSources: vi.fn(),
    updateWiki: vi.fn(),
    refreshSession: vi.fn(),
    selectItem: vi.fn(),
    setFilter: vi.fn(),
    loadPreview: vi.fn(),
    loadMergeContext: vi.fn(),
    setItemResolution: vi.fn(),
    stageManualMerge: vi.fn(),
    loadSession: vi.fn(),
    loadCompletion: vi.fn(),
    invokeLocalAgent: vi.fn(),
    acceptAgentCandidate: vi.fn(),
    selectAgentCandidate: vi.fn(),
    discardAgentCandidate: vi.fn(),
    beginLogin: vi.fn(),
    completeLogin: vi.fn(),
    revokeLogin: vi.fn(),
    authorizePrivateTarget: vi.fn(),
    getCapabilityRequirement: vi.fn(),
    getAsrEnablementPlan: vi.fn(),
    installCapability: vi.fn(),
    scanMigration: vi.fn(),
    planMigration: vi.fn(),
    applyMigration: vi.fn(),
    getMigrationStatus: vi.fn(),
    resumeMigration: vi.fn(),
    listHistory: vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] }),
    loadHistoryDetail: vi.fn(),
    isConfirming: false,
    requestClipboard: vi.fn(),
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("Import V2 end-to-end presentation boundary", () => {
  it("keeps a failed sibling actionable while allowing a selected ready item to commit", () => {
    const workflow = makeWorkflow([
      makeItem("notes.md", "preview_ready", true),
      makeItem("processing.pdf", "extracting"),
      makeItem("failed.docx", "failed"),
    ]);
    render(<ImportView workflow={workflow} />);

    expect(screen.getByRole("button", { name: /import to source library/i })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: /import to source library/i }));
    expect(workflow.confirm).toHaveBeenCalledWith([
      { itemId: "notes.md", resolution: null },
    ]);

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    fireEvent.click(screen.getByRole("button", { name: "More actions for processing.pdf" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Cancel" }));
    expect(workflow.retryItem).toHaveBeenCalledWith("failed.docx");
    expect(workflow.cancelItem).toHaveBeenCalledWith("processing.pdf");
    expect(
      screen.getByText("Queue updated: 3 items, 1 need action, 1 ready.", {
        selector: '[aria-live="polite"]',
      }),
    ).toHaveTextContent(
      "Queue updated: 3 items, 1 need action, 1 ready.",
    );
  });

  it("keeps compatibility engineering UI out of the normal workbench", () => {
    const workflow = makeWorkflow([]);
    workflow.bootstrapState = "ready";
    workflow.readiness = { backendVersion: "2.0.0", active: false, migrationStatus: "awaiting_confirmation", unfinishedSessionId: null, legacyHistoryAvailable: true };
    workflow.counts = { all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0 };
    workflow.progress = { completed: 0, total: 0, active: 0 };
    render(<ImportView workflow={workflow} />);

    expect(screen.queryByText(/migration|fingerprint|dry-run/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /review migration/i })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /add files|choose files/i }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: /switch to v1|legacy write/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /import to source library/i })).toBeDisabled();
  });

  it("bounds a large queue and exposes incremental loading", () => {
    const workflow = makeWorkflow(Array.from({ length: 2_000 }, (_, index) => makeItem(`item-${index}.md`, "preview_ready")));
    render(<ImportView workflow={workflow} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(200);
    expect(screen.getByText(/showing 200 of 2000 items/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    expect(screen.getAllByRole("listitem")).toHaveLength(400);
  }, 10_000);
});

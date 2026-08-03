import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { ImportWorkflow } from "../../features/import/useImportWorkflow";
import type { WorkflowsController } from "../../features/workflows/useWorkflowsController";

const capabilities: AiCapabilitiesWorkflow = {
  agents: [],
  providers: [],
  refreshing: false,
  refresh: vi.fn(),
};
const importWorkflow: ImportWorkflow = {
  projectKey: "project-a\0D:/wiki/project-a",
  session: null,
  completion: null,
  readiness: null,
  bootstrapState: "loading",
  visibleItems: [],
  counts: { all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0 },
  progress: { completed: 0, total: 0, active: 0 },
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
  refreshSession: vi.fn(),
  selectItem: vi.fn(),
  setFilter: vi.fn(),
    loadPreview: vi.fn(),
    loadMergeContext: vi.fn(),
    setItemResolution: vi.fn(),
    stageManualMerge: vi.fn(),
    loadSession: vi.fn(),
  loadCompletion: vi.fn(),
  isConfirming: false,
  requestClipboard: vi.fn(),
  confirm: vi.fn(),
  restrictedCommitPending: false,
  confirmRestrictedContent: vi.fn(),
  dismissRestrictedContent: vi.fn(),
  viewImportedSources: vi.fn(),
  updateWiki: vi.fn(),
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
  listHistory: vi.fn(),
};
const workflowsController: WorkflowsController = {
  refresh: vi.fn(),
  prepare: vi.fn(),
  startPrepared: vi.fn(),
  cancel: vi.fn(),
  undoCancel: vi.fn(),
  reorder: vi.fn(),
  retry: vi.fn(),
  adjustAndPrepare: vi.fn(),
  openRun: vi.fn(),
  openResult: vi.fn(),
  confirm: vi.fn(),
  discard: vi.fn(),
  continueQueue: vi.fn(),
  loadHistoryMore: vi.fn(),
  handlePrerequisite: vi.fn(),
  backToOverview: vi.fn(),
};

function installViewMocks() {
  vi.doMock("../../features/dashboard/DashboardView", () => ({
    DashboardView: () => <div data-testid="dashboard-view" />,
  }));
  vi.doMock("../../features/wiki/WikiView", () => ({
    WikiView: () => <div data-testid="wiki-view" />,
  }));
  vi.doMock("../../features/chat/ChatView", () => ({
    ChatView: () => <div data-testid="chat-view" />,
  }));
  vi.doMock("../../features/graph/GraphView", () => ({
    GraphView: () => <div data-testid="graph-view" />,
  }));
  vi.doMock("../../features/lint/LintView", () => ({
    LintView: () => <div data-testid="lint-view" />,
  }));
  vi.doMock("../../features/exports/ExportsView", () => ({
    ExportsView: () => <div data-testid="exports-view" />,
  }));
  vi.doMock("../../features/import/ImportView", () => ({
    ImportView: () => <div data-testid="import-view" />,
  }));
  vi.doMock("../../features/workflows/WorkflowsView", () => ({
    WorkflowsView: () => <div data-testid="workflows-view" />,
  }));
}

const sharedProps = {
  capabilities,
  importWorkflow,
  workflowsController,
  onOpenTask: vi.fn(),
};

afterEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
});

describe("WorkspaceRouter", () => {
  it("maps every workspace view while keeping Dashboard static", async () => {
    installViewMocks();
    const { WorkspaceRouter } = await import("./WorkspaceRouter");
    const { rerender } = render(
      <WorkspaceRouter activeView="dashboard" {...sharedProps} />,
    );

    expect(screen.getByTestId("dashboard-view")).toBeInTheDocument();
    for (const view of [
      "wiki",
      "chat",
      "graph",
      "lint",
      "exports",
      "import",
      "workflows",
    ] as const) {
      rerender(<WorkspaceRouter activeView={view} {...sharedProps} />);
      expect(await screen.findByTestId(`${view}-view`)).toBeInTheDocument();
    }
  });
});

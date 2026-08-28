import { render, screen } from "@testing-library/react";
import { afterAll, describe, expect, it, vi } from "vitest";

import type { ImportWorkflow } from "../../features/import/useImportWorkflow";
import type { WorkflowsController } from "../../features/workflows/useWorkflowsController";
import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";

vi.mock("../../features/workflows/WorkflowsView", () => {
  throw new Error("workflows chunk missing");
});

import { WorkspaceRouter } from "./WorkspaceRouter";

const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});

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
  loadHistoryDetail: vi.fn(),
};
const workflowsController: WorkflowsController = {
  refresh: vi.fn(), prepare: vi.fn(), startPrepared: vi.fn(), cancel: vi.fn(),
  undoCancel: vi.fn(), reorder: vi.fn(), retry: vi.fn(), confirm: vi.fn(),
  adjustAndPrepare: vi.fn(), openRun: vi.fn(), openResult: vi.fn(),
  discard: vi.fn(), continueQueue: vi.fn(), filterHistory: vi.fn(), loadHistoryMore: vi.fn(), handlePrerequisite: vi.fn(), backToOverview: vi.fn(),
};

afterAll(() => {
  consoleError.mockRestore();
});

describe("WorkspaceRouter lazy failure", () => {
  it("contains a rejected lazy chunk and exposes local recovery", async () => {
    render(
      <WorkspaceRouter
        activeView="workflows"
        capabilities={capabilities}
        importWorkflow={importWorkflow}
        workflowsController={workflowsController}
        onOpenTask={vi.fn()}
      />,
    );

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /reload|retry/i }),
    ).toBeInTheDocument();
  });
});

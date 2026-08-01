import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { WorkflowPreparation, WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { WorkflowPipeline } from "./WorkflowPipeline";
import { WorkflowPreparationView } from "./WorkflowPreparationView";
import { WorkflowTaskDetail } from "./WorkflowTaskDetail";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowsOverviewView } from "./WorkflowsOverview";
import { attentionRun, groupWorkflowAttempts, WORKFLOW_STATUSES } from "./workflowPresentation";

const overview: WorkflowsOverview = {
  schemaVersion: 1,
  projectAccess: {
    projectId: "project-a",
    canonicalIdentityKey: "identity-a",
    identityRevision: "revision-a",
    trust: "trusted",
    filesystemAccess: "writable",
    persistence: "persistent",
    gitState: "clean",
  },
  rows: [
    { kind: "update_wiki", state: "ready", recommended: true, activeTaskId: null, lastCompletedAt: null, prerequisite: null },
    { kind: "health_check", state: "ready", recommended: false, activeTaskId: null, lastCompletedAt: null, prerequisite: null },
    { kind: "generate_content", state: "needs_prerequisite", recommended: false, activeTaskId: null, lastCompletedAt: null, prerequisite: null },
  ],
};

describe("Workflows overview", () => {
  it("renders exactly the three fixed workflows and a single recommendation", () => {
    const prepare = vi.fn();
    render(<WorkflowsOverviewView overview={overview} runs={[]} onPrepare={prepare} onOpenRun={vi.fn()} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getAllByText("workflows.recommended")).toHaveLength(1);
    fireEvent.click(screen.getAllByRole("button", { name: "workflows.action.prepare" })[1]!);
    expect(prepare).toHaveBeenCalledWith("health_check");
  });

  it("re-prepares each workflow from structured scope controls", () => {
    const reprepare = vi.fn();
    const base = {
      schemaVersion: 1, preparationId: "prep", preparationRevision: "r1",
      projectAccess: overview.projectAccess!, baseline: { fingerprint: "base", capturedAt: "2026-08-01T00:00:00Z", itemCount: 2 },
      route: null, prerequisites: [], output: { labelKey: "workflows.output.session", location: null, mayChangeWiki: false }, gitPolicy: "not_required" as const,
      requiresScopeConfirmation: false, quickRerunEligible: false,
    };
    const props = { onBack: vi.fn(), onStart: vi.fn(), onPrerequisite: vi.fn(), onReprepare: reprepare };
    const update: WorkflowPreparation = { ...base, kind: "update_wiki", scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "来源一", versionId: "v1" }] } };
    const view = render(<WorkflowPreparationView preparation={update} {...props} />);
    fireEvent.click(screen.getByLabelText("workflows.mode.fullRecompile"));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.updatePreparation" }));
    expect(reprepare).toHaveBeenLastCalledWith(expect.objectContaining({ mode: "full_recompile" }), null);

    view.rerender(<WorkflowPreparationView preparation={{ ...base, kind: "health_check", scope: { kind: "health_check", mode: "local_quick" } }} {...props} />);
    fireEvent.click(screen.getByLabelText("workflows.mode.complete"));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.updatePreparation" }));
    expect(reprepare).toHaveBeenLastCalledWith({ kind: "health_check", mode: "complete" }, null);

    view.rerender(<WorkflowPreparationView preparation={{ ...base, kind: "generate_content", scope: { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: "exports/project-report.html" }, availableWikiPages: ["wiki/中文.md"], quickRerunEligible: true }} {...props} />);
    fireEvent.change(screen.getByLabelText("workflows.preparation.artifactType"), { target: { value: "knowledge_card" } });
    fireEvent.click(screen.getByLabelText("wiki/中文.md"));
    fireEvent.change(screen.getByLabelText("workflows.preparation.outputPath"), { target: { value: "exports/知识卡.html" } });
    expect(screen.getByRole("button", { name: "workflows.action.runAgain" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.updatePreparation" }));
    expect(reprepare).toHaveBeenLastCalledWith(expect.objectContaining({ artifactType: "knowledge_card", outputPath: "exports/知识卡.html" }), null);
    expect(screen.getByRole("button", { name: "workflows.action.runAgain" })).toBeInTheDocument();
  });

  it("renders indeterminate counts without claiming 100 percent completion", () => {
    render(<WorkflowPipeline stages={(["pending", "running", "completed", "failed", "waiting", "skipped"] as const).map((status, index) => ({ id: status, ordinal: index + 1, status, labelKey: status, startedAt: null, completedAt: null, currentItem: status === "running" ? "wiki/中文.md" : null, progress: status === "running" ? { current: 3, total: null } : null, decision: null }))} />);
    expect(screen.getByText("workflows.progress.current")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(6);
    expect(WORKFLOW_STATUSES).toEqual(["queued", "running", "waiting_for_confirmation", "completed", "failed", "cancelled", "interrupted"]);
  });

  it("prioritizes waiting, failed, and running runs for attention", () => {
    const base = { taskId: "running", displayStatus: "running" } as WorkflowRun;
    expect(attentionRun([{ ...base }, { ...base, taskId: "failed", displayStatus: "failed" }, { ...base, taskId: "waiting", displayStatus: "waiting_for_confirmation" }])?.taskId).toBe("waiting");
    expect(attentionRun([{ ...base }, { ...base, taskId: "failed", displayStatus: "failed" }])?.taskId).toBe("failed");
    expect(attentionRun([base])?.taskId).toBe("running");
  });

  it("renders the no-project state without inventing a workflow", () => {
    render(<WorkflowsOverviewView overview={null} runs={[]} onPrepare={vi.fn()} onOpenRun={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "workflows.noProject.title" })).toBeInTheDocument();
    expect(screen.queryByRole("listitem")).not.toBeInTheDocument();
  });

  it("shows complete confirmation evidence and valid queue actions", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const waiting: WorkflowRun = {
      schemaVersion: 1, taskId: "waiting-a", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b",
      stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null,
      pendingAction: { id: "action-a", actionType: "batch_rewrite", riskLevel: "high", affectedPaths: ["wiki/甲.md", "wiki/乙.md"], candidate: null, expiresAt: null, checkpointHash: "abc123" }, result: null, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:00:00Z", completedAt: null,
    };
    render(<WorkflowTaskDetail run={waiting} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    expect(screen.getByText("abc123")).toBeInTheDocument();
    expect(screen.getByText("wiki/甲.md")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.applyChanges" }));
    expect(controller.confirm).toHaveBeenCalledWith("waiting-a", "action-a");
  });

  it("groups retries under their original attempt", () => {
    const base = {
      taskId: "first",
      updatedAt: "2026-08-01T00:00:00Z",
      retry: null,
    };
    const groups = groupWorkflowAttempts([
      base,
      { ...base, taskId: "retry", retry: { attemptOf: "first", attemptNumber: 2 } },
    ] as never);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.runs.map((run) => run.taskId)).toEqual(["first", "retry"]);
  });
});

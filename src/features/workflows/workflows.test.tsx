import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const i18nMocks = vi.hoisted(() => ({
  t: (key: string) => key,
}));

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({ t: (key: string) => i18nMocks.t(key) }),
}));

import type { WorkflowPreparation, WorkflowRun, WorkflowsOverview } from "../../types/workflow";
import { WorkflowPipeline } from "./WorkflowPipeline";
import { WorkflowPreparationView } from "./WorkflowPreparationView";
import { WorkflowTaskDetail } from "./WorkflowTaskDetail";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowsOverviewView } from "./WorkflowsOverview";
import { WorkflowsView } from "./WorkflowsView";
import { attentionRun, groupWorkflowAttempts, WORKFLOW_STATUSES } from "./workflowPresentation";
import { WorkflowsRightPanel } from "./WorkflowsRightPanel";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";

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

afterEach(() => {
  i18nMocks.t = (key: string) => key;
  useWorkflowStore.getState().reset();
});

describe("Workflows overview", () => {
  it("renders exactly the three fixed workflows and a single recommendation", () => {
    const prepare = vi.fn();
    render(<WorkflowsOverviewView overview={overview} overviewStatus="ready" error={null} runs={[]} onRetry={vi.fn()} onPrepare={prepare} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
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

  it("keeps an explicitly selected completed run ahead of another attention run", () => {
    const completed = {
      schemaVersion: 1, taskId: "selected-completed", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "health_check", displayStatus: "completed",
      scope: { kind: "health_check", mode: "local_quick" }, route: { kind: "local", routeRevision: "local" }, fingerprint: "f", baselineFingerprint: "b",
      stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null, pendingAction: null, result: null, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: "2026-08-01T00:01:00Z",
    } satisfies WorkflowRun;
    const waiting = {
      ...completed,
      taskId: "waiting-attention",
      displayStatus: "waiting_for_confirmation" as const,
      updatedAt: "2026-08-01T00:02:00Z",
      completedAt: null,
    };
    useProjectStore.setState({ currentProject: projectSummary });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({ runs: [waiting, completed], selectedTaskId: completed.taskId, surface: "detail" });

    render(<WorkflowsRightPanel />);

    expect(screen.getByText("selected")).toBeInTheDocument();
    expect(screen.queryByText("waiting-")).not.toBeInTheDocument();
    expect(screen.getByText("workflows.context.selection")).toBeInTheDocument();
  });

  it("derives preparation and history context from the active surface instead of stale selection", () => {
    const selected = {
      schemaVersion: 1, taskId: "stale-selected", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "health_check", displayStatus: "completed",
      scope: { kind: "health_check", mode: "local_quick" }, route: { kind: "local", routeRevision: "local" }, fingerprint: "f", baselineFingerprint: "b",
      stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null, pendingAction: null, result: null, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: "2026-08-01T00:01:00Z",
    } satisfies WorkflowRun;
    const prep = {
      schemaVersion: 1, preparationId: "prep-a", preparationRevision: "prep-revision-a", projectAccess: overview.projectAccess!,
      kind: "update_wiki", scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 2 }, route: null, prerequisites: [],
      output: { labelKey: "workflows.output.wiki", location: "wiki", mayChangeWiki: true }, gitPolicy: "required_before_write" as const,
      requiresScopeConfirmation: false, quickRerunEligible: false,
    } satisfies WorkflowPreparation;
    useProjectStore.setState({ currentProject: projectSummary });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({
      runs: [selected],
      selectedTaskId: selected.taskId,
      preparation: prep,
      surface: "preparation",
    });

    const view = render(<WorkflowsRightPanel />);
    expect(screen.getByText("workflows.context.preparation")).toBeInTheDocument();
    expect(screen.queryByText("stale-se")).not.toBeInTheDocument();

    useWorkflowStore.setState({ surface: "history" });
    view.rerender(<WorkflowsRightPanel />);
    expect(screen.getByText("workflows.context.project")).toBeInTheDocument();
    expect(screen.queryByText("stale-se")).not.toBeInTheDocument();
  });

  it("keeps long English context labels and actions keyboard reachable at 200 percent text size", () => {
    const longTitle = "Workflow context for a knowledge base with unusually descriptive English labels";
    i18nMocks.t = (key: string) => key === "workflows.context.title" ? longTitle : key;
    useProjectStore.setState({ currentProject: {
      ...projectSummary,
      name: "A very long English knowledge base name that must remain available to assistive technology",
    } });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({ runs: [{
      schemaVersion: 1, taskId: "long-label-run", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "health_check", displayStatus: "running",
      scope: { kind: "health_check", mode: "local_quick" }, route: { kind: "local", routeRevision: "local" }, fingerprint: "f", baselineFingerprint: "b",
      stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null, pendingAction: null, result: null, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: null,
    }] });

    const view = render(<div style={{ fontSize: "200%" }}><WorkflowsRightPanel /></div>);
    const panel = view.container.querySelector("#right-context-panel");
    const buttons = screen.getAllByRole("button");

    expect(panel).toHaveAttribute("aria-label", longTitle);
    expect(screen.getByText(longTitle)).toBeInTheDocument();
    expect(buttons.length).toBeGreaterThan(0);
    buttons[0]?.focus();
    expect(buttons[0]).toHaveFocus();
  });

  it("does not mislabel a pending overview as no project", () => {
    render(<WorkflowsOverviewView overview={null} overviewStatus="loading" error={null} runs={[]} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("heading", { name: "workflows.loading.title" })).toBeInTheDocument();
    expect(screen.queryByText("workflows.noProject.title")).not.toBeInTheDocument();
  });

  it("shows an actionable error when the overview request fails", () => {
    const retry = vi.fn();
    render(<WorkflowsOverviewView overview={null} overviewStatus="error" error={{ summary: "overview unavailable", technicalDetails: "OVERVIEW_FAILED" }} runs={[]} onRetry={retry} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("overview unavailable");
    expect(screen.getByText("OVERVIEW_FAILED")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));
    expect(retry).toHaveBeenCalledOnce();
  });

  it("renders the backend no-project overview as fixed workflow prerequisites", () => {
    const handlePrerequisite = vi.fn();
    const prerequisite = { code: "WORKFLOW_PROJECT_REQUIRED", messageKey: "workflows.prerequisite.openOrCreateProject", blocking: true, action: "open_or_create_project" as const };
    const noProjectOverview: WorkflowsOverview = {
      schemaVersion: 1,
      projectAccess: null,
      rows: overview.rows.map((row) => ({ ...row, state: "needs_prerequisite", recommended: false, prerequisite })),
    };
    render(<WorkflowsOverviewView overview={noProjectOverview} overviewStatus="ready" error={null} runs={[]} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={handlePrerequisite} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.queryByText("workflows.prerequisite.openOrCreateProject")).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "workflows.action.openOrCreateProject" })[0]!);
    expect(handlePrerequisite).toHaveBeenCalledWith("open_or_create_project");
  });

  it("prepares project-present route and acknowledgement prerequisites before resolving them", () => {
    const prepare = vi.fn();
    const handlePrerequisite = vi.fn();
    const actions = [
      "configure_execution_route",
      "acknowledge_remote_provider",
      "acknowledge_restricted_content",
    ] as const;
    const view = render(<div />);

    for (const action of actions) {
      const blockedOverview: WorkflowsOverview = {
        ...overview,
        rows: overview.rows.map((row) => row.kind === "update_wiki" ? {
          ...row,
          state: "needs_prerequisite",
          prerequisite: { code: action, messageKey: `workflows.prerequisite.${action}`, blocking: true, action },
        } : row),
      };
      view.rerender(<WorkflowsOverviewView overview={blockedOverview} overviewStatus="ready" error={null} runs={[]} onRetry={vi.fn()} onPrepare={prepare} onPrerequisite={handlePrerequisite} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
      fireEvent.click(screen.getAllByRole("button", { name: "workflows.action.prepare" })[0]!);
    }

    expect(prepare).toHaveBeenCalledTimes(actions.length);
    expect(prepare).toHaveBeenCalledWith("update_wiki");
    expect(handlePrerequisite).not.toHaveBeenCalled();
  });

  it("keeps an overview load failure visible after switching to history", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      surface: "history",
      overview: null,
      overviewStatus: "error",
      operations: {
        "overview:init": { requestId: 1, pending: false, error: { summary: "overview unavailable", technicalDetails: "OVERVIEW_FAILED" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent("overview unavailable");
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));
    expect(controller.refresh).toHaveBeenCalledOnce();
  });

  it("offers refresh when a cached overview refresh fails", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      surface: "overview",
      overview,
      overviewStatus: "error",
      operations: {
        "overview:reconcile": { requestId: 1, pending: false, error: { summary: "refresh unavailable", technicalDetails: "REFRESH_FAILED" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent("refresh unavailable");
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.refresh" }));
    expect(controller.refresh).toHaveBeenCalledOnce();
  });

  it("keeps background reconcile state from blocking or overwriting preparation", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const prep = {
      schemaVersion: 1,
      preparationId: "prep-health",
      preparationRevision: "prep-revision",
      projectAccess: overview.projectAccess!,
      kind: "health_check",
      scope: { kind: "health_check", mode: "local_quick" },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 1 },
      route: { kind: "local", routeRevision: "local" },
      prerequisites: [],
      output: { labelKey: "workflows.output.session", location: null, mayChangeWiki: false },
      gitPolicy: "not_required" as const,
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
    } satisfies WorkflowPreparation;
    useWorkflowStore.setState({
      overview,
      overviewStatus: "ready",
      preparation: prep,
      surface: "preparation",
      operations: {
        "overview:reconcile": { requestId: 1, pending: true, error: null },
        "prepare:update_wiki": { requestId: 2, pending: true, error: null },
        "prepare:health_check": { requestId: 3, pending: false, error: { summary: "health preparation failed", technicalDetails: "PREP_FAILED" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent("health preparation failed");
    expect(screen.queryByText("refresh unavailable")).not.toBeInTheDocument();
    expect(document.querySelector(".workflows-view")).toHaveAttribute("aria-busy", "false");
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeEnabled();
  });

  it("retries detail hydration with a targeted open and clears only its error", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const waiting = {
      schemaVersion: 1, taskId: "waiting-retry", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b", stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null,
      pendingAction: { id: "action-a", actionType: "batch_rewrite", riskLevel: "high", affectedPaths: [], candidate: null, expiresAt: null, checkpointHash: null }, result: null, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: null,
    } satisfies WorkflowRun;
    useWorkflowStore.setState({
      overview,
      overviewStatus: "ready",
      runs: [waiting],
      selectedTaskId: waiting.taskId,
      surface: "detail",
      operations: {
        "task:waiting-retry:hydrate:action-a": {
          requestId: 8,
          pending: false,
          error: { summary: "detail unavailable", technicalDetails: "DETAIL_FAILED" },
        },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));

    expect(controller.openRun).toHaveBeenCalledWith(waiting.taskId);
    expect(controller.refresh).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState().operations["task:waiting-retry:hydrate:action-a"]?.error).toBeNull();
  });

  it("shows and retries an overview-owned task open failure", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      overview,
      overviewStatus: "ready",
      surface: "overview",
      operations: {
        "task:failed-open:open": {
          requestId: 9,
          pending: false,
          error: { summary: "task detail unavailable", technicalDetails: "OPEN_FAILED" },
        },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("task detail unavailable");
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));

    expect(controller.openRun).toHaveBeenCalledWith("failed-open");
    expect(useWorkflowStore.getState().operations["task:failed-open:open"]?.error).toBeNull();
  });

  it("shows complete confirmation evidence and valid queue actions", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
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

  it("keeps confirmation mutations enabled while read-only detail hydration is pending", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const waiting = {
      schemaVersion: 1, taskId: "waiting-hydrate", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b", stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null,
      pendingAction: { id: "action-a", actionType: "batch_rewrite", riskLevel: "high", affectedPaths: ["wiki/a.md"], candidate: null, expiresAt: null, checkpointHash: null }, result: null, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: null,
    } as WorkflowRun;
    useWorkflowStore.setState({
      operations: {
        "task:waiting-hydrate:hydrate:action-a": { requestId: 1, pending: true, error: null },
      },
    });

    render(<WorkflowTaskDetail run={waiting} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);

    expect(screen.getByRole("button", { name: /workflows.action.applyChanges/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: "workflows.action.discard" })).toBeEnabled();
  });

  it("prepares a recommended next workflow without starting it", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const completed = {
      schemaVersion: 1, taskId: "completed-a", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "completed",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b",
      stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null, pendingAction: null,
      result: { kind: "update_wiki", created: 1, updated: 0, skipped: 0, deleted: 0, conflicted: 0, checkpointHash: "abc", finalCommit: "def", affectedPaths: ["wiki/a.md"] }, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: "2026-08-01T00:01:00Z",
    } satisfies WorkflowRun;
    render(<WorkflowTaskDetail run={completed} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.prepareNext" }));
    expect(controller.prepare).toHaveBeenCalledWith("health_check");
    expect(controller.startPrepared).not.toHaveBeenCalled();
  });

  it("disables the recommended preparation action while its own request is pending", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const completed = {
      schemaVersion: 1, taskId: "completed-pending", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "completed",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b", stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null, pendingAction: null,
      result: { kind: "update_wiki", created: 1, updated: 0, skipped: 0, deleted: 0, conflicted: 0, checkpointHash: null, finalCommit: null, affectedPaths: [] }, error: null,
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: "2026-08-01T00:01:00Z",
    } satisfies WorkflowRun;
    useWorkflowStore.setState({ operations: { "prepare:health_check": { requestId: 1, pending: true, error: null } } });

    render(<WorkflowTaskDetail run={completed} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);

    expect(screen.getByRole("button", { name: "workflows.action.prepareNext" })).toBeDisabled();
  });

  it("exposes retry choices as a disclosed button group", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const failed = {
      schemaVersion: 1, taskId: "failed-a", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "health_check", displayStatus: "failed",
      scope: { kind: "health_check", mode: "complete" }, route: { kind: "local", routeRevision: "local" }, fingerprint: "f", baselineFingerprint: "b",
      stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null, pendingAction: null, result: null,
      error: { code: "FAILED", messageKey: "failed", recoverable: true, userActionRequired: false, suggestedAction: null },
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: "2026-08-01T00:01:00Z",
    } satisfies WorkflowRun;

    render(<WorkflowTaskDetail run={failed} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    const disclosure = screen.getByRole("button", { name: "workflows.action.retry" });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(disclosure);

    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    const options = screen.getByRole("group", { name: "workflows.retry.options" });
    expect(disclosure).toHaveAttribute("aria-controls", options.id);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.retry.sameSettings" }));
    expect(controller.retry).toHaveBeenCalledWith("failed-a");
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
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

const projectSummary = {
  projectId: "project-a", name: "Project A", rootPath: "D:/a", template: "general" as const,
  wikiPageCount: 1, sourceCount: 1, taskCount: 0, indexState: "indexed" as const,
  graphState: "cached" as const, agentRoute: "byok" as const,
  health: { isWikiProject: true, hasPurpose: true, hasSchema: true, hasAppState: true, hasObsidian: false, missingPaths: [] },
};

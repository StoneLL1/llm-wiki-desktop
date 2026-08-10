import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const i18nMocks = vi.hoisted(() => ({
  t: (key: string) => key,
  language: "en-US",
}));
const workflowApiMocks = vi.hoisted(() => ({ getWorkflowFileDiff: vi.fn() }));

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    t: (key: string) => i18nMocks.t(key),
    i18n: { language: i18nMocks.language, resolvedLanguage: i18nMocks.language },
  }),
}));
vi.mock("../../services/workflowApi", () => ({
  getWorkflowFileDiff: workflowApiMocks.getWorkflowFileDiff,
}));

import type { WorkflowFileDiffPage, WorkflowPreparation, WorkflowRun, WorkflowRunSummary, WorkflowsOverview } from "../../types/workflow";
import { WorkflowHistoryView } from "./WorkflowHistoryView";
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
import enLocale from "../../i18n/locales/en.json";
import zhLocale from "../../i18n/locales/zh-CN.json";

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
    { kind: "update_wiki", state: "ready", recommended: true, activeTaskId: null, activeContinuationRequired: false, lastCompletedAt: null, lastCompletedTaskId: null, prerequisite: null },
    { kind: "health_check", state: "ready", recommended: false, activeTaskId: null, activeContinuationRequired: false, lastCompletedAt: null, lastCompletedTaskId: null, prerequisite: null },
    { kind: "generate_content", state: "needs_prerequisite", recommended: false, activeTaskId: null, activeContinuationRequired: false, lastCompletedAt: null, lastCompletedTaskId: null, prerequisite: null },
  ],
  contextSummary: {
    pendingSourceCount: 0,
    lastHealth: null,
    recentArtifact: null,
    queueCount: 0,
    queuedRuns: [],
  },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function workflowRun(overrides: Partial<WorkflowRun> & Pick<WorkflowRun, "taskId" | "kind" | "displayStatus">): WorkflowRun {
  return {
    schemaVersion: 1,
    projectId: "project-a",
    canonicalIdentityKey: "identity-a",
    identityRevision: "revision-a",
    scope: overrides.kind === "update_wiki"
      ? { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }
      : overrides.kind === "generate_content"
        ? { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: "exports/report.html" }
        : { kind: "health_check", mode: "local_quick" },
    route: null,
    fingerprint: `fingerprint-${overrides.taskId}`,
    baselineFingerprint: `baseline-${overrides.taskId}`,
    stages: [],
    currentStageId: null,
    queuePosition: null,
    continuationRequired: false,
    retry: null,
    pendingAction: null,
    result: null,
    error: null,
    startedAt: "2026-08-10T08:00:00Z",
    updatedAt: "2026-08-10T08:01:00Z",
    completedAt: null,
    ...overrides,
  };
}

afterEach(() => {
  i18nMocks.t = (key: string) => key;
  i18nMocks.language = "en-US";
  useWorkflowStore.getState().reset();
  workflowApiMocks.getWorkflowFileDiff.mockReset();
});

describe("Workflows overview", () => {
  it("renders exactly the three fixed workflows and a single recommendation", () => {
    const prepare = vi.fn();
    render(<WorkflowsOverviewView overview={overview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={prepare} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getAllByText("workflows.recommended")).toHaveLength(1);
    fireEvent.click(screen.getAllByRole("button", { name: /^workflows\.action\.run:/ })[1]!);
    expect(prepare).toHaveBeenCalledWith("health_check");
  });

  it("orders attention, the three available workflows, and the backend-bounded recent five", () => {
    const waiting = workflowRun({
      taskId: "waiting-review",
      kind: "update_wiki",
      displayStatus: "waiting_for_confirmation",
      updatedAt: "2026-08-10T09:00:00Z",
    });
    const recentRuns = Array.from({ length: 6 }, (_, index) => workflowRun({
      taskId: `recent-${index}`,
      kind: index % 2 === 0 ? "health_check" : "generate_content",
      displayStatus: "completed",
      updatedAt: `2026-08-10T08:0${5 - index}:00Z`,
      completedAt: `2026-08-10T08:0${5 - index}:00Z`,
    }));
    const snapshot = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "update_wiki"
        ? { ...row, state: "waiting_for_confirmation" as const, activeTaskId: waiting.taskId }
        : row),
      recentRuns,
    };

    render(<WorkflowsOverviewView overview={snapshot} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    const attention = screen.getByRole("region", { name: "workflows.overview.attention" });
    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    const recent = screen.getByRole("region", { name: "workflows.overview.recent" });
    expect(attention.compareDocumentPosition(available) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(available.compareDocumentPosition(recent) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(attention).getByText("workflows.status.waiting_for_confirmation")).toBeInTheDocument();
    expect(within(available).getAllByRole("listitem")).toHaveLength(3);
    const visibleRecentRuns = within(recent).getAllByRole("listitem");
    expect(visibleRecentRuns).toHaveLength(5);
    expect(visibleRecentRuns.map((row) => row.querySelector("time")?.dateTime)).toEqual(
      recentRuns.slice(0, 5).map((run) => run.updatedAt),
    );
  });

  it("maps row state to one clear action while an attention task owns the only primary action", () => {
    const active = workflowRun({ taskId: "running-update", kind: "update_wiki", displayStatus: "running" });
    const completed = workflowRun({ taskId: "completed-export", kind: "generate_content", displayStatus: "completed", completedAt: "2026-08-10T08:01:00Z" });
    const handleOpenRun = vi.fn();
    const handlePrepare = vi.fn();
    const statefulOverview: WorkflowsOverview = {
      ...overview,
      rows: [
        { ...overview.rows[0]!, state: "running", activeTaskId: active.taskId },
        { ...overview.rows[1]!, state: "ready", recommended: true },
        { ...overview.rows[2]!, state: "up_to_date", lastCompletedAt: completed.completedAt, lastCompletedTaskId: completed.taskId, prerequisite: null },
      ],
      recentRuns: [],
    };

    const { container } = render(<WorkflowsOverviewView overview={statefulOverview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={handlePrepare} onPrerequisite={vi.fn()} onOpenRun={handleOpenRun} onContinueQueue={vi.fn()} />);

    const attention = screen.getByRole("region", { name: "workflows.overview.attention" });
    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    expect(attention.querySelectorAll(".workflow-status svg")).toHaveLength(1);
    expect(attention.querySelector(".workflow-attention-run__icon .animate-spin")).not.toBeInTheDocument();
    expect(within(attention).getByRole("button", { name: /^workflows\.action\.viewProgress:/ })).toBeInTheDocument();
    expect(within(available).getByRole("button", { name: /^workflows\.action\.viewProgress:/ })).toBeInTheDocument();
    expect(within(available).getByRole("button", { name: /^workflows\.action\.queue:/ })).toBeInTheDocument();
    expect(within(available).getByRole("button", { name: /^workflows\.action\.view:/ })).toBeInTheDocument();
    expect(screen.queryByText("workflows.recommended")).not.toBeInTheDocument();
    expect(container.querySelectorAll(".btn--primary")).toHaveLength(1);
    fireEvent.click(within(available).getByRole("button", { name: /^workflows\.action\.view:/ }));
    expect(handleOpenRun).toHaveBeenCalledWith(completed.taskId);
    expect(handlePrepare).not.toHaveBeenCalledWith("generate_content");
  });

  it("keeps prerequisite guidance and last-completed context together without forcing a fixed row height", () => {
    const snapshot: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "update_wiki"
        ? {
            ...row,
            state: "needs_prerequisite" as const,
            lastCompletedAt: "2026-08-10T08:01:00Z",
            lastCompletedTaskId: "completed-before-route-change",
            prerequisite: {
              code: "WORKFLOW_ROUTE_REQUIRED",
              messageKey: "workflows.prerequisite.routeRequired",
              blocking: true,
              action: "configure_execution_route" as const,
            },
          }
        : row),
    };

    const { container } = render(<WorkflowsOverviewView overview={snapshot} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    const row = container.querySelector<HTMLElement>(".workflow-row")!;
    expect(within(row).getByText("workflows.prerequisite.routeRequired")).toBeInTheDocument();
    expect(within(row).getByText("workflows.overview.lastCompleted")).toBeInTheDocument();
  });

  it("continues a recovered queue from row truth when the active run is outside the bounded snapshot", () => {
    const handleContinueQueue = vi.fn();
    const snapshot: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "health_check"
        ? {
            ...row,
            state: "queued" as const,
            activeTaskId: "recovered-queue-outside-recent",
            activeContinuationRequired: true,
          }
        : row),
      recentRuns: [],
    };

    render(<WorkflowsOverviewView overview={snapshot} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={handleContinueQueue} />);

    const attention = screen.getByRole("region", { name: "workflows.overview.attention" });
    fireEvent.click(within(attention).getByRole("button", { name: /^workflows\.action\.continueQueue:/ }));
    expect(handleContinueQueue).toHaveBeenCalledTimes(1);
  });

  it("keeps an up-to-date action stable when its completion is outside recent runs", () => {
    const handleOpenRun = vi.fn();
    const currentWithoutTarget: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "update_wiki"
        ? {
            ...row,
            state: "up_to_date" as const,
            lastCompletedAt: "2026-07-01T08:00:00Z",
            lastCompletedTaskId: null,
          }
        : row),
      recentRuns: [],
    };
    const props = { overviewStatus: "ready" as const, error: null, onRetry: vi.fn(), onPrepare: vi.fn(), onPrerequisite: vi.fn(), onOpenRun: handleOpenRun, onContinueQueue: vi.fn() };
    const view = render(<WorkflowsOverviewView overview={currentWithoutTarget} {...props} />);
    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    const updateRow = within(available).getByText("workflows.kind.update_wiki").closest<HTMLElement>('[role="listitem"]')!;
    expect(within(updateRow).getByRole("button", { name: /^workflows\.status\.up_to_date:/ })).toBeDisabled();
    expect(within(updateRow).queryByRole("button", { name: /^workflows\.action\.run:/ })).not.toBeInTheDocument();

    view.rerender(<WorkflowsOverviewView overview={{
      ...currentWithoutTarget,
      rows: currentWithoutTarget.rows.map((row) => row.kind === "update_wiki"
        ? { ...row, lastCompletedTaskId: "older-completed-update" }
        : row),
    }} {...props} />);
    fireEvent.click(within(updateRow).getByRole("button", { name: /^workflows\.action\.view:/ }));
    expect(handleOpenRun).toHaveBeenCalledWith("older-completed-update");
  });

  it("disables an up-to-date View action while its bounded task target is opening", () => {
    const taskId = "older-completed-update";
    useWorkflowStore.setState({
      operations: {
        [`task:${taskId}:open`]: { requestId: 1, pending: true, error: null },
      },
    });
    const snapshot: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "update_wiki"
        ? {
            ...row,
            state: "up_to_date" as const,
            lastCompletedAt: "2026-07-01T08:00:00Z",
            lastCompletedTaskId: taskId,
          }
        : row),
      recentRuns: [],
    };

    render(<WorkflowsOverviewView overview={snapshot} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    const updateRow = within(available).getByText("workflows.kind.update_wiki").closest<HTMLElement>('[role="listitem"]')!;
    expect(within(updateRow).getByRole("button", { name: /^workflows\.action\.view:/ })).toBeDisabled();
  });

  it("renders only the first backend recommendation", () => {
    const duplicateRecommendations: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => ({ ...row, recommended: true })),
      recentRuns: [],
    };

    const { container } = render(<WorkflowsOverviewView overview={duplicateRecommendations} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    expect(screen.getAllByText("workflows.recommended")).toHaveLength(1);
    expect(container.querySelectorAll(".workflow-row .btn--primary")).toHaveLength(1);
  });

  it("formats recent run time with the selected application language", () => {
    const recent = workflowRun({ taskId: "recent-locale", kind: "health_check", displayStatus: "completed", updatedAt: "2026-08-10T08:05:00Z", completedAt: "2026-08-10T08:05:00Z" });
    const snapshot = { ...overview, recentRuns: [recent] };
    const props = { overview: snapshot, overviewStatus: "ready" as const, error: null, onRetry: vi.fn(), onPrepare: vi.fn(), onPrerequisite: vi.fn(), onOpenRun: vi.fn(), onContinueQueue: vi.fn() };
    const view = render(<WorkflowsOverviewView {...props} />);
    const time = screen.getByRole("region", { name: "workflows.overview.recent" }).querySelector("time")!;
    const enLabel = new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short" }).format(new Date(recent.updatedAt));
    expect(time).toHaveTextContent(enLabel);

    i18nMocks.language = "zh-CN";
    view.rerender(<WorkflowsOverviewView {...props} />);
    const zhLabel = new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(recent.updatedAt));
    expect(time).toHaveTextContent(zhLabel);
    expect(zhLabel).not.toBe(enLabel);
  });

  it("uses overview activeTaskId when the attention run is outside the recent snapshot", () => {
    const handleOpenRun = vi.fn();
    const snapshot: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "health_check"
        ? { ...row, state: "running" as const, activeTaskId: "older-running-health" }
        : row.kind === "update_wiki"
          ? { ...row, state: "failed" as const, activeTaskId: "newer-failed-update" }
          : row),
      recentRuns: [],
    };

    render(<WorkflowsOverviewView overview={snapshot} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={handleOpenRun} onContinueQueue={vi.fn()} />);

    const attention = screen.getByRole("region", { name: "workflows.overview.attention" });
    expect(within(attention).getByText("workflows.status.running")).toBeInTheDocument();
    fireEvent.click(within(attention).getByRole("button", { name: /^workflows\.action\.viewProgress:/ }));
    expect(handleOpenRun).toHaveBeenCalledWith("older-running-health");
  });

  it("keeps prerequisite guidance ahead of queueing when another workflow is active", () => {
    const active = workflowRun({ taskId: "running-update", kind: "update_wiki", displayStatus: "running" });
    const prerequisite = { code: "WORKFLOW_ROUTE_REQUIRED", messageKey: "workflows.prerequisite.routeRequired", blocking: true, action: "configure_execution_route" as const };
    const handlePrerequisite = vi.fn();
    const snapshot: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "update_wiki"
        ? { ...row, state: "running" as const, activeTaskId: active.taskId }
        : row.kind === "generate_content"
          ? { ...row, prerequisite }
          : row),
      recentRuns: [],
    };

    render(<WorkflowsOverviewView overview={snapshot} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={handlePrerequisite} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    const generateRow = within(available).getByText("workflows.kind.generate_content").closest<HTMLElement>('[role="listitem"]')!;
    expect(within(generateRow).getByRole("button", { name: /^workflows\.action\.run:/ })).toBeInTheDocument();
    expect(within(generateRow).queryByRole("button", { name: /^workflows\.action\.queue:/ })).not.toBeInTheDocument();
  });

  it("gives repeated overview actions workflow-specific accessible names", () => {
    render(<WorkflowsOverviewView overview={{ ...overview, recentRuns: [] }} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    expect(screen.getByRole("button", { name: "workflows.action.run: workflows.kind.update_wiki" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "workflows.action.run: workflows.kind.health_check" })).toBeInTheDocument();
  });

  it("keeps failed recovery in attention without treating it as an active queue owner", () => {
    const failed = workflowRun({ taskId: "failed-update", kind: "update_wiki", displayStatus: "failed" });
    const failedOverview: WorkflowsOverview = {
      ...overview,
      rows: overview.rows.map((row) => row.kind === "update_wiki"
        ? { ...row, state: "failed" as const, activeTaskId: failed.taskId }
        : row),
      recentRuns: [],
    };

    render(<WorkflowsOverviewView overview={failedOverview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    const attention = screen.getByRole("region", { name: "workflows.overview.attention" });
    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    expect(within(attention).getByText("workflows.status.failed")).toBeInTheDocument();
    expect(within(available).queryByRole("button", { name: /^workflows\.action\.queue:/ })).not.toBeInTheDocument();
    expect(within(available).getAllByRole("button", { name: /^workflows\.action\.run:/ })).toHaveLength(2);
  });

  it("keeps all three workflows discoverable without a project and expands the project prerequisite from Run", () => {
    const handlePrerequisite = vi.fn();
    const prerequisite = { code: "WORKFLOW_PROJECT_REQUIRED", messageKey: "workflows.prerequisite.openOrCreateProject", blocking: true, action: "open_or_create_project" as const };
    const noProjectOverview: WorkflowsOverview = {
      schemaVersion: 1,
      projectAccess: null,
      rows: overview.rows.map((row) => ({ ...row, state: "needs_prerequisite", recommended: false, prerequisite })),
      recentRuns: [],
    };

    render(<WorkflowsOverviewView overview={noProjectOverview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={handlePrerequisite} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);

    const available = screen.getByRole("region", { name: "workflows.overview.available" });
    expect(within(available).getAllByRole("listitem")).toHaveLength(3);
    expect(screen.getByText("workflows.overview.noRecentRuns")).toBeInTheDocument();
    fireEvent.click(within(available).getAllByRole("button", { name: /^workflows\.action\.run:/ })[0]!);
    expect(handlePrerequisite).toHaveBeenCalledWith("open_or_create_project");
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

  it("orders non-Health preparation decisions and keeps technical details collapsed", () => {
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-generate",
      preparationRevision: "revision-generate",
      projectAccess: overview.projectAccess!,
      kind: "generate_content",
      scope: {
        kind: "generate_content",
        artifactType: "knowledge_card",
        pagePaths: ["wiki/alpha.md", "wiki/中文.md"],
        outputPath: "exports/cards.html",
      },
      baseline: { fingerprint: "baseline-fingerprint", capturedAt: "2026-08-01T00:00:00Z", itemCount: 2 },
      route: { kind: "byok", provider: "open_ai", model: "gpt-5", routeRevision: "route-a" },
      prerequisites: [],
      output: { labelKey: "workflows.output.artifact", location: "exports/cards.html", mayChangeWiki: false },
      gitPolicy: "not_required",
      requiresScopeConfirmation: false,
      quickRerunEligible: true,
      availableWikiPages: ["wiki/alpha.md", "wiki/中文.md"],
      availableRoutes: [{ kind: "byok", provider: "open_ai" }],
    } satisfies WorkflowPreparation;

    const view = render(<WorkflowPreparationView preparation={preparation} onBack={vi.fn()} onStart={vi.fn()} onPrerequisite={vi.fn()} onReprepare={vi.fn()} />);
    const steps = [...view.container.querySelectorAll<HTMLElement>("[data-decision-step]")]
      .map((node) => node.dataset.decisionStep);

    expect(steps).toEqual(["1", "2", "3", "4", "5", "6", "7", "8"]);
    const details = view.container.querySelector<HTMLDetailsElement>(".workflow-execution-details");
    expect(details).not.toHaveAttribute("open");
    expect(within(details!).getByText("baseline-fingerprint")).toBeInTheDocument();
    expect(within(details!).getByText("gpt-5")).toBeInTheDocument();
    expect(within(details!).getByText("html-knowledge-card")).toBeInTheDocument();
    expect(view.container.querySelector("[data-decision-step='1']")).toHaveTextContent("workflows.kind.generate_content.description");
  });

  it("requires first-run scope confirmation but keeps an eligible quick rerun explicit", () => {
    const start = vi.fn();
    const firstRun = {
      schemaVersion: 1,
      preparationId: "prep-first",
      preparationRevision: "revision-first",
      projectAccess: overview.projectAccess!,
      kind: "update_wiki",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [{ sourceId: "source-a", versionId: "v1" }] },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 1 },
      route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route-a" },
      prerequisites: [],
      output: { labelKey: "workflows.output.wiki", location: "wiki", mayChangeWiki: true },
      gitPolicy: "required_before_write",
      requiresScopeConfirmation: true,
      quickRerunEligible: false,
      availableSourceVersions: [{ sourceId: "source-a", versionId: "v1" }],
    } satisfies WorkflowPreparation;
    const props = { onBack: vi.fn(), onStart: start, onPrerequisite: vi.fn(), onReprepare: vi.fn() };
    const view = render(<WorkflowPreparationView preparation={firstRun} {...props} />);

    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();
    fireEvent.click(screen.getByLabelText("workflows.confirm.scope"));
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.clearSelection" }));
    expect(screen.getByLabelText("workflows.confirm.scope")).not.toBeChecked();
    fireEvent.click(screen.getByLabelText("source-a:v1"));
    fireEvent.click(screen.getByLabelText("workflows.confirm.scope"));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(start).toHaveBeenCalledOnce();

    view.rerender(<WorkflowPreparationView preparation={{ ...firstRun, preparationRevision: "revision-rerun", requiresScopeConfirmation: false, quickRerunEligible: true }} {...props} />);
    expect(screen.queryByLabelText("workflows.confirm.scope")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "workflows.action.runAgain" })).toBeEnabled();
  });

  it("explains Update Wiki no-change and invalid exclusion states without starting", () => {
    const start = vi.fn();
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-no-change",
      preparationRevision: "revision-no-change",
      projectAccess: overview.projectAccess!,
      kind: "update_wiki",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 0 },
      route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route-a" },
      prerequisites: [],
      output: { labelKey: "workflows.output.wiki", location: "wiki", mayChangeWiki: true },
      gitPolicy: "required_before_write",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
      availableSourceVersions: [{ sourceId: "source-a", versionId: "v1" }],
    } satisfies WorkflowPreparation;
    const view = render(<WorkflowPreparationView preparation={preparation} onBack={vi.fn()} onStart={start} onPrerequisite={vi.fn()} onReprepare={vi.fn()} />);

    expect(screen.getByText("workflows.preparation.noChanges")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();

    view.rerender(<WorkflowPreparationView preparation={{ ...preparation, preparationId: "prep-selection", preparationRevision: "revision-selection", scope: { ...preparation.scope, sourceVersions: [{ sourceId: "source-a", versionId: "v1" }] }, baseline: { ...preparation.baseline, itemCount: 1 } }} onBack={vi.fn()} onStart={start} onPrerequisite={vi.fn()} onReprepare={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.clearSelection" }));
    expect(screen.getByRole("alert")).toHaveTextContent("workflows.preparation.invalid.updateWikiEmpty");
    expect(screen.getByRole("button", { name: "workflows.action.updatePreparation" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();
    expect(start).not.toHaveBeenCalled();
  });

  it("covers multi-page generation, whole-project reports, route setup, and submitting state", () => {
    const prerequisite = vi.fn();
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-generation-states",
      preparationRevision: "revision-generation-states",
      projectAccess: overview.projectAccess!,
      kind: "generate_content",
      scope: { kind: "generate_content", artifactType: "knowledge_card", pagePaths: ["wiki/a.md", "wiki/b.md"], outputPath: "exports/cards.html" },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 2 },
      route: null,
      prerequisites: [{ code: "ROUTE_REQUIRED", messageKey: "workflows.prerequisite.configureExecutionRoute", blocking: true, action: "configure_execution_route" }],
      output: { labelKey: "workflows.output.artifact", location: "exports/cards.html", mayChangeWiki: false },
      gitPolicy: "not_required",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
      availableWikiPages: ["wiki/a.md", "wiki/b.md"],
    } satisfies WorkflowPreparation;
    const view = render(<WorkflowPreparationView preparation={preparation} onBack={vi.fn()} onStart={vi.fn()} onPrerequisite={prerequisite} onReprepare={vi.fn()} />);

    expect(screen.getByText("workflows.preparation.generate.knowledge_card")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.openSettings" }));
    expect(prerequisite).toHaveBeenCalledWith("configure_execution_route");
    fireEvent.change(screen.getByLabelText("workflows.preparation.artifactType"), { target: { value: "project_report" } });
    expect(screen.getByText("workflows.preparation.generate.project_report")).toBeInTheDocument();
    expect(screen.getByText("workflows.preparation.fixedScopePending")).toBeInTheDocument();
    expect(screen.queryByLabelText("wiki/a.md")).not.toBeInTheDocument();

    useWorkflowStore.setState({ operations: { [`start:${preparation.preparationId}`]: { requestId: 1, pending: true, error: null } } });
    view.rerender(<WorkflowPreparationView preparation={preparation} onBack={vi.fn()} onStart={vi.fn()} onPrerequisite={prerequisite} onReprepare={vi.fn()} />);
    expect(screen.getByRole("button", { name: "workflows.action.starting" })).toBeDisabled();
  });

  it("keeps Health route presentation fail-closed while Decision Gate H is unresolved", () => {
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-health-gate",
      preparationRevision: "revision-health-gate",
      projectAccess: overview.projectAccess!,
      kind: "health_check",
      scope: { kind: "health_check", mode: "complete" },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 42 },
      route: { kind: "agent", agent: "codex", model: "gpt-5", routeRevision: "route-agent" },
      prerequisites: [{ code: "ROUTE_CHOICE", messageKey: "choice", blocking: true, action: "choose_execution_route" }],
      output: { labelKey: "workflows.output.healthReport", location: null, mayChangeWiki: false },
      gitPolicy: "not_required",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
      availableRoutes: [
        { kind: "agent", agent: "codex" },
        { kind: "byok", provider: "ollama" },
      ],
    } satisfies WorkflowPreparation;
    const prerequisite = vi.fn();
    const view = render(<WorkflowPreparationView preparation={preparation} onBack={vi.fn()} onStart={vi.fn()} onPrerequisite={prerequisite} onReprepare={vi.fn()} />);

    expect(view.container.querySelector("[data-decision-step='2']")).toHaveTextContent("workflows.preparation.fixedScopeCount");
    expect(view.container.querySelector("[data-decision-step='6']")).toHaveTextContent("workflows.route.agent");
    expect(view.container.querySelector("[data-decision-step='6']")).not.toHaveTextContent("codex");
    const routeOverride = screen.getByLabelText("workflows.preparation.routeOverride");
    expect(within(routeOverride).queryByRole("option", { name: /codex/ })).not.toBeInTheDocument();
    expect(within(routeOverride).getByRole("option", { name: /ollama/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.chooseRoute" }));
    expect(view.container.querySelector(".workflow-execution-details")).toHaveAttribute("open");
    expect(prerequisite).not.toHaveBeenCalled();
  });

  it("preserves a one-run route override across later preparation edits", () => {
    const reprepare = vi.fn();
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-route-draft",
      preparationRevision: "revision-route-draft-a",
      projectAccess: overview.projectAccess!,
      kind: "generate_content",
      scope: { kind: "generate_content", artifactType: "knowledge_card", pagePaths: ["wiki/a.md"], outputPath: null },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 1 },
      route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route-default" },
      prerequisites: [],
      output: { labelKey: "workflows.output.export", location: "exports/default.html", mayChangeWiki: false },
      gitPolicy: "not_required",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
      availableWikiPages: ["wiki/a.md"],
      availableRoutes: [{ kind: "byok", provider: "ollama" }, { kind: "byok", provider: "open_ai" }],
    } satisfies WorkflowPreparation;
    const props = { onBack: vi.fn(), onStart: vi.fn(), onPrerequisite: vi.fn(), onReprepare: reprepare };
    const view = render(<WorkflowPreparationView preparation={preparation} {...props} />);

    fireEvent.change(screen.getByLabelText("workflows.preparation.routeOverride"), { target: { value: "byok:open_ai" } });
    expect(view.container.querySelector("[data-decision-step='3']")).toHaveTextContent("exports/default.html");
    expect(view.container.querySelector("[data-decision-step='3']")).not.toHaveTextContent("workflows.output.defaultPending");
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.updatePreparation" }));
    expect(reprepare).toHaveBeenLastCalledWith(preparation.scope, { kind: "byok", provider: "open_ai" });

    view.rerender(<WorkflowPreparationView preparation={{ ...preparation, preparationRevision: "revision-route-draft-b", route: { kind: "byok", provider: "open_ai", model: "gpt-5", routeRevision: "route-override" } }} {...props} />);
    fireEvent.change(screen.getByLabelText("workflows.preparation.outputPath"), { target: { value: "exports/b.html" } });
    const updateButton = screen.getByRole("button", { name: "workflows.action.updatePreparation" });
    expect(view.container.querySelector(".workflow-execution-details")).not.toContainElement(updateButton);
    fireEvent.click(updateButton);
    expect(reprepare).toHaveBeenLastCalledWith(expect.objectContaining({ outputPath: "exports/b.html" }), { kind: "byok", provider: "open_ai" });
  });

  it("keeps edited Settings drafts and treats route choice as an in-place advanced action", () => {
    const prerequisite = vi.fn();
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-settings-draft",
      preparationRevision: "revision-settings-draft",
      projectAccess: overview.projectAccess!,
      kind: "generate_content",
      scope: { kind: "generate_content", artifactType: "knowledge_card", pagePaths: ["wiki/a.md"], outputPath: "exports/a.html" },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 2 },
      route: null,
      prerequisites: [{ code: "ROUTE_REQUIRED", messageKey: "route", blocking: true, action: "configure_execution_route" }],
      output: { labelKey: "workflows.output.export", location: "exports/a.html", mayChangeWiki: false },
      gitPolicy: "not_required",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
      availableWikiPages: ["wiki/a.md", "wiki/b.md"],
    } satisfies WorkflowPreparation;
    const props = { onBack: vi.fn(), onStart: vi.fn(), onPrerequisite: prerequisite, onReprepare: vi.fn() };
    const view = render(<WorkflowPreparationView preparation={preparation} {...props} />);
    fireEvent.click(screen.getByLabelText("wiki/b.md"));
    fireEvent.change(screen.getByLabelText("workflows.preparation.outputPath"), { target: { value: "exports/draft.html" } });
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.openSettings" }));
    expect(prerequisite).toHaveBeenCalledWith("configure_execution_route", {
      scope: expect.objectContaining({ pagePaths: ["wiki/a.md", "wiki/b.md"], outputPath: "exports/draft.html" }),
      routeSelection: null,
    });

    view.rerender(<WorkflowPreparationView preparation={{ ...preparation, prerequisites: [{ code: "ROUTE_CHOICE", messageKey: "choice", blocking: true, action: "choose_execution_route" }], availableRoutes: [{ kind: "byok", provider: "ollama" }] }} {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.chooseRoute" }));
    expect(view.container.querySelector(".workflow-execution-details")).toHaveAttribute("open");
    expect(prerequisite).toHaveBeenCalledTimes(1);
  });

  it("supports Full to Changed auto-detection and shows truthful draft output and fixed-scope counts", () => {
    const reprepare = vi.fn();
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-update-mode",
      preparationRevision: "revision-update-mode",
      projectAccess: overview.projectAccess!,
      kind: "update_wiki",
      scope: { kind: "update_wiki", mode: "full_recompile", sourceVersions: [{ sourceId: "source-a", versionId: "v1" }] },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-01T00:00:00Z", itemCount: 1 },
      route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route" },
      prerequisites: [],
      output: { labelKey: "workflows.output.wiki", location: "wiki", mayChangeWiki: true },
      gitPolicy: "required_before_write",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
      availableSourceVersions: [{ sourceId: "source-a", versionId: "v1" }],
    } satisfies WorkflowPreparation;
    const props = { onBack: vi.fn(), onStart: vi.fn(), onPrerequisite: vi.fn(), onReprepare: reprepare };
    const view = render(<WorkflowPreparationView preparation={preparation} {...props} />);
    fireEvent.click(screen.getByLabelText("workflows.mode.changedSources"));
    expect(screen.getByText("workflows.preparation.autoDetectChanges")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.updatePreparation" }));
    expect(reprepare).toHaveBeenLastCalledWith({ kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, null);

    const generation = {
      ...preparation,
      preparationId: "prep-project-report",
      preparationRevision: "revision-project-report",
      kind: "generate_content",
      scope: { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: "exports/report.html" },
      output: { labelKey: "workflows.output.export", location: "exports/report.html", mayChangeWiki: false },
      gitPolicy: "not_required",
      baseline: { ...preparation.baseline, itemCount: 42 },
      availableSourceVersions: undefined,
      availableWikiPages: ["wiki/a.md"],
    } satisfies WorkflowPreparation;
    view.rerender(<WorkflowPreparationView preparation={generation} {...props} />);
    expect(view.container.querySelector("[data-decision-step='2']")).toHaveTextContent("workflows.preparation.fixedScopeCount");
    fireEvent.change(screen.getByLabelText("workflows.preparation.outputPath"), { target: { value: "" } });
    expect(view.container.querySelector("[data-decision-step='3']")).toHaveTextContent("workflows.output.defaultPending");
    expect(view.container.querySelector("[data-decision-step='3']")).not.toHaveTextContent("exports/report.html");
  });

  it("renders indeterminate counts without claiming 100 percent completion", () => {
    render(<WorkflowPipeline stages={(["pending", "running", "completed", "failed", "waiting", "skipped"] as const).map((status, index) => ({ id: status, ordinal: index + 1, status, labelKey: status, startedAt: null, completedAt: null, currentItem: status === "running" ? "wiki/中文.md" : null, progress: status === "running" ? { current: 3, total: null } : null, decision: null }))} />);
    expect(screen.getByText("workflows.progress.current")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "workflows.pipeline.overallProgress" })).not.toHaveAttribute("value");
    expect(screen.queryByRole("progressbar", { name: "running" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(6);
    expect(WORKFLOW_STATUSES).toEqual(["queued", "running", "waiting_for_confirmation", "completed", "failed", "cancelled", "interrupted"]);
  });

  it("prioritizes queue-owning work before terminal recovery", () => {
    const base = { taskId: "running", displayStatus: "running" } as WorkflowRun;
    expect(attentionRun([{ ...base }, { ...base, taskId: "failed", displayStatus: "failed" }, { ...base, taskId: "waiting", displayStatus: "waiting_for_confirmation" }])?.taskId).toBe("waiting");
    expect(attentionRun([{ ...base }, { ...base, taskId: "failed", displayStatus: "failed" }])?.taskId).toBe("running");
    expect(attentionRun([base])?.taskId).toBe("running");
    expect(attentionRun([{ ...base, taskId: "queued", displayStatus: "queued" }])?.taskId).toBe("queued");
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
    expect(screen.getByText("workflows.context.history")).toBeInTheDocument();
    expect(screen.queryByText("workflows.context.project")).not.toBeInTheDocument();
    expect(screen.queryByText("stale-se")).not.toBeInTheDocument();
  });

  it("renders complete, surface-owned right-panel facts without leaking stale preparation actions", () => {
    const run = workflowRun({
      taskId: "detail-run",
      kind: "generate_content",
      displayStatus: "waiting_for_confirmation",
      route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route-a" },
      scope: {
        kind: "generate_content",
        artifactType: "project_report",
        pagePaths: [],
        outputPath: "exports/项目报告.html",
      },
      stages: [{
        id: "write-export",
        ordinal: 8,
        status: "waiting",
        labelKey: "workflows.stage.generateContent.writeExport",
        startedAt: "2026-08-10T08:00:00Z",
        completedAt: null,
        currentItem: "exports/项目报告.html",
        progress: null,
        decision: null,
      }],
      currentStageId: "write-export",
      pendingAction: {
        id: "action-a",
        actionType: "overwrite_file",
        riskLevel: "high",
        affectedPaths: ["exports/项目报告.html"],
        candidate: null,
        expiresAt: null,
        checkpointHash: "checkpoint-a",
      },
    });
    const preparation = {
      schemaVersion: 1,
      preparationId: "prep-context",
      preparationRevision: "prep-context-revision",
      projectAccess: overview.projectAccess!,
      kind: "update_wiki",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
      baseline: { fingerprint: "baseline", capturedAt: "2026-08-10T08:00:00Z", itemCount: 12 },
      route: { kind: "byok", provider: "ollama", model: "qwen", routeRevision: "route-b" },
      prerequisites: [{ code: "DIRTY_GIT", messageKey: "workflows.prerequisite.resolveDirtyGit", blocking: true, action: "resolve_dirty_git" }],
      output: { labelKey: "workflows.output.wiki", location: "wiki", mayChangeWiki: true },
      gitPolicy: "required_before_write",
      requiresScopeConfirmation: false,
      quickRerunEligible: false,
    } satisfies WorkflowPreparation;
    useProjectStore.setState({ currentProject: projectSummary });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({
      overview: { ...overview, recentRuns: [run] },
      runs: [run],
      preparation,
      surface: "preparation",
    });

    const view = render(<WorkflowsRightPanel />);
    expect(screen.getByText("workflows.context.prerequisites")).toBeInTheDocument();
    expect(screen.getByText("workflows.prerequisite.resolveDirtyGit")).toBeInTheDocument();
    expect(screen.getByText("workflows.git.required_before_write")).toBeInTheDocument();
    expect(screen.getByText("wiki")).toHaveAttribute("title", "wiki");
    expect(screen.queryByText("workflows.context.queue")).not.toBeInTheDocument();

    useWorkflowStore.setState({ surface: "detail", selectedTaskId: run.taskId, preparation: null });
    view.rerender(<WorkflowsRightPanel />);
    expect(screen.getByText("workflows.context.currentStage")).toBeInTheDocument();
    expect(screen.getByText("workflows.stage.generateContent.writeExport")).toBeInTheDocument();
    expect(screen.getByText("workflows.stageStatus.waiting")).toBeInTheDocument();
    expect(screen.getByText("workflows.status.waiting_for_confirmation")).toBeInTheDocument();
    expect(screen.getByText("workflows.gitState.clean")).toBeInTheDocument();
    expect(screen.getByText("exports/项目报告.html")).toHaveAttribute("title", "exports/项目报告.html");
    expect(screen.queryByText("waiting_for_confirmation")).not.toBeInTheDocument();
    expect(screen.queryByText("overwrite_file")).not.toBeInTheDocument();

    const completedUpdate = workflowRun({
      taskId: "completed-update",
      kind: "update_wiki",
      displayStatus: "completed",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
      stages: [{
        id: "record-result",
        ordinal: 9,
        status: "completed",
        labelKey: "workflows.stage.updateWiki.recordResult",
        startedAt: "2026-08-10T08:00:00Z",
        completedAt: "2026-08-10T08:01:00Z",
        currentItem: null,
        progress: null,
        decision: null,
      }],
      currentStageId: "record-result",
      result: {
        kind: "update_wiki",
        created: 0,
        updated: 1,
        skipped: 0,
        deleted: 0,
        conflicted: 0,
        affectedPaths: ["wiki/page.md"],
        checkpointHash: "checkpoint-completed",
        finalCommit: "commit-completed",
      },
    });
    useWorkflowStore.setState({ runs: [completedUpdate], selectedTaskId: completedUpdate.taskId });
    view.rerender(<WorkflowsRightPanel />);
    expect(screen.getByText("wiki/page.md")).toHaveAttribute("title", "wiki/page.md");
    expect(screen.getByText("checkpoint-completed")).toBeInTheDocument();
    expect(screen.getByText("commit-completed")).toBeInTheDocument();
    expect(screen.getByText("workflows.stageStatus.completed")).toBeInTheDocument();

    useWorkflowStore.setState({
      surface: "history",
      selectedTaskId: null,
      historyKind: "generate_content",
      historyStatus: "completed",
      historyRuns: [{
        schemaVersion: 1,
        taskId: "history-a",
        projectId: "project-a",
        canonicalIdentityKey: "identity-a",
        identityRevision: "revision-a",
        kind: "generate_content",
        displayStatus: "completed",
        retry: null,
        startedAt: "2026-08-09T08:00:00Z",
        updatedAt: "2026-08-09T08:01:00Z",
        completedAt: "2026-08-09T08:01:00Z",
      }],
    });
    view.rerender(<WorkflowsRightPanel />);
    expect(screen.getByText("workflows.context.history")).toBeInTheDocument();
    expect(screen.getByText("workflows.kind.generate_content")).toBeInTheDocument();
    expect(screen.getByText("workflows.status.completed")).toBeInTheDocument();
    expect(screen.queryByText("workflows.context.preparation")).not.toBeInTheDocument();
    expect(screen.queryByText("workflows.context.queue")).not.toBeInTheDocument();
  });

  it("renders full-project overview facts only from the backend context summary", () => {
    const queuedRuns = Array.from({ length: 5 }, (_, index) => ({
      taskId: `summary-queue-${index}`,
      kind: "update_wiki" as const,
      queuePosition: index + 1,
      startedAt: `2026-08-10T0${index}:00:00Z`,
    }));
    const staleGenericRun = workflowRun({
      taskId: "stale-generic-queue",
      kind: "health_check",
      displayStatus: "queued",
    });
    const recentRuns = Array.from({ length: 5 }, (_, index) => workflowRun({
      taskId: `recent-unrelated-${index}`,
      kind: "update_wiki",
      displayStatus: "completed",
    }));
    useProjectStore.setState({ currentProject: projectSummary });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({
      overview: {
        ...overview,
        recentRuns,
        contextSummary: {
          pendingSourceCount: 4,
          lastHealth: { taskId: "older-health", completedAt: "2026-08-01T00:00:00Z", errorCount: 1, warningCount: 2, infoCount: 3 },
          recentArtifact: { taskId: "older-artifact", completedAt: "2026-07-31T00:00:00Z", artifactType: "project_report" },
          queueCount: 6,
          queuedRuns,
        },
      },
      runs: [staleGenericRun],
      surface: "overview",
    });

    render(<WorkflowsRightPanel />);

    expect(screen.getByText("4", { selector: "dd" })).toBeInTheDocument();
    expect(screen.getByText("6", { selector: "dd" })).toBeInTheDocument();
    expect(screen.getByText("workflows.context.healthSummary")).toBeInTheDocument();
    expect(screen.getByText("workflows.artifact.projectReport")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "workflows.context.openQueuedRun" })).toHaveLength(5);
    expect(screen.queryByText("stale-generic-queue")).not.toBeInTheDocument();
  });

  it("does not present a missing context summary as an empty queue", () => {
    useProjectStore.setState({ currentProject: projectSummary });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({ overview: { ...overview, contextSummary: undefined }, surface: "overview" });

    render(<WorkflowsRightPanel />);

    expect(screen.getAllByText("workflows.context.summaryUnavailable").length).toBeGreaterThan(0);
    expect(screen.queryByText("workflows.context.queueEmpty")).not.toBeInTheDocument();
  });

  it("keeps the authoritative Workflows terminology explicit and locale keys in parity", () => {
    const enWorkflowKeys = Object.keys(enLocale).filter((key) => key.startsWith("workflows.")).sort();
    const zhWorkflowKeys = Object.keys(zhLocale).filter((key) => key.startsWith("workflows.")).sort();

    expect(zhWorkflowKeys).toEqual(enWorkflowKeys);
    expect(enLocale["workflows.result.artifactType"]).toBe("Output type");
    expect(zhLocale["workflows.result.artifactType"]).toBe("输出类型");
    expect(enLocale["workflows.artifact.beautifulRead"]).toBe("Comfortable reading page");
    expect(zhLocale["workflows.artifact.beautifulRead"]).toBe("舒适阅读页");
    expect(enLocale["workflows.artifact.projectReport"]).toBe("Project report");
    expect(zhLocale["workflows.context.git"]).toBe("Git 检查点");
  });

  it("keeps long English context labels semantic and keyboard operable under enlarged text", () => {
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
    const { container } = render(<WorkflowsOverviewView overview={null} overviewStatus="loading" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("heading", { name: "workflows.loading.title" })).toBeInTheDocument();
    expect(screen.queryByText("workflows.noProject.title")).not.toBeInTheDocument();
    expect(container.querySelector(".workflows-overview.is-loading")).toBeInTheDocument();
    expect(container.querySelectorAll(".workflow-row")).toHaveLength(3);
    expect(container.querySelectorAll(".workflow-recent-row")).toHaveLength(5);
  });

  it("uses the same icon-and-label status treatment across overview, history, and task detail without repeating detail status", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const completed = workflowRun({
      taskId: "status-completed",
      kind: "health_check",
      displayStatus: "completed",
      completedAt: "2026-08-10T08:05:00Z",
    });

    const overviewView = render(<WorkflowsOverviewView overview={overview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(overviewView.container.querySelectorAll(".workflow-row .workflow-status svg")).toHaveLength(3);
    overviewView.unmount();

    const historyView = render(<WorkflowHistoryView runs={[completed]} onBack={vi.fn()} onOpen={vi.fn()} onRetry={vi.fn()} onLoadMore={vi.fn()} onFilter={vi.fn()} />);
    expect(historyView.container.querySelector(".workflow-history__status.workflow-status svg")).toBeInTheDocument();
    historyView.unmount();

    const detailView = render(<WorkflowTaskDetail run={completed} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    const heading = detailView.container.querySelector<HTMLElement>(".workflow-detail__heading")!;
    expect(heading.querySelector(".workflow-status svg")).toBeInTheDocument();
    expect(within(heading).getAllByText("workflows.status.completed")).toHaveLength(1);
  });

  it("shows an actionable error when the overview request fails", () => {
    const retry = vi.fn();
    render(<WorkflowsOverviewView overview={null} overviewStatus="error" error={{ summary: "overview unavailable", technicalDetails: "OVERVIEW_FAILED" }} onRetry={retry} onPrepare={vi.fn()} onPrerequisite={vi.fn()} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
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
    render(<WorkflowsOverviewView overview={noProjectOverview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={vi.fn()} onPrerequisite={handlePrerequisite} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(screen.queryByText("workflows.prerequisite.openOrCreateProject")).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: /^workflows\.action\.run:/ })[0]!);
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
      view.rerender(<WorkflowsOverviewView overview={blockedOverview} overviewStatus="ready" error={null} onRetry={vi.fn()} onPrepare={prepare} onPrerequisite={handlePrerequisite} onOpenRun={vi.fn()} onContinueQueue={vi.fn()} />);
      fireEvent.click(screen.getAllByRole("button", { name: /^workflows\.action\.run:/ })[0]!);
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
      decisionReview: { reason: "review", counts: { created: 0, modified: 2, overwritten: 0, deleted: 0 }, userEditsDetected: false, fileDiffs: [] },
      startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:00:00Z", completedAt: null,
    };
    render(<WorkflowTaskDetail run={waiting} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    expect(screen.getByText("abc123")).toBeInTheDocument();
    expect(screen.getByText("wiki/甲.md")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.applyChanges" }));
    expect(controller.confirm).toHaveBeenCalledWith("waiting-a", "action-a");
  });

  it("presents confirmation risk and action types as localized product language in review order", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const waiting = workflowRun({
      taskId: "waiting-localized",
      kind: "update_wiki",
      displayStatus: "waiting_for_confirmation",
      pendingAction: {
        id: "action-localized",
        actionType: "batch_rewrite",
        riskLevel: "high",
        affectedPaths: ["wiki/冲突.md"],
        candidate: null,
        expiresAt: null,
        checkpointHash: "checkpoint-123",
      },
      decisionReview: {
        reason: "External edits overlap this candidate.",
        counts: { created: 0, modified: 1, overwritten: 1, deleted: 0 },
        userEditsDetected: true,
        fileDiffs: [{ path: "wiki/冲突.md", diff: "baseline / current / candidate", kind: "three_way" }],
      },
    });

    const { container } = render(<WorkflowTaskDetail run={waiting} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);

    expect(screen.getByText("workflows.risk.high")).toBeInTheDocument();
    expect(screen.getByText("workflows.actionType.batch_rewrite")).toBeInTheDocument();
    expect(screen.queryByText("high")).not.toBeInTheDocument();
    expect(screen.queryByText("batch_rewrite")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("workflows.attention.userEditsConflict");
    expect(screen.getByRole("button", { name: "workflows.action.applyChanges" })).toHaveTextContent("workflows.action.applyChanges");

    const reason = screen.getByText("External edits overlap this candidate.");
    const counts = container.querySelector(".workflow-decision-counts")!;
    const paths = screen.getByRole("region", { name: "workflows.attention.paths" });
    const edits = screen.getByRole("alert");
    const checkpoint = screen.getByText("checkpoint-123").closest(".workflow-decision-checkpoint")!;
    const diff = screen.getByRole("group", { name: "workflows.diff.threeWay" });
    expect(reason.compareDocumentPosition(counts) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(counts.compareDocumentPosition(paths) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(paths.compareDocumentPosition(edits) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(edits.compareDocumentPosition(checkpoint) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(checkpoint.compareDocumentPosition(diff) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("blocks apply while authoritative decision review hydration is pending", () => {
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

    expect(screen.getByRole("button", { name: /workflows.action.applyChanges/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "workflows.action.discard" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("workflows.attention.reviewLoading");
    expect(screen.queryByText("workflows.result.no")).not.toBeInTheDocument();
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

  it.each([
    {
      name: "update wiki",
      run: workflowRun({
        taskId: "result-update",
        kind: "update_wiki",
        displayStatus: "completed",
        completedAt: "2026-08-10T08:02:00Z",
        route: { kind: "byok", provider: "open_ai", model: "gpt-5", routeRevision: "route-a" },
        result: {
          kind: "update_wiki",
          created: 2,
          updated: 3,
          skipped: 4,
          deleted: 0,
          conflicted: 0,
          affectedPaths: ["wiki/a.md"],
          checkpointHash: "checkpoint-a",
          finalCommit: "commit-a",
          internalSecret: "must-not-render",
        } as WorkflowRun["result"],
      }),
      title: "workflows.result.update_wiki.title",
      action: "workflows.action.viewUpdates",
    },
    {
      name: "health check",
      run: workflowRun({
        taskId: "result-health",
        kind: "health_check",
        displayStatus: "completed",
        scope: { kind: "health_check", mode: "local_quick" },
        completedAt: "2026-08-10T08:02:00Z",
        route: { kind: "local", routeRevision: "local" },
        result: {
          kind: "health_check",
          reportId: "report-a",
          persistent: true,
          errorCount: 1,
          warningCount: 2,
          infoCount: 3,
          coverage: { mode: "local_quick", scannedPages: 42, deepCoveredPages: null, deepTruncated: false },
          findingsByType: { missing_frontmatter: 2, dead_link: 1 },
        },
      }),
      title: "workflows.result.health_check.title",
      action: "workflows.action.openLintResults",
    },
    {
      name: "generated content",
      run: workflowRun({
        taskId: "result-generate",
        kind: "generate_content",
        displayStatus: "completed",
        scope: { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: null },
        completedAt: "2026-08-10T08:02:00Z",
        route: { kind: "byok", provider: "open_ai", model: "gpt-5", routeRevision: "route-b" },
        result: { kind: "generate_content", artifactType: "project_report", recordId: "record-a", outputPaths: ["exports/report.html"], artifactCount: 7, validationPassed: true },
      }),
      title: "workflows.result.generate_content.title",
      action: "workflows.action.viewGeneratedResult",
    },
  ])("renders the $name typed result presenter without generic object dumping", ({ run, title, action }) => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;

    render(<WorkflowTaskDetail run={run} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);

    expect(screen.getByRole("region", { name: title })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: action })).toBeInTheDocument();
    expect(screen.getByText("workflows.result.duration")).toBeInTheDocument();
    expect(screen.getByText("workflows.result.route")).toBeInTheDocument();
    expect(screen.queryByText("must-not-render")).not.toBeInTheDocument();
    if (run.result?.kind === "health_check") {
      expect(screen.getByText("workflows.result.findingTypes")).toBeInTheDocument();
      expect(screen.getByText("lint.issueType.missing_frontmatter")).toBeInTheDocument();
      expect(screen.getByText("workflows.result.scannedPages")).toBeInTheDocument();
    }
    if (run.result?.kind === "generate_content") {
      expect(screen.getByText("7")).toBeInTheDocument();
    }
  });

  it("separates failed and cancelled recovery while keeping logs subordinate", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const failed = workflowRun({
      taskId: "failed-pipeline",
      kind: "update_wiki",
      displayStatus: "failed",
      currentStageId: "apply",
      stages: [
        { id: "prepare", ordinal: 1, status: "completed", labelKey: "stage.prepare", startedAt: "2026-08-10T08:00:00Z", completedAt: "2026-08-10T08:00:05Z", currentItem: null, progress: null, decision: null },
        { id: "apply", ordinal: 2, status: "failed", labelKey: "stage.apply", startedAt: "2026-08-10T08:00:05Z", completedAt: "2026-08-10T08:00:08Z", currentItem: "wiki/a.md", progress: null, decision: null },
      ],
      error: { code: "APPLY_FAILED", messageKey: "workflows.error.updateWikiFailed", recoverable: true, userActionRequired: true, suggestedAction: "prepare_again" },
    });
    const view = render(<WorkflowTaskDetail run={failed} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);

    const failureRegion = screen.getByRole("region", { name: "workflows.failure.title" });
    expect(failureRegion).toHaveTextContent("workflows.failure.mutation.unknown");
    expect(within(failureRegion).getByText("stage.apply")).toBeInTheDocument();
    expect(within(failureRegion).queryByText("apply")).not.toBeInTheDocument();
    expect(screen.getByText("workflows.prerequisiteAction.prepare_again")).toBeInTheDocument();
    expect(view.container.querySelector('details[data-stage-status="failed"]')).toHaveAttribute("open");
    const logs = screen.getByText("workflows.logs.title").closest("details")!;
    expect(logs).not.toHaveAttribute("open");

    view.rerender(<WorkflowTaskDetail run={{ ...failed, displayStatus: "interrupted" }} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("workflows.interrupted.title");
    expect(screen.queryByRole("region", { name: "workflows.failure.title" })).not.toBeInTheDocument();

    view.rerender(<WorkflowTaskDetail run={workflowRun({ taskId: "cancelled-pipeline", kind: "update_wiki", displayStatus: "cancelled" })} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("workflows.cancelled.description");
    expect(screen.queryByRole("region", { name: "workflows.failure.title" })).not.toBeInTheDocument();
  });

  it("exposes truthful overall and stage progress with expanded current work and real duration", () => {
    const stages: WorkflowRun["stages"] = [
      { id: "done", ordinal: 1, status: "completed", labelKey: "stage.done", startedAt: "2026-08-10T08:00:00Z", completedAt: "2026-08-10T08:00:05Z", currentItem: null, progress: null, decision: null },
      { id: "current", ordinal: 2, status: "running", labelKey: "stage.current", startedAt: "2026-08-10T08:00:05Z", completedAt: null, currentItem: "wiki/非常长的路径/页面.md", progress: { current: 8, total: 14 }, decision: null },
      { id: "future", ordinal: 3, status: "pending", labelKey: "stage.future", startedAt: null, completedAt: null, currentItem: null, progress: null, decision: null },
    ];

    const { container } = render(<WorkflowPipeline stages={stages} currentStageId="current" displayStatus="running" />);

    const overall = screen.getByRole("progressbar", { name: "workflows.pipeline.overallProgress" });
    expect(overall).toHaveAttribute("aria-valuetext", "workflows.pipeline.overallValue");
    expect(container.querySelector('details[data-stage-status="running"]')).toHaveAttribute("open");
    expect(container.querySelector('details[data-stage-status="completed"]')).not.toHaveAttribute("open");
    expect(screen.getByText("workflows.duration.seconds")).toBeInTheDocument();
    expect(screen.getByText("wiki/非常长的路径/页面.md")).toHaveAttribute("title", "wiki/非常长的路径/页面.md");
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

  it("renders localized linked attempts with duration, compact outcome, and recovery", () => {
    i18nMocks.language = "zh-CN";
    const onRetry = vi.fn();
    const completed = {
      schemaVersion: 1,
      taskId: "first",
      projectId: "project-a",
      canonicalIdentityKey: "identity-a",
      identityRevision: "revision-a",
      kind: "update_wiki",
      displayStatus: "completed",
      retry: null,
      outcome: { kind: "update_wiki", created: 2, updated: 3, skipped: 1 },
      startedAt: "2026-08-10T08:00:00Z",
      updatedAt: "2026-08-10T08:01:30Z",
      completedAt: "2026-08-10T08:01:30Z",
    } as WorkflowRunSummary;
    const failed = {
      ...completed,
      taskId: "retry",
      displayStatus: "failed",
      retry: { attemptOf: "first", attemptNumber: 2 },
      outcome: null,
      updatedAt: "2026-08-10T08:02:00Z",
      completedAt: null,
    } as WorkflowRunSummary;

    render(<WorkflowHistoryView runs={[failed, completed]} onBack={vi.fn()} onOpen={vi.fn()} onRetry={onRetry} onLoadMore={vi.fn()} onFilter={vi.fn()} />);

    expect(screen.getByText(new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(completed.updatedAt)))).toBeInTheDocument();
    expect(screen.getByText("workflows.duration.minutesSeconds")).toBeInTheDocument();
    expect(screen.getByText("workflows.history.outcome.updateWiki")).toBeInTheDocument();
    expect(screen.getAllByText("workflows.history.retryAttempt")).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: /^workflows\.action\.retry:/ }));
    expect(onRetry).toHaveBeenCalledWith("retry");
  });

  it("keeps partial-page retry numbering accurate and recovery names unique", () => {
    const failed = (taskId: string, updatedAt: string): WorkflowRunSummary => ({
      schemaVersion: 1,
      taskId,
      projectId: "project-a",
      canonicalIdentityKey: "identity-a",
      identityRevision: "revision-a",
      kind: "health_check",
      displayStatus: "failed",
      retry: { attemptOf: "original-on-another-page", attemptNumber: 2 },
      outcome: null,
      startedAt: updatedAt,
      updatedAt,
      completedAt: updatedAt,
    });

    render(<WorkflowHistoryView runs={[
      failed("attempt-2", "2026-08-10T08:02:10Z"),
      failed("attempt-3", "2026-08-10T08:02:40Z"),
    ]} onBack={vi.fn()} onOpen={vi.fn()} onRetry={vi.fn()} onLoadMore={vi.fn()} onFilter={vi.fn()} />);

    expect(screen.queryByText("workflows.history.attempts")).not.toBeInTheDocument();
    expect(screen.getAllByText("workflows.history.retryAttempt")).toHaveLength(2);
    const retries = screen.getAllByRole("button", { name: /^workflows\.action\.retry:/ });
    expect(retries).toHaveLength(2);
    expect(retries[0]).not.toHaveAccessibleName(retries[1]?.getAttribute("aria-label") ?? "");
  });

  it("renders compact key results for every workflow kind", () => {
    const base = {
      schemaVersion: 1,
      projectId: "project-a",
      canonicalIdentityKey: "identity-a",
      identityRevision: "revision-a",
      displayStatus: "completed" as const,
      retry: null,
      startedAt: "2026-08-10T08:00:00Z",
      updatedAt: "2026-08-10T08:01:00Z",
      completedAt: "2026-08-10T08:01:00Z",
    };
    const runs = [
      { ...base, taskId: "update", kind: "update_wiki", outcome: { kind: "update_wiki", created: 1, updated: 2, skipped: 3 } },
      { ...base, taskId: "health", kind: "health_check", outcome: { kind: "health_check", errorCount: 1, warningCount: 2, infoCount: 3 } },
      { ...base, taskId: "generate", kind: "generate_content", outcome: { kind: "generate_content", artifactType: "project_report", artifactCount: 2, validationPassed: true } },
    ] as WorkflowRunSummary[];

    render(<WorkflowHistoryView runs={runs} onBack={vi.fn()} onOpen={vi.fn()} onRetry={vi.fn()} onLoadMore={vi.fn()} onFilter={vi.fn()} />);

    expect(screen.getByText("workflows.history.outcome.updateWiki")).toBeInTheDocument();
    expect(screen.getByText("workflows.history.outcome.healthCheck")).toBeInTheDocument();
    expect(screen.getByText("workflows.history.outcome.generateContent")).toBeInTheDocument();
  });

  it("distinguishes first-run, filtered-empty, and filter-loading history", () => {
    const props = { runs: [], onBack: vi.fn(), onOpen: vi.fn(), onRetry: vi.fn(), onLoadMore: vi.fn(), onFilter: vi.fn() };
    const view = render(<WorkflowHistoryView {...props} />);
    expect(screen.getByText("workflows.history.emptyFirstRun")).toBeInTheDocument();

    useWorkflowStore.setState({ historyKind: "health_check", historyStatus: "failed" });
    view.rerender(<WorkflowHistoryView {...props} />);
    expect(screen.getByText("workflows.history.emptyFiltered")).toBeInTheDocument();

    useWorkflowStore.setState({
      operations: {
        "history:filter": { requestId: 1, pending: true, error: null },
      },
    });
    view.rerender(<WorkflowHistoryView {...props} />);
    expect(screen.getByRole("status")).toHaveTextContent("workflows.history.loading");
    expect(screen.queryByText("workflows.history.emptyFiltered")).not.toBeInTheDocument();
    view.unmount();
  });

  it("retries the current server-filtered history after a history error", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      overview,
      surface: "history",
      historyKind: "health_check",
      historyStatus: "failed",
      operations: {
        "history:page": { requestId: 1, pending: false, error: { summary: "history failed", technicalDetails: "cursor stale" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));

    expect(controller.filterHistory).toHaveBeenCalledWith("health_check", "failed");
  });

  it("continues a transient failed history page without discarding loaded results", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      overview,
      surface: "history",
      historyCursor: "cursor-a",
      historyRuns: [{
        schemaVersion: 1, taskId: "loaded", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a",
        kind: "update_wiki", displayStatus: "completed", retry: null, outcome: null,
        startedAt: "2026-08-10T08:00:00Z", updatedAt: "2026-08-10T08:01:00Z", completedAt: "2026-08-10T08:01:00Z",
      }],
      operations: {
        "history:page": { requestId: 1, pending: false, error: { summary: "network failed", technicalDetails: "offline" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));

    expect(controller.loadHistoryMore).toHaveBeenCalledTimes(1);
    expect(controller.filterHistory).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState().historyRuns.map((run) => run.taskId)).toEqual(["loaded"]);
  });

  it("refreshes instead of replaying a deterministic foreign-identity page", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      overview,
      surface: "history",
      historyCursor: "cursor-a",
      operations: {
        "history:page": { requestId: 1, pending: false, error: { summary: "wrong identity", technicalDetails: "WORKFLOW_HISTORY_IDENTITY_MISMATCH" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.refresh" }));

    expect(controller.refresh).toHaveBeenCalledTimes(1);
    expect(controller.loadHistoryMore).not.toHaveBeenCalled();
    expect(controller.filterHistory).not.toHaveBeenCalled();
  });

  it("surfaces and retries a task retry failure from History", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    useWorkflowStore.setState({
      overview,
      surface: "history",
      operations: {
        "task:failed-a:retry": { requestId: 1, pending: false, error: { summary: "retry failed", technicalDetails: "identity changed" } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("retry failed");
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.retry" }));
    expect(controller.retry).toHaveBeenCalledWith("failed-a");
  });

  it("keeps task retry errors dismiss-only outside History", () => {
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const failed = workflowRun({ taskId: "detail-failed", kind: "health_check", displayStatus: "failed" });
    useWorkflowStore.setState({
      overview,
      runs: [failed],
      selectedTaskId: failed.taskId,
      surface: "detail",
      operations: {
        "task:detail-failed:retry": { requestId: 1, pending: false, error: { summary: "retry failed", technicalDetails: null } },
      },
    });

    render(<WorkflowsView controller={controller} onOpenTask={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.dismiss" }));

    expect(controller.retry).not.toHaveBeenCalled();
    expect(useWorkflowStore.getState().operations["task:detail-failed:retry"]?.error ?? null).toBeNull();
  });

  it("loads one guarded diff page only when its disclosure opens", async () => {
    workflowApiMocks.getWorkflowFileDiff.mockResolvedValue({
      fileId: "file-00000000",
      path: "wiki/中文/长路径.md",
      kind: "two_way",
      diff: "@@ first chunk @@",
      nextCursor: null,
      truncated: false,
    });
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const waiting = {
      schemaVersion: 1, taskId: "waiting-diff", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b", stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null,
      pendingAction: { id: "action-a", actionType: "batch_rewrite", riskLevel: "high", affectedPaths: ["wiki/中文/长路径.md"], candidate: { kind: "task_owned", candidateId: "candidate-a" }, expiresAt: null, checkpointHash: null },
      decisionReview: { reason: "review", counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 }, userEditsDetected: false, fileDiffs: [{ fileId: "file-00000000", path: "wiki/中文/长路径.md", diffBytes: 300_000, diff: null, kind: "two_way" }] },
      result: null, error: null, startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: null,
    } satisfies WorkflowRun;

    const view = render(<WorkflowTaskDetail run={waiting} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    expect(workflowApiMocks.getWorkflowFileDiff).not.toHaveBeenCalled();
    expect(view.container.querySelectorAll(".workflow-file-diff pre")).toHaveLength(0);

    fireEvent.click(view.container.querySelector(".workflow-file-diff summary")!);

    await waitFor(() => expect(workflowApiMocks.getWorkflowFileDiff).toHaveBeenCalledTimes(1));
    expect(workflowApiMocks.getWorkflowFileDiff).toHaveBeenCalledWith(expect.objectContaining({
      taskId: "waiting-diff",
      pendingActionId: "action-a",
      fileId: "file-00000000",
      cursor: null,
      limitBytes: 64 * 1024,
    }));
    expect(await screen.findByText("@@ first chunk @@")).toBeInTheDocument();
  });

  it("blocks Apply and offers prepare-again when a lazy diff snapshot is stale", async () => {
    workflowApiMocks.getWorkflowFileDiff.mockRejectedValue({
      code: "WORKFLOW_OUTPUT_BASELINE_CHANGED",
      message: "changed",
    });
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const waiting = {
      schemaVersion: 1, taskId: "waiting-stale", projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a", kind: "update_wiki", displayStatus: "waiting_for_confirmation",
      scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] }, route: null, fingerprint: "f", baselineFingerprint: "b", stages: [], currentStageId: null, queuePosition: null, continuationRequired: false, retry: null,
      pendingAction: { id: "action-stale", actionType: "batch_rewrite", riskLevel: "high", affectedPaths: ["wiki/stale.md"], candidate: { kind: "task_owned", candidateId: "candidate-stale" }, expiresAt: null, checkpointHash: "checkpoint" },
      decisionReview: { reason: "review", counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 }, userEditsDetected: false, fileDiffs: [{ fileId: "file-00000000", path: "wiki/stale.md", diffBytes: 300_000, diff: null, kind: "three_way" }] },
      result: null, error: null, startedAt: "2026-08-01T00:00:00Z", updatedAt: "2026-08-01T00:01:00Z", completedAt: null,
    } satisfies WorkflowRun;

    const view = render(<WorkflowTaskDetail run={waiting} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    expect(screen.getByRole("button", { name: "workflows.action.applyChanges" })).toBeEnabled();

    fireEvent.click(view.container.querySelector(".workflow-file-diff summary")!);

    await waitFor(() => expect(screen.getByRole("button", { name: "workflows.action.applyChanges" })).toBeDisabled());
    expect(screen.getAllByText("workflows.diff.stale").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.prepareAgain" }));
    expect(controller.adjustAndPrepare).toHaveBeenCalledWith(waiting);
  });

  it("does not leak a delayed lazy diff response across task identities", async () => {
    const first = deferred<WorkflowFileDiffPage>();
    workflowApiMocks.getWorkflowFileDiff
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({
        fileId: "file-00000000",
        path: "wiki/b.md",
        kind: "two_way",
        diff: "task-b-diff",
        nextCursor: null,
        truncated: false,
      });
    const controller = Object.fromEntries(["refresh", "prepare", "startPrepared", "cancel", "undoCancel", "reorder", "retry", "adjustAndPrepare", "openRun", "openResult", "confirm", "discard", "continueQueue", "filterHistory", "loadHistoryMore", "handlePrerequisite", "backToOverview"].map((key) => [key, vi.fn()])) as unknown as WorkflowsController;
    const createRun = (taskId: string, path: string): WorkflowRun => workflowRun({
      taskId,
      kind: "update_wiki",
      displayStatus: "waiting_for_confirmation",
      pendingAction: { id: `action-${taskId}`, actionType: "batch_rewrite", riskLevel: "high", affectedPaths: [path], candidate: { kind: "task_owned", candidateId: `candidate-${taskId}` }, expiresAt: null, checkpointHash: null },
      decisionReview: { reason: "review", counts: { created: 0, modified: 1, overwritten: 0, deleted: 0 }, userEditsDetected: false, fileDiffs: [{ fileId: "file-00000000", path, diffBytes: 300_000, diff: null, kind: "two_way" }] },
    });
    const view = render(<WorkflowTaskDetail run={createRun("task-a", "wiki/a.md")} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);

    fireEvent.click(view.container.querySelector(".workflow-file-diff summary")!);
    await waitFor(() => expect(workflowApiMocks.getWorkflowFileDiff).toHaveBeenCalledTimes(1));

    view.rerender(<WorkflowTaskDetail run={createRun("task-b", "wiki/b.md")} controller={controller} queuedRuns={[]} onOpenLogs={vi.fn()} />);
    fireEvent.click(view.container.querySelector(".workflow-file-diff summary")!);
    expect(await screen.findByText("task-b-diff")).toBeInTheDocument();

    first.resolve({ fileId: "file-00000000", path: "wiki/a.md", kind: "two_way", diff: "task-a-diff", nextCursor: null, truncated: false });
    await waitFor(() => expect(screen.queryByText("task-a-diff")).not.toBeInTheDocument());
    expect(screen.getByText("task-b-diff")).toBeInTheDocument();
  });

  it("groups 10,000 attempts in linear time while preserving stable attempt order", () => {
    const startedAt = performance.now();
    const attempts = Array.from({ length: 10_000 }, (_, index) => ({
      taskId: index === 0 ? "first" : `retry-${index}`,
      updatedAt: `2026-08-01T00:${String(index % 60).padStart(2, "0")}:00Z`,
      retry: index === 0 ? null : { attemptOf: "first", attemptNumber: index + 1 },
    })) as WorkflowRun[];

    const groups = groupWorkflowAttempts(attempts);

    expect(groups).toHaveLength(1);
    expect(groups[0]?.runs).toHaveLength(10_000);
    expect(groups[0]?.runs[0]?.taskId).toBe("first");
    expect(groups[0]?.runs.at(-1)?.taskId).toBe("retry-9999");
    expect(performance.now() - startedAt).toBeLessThan(200);
  });
});

const projectSummary = {
  projectId: "project-a", name: "Project A", rootPath: "D:/a", template: "general" as const,
  wikiPageCount: 1, sourceCount: 1, taskCount: 0, indexState: "indexed" as const,
  graphState: "cached" as const, agentRoute: "byok" as const,
  health: { isWikiProject: true, hasPurpose: true, hasSchema: true, hasAppState: true, hasObsidian: false, missingPaths: [] },
};

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "en", resolvedLanguage: "en" },
  }),
}));

import type { WorkflowsController } from "./useWorkflowsController";
import { useWorkflowStore } from "../../stores/workflowStore";
import { WorkflowHistoryView } from "./WorkflowHistoryView";
import { WorkflowPreparationView } from "./WorkflowPreparationView";
import { WorkflowTaskDetail } from "./WorkflowTaskDetail";
import {
  WORKFLOW_BASELINE_SIZES,
  baselineFixtureSignature,
  makeBaselineRun,
  makeDecisionReview,
  makeHistoryAttempts,
  makeMarkdownPaths,
  makePreparationWithOptions,
  makeProgressUpdates,
  makeScopeOptions,
  makeWorkflowEventBurst,
} from "./workflowBaselineFixtures";
import { WORKFLOW_KINDS } from "./workflowPresentation";

const noopController = {
  refresh: vi.fn(), prepare: vi.fn(), startPrepared: vi.fn(), cancel: vi.fn(), undoCancel: vi.fn(),
  reorder: vi.fn(), retry: vi.fn(), adjustAndPrepare: vi.fn(), openRun: vi.fn(), openResult: vi.fn(),
  confirm: vi.fn(), discard: vi.fn(), continueQueue: vi.fn(), loadHistoryMore: vi.fn(),
  handlePrerequisite: vi.fn(), backToOverview: vi.fn(), showHistory: vi.fn(),
} as unknown as WorkflowsController;

afterEach(() => {
  cleanup();
  useWorkflowStore.getState().reset();
});

describe("Workflows Batch 0 deterministic scale fixtures", () => {
  it("keeps the visible workflow baseline fixed to three kinds before repair operations land", () => {
    expect(WORKFLOW_KINDS).toEqual(["update_wiki", "health_check", "generate_content"]);
  });

  it("rebuilds the complete fixed fixture signature identically ten times", () => {
    const signatures = Array.from({ length: 10 }, () => baselineFixtureSignature());
    expect(new Set(signatures)).toEqual(new Set([signatures[0]]));
    expect(makeWorkflowEventBurst()).toHaveLength(WORKFLOW_BASELINE_SIZES.workflowEvents);
    const eventBurst = makeWorkflowEventBurst();
    expect(eventBurst.at(-1)?.payload.displayStatus).toBe("completed");
    expect(Date.parse(eventBurst.at(-1)!.timestamp) - Date.parse(eventBurst[0]!.timestamp)).toBe(1_990);
    expect(makeMarkdownPaths()).toHaveLength(WORKFLOW_BASELINE_SIZES.markdownFiles);
    const progressUpdates = makeProgressUpdates();
    expect(progressUpdates).toHaveLength(WORKFLOW_BASELINE_SIZES.progressUpdates);
    expect(progressUpdates.at(-1)?.atMs).toBe(9_980);
    expect(makeScopeOptions()).toHaveLength(WORKFLOW_BASELINE_SIZES.scopeOptions);
    expect(makeHistoryAttempts()).toHaveLength(WORKFLOW_BASELINE_SIZES.historyAttempts);
    const review = makeDecisionReview();
    expect(review.fileDiffs).toHaveLength(WORKFLOW_BASELINE_SIZES.diffFiles);
    expect(review.fileDiffs.every((item) => item.diff?.length === WORKFLOW_BASELINE_SIZES.diffBytes)).toBe(true);
  }, 30_000);

  it("bounds Preparation DOM and supports searching and selecting the current results", () => {
    const view = render(
      <WorkflowPreparationView
        preparation={makePreparationWithOptions()}
        onBack={vi.fn()}
        onPrerequisite={vi.fn()}
        onReprepare={vi.fn()}
        onStart={vi.fn()}
      />,
    );
    expect(view.container.querySelectorAll(".workflow-scope-items label").length).toBeLessThanOrEqual(200);
    expect(screen.queryByLabelText("source-00200:version-00200")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.nextOptions" }));
    expect(screen.getByLabelText("source-00200:version-00200")).toBeInTheDocument();
    expect(view.container.querySelectorAll(".workflow-scope-items label").length).toBeLessThanOrEqual(200);
    fireEvent.change(screen.getByLabelText("workflows.preparation.searchOptions"), {
      target: { value: "source-09999" },
    });
    expect(view.container.querySelectorAll(".workflow-scope-items label")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.selectResults" }));
    expect(screen.getByText("workflows.preparation.selectionCount")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.clearSelection" }));
    expect(screen.getByLabelText("source-09999:version-09999")).not.toBeChecked();
  }, 30_000);

  it("keeps the History DOM bounded for 10,000 attempts", () => {
    useWorkflowStore.setState({ historyCursor: null, historyKind: null, historyStatus: null });
    const view = render(
      <WorkflowHistoryView
        runs={makeHistoryAttempts()}
        onBack={vi.fn()}
        onLoadMore={vi.fn()}
        onOpen={vi.fn()}
        onRetry={vi.fn()}
      />,
    );
    expect(view.container.querySelectorAll(".workflow-history__run").length).toBeLessThanOrEqual(200);
  }, 30_000);

  it("mounts decision diff text only when a file disclosure opens", () => {
    const review = makeDecisionReview();
    const affectedPaths = review.fileDiffs.map((item) => item.path);
    const run = makeBaselineRun(0, {
      displayStatus: "waiting_for_confirmation",
      pendingAction: {
        id: "decision-scale",
        actionType: "merge_conflict",
        riskLevel: "high",
        affectedPaths,
        candidate: { kind: "task_owned", candidateId: "candidate-scale" },
        expiresAt: null,
        checkpointHash: "b".repeat(40),
      },
      decisionReview: review,
    });
    const view = render(
      <WorkflowTaskDetail
        controller={noopController}
        onOpenLogs={vi.fn()}
        queuedRuns={[]}
        run={run}
      />,
    );
    expect(view.container.querySelectorAll(".workflow-file-diff pre")).toHaveLength(0);
    fireEvent.click(view.container.querySelector(".workflow-file-diff summary")!);
    expect(view.container.querySelectorAll(".workflow-file-diff pre")).toHaveLength(1);
  }, 30_000);
});

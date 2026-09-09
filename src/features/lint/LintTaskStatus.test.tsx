import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { WorkflowRunSummary } from "../../types/workflow";
import { LintTaskStatus } from "./LintTaskStatus";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const run: WorkflowRunSummary = {
  schemaVersion: 2, taskId: "health-running", projectId: "p", canonicalIdentityKey: "identity", identityRevision: "revision",
  kind: "health_check", operation: { kind: "built_in" }, displayStatus: "running", cancellable: true,
  startedAt: "2026-09-08T00:00:00Z", updatedAt: "2026-09-08T00:00:01Z", completedAt: null, retry: null,
  currentStage: {
    id: "deep_check", labelKey: "workflows.stage.healthCheck.deepCheck", ordinal: 4, status: "running",
    startedAt: "2026-09-08T00:00:01Z", completedAt: null, currentItem: null,
    progress: { current: 3, total: 8 }, decision: null,
  },
};

describe("LintTaskStatus", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useProjectStore.setState({
      currentProject: { projectId: "p", rootPath: "/wiki" },
      authority: { projectId: "p", canonicalIdentityKey: "identity", identityRevision: "revision" },
    } as never);
    useTaskStore.setState({ workflowById: { [run.taskId]: run }, workflowSessionId: null });
    useWorkflowStore.getState().reset();
    useNavigationStore.setState({ activeView: "lint" });
  });

  it("shows canonical workflow progress and cancels the actual workflow once", async () => {
    let finish!: (value: unknown) => void;
    invokeMock.mockImplementation(() => new Promise((resolve) => { finish = resolve; }) as never);
    render(<LintTaskStatus />);
    expect(screen.getByRole("status")).toHaveTextContent("Health Check · Running · Run deep checks");
    const cancel = screen.getByRole("button", { name: "Cancel" });
    fireEvent.click(cancel);
    fireEvent.click(cancel);
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("cancel_workflow_run", {
      request: { projectId: "p", projectRootPath: "/wiki", taskId: run.taskId },
    });
    await act(async () => finish({ ...run, displayStatus: "cancelled", updatedAt: "2026-09-08T00:00:02Z" }));
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  });

  it("opens the same task and hides tasks from another project identity", async () => {
    invokeMock.mockResolvedValue(run);
    render(<LintTaskStatus />);
    fireEvent.click(screen.getByRole("button", { name: "View progress" }));
    await waitFor(() => expect(useWorkflowStore.getState().selectedTaskId).toBe(run.taskId));
    expect(useNavigationStore.getState().activeView).toBe("workflows");
    act(() => useProjectStore.setState({ authority: {
      projectId: "p", canonicalIdentityKey: "different-location", identityRevision: "revision",
    } as never }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

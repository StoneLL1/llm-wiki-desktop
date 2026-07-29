import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { ConfirmedAction, PendingAction } from "../../types/backend";
import type { BackendTask } from "../../types/task";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./ConfirmationDialog", () => ({
  ConfirmationDialog: ({
    action,
    onCancel,
    onConfirm,
  }: {
    action: PendingAction;
    onCancel: () => void;
    onConfirm: () => void;
  }) => (
    <div role="dialog" aria-label={action.title}>
      <button onClick={onCancel}>Cancel action</button>
      <button onClick={onConfirm}>Confirm action</button>
    </div>
  ),
}));
vi.mock("./CompileConflictDialog", () => ({
  CompileConflictDialog: ({
    action,
    onCancel,
    onResolved,
  }: {
    action: PendingAction;
    onCancel: () => void;
    onResolved: (task: BackendTask) => void;
  }) => (
    <div role="dialog" aria-label={action.title}>
      <button onClick={onCancel}>Cancel conflict</button>
      <button onClick={() => onResolved(task)}>Resolve conflict</button>
    </div>
  ),
}));

import { ProjectConfirmationController } from "./ProjectConfirmationController";

const project = {
  ...defaultProject,
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
};
const projectAction: PendingAction = {
  id: "project-action",
  actionType: "delete_file",
  title: "Delete file",
  message: "Delete file",
  riskLevel: "destructive",
  affectedPaths: ["wiki/a.md"],
  preview: null,
  expiresAt: null,
};
const compileAction: PendingAction = {
  ...projectAction,
  id: "compile-action",
  actionType: "batch_rewrite",
  title: "Apply compile",
};
const task: BackendTask = {
  id: "task-1",
  taskType: "wiki_compile",
  projectId: project.projectId,
  title: "Compile",
  status: "waiting_for_confirmation",
  progress: null,
  startedAt: "2026-07-10T00:00:00Z",
  updatedAt: "2026-07-10T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: { summary: "Waiting", affectedPaths: [], pendingAction: compileAction },
  error: null,
};

let confirmPendingAction: ReturnType<
  typeof vi.fn<() => Promise<ConfirmedAction | undefined>>
>;
let cancelPendingAction: ReturnType<typeof vi.fn<() => Promise<void>>>;

beforeEach(() => {
  mocks.invoke.mockReset();
  confirmPendingAction = vi.fn(async () => ({
    action: projectAction,
    status: "confirmed",
    checkpointExists: true,
    projectSummary: project,
  }));
  cancelPendingAction = vi.fn(async () => undefined);
  useProjectStore.setState({
    currentProject: project,
    pendingAction: undefined,
    confirmPendingAction: async () => confirmPendingAction(),
    cancelPendingAction: async () => cancelPendingAction(),
  });
  useTaskStore.setState({
    tasks: [],
    logs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
  });
});

describe("ProjectConfirmationController", () => {
  it("confirms a generic project action without starting another workflow", async () => {
    useProjectStore.setState({ pendingAction: projectAction });
    render(<ProjectConfirmationController />);

    fireEvent.click(screen.getByRole("button", { name: "Confirm action" }));

    await waitFor(() => expect(confirmPendingAction).toHaveBeenCalledTimes(1));
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it.each([
    ["Confirm action", true],
    ["Cancel action", false],
  ] as const)("submits compile confirmation %s and upserts the returned task", async (label, confirmed) => {
    const updated = {
      ...task,
      status: confirmed ? ("running" as const) : ("cancelled" as const),
      result: null,
    };
    mocks.invoke.mockResolvedValue(updated);
    useTaskStore.setState({ tasks: [task] });
    render(<ProjectConfirmationController />);

    fireEvent.click(screen.getByRole("button", { name: label }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("confirm_compile_action", {
        request: { actionId: compileAction.id, confirmed },
      }),
    );
    expect(useTaskStore.getState().tasks).toEqual([updated]);
  });

  it("keeps merge-conflict resolution in the shared task store", async () => {
    const mergeAction = { ...compileAction, actionType: "merge_conflict" as const };
    const waiting = {
      ...task,
      result: { ...task.result!, pendingAction: mergeAction },
    };
    useTaskStore.setState({ tasks: [waiting] });
    render(<ProjectConfirmationController />);

    fireEvent.click(screen.getByRole("button", { name: "Resolve conflict" }));

    expect(useTaskStore.getState().tasks).toEqual([task]);
  });

  it("ignores waiting compile confirmations owned by another project", () => {
    useProjectStore.setState({ currentProject: { ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" } });
    useTaskStore.setState({ tasks: [task] });
    render(<ProjectConfirmationController />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

});

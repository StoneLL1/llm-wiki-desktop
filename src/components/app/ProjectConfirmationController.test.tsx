import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { ConfirmedAction, PendingAction } from "../../types/backend";
import type { BackendTask } from "../../types/task";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  startCompile: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../../hooks/useTaskLauncher", () => ({
  useTaskLauncher: () => ({
    startCompile: mocks.startCompile,
    startDeepLint: vi.fn(),
    startExport: vi.fn(),
    cancel: vi.fn(),
  }),
}));
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
const sourceAction: PendingAction = {
  id: "source-action",
  actionType: "delete_source",
  title: "Delete source",
  message: "Delete source",
  riskLevel: "destructive",
  affectedPaths: ["raw/sources/a.pdf"],
  preview: null,
  expiresAt: null,
};
const compileAction: PendingAction = {
  ...sourceAction,
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
  mocks.startCompile.mockReset().mockResolvedValue(task);
  confirmPendingAction = vi.fn(async () => ({
    action: sourceAction,
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
  useToastStore.setState({ toasts: [] });
});

describe("ProjectConfirmationController", () => {
  it("confirms a source action before starting a shared compile task", async () => {
    useProjectStore.setState({ pendingAction: sourceAction });
    render(<ProjectConfirmationController />);

    fireEvent.click(screen.getByRole("button", { name: "Confirm action" }));

    await waitFor(() => expect(confirmPendingAction).toHaveBeenCalledTimes(1));
    expect(mocks.startCompile).toHaveBeenCalledWith({
      route: "auto",
      agent: null,
      provider: null,
    });
    expect(confirmPendingAction.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.startCompile.mock.invocationCallOrder[0],
    );
  });

  it("reports source compile errors without losing their context", async () => {
    mocks.startCompile.mockRejectedValue(new Error("compile worker unavailable"));
    useProjectStore.setState({ pendingAction: sourceAction });
    render(<ProjectConfirmationController />);

    fireEvent.click(screen.getByRole("button", { name: "Confirm action" }));

    await waitFor(() =>
      expect(useToastStore.getState().toasts[0]?.message).toContain(
        "compile worker unavailable",
      ),
    );
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

  it("does not compile after a confirmed source action when the project changed", async () => {
    let resolve!: (value: ConfirmedAction) => void;
    confirmPendingAction.mockImplementation(() => new Promise((next) => { resolve = next; }));
    useProjectStore.setState({ pendingAction: sourceAction });
    render(<ProjectConfirmationController />);
    fireEvent.click(screen.getByRole("button", { name: "Confirm action" }));
    useProjectStore.getState().setCurrentProject({ ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" });
    resolve({ action: sourceAction, status: "confirmed", checkpointExists: true, projectSummary: project });
    await waitFor(() => expect(confirmPendingAction).toHaveBeenCalled());
    expect(mocks.startCompile).not.toHaveBeenCalled();
  });

  it("does not toast a source compile failure after the project changed", async () => {
    let rejectCompile!: (reason: Error) => void;
    mocks.startCompile.mockImplementation(
      () => new Promise((_, reject) => { rejectCompile = reject; }),
    );
    useProjectStore.setState({ pendingAction: sourceAction });
    render(<ProjectConfirmationController />);

    fireEvent.click(screen.getByRole("button", { name: "Confirm action" }));
    await waitFor(() => expect(mocks.startCompile).toHaveBeenCalledTimes(1));

    useProjectStore.getState().setCurrentProject({
      ...project,
      projectId: "project-b",
      rootPath: "D:/wiki/project-b",
    });
    await act(async () => {
      rejectCompile(new Error("old project compile failed"));
    });

    expect(useToastStore.getState().toasts).toEqual([]);
  });
});

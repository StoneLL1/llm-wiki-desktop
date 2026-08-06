import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendTask } from "../../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) =>
      key === "task.cancelError" ? `Could not cancel task: ${values?.message}` :
        key === "task.importSummary.title" ? "Import activity (visible tasks)" :
          key === "task.importSummary.progress" ? `${values?.processed}/${values?.total} visible import tasks finished` :
            key === "task.importSummary.summary" ? `${values?.active} active / ${values?.waitingForConfirmation} waiting for action / ${values?.failed} failed / ${values?.cancelled} cancelled` :
              key === "task.importBatches.title" ? "Import batches" :
                  key === "task.importBatches.batch" ? `Batch ${values?.index} · ${values?.title}` :
                  key === "task.importBatches.progress" ? `${values?.processed}/${values?.total}` :
                    key === "task.importBatches.summary" ? `${values?.active} active / ${values?.waitingForConfirmation} waiting for action / ${values?.failed} failed / ${values?.cancelled} cancelled` : key,
  }),
}));

import { TaskLogDrawer } from "./TaskLogDrawer";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";

const runningTask: BackendTask = {
  id: "task-1",
  taskType: "import",
  projectId: "project-1",
  title: "Import sources",
  status: "running",
  progress: null,
  startedAt: "2026-06-21T00:00:00Z",
  updatedAt: "2026-06-21T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
};

describe("TaskLogDrawer", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    window.localStorage.clear();
    useTaskStore.setState({
      activeProjectId: "project-1",
      activeProjectRootPath: "D:/project-1",
      tasks: [runningTask],
      logs: {},
      activities: {},
      taskOutputs: {},
      drawerOpen: true,
      selectedTaskId: "task-1",
      runningCount: 1,
      tasksHydrated: true,
    });
    useToastStore.setState({ toasts: [] });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: { invoke: invokeMock },
      configurable: true,
    });
  });

  it("reports a cancellation IPC failure instead of swallowing it", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "cancel_task") return Promise.reject(new Error("backend unavailable"));
      if (command === "get_task_logs") return Promise.resolve([]);
      return Promise.resolve(null);
    });

    render(<TaskLogDrawer />);
    fireEvent.click(screen.getAllByRole("button", { name: "task.action.cancel" })[0]);

    await waitFor(() =>
      expect(useToastStore.getState().toasts).toEqual([
        expect.objectContaining({
          tone: "error",
          message: "Could not cancel task: backend unavailable",
        }),
      ]),
    );
  });

  it("deduplicates cancellation requests while one is pending", async () => {
    let resolveCancel: ((task: BackendTask) => void) | undefined;
    const pendingCancel = new Promise<BackendTask>((resolve) => { resolveCancel = resolve; });
    invokeMock.mockImplementation((command: string) => {
      if (command === "cancel_task") return pendingCancel;
      if (command === "get_task_logs") return Promise.resolve([]);
      return Promise.resolve(null);
    });

    render(<TaskLogDrawer />);
    const cancelButton = screen.getAllByRole("button", { name: "task.action.cancel" })[0];
    fireEvent.click(cancelButton);
    fireEvent.click(cancelButton);

    expect(invokeMock.mock.calls.filter(([command]) => command === "cancel_task")).toHaveLength(1);
    resolveCancel?.({ ...runningTask, status: "cancelled", cancellable: false, completedAt: "2026-07-15T00:00:02Z" });
    await waitFor(() => expect(cancelButton).not.toBeDisabled());
  });

  it("keeps cancellation disabled while the backend reports cancelling", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "cancel_task") return Promise.resolve({ ...runningTask, status: "cancelling" });
      if (command === "get_task_logs") return Promise.resolve([]);
      return Promise.resolve(null);
    });

    render(<TaskLogDrawer />);
    const cancelButton = screen.getAllByRole("button", { name: "task.action.cancel" })[0];
    fireEvent.click(cancelButton);
    await waitFor(() => expect(cancelButton).toBeDisabled());
  });

  it("shows latest execution tasks first by default regardless of status", () => {
    const oldRunning = {
      ...runningTask,
      id: "old-running",
      title: "Old running",
      status: "running" as const,
      startedAt: "2026-07-04T01:00:00Z",
      updatedAt: "2026-07-04T05:00:00Z",
    };
    const newFailed = {
      ...runningTask,
      id: "new-failed",
      title: "New failed",
      status: "failed" as const,
      startedAt: "2026-07-04T03:00:00Z",
      updatedAt: "2026-07-04T03:01:00Z",
      completedAt: "2026-07-04T03:02:00Z",
    };
    useTaskStore.setState({
      tasks: [oldRunning, newFailed],
      drawerOpen: true,
      selectedTaskId: null,
    });

    render(<TaskLogDrawer />);

    const buttons = screen.getAllByRole("button").map((button) => button.textContent ?? "");
    expect(buttons.indexOf("New failed")).toBeLessThan(buttons.indexOf("Old running"));
  });

  it("switches to status sort without changing the selected task", () => {
    const oldRunning = {
      ...runningTask,
      id: "old-running",
      title: "Old running",
      status: "running" as const,
      startedAt: "2026-07-04T01:00:00Z",
      updatedAt: "2026-07-04T01:00:00Z",
    };
    const newFailed = {
      ...runningTask,
      id: "new-failed",
      title: "New failed",
      status: "failed" as const,
      startedAt: "2026-07-04T03:00:00Z",
      updatedAt: "2026-07-04T03:00:00Z",
      completedAt: "2026-07-04T03:01:00Z",
    };
    useTaskStore.setState({
      tasks: [newFailed, oldRunning],
      drawerOpen: true,
      selectedTaskId: "new-failed",
    });

    render(<TaskLogDrawer />);
    fireEvent.click(screen.getByRole("button", { name: "task.sort.status" }));

    expect(useTaskStore.getState().selectedTaskId).toBe("new-failed");
  });

  it("summarizes multiple import tasks above the task list", () => {
    const failedTask = {
      ...runningTask,
      id: "task-failed",
      title: "Failed import",
      status: "failed" as const,
      cancellable: false,
      completedAt: "2026-07-15T00:00:02Z",
    };
    useTaskStore.setState({
      tasks: [runningTask, failedTask],
      selectedTaskId: "task-1",
    });

    render(<TaskLogDrawer />);

    expect(screen.getByText("Import activity (visible tasks)")).toBeInTheDocument();
    expect(screen.getByText("1/2 visible import tasks finished")).toBeInTheDocument();
    expect(screen.getByText("1 active / 0 waiting for action / 1 failed / 0 cancelled")).toBeInTheDocument();
  });

  it("shows preview-ready imports as reviewable rather than active", () => {
    const waitingTask = {
      ...runningTask,
      id: "waiting-task",
      title: "Ready import",
      status: "waiting_for_confirmation" as const,
      cancellable: true,
    };
    const failedTask = {
      ...runningTask,
      id: "waiting-failed",
      title: "Failed import",
      status: "failed" as const,
      cancellable: false,
      completedAt: "2026-07-15T00:00:02Z",
    };
    useTaskStore.setState({ tasks: [waitingTask, failedTask], selectedTaskId: "waiting-task" });

    render(<TaskLogDrawer />);

    expect(screen.getByText("2/2 visible import tasks finished")).toBeInTheDocument();
    expect(screen.getByText("0 active / 1 waiting for action / 1 failed / 0 cancelled")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "task.action.cancel" })).not.toBeInTheDocument();
  });

  it("folds batched imports and exposes their recent logs and child tasks", () => {
    const first = { ...runningTask, id: "batch-task-1", title: "Import first", batchId: "batch-1" };
    const second = { ...runningTask, id: "batch-task-2", title: "Import second", batchId: "batch-1", status: "failed" as const, cancellable: false };
    useTaskStore.setState({
      tasks: [first, second],
      logs: { "batch-task-1": [{ timestamp: "2026-06-21T00:00:01Z", level: "info", message: "started" }] },
      selectedTaskId: "batch-task-1",
    });

    render(<TaskLogDrawer />);

    expect(screen.getByText("Import batches")).toBeInTheDocument();
    fireEvent.click(screen.getByText(/Batch 1/));
    expect(screen.getByText("Import first: started")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Import second$/ }));
    expect(useTaskStore.getState().selectedTaskId).toBe("batch-task-2");
  });

  it("shows a typed URL operation as one task instead of a numbered batch", () => {
    const operation = {
      ...runningTask,
      id: "url-operation",
      batchId: "url-operation",
      title: "Import https://example.com/article",
      operation: {
        kind: "import_batch" as const,
        sessionId: "session-1",
        itemCount: 1,
        sourceLabel: "https://example.com/article",
      },
    };
    useTaskStore.setState({ tasks: [operation], selectedTaskId: operation.id });

    render(<TaskLogDrawer />);

    expect(screen.getByRole("button", { name: /Import https:\/\/example.com\/article/ })).toBeInTheDocument();
    expect(screen.queryByText("Import batches")).not.toBeInTheDocument();
    expect(screen.queryByText(/Batch 1/)).not.toBeInTheDocument();
  });

  it("moves focus into the drawer and returns it to the opener", async () => {
    useTaskStore.setState({ drawerOpen: false, selectedTaskId: null });

    render(
      <>
        <button type="button" onClick={() => useTaskStore.getState().openDrawer("task-1")}>
          Open tasks
        </button>
        <TaskLogDrawer />
      </>,
    );

    const opener = screen.getByRole("button", { name: "Open tasks" });
    opener.focus();
    fireEvent.click(opener);

    const closeButton = await screen.findByRole("button", { name: "task.drawer.close" });
    await waitFor(() => expect(closeButton).toHaveFocus());

    fireEvent.click(closeButton);
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("closes on Escape without leaking the shortcut to the shell", async () => {
    useTaskStore.setState({ drawerOpen: false, selectedTaskId: null });

    render(
      <>
        <button type="button" onClick={() => useTaskStore.getState().openDrawer("task-1")}>
          Open tasks
        </button>
        <TaskLogDrawer />
      </>,
    );

    const opener = screen.getByRole("button", { name: "Open tasks" });
    opener.focus();
    fireEvent.click(opener);
    const closeButton = await screen.findByRole("button", { name: "task.drawer.close" });
    await waitFor(() => expect(closeButton).toHaveFocus());

    fireEvent.keyDown(closeButton, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(opener).toHaveFocus();
  });
});

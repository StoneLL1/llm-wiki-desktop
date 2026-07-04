import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendTask } from "../../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) =>
      key === "task.cancelError" ? `Could not cancel task: ${values?.message}` : key,
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
      tasks: [runningTask],
      logs: {},
      drawerOpen: true,
      selectedTaskId: "task-1",
      runningCount: 1,
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
});

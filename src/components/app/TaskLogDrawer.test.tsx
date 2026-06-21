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
});

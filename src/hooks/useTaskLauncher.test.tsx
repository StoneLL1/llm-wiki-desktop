import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useTaskLauncher } from "./useTaskLauncher";

const project = {
  ...defaultProject,
  projectId: "p1",
  name: "Project One",
  rootPath: "/wiki/p1",
};

beforeEach(() => {
  invokeMock.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useTaskStore.setState({
    activeProjectId: "p1",
    activeProjectRootPath: "/wiki/p1",
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
  });
  useToastStore.setState({ toasts: [] });
});

describe("useTaskLauncher", () => {
  it("does not toast a cancel failure after the project changed", async () => {
    let rejectCancel!: (reason: Error) => void;
    invokeMock.mockReturnValue(
      new Promise((_, reject) => { rejectCancel = reject; }),
    );
    const projectB = { ...project, projectId: "p2", rootPath: "/wiki/p2" };
    const { result, rerender } = renderHook(
      ({ current }) => useTaskLauncher(current),
      { initialProps: { current: project } },
    );

    const pending = result.current.cancel("task-1");
    rerender({ current: projectB });
    await act(async () => {
      rejectCancel(new Error("old project cancel failed"));
      await pending;
    });

    expect(useToastStore.getState().toasts).toEqual([]);
  });

  it("cancels through the existing task request and preserves error context in the toast", async () => {
    invokeMock.mockRejectedValue(new Error("worker already exited"));
    const { result } = renderHook(() => useTaskLauncher(project));

    await act(async () => {
      await result.current.cancel("task-1");
    });

    expect(invokeMock).toHaveBeenCalledWith("cancel_task", {
      request: {
        taskId: "task-1",
        projectId: "p1",
        projectRootPath: "/wiki/p1",
      },
    });
    expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({
        tone: "error",
        message: expect.stringContaining("worker already exited"),
      }),
    ]);
  });
});

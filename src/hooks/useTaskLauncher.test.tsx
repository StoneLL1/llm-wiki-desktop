import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";
import type { BackendTask } from "../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useTaskLauncher } from "./useTaskLauncher";

const project = {
  ...defaultProject,
  projectId: "p1",
  name: "Project One",
  rootPath: "/wiki/p1",
};

const task: BackendTask = {
  id: "task-1",
  taskType: "wiki_compile",
  projectId: project.projectId,
  title: "Compile",
  status: "queued",
  progress: null,
  startedAt: "2026-07-10T00:00:00Z",
  updatedAt: "2026-07-10T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
};

beforeEach(() => {
  invokeMock.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
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

describe("useTaskLauncher", () => {
  it("does not start compile after source discovery returns to a different project", async () => {
    let resolveSources!: (value: Array<{ sourceId: string; versionId: string; contentHash: string }>) => void;
    invokeMock.mockReturnValue(new Promise((resolve) => { resolveSources = resolve; }));
    const projectB = { ...project, projectId: "p2", rootPath: "/wiki/p2" };
    const { result, rerender } = renderHook(({ current }) => useTaskLauncher(current), {
      initialProps: { current: project },
    });
    const pending = result.current.startCompile();
    rerender({ current: projectB });
    resolveSources([{ sourceId: "source-a", versionId: "version-a", contentHash: "a".repeat(64) }]);
    await act(async () => {
      await expect(pending).rejects.toThrow(/active project changed/i);
    });

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("list_compile_source_versions", {
      request: { projectId: "p1", projectRootPath: "/wiki/p1" },
    });
    expect(useTaskStore.getState().tasks).toEqual([]);
    expect(useTaskStore.getState()).toMatchObject({ drawerOpen: false, selectedTaskId: null });
  });

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

  it("starts compile tasks and tracks them in the shared drawer", async () => {
    const sourceVersions = [
      { sourceId: "source-a", versionId: "version-a", contentHash: "a".repeat(64) },
    ];
    invokeMock.mockResolvedValueOnce(sourceVersions).mockResolvedValueOnce(task);
    const { result } = renderHook(() => useTaskLauncher(project));

    await act(async () => {
      await expect(result.current.startCompile()).resolves.toEqual(task);
    });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "list_compile_source_versions", {
      request: { projectId: "p1", projectRootPath: "/wiki/p1" },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "start_wiki_compile", {
      request: {
        projectId: "p1",
        projectRootPath: "/wiki/p1",
        route: "auto",
        agent: null,
        provider: null,
        sourceVersions,
      },
    });
    expect(useTaskStore.getState().tasks).toContainEqual(task);
    expect(useTaskStore.getState()).toMatchObject({
      drawerOpen: true,
      selectedTaskId: task.id,
    });
  });

  it("builds typed deep-lint and export requests", async () => {
    const deepLintTask = { ...task, id: "lint-1", taskType: "deep_lint" as const };
    const exportTask = { ...task, id: "export-1", taskType: "export" as const };
    invokeMock
      .mockResolvedValueOnce(deepLintTask)
      .mockResolvedValueOnce(exportTask);
    const { result } = renderHook(() => useTaskLauncher(project));
    const options = {
      route: "byok" as const,
      agent: null,
      provider: "anthropic" as const,
    };

    await act(async () => {
      await result.current.startDeepLint(options);
      await result.current.startExport("project_report", null, options);
    });

    expect(invokeMock.mock.calls).toEqual([
      [
        "start_deep_lint",
        {
          request: {
            projectId: "p1",
            projectRootPath: "/wiki/p1",
            ...options,
          },
        },
      ],
      [
        "start_export",
        {
          request: {
            projectId: "p1",
            projectRootPath: "/wiki/p1",
            exportType: "project_report",
            sourcePath: null,
            ...options,
          },
        },
      ],
    ]);
    expect(useTaskStore.getState().tasks).toEqual([deepLintTask, exportTask]);
    expect(useTaskStore.getState().selectedTaskId).toBe(exportTask.id);
  });

  it("cancels through the existing task request and preserves error context in the toast", async () => {
    invokeMock.mockRejectedValue(new Error("worker already exited"));
    const { result } = renderHook(() => useTaskLauncher(project));

    await act(async () => {
      await result.current.cancel("task-1");
    });

    expect(invokeMock).toHaveBeenCalledWith("cancel_task", {
      request: { taskId: "task-1" },
    });
    expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({
        tone: "error",
        message: expect.stringContaining("worker already exited"),
      }),
    ]);
  });
});

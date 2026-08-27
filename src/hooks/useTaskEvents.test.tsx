import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent, BackendTask } from "../types/task";
import { defaultProject, useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { useChatStore } from "../stores/chatStore";
import {
  clearPendingTaskEvents,
  dispatchTaskEvent,
  registerTaskEventListener,
} from "../services/taskEventDispatcher";
import { useChatStream } from "./useChatStream";
import {
  isTaskEventForProject,
  useTaskEvents,
} from "./useTaskEvents";
import {
  observeProjectResources,
  registerProjectResource,
} from "../stores/projectScope";

const listenMock = vi.hoisted(() => vi.fn());
const notifyTaskEventMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("../services/notifications", () => ({
  invalidateNotificationPermissionEpoch: vi.fn(),
  notifyTaskEvent: notifyTaskEventMock,
  registerNotificationActionListener: vi.fn(async () => vi.fn()),
}));

const event: BackendEvent = {
  eventId: "event-1",
  eventType: "task_completed",
  projectId: "project-a",
  taskId: "task-1",
  timestamp: "2026-07-13T00:00:00Z",
  payload: {},
};

beforeEach(() => {
  listenMock.mockReset();
  notifyTaskEventMock.mockReset();
  listenMock.mockResolvedValue(vi.fn());
  clearPendingTaskEvents();
  useProjectStore.setState({
    currentProject: {
      ...defaultProject,
      projectId: "project-a",
      rootPath: "",
    },
  });
  useTaskStore.setState({
    activeProjectId: "project-a",
    activeProjectRootPath: "",
    taskById: {},
    taskIdsByProject: {},
    runningCountByProject: {},
    taskFacts: {},
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
  });
  useChatStore.getState().reset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

afterEach(() => {
  clearPendingTaskEvents();
  vi.restoreAllMocks();
});

describe("task event listener bridge", () => {
  it("accepts only events owned by the active project", () => {
    expect(isTaskEventForProject(event, "project-a")).toBe(true);
    expect(isTaskEventForProject(event, "project-b")).toBe(false);
    expect(isTaskEventForProject({ ...event, projectId: null }, "project-a")).toBe(false);
  });
  it("returns an unsubscribe handle for workflow-owned listeners", () => {
    const listener = vi.fn();
    const unsubscribe = registerTaskEventListener(listener);
    dispatchTaskEvent(event);
    expect(listener).toHaveBeenCalledWith(event);
    expect(typeof unsubscribe).toBe("function");
    unsubscribe();
    dispatchTaskEvent(event);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("keeps independent listeners isolated", () => {
    const first = vi.fn();
    const second = vi.fn();
    const removeFirst = registerTaskEventListener(first);
    const removeSecond = registerTaskEventListener(second);
    dispatchTaskEvent(event);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    removeFirst();
    removeSecond();
    dispatchTaskEvent(event);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("owns the only Tauri stream subscription while the chat bridge is mounted", async () => {
    const { unmount } = renderHook(() => {
      useTaskEvents();
      useChatStream();
    });

    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    expect(
      listenMock.mock.calls.filter(([channel]) => channel === "task://stream-output"),
    ).toHaveLength(1);
    unmount();
  });

  it("records a background task fact without changing active-project presentation", async () => {
    let taskUpdated!: (event: { payload: BackendEvent<BackendTask> }) => void;
    listenMock.mockImplementation(async (channel: string, callback: typeof taskUpdated) => {
      if (channel === "task://updated") taskUpdated = callback;
      return vi.fn();
    });
    useTaskStore.setState({ drawerOpen: true, selectedTaskId: "task-a" });
    const mounted = renderHook(() => useTaskEvents());
    await waitFor(() => expect(taskUpdated).toBeTypeOf("function"));
    const backgroundTask: BackendTask = {
      id: "task-b",
      taskType: "import",
      projectId: "project-b",
      title: "Background Import",
      status: "running",
      progress: { current: 1, total: 2, label: "Importing" },
      startedAt: "2026-08-16T00:00:00Z",
      updatedAt: "2026-08-16T00:00:01Z",
      completedAt: null,
      cancellable: true,
      logPath: null,
      result: null,
      error: null,
    };

    act(() => {
      taskUpdated({
        payload: {
          eventId: "background-task",
          eventType: "task_updated",
          projectId: "project-b",
          taskId: backgroundTask.id,
          timestamp: backgroundTask.updatedAt,
          payload: backgroundTask,
        },
      });
    });

    const state = useTaskStore.getState();
    expect(state.taskById[backgroundTask.id]).toEqual(backgroundTask);
    expect(state.tasks).toEqual([]);
    expect(state.drawerOpen).toBe(true);
    expect(state.selectedTaskId).toBe("task-a");
    expect(notifyTaskEventMock).not.toHaveBeenCalled();
    mounted.unmount();
  });

  it("marks and revalidates only observed resources on window focus", async () => {
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-a",
        rootPath: "D:/wiki",
      },
    });
    const invalidate = vi.fn();
    const revalidate = vi.fn();
    const unregister = registerProjectResource("wiki", { invalidate }, revalidate);
    const unobserve = observeProjectResources(
      { projectId: "project-a", rootPath: "D:/wiki" },
      ["wiki"],
    );
    const mounted = renderHook(() => useTaskEvents());
    await waitFor(() => expect(listenMock).toHaveBeenCalled());

    act(() => window.dispatchEvent(new Event("focus")));

    await waitFor(() => expect(invalidate).toHaveBeenCalledTimes(1));
    expect(revalidate).toHaveBeenCalledWith({ projectId: "project-a", rootPath: "D:/wiki" });
    mounted.unmount();
    unobserve();
    unregister();
  });

  it("drops deferred invalidation after the active project switches", async () => {
    let wikiChanged!: (event: { payload: BackendEvent }) => void;
    listenMock.mockImplementation(async (channel: string, callback: typeof wikiChanged) => {
      if (channel === "wiki://changed") wikiChanged = callback;
      return vi.fn();
    });
    const invalidate = vi.fn();
    const unregister = registerProjectResource("wiki", { invalidate }, vi.fn());
    const mounted = renderHook(() => useTaskEvents());
    await waitFor(() => expect(wikiChanged).toBeTypeOf("function"));

    act(() => {
      wikiChanged({ payload: { ...event, eventType: "wiki_changed" } });
      useProjectStore.getState().setCurrentProject({
        ...defaultProject,
        projectId: "project-b",
        rootPath: "D:/b",
      });
    });
    await Promise.resolve();

    expect(invalidate).not.toHaveBeenCalled();
    mounted.unmount();
    unregister();
  });

  it("keeps deferred invalidation after a same-scope project summary refresh", async () => {
    let wikiChanged!: (event: { payload: BackendEvent }) => void;
    listenMock.mockImplementation(async (channel: string, callback: typeof wikiChanged) => {
      if (channel === "wiki://changed") wikiChanged = callback;
      return vi.fn();
    });
    const invalidate = vi.fn();
    const unregister = registerProjectResource("wiki", { invalidate }, vi.fn());
    const mounted = renderHook(() => useTaskEvents());
    await waitFor(() => expect(wikiChanged).toBeTypeOf("function"));

    act(() => {
      wikiChanged({ payload: { ...event, eventType: "wiki_changed" } });
      useProjectStore.getState().setCurrentProject({ ...useProjectStore.getState().currentProject });
    });
    await waitFor(() => expect(invalidate).toHaveBeenCalledTimes(1));

    mounted.unmount();
    unregister();
  });

  it("ignores callbacks from a delayed listener after StrictMode-style remount", () => {
    const callbacks = new Map<string, Array<(event: { payload: BackendEvent }) => void>>();
    listenMock.mockImplementation((channel: string, callback: (event: { payload: BackendEvent }) => void) => {
      callbacks.set(channel, [...(callbacks.get(channel) ?? []), callback]);
      return new Promise<() => void>(() => {});
    });
    const observed = vi.fn();
    const unregister = registerTaskEventListener(observed);
    const firstMount = renderHook(() => useTaskEvents());
    firstMount.unmount();
    const secondMount = renderHook(() => useTaskEvents());
    const streamCallbacks = callbacks.get("task://stream-output") ?? [];
    const terminalCallbacks = callbacks.get("task://completed") ?? [];
    expect(streamCallbacks).toHaveLength(2);
    expect(terminalCallbacks).toHaveLength(2);

    act(() => {
      streamCallbacks[0]?.({
        payload: {
          eventId: "stale-stream",
          eventType: "task_stream_output",
          projectId: "project-a",
          taskId: "task-stale",
          timestamp: "2026-08-16T00:00:00Z",
          payload: { delta: "duplicate", route: "chat-agent" },
        },
      });
      terminalCallbacks[0]?.({ payload: { ...event, taskId: "task-stale" } });
      streamCallbacks[1]?.({
        payload: {
          eventId: "current-stream",
          eventType: "task_stream_output",
          projectId: "project-a",
          taskId: "task-current",
          timestamp: "2026-08-16T00:00:00Z",
          payload: { delta: "once", route: "chat-agent" },
        },
      });
      terminalCallbacks[1]?.({ payload: { ...event, taskId: "task-current" } });
    });

    expect(observed.mock.calls.map(([nextEvent]) => nextEvent.taskId)).toEqual([
      "task-current",
      "task-current",
    ]);
    secondMount.unmount();
    unregister();
  });

  it("flushes the same task's final stream batch before its terminal event", async () => {
    const observed: BackendEvent[] = [];
    const unregister = registerTaskEventListener((nextEvent) => observed.push(nextEvent));
    useChatStore.setState({ sendTaskId: "task-1", sendSessionId: "session-1" });
    let taskOutputPublications = 0;
    let chatStreamPublications = 0;
    let previousTaskOutput = useTaskStore.getState().taskOutputs["task-1"];
    let previousChatText = useChatStore.getState().streamingText;
    const unsubscribeTask = useTaskStore.subscribe((state) => {
      if (state.taskOutputs["task-1"] !== previousTaskOutput) {
        previousTaskOutput = state.taskOutputs["task-1"];
        taskOutputPublications += 1;
      }
    });
    const unsubscribeChat = useChatStore.subscribe((state) => {
      if (state.streamingText !== previousChatText) {
        previousChatText = state.streamingText;
        chatStreamPublications += 1;
      }
    });
    renderHook(() => {
      useTaskEvents();
      useChatStream();
    });
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    const streamHandler = listenMock.mock.calls.find(
      ([channel]) => channel === "task://stream-output",
    )?.[1] as ((event: { payload: BackendEvent }) => void) | undefined;
    const terminalHandler = listenMock.mock.calls.find(
      ([channel]) => channel === "task://completed",
    )?.[1] as ((event: { payload: BackendEvent }) => void) | undefined;

    act(() => {
      streamHandler?.({
        payload: {
          eventId: "stream-1",
          eventType: "task_stream_output",
          projectId: "project-a",
          taskId: "task-1",
          timestamp: "2026-08-16T00:00:00Z",
          payload: { delta: "complete tail", route: "chat-byok" },
        },
      });
      terminalHandler?.({ payload: event });
    });

    expect(observed.map((nextEvent) => nextEvent.eventType)).toEqual([
      "task_stream_output",
      "task_completed",
    ]);
    expect(useTaskStore.getState().taskOutputs["task-1"]).toBe("complete tail");
    expect(useChatStore.getState().streamingText).toBe("complete tail");
    expect(taskOutputPublications).toBe(1);
    expect(chatStreamPublications).toBe(1);
    unsubscribeTask();
    unsubscribeChat();
    unregister();
  });

  it("does not bridge a queued stream from the previous project into Chat", () => {
    useChatStore.setState({ sendTaskId: "shared-task-id", sendSessionId: "session-b" });
    const { unmount } = renderHook(() => useChatStream());
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-b",
        rootPath: "D:/project-b",
      },
    });

    act(() => {
      dispatchTaskEvent({
        eventId: "old-stream",
        eventType: "task_stream_output",
        projectId: "project-a",
        taskId: "shared-task-id",
        timestamp: "2026-08-16T00:00:00Z",
        payload: { delta: "must not leak", route: "chat-agent" },
      });
      dispatchTaskEvent({
        ...event,
        eventId: "old-terminal",
        taskId: "shared-task-id",
      });
    });

    expect(useChatStore.getState().streamingText).toBe("");
    unmount();
  });

  it.each([
    ["task://failed", "task_failed", "failed"],
    ["task://cancelled", "task_cancelled", "cancelled"],
  ] as const)("flushes the tail and preserves %s task state", async (channel, eventType, status) => {
    renderHook(() => {
      useTaskEvents();
      useChatStream();
    });
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    useChatStore.setState({ sendTaskId: "task-terminal", sendSessionId: "session-1" });
    const streamHandler = listenMock.mock.calls.find(
      ([registeredChannel]) => registeredChannel === "task://stream-output",
    )?.[1] as ((event: { payload: BackendEvent }) => void) | undefined;
    const terminalHandler = listenMock.mock.calls.find(
      ([registeredChannel]) => registeredChannel === channel,
    )?.[1] as ((event: { payload: BackendEvent }) => void) | undefined;

    act(() => {
      streamHandler?.({
        payload: {
          eventId: "terminal-tail",
          eventType: "task_stream_output",
          projectId: "project-a",
          taskId: "task-terminal",
          timestamp: "2026-08-16T00:00:00Z",
          payload: { delta: "last bytes", route: "chat-byok" },
        },
      });
      terminalHandler?.({
        payload: {
          eventId: `terminal-${status}`,
          eventType,
          projectId: "project-a",
          taskId: "task-terminal",
          timestamp: "2026-08-16T00:00:01Z",
          payload: {
            id: "task-terminal",
            taskType: "llm_request",
            projectId: "project-a",
            title: "Chat",
            status,
            progress: null,
            startedAt: "2026-08-16T00:00:00Z",
            updatedAt: "2026-08-16T00:00:01Z",
            completedAt: "2026-08-16T00:00:01Z",
            cancellable: false,
            logPath: null,
            result: null,
            error: status === "failed" ? {
              code: "CHAT_FAILED",
              message: "generation failed",
              details: null,
              recoverable: true,
              userActionRequired: false,
            } : null,
          },
        },
      });
    });

    expect(useTaskStore.getState().taskOutputs["task-terminal"]).toBe("last bytes");
    expect(useChatStore.getState().streamingText).toBe("last bytes");
    expect(useTaskStore.getState().tasks[0]?.status).toBe(status);
    if (status === "failed") {
      expect(useTaskStore.getState().tasks[0]?.error?.message).toBe("generation failed");
    }
  });
});

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useChatStore } from "../../stores/chatStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useTaskStore } from "../../stores/taskStore";
import { resetRoutePresentation } from "../../hooks/useRouteScrollRestoration";
import { useWikiStore } from "../wiki/wikiStore";
import type { ChatMessage, ChatSession } from "../../types/chat";
import type { BackendTask } from "../../types/task";
import { normalizeBackendError } from "../../lib/backendError";
import { ChatView, StreamingBubble, useTranscriptScroll } from "./ChatView";

function ScrollHarness({ streamRevision }: { streamRevision: number }) {
  const transcript = useTranscriptScroll("session-1", 1, streamRevision, 0);
  return (
    <div>
      <div ref={transcript.ref} role="log" onScroll={transcript.onScroll} />
      {transcript.showBackToLatest ? <span>Back to latest</span> : null}
    </div>
  );
}

function RestoredScrollHarness() {
  const transcript = useTranscriptScroll("session-1", 1, 0, 0, "project-1\0d:/wiki");
  return <div ref={transcript.ref} role="log" onScroll={transcript.onScroll} />;
}

const PROJECT = {
  ...defaultProject,
  projectId: "project-1",
  name: "Project",
  rootPath: "D:/wiki",
};

function session(messages: ChatMessage[] = []): ChatSession {
  return {
    id: "session-1",
    title: "Test",
    projectId: PROJECT.projectId,
    createdAt: "2026-06-23T00:00:00Z",
    updatedAt: "2026-06-23T00:00:00Z",
    messages,
  };
}

function seedActiveSession(messages: ChatMessage[] = []) {
  const sendSpy = vi.fn(async () => "task-1");
  useProjectStore.setState({ currentProject: PROJECT });
  useSettingsStore.setState({
    chatConvenienceAuthorization: {
      enabled: true,
      confirmedAt: "2026-07-05T00:00:00Z",
      projectId: PROJECT.projectId,
      rootPathFingerprint: "fp",
    },
    ensureChatConvenienceAuthorization: async () => ({
      enabled: true,
      confirmedAt: "2026-07-05T00:00:00Z",
      projectId: PROJECT.projectId,
      rootPathFingerprint: "fp",
    }),
  });
  useChatStore.setState({
    sessions: [],
    activeSessionId: "session-1",
    activeSession: session(messages),
    ensureSessions: async () => {},
    send: sendSpy as never,
  });
  return sendSpy;
}

describe("ChatView", () => {
  beforeEach(() => {
    useChatStore.getState().reset();
    useSettingsStore.getState().reset();
    useProjectStore.setState({ currentProject: PROJECT });
    useTaskStore.getState().setTasks([]);
  });

  it("mounts with empty session list and composer placeholder", () => {
    // Neutralize the auto-load mount effect so the empty-session branch is
    // observable in a non-Tauri/jsdom environment.
    useChatStore.getState().reset();
    useChatStore.setState({
      sessions: [],
      ensureSessions: async () => {},
    });
    render(<ChatView />);
    expect(screen.getByPlaceholderText(/Ask about this wiki/i)).toBeInTheDocument();
    expect(screen.getByText(/No chat sessions yet/i)).toBeInTheDocument();
  });

  it("renders design-aligned session search and icon controls", () => {
    useChatStore.setState({
      sessions: [
        {
          id: "s1",
          title: "Agent Memory",
          createdAt: "x",
          updatedAt: "2026-07-07T10:00:00Z",
          messageCount: 2,
        },
      ],
      ensureSessions: async () => {},
    });
    render(<ChatView />);

    expect(screen.getByPlaceholderText(/Search chats/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /new chat/i })).toBeInTheDocument();
    expect(screen.queryByText("鉁?")).not.toBeInTheDocument();
    expect(screen.queryByText("脳")).not.toBeInTheDocument();
  });

  it("keeps the session toolbar outside the scrollable message region", () => {
    seedActiveSession();
    render(<ChatView />);

    const log = screen.getByRole("log");
    // The scrollable region owns the chat-scroll-region contract class.
    expect(log).toHaveClass("chat-scroll-region");
    // The session toolbar must not live inside the scrollable log region.
    expect(log.querySelector(".view-toolbar")).toBeNull();
    // It lives inside the fixed chat-stream-wrap column instead.
    expect(document.querySelector(".chat-stream-wrap .view-toolbar")).toBeInTheDocument();
  });

  it("renders active stream text without Markdown, KaTeX, or highlight structures", () => {
    const { container } = render(
      <StreamingBubble
        text={"**bold**\n\n```ts\nconst answer = 42;\n```\n\n$E=mc^2$"}
        activities={[]}
        taskStatus="running"
        routeLabel="BYOK"
        agentLabel="Agent"
        placeholder="Generating"
        onOpenLogs={() => {}}
        openLogsLabel="Open logs"
      />,
    );

    expect(screen.getByText(/\*\*bold\*\*/)).toBeInTheDocument();
    expect(container.querySelector("pre")).toBeNull();
    expect(container.querySelector(".katex")).toBeNull();
    expect(container.querySelector(".hljs")).toBeNull();
  });

  it("reloads a terminal answer and restores full Markdown rendering", async () => {
    seedActiveSession();
    const persistedAnswer = "**Finished answer with complete tail**";
    const reloadActive = vi.fn(async () => {
      useChatStore.setState({
        activeSession: session([{
          id: "assistant-terminal",
          role: "assistant",
          content: persistedAnswer,
          createdAt: "2026-08-16T00:00:01Z",
          citations: [],
        }]),
      });
      return true;
    });
    useChatStore.setState({
      sendTaskId: "task-terminal",
      sendSessionId: "session-1",
      streamingText: "**Finished answer with complete tail**",
      reloadActive: reloadActive as never,
    });
    const runningTask: BackendTask = {
      id: "task-terminal",
      taskType: "llm_request",
      projectId: PROJECT.projectId,
      title: "Chat",
      status: "running",
      progress: null,
      startedAt: "2026-08-16T00:00:00Z",
      updatedAt: "2026-08-16T00:00:00Z",
      completedAt: null,
      cancellable: true,
      logPath: null,
      result: null,
      error: null,
    };
    useTaskStore.getState().setTasks([runningTask]);
    const { container } = render(<ChatView />);

    act(() => {
      useTaskStore.getState().setTasks([{
        ...runningTask,
        status: "succeeded",
        updatedAt: "2026-08-16T00:00:01Z",
        completedAt: "2026-08-16T00:00:01Z",
        cancellable: false,
      }]);
    });

    await waitFor(() => {
      expect(reloadActive).toHaveBeenCalledTimes(1);
      expect(container.querySelector("strong")).toHaveTextContent(
        "Finished answer with complete tail",
      );
    });
    expect(useChatStore.getState().streamingText).toBe("");
  });

  it("reconciles a send that completed while Chat was unmounted", async () => {
    seedActiveSession();
    const reloadActive = vi.fn(async () => {
      useChatStore.setState({
        activeSession: session([{
          id: "assistant-after-return",
          role: "assistant",
          content: "Persisted after leaving Chat",
          createdAt: "2026-08-16T00:00:01Z",
          citations: [],
        }]),
      });
      return true;
    });
    useChatStore.setState({
      sendTaskId: "task-away",
      sendSessionId: "session-1",
      streamingText: "temporary",
      reloadActive: reloadActive as never,
    });
    const runningTask: BackendTask = {
      id: "task-away",
      taskType: "llm_request",
      projectId: PROJECT.projectId,
      title: "Chat",
      status: "running",
      progress: null,
      startedAt: "2026-08-16T00:00:00Z",
      updatedAt: "2026-08-16T00:00:00Z",
      completedAt: null,
      cancellable: true,
      logPath: null,
      result: null,
      error: null,
    };
    useTaskStore.getState().setTasks([runningTask]);
    const mounted = render(<ChatView />);
    mounted.unmount();

    act(() => {
      useTaskStore.getState().setTasks([{
        ...runningTask,
        status: "succeeded",
        completedAt: "2026-08-16T00:00:01Z",
        updatedAt: "2026-08-16T00:00:01Z",
        cancellable: false,
      }]);
    });
    render(<ChatView />);

    await waitFor(() => expect(reloadActive).toHaveBeenCalledTimes(1));
    expect(screen.getByText("Persisted after leaving Chat")).toBeInTheDocument();
    expect(useChatStore.getState().sendTaskId).toBeNull();
    expect(useChatStore.getState().streamingText).toBe("");
  });

  it("keeps terminal stream presentation until a failed persisted reload is retried", async () => {
    seedActiveSession();
    const reloadActive = vi.fn()
      .mockImplementationOnce(async () => {
        useChatStore.setState({
          error: normalizeBackendError("offline", {
            defaultSummaryKey: "backendError.summary.chat",
            defaultActionKind: "retry",
            defaultRecoverable: true,
          }),
        });
        return false;
      })
      .mockResolvedValueOnce(true);
    useChatStore.setState({
      sendTaskId: "task-retry",
      sendSessionId: "session-1",
      streamingText: "temporary answer",
      reloadActive: reloadActive as never,
    });
    useTaskStore.getState().setTasks([{
      id: "task-retry",
      taskType: "llm_request",
      projectId: PROJECT.projectId,
      title: "Chat",
      status: "succeeded",
      progress: null,
      startedAt: "2026-08-16T00:00:00Z",
      updatedAt: "2026-08-16T00:00:01Z",
      completedAt: "2026-08-16T00:00:01Z",
      cancellable: false,
      logPath: null,
      result: null,
      error: null,
    }]);
    render(<ChatView />);

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        "The message could not be sent or saved.",
      );
    });
    expect(screen.getByText("offline")).not.toBeVisible();
    expect(useChatStore.getState().sendTaskId).toBe("task-retry");
    expect(useChatStore.getState().streamingText).toBe("temporary answer");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(useChatStore.getState().sendTaskId).toBeNull());
    expect(useChatStore.getState().error).toBeNull();
    expect(reloadActive).toHaveBeenCalledTimes(2);
  });

  it("does not move scrollTop after the user unpins during streaming", () => {
    vi.useFakeTimers();
    const { rerender } = render(<ScrollHarness streamRevision={0} />);
    const log = screen.getByRole("log");
    Object.defineProperty(log, "scrollHeight", { value: 1_000, configurable: true });
    Object.defineProperty(log, "clientHeight", { value: 200, configurable: true });
    log.scrollTop = 300;
    fireEvent.scroll(log);

    expect(screen.getByText("Back to latest")).toBeInTheDocument();
    rerender(<ScrollHarness streamRevision={1} />);
    act(() => vi.runAllTimers());

    expect(log.scrollTop).toBe(300);
    vi.useRealTimers();
  });

  it("restores an unpinned transcript after the chat route remounts", () => {
    resetRoutePresentation();
    const first = render(<RestoredScrollHarness />);
    const log = screen.getByRole("log");
    Object.defineProperty(log, "scrollHeight", { value: 1_000, configurable: true });
    Object.defineProperty(log, "clientHeight", { value: 200, configurable: true });
    log.scrollTop = 320;
    fireEvent.scroll(log);
    first.unmount();

    const second = render(<RestoredScrollHarness />);
    expect(screen.getByRole("log").scrollTop).toBe(320);
    second.unmount();
  });

  it("coalesces transcript updates into one RAF and cancels it on unmount", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    let nextFrame = 1;
    const requestFrame = vi.fn((callback: FrameRequestCallback) => {
      const id = nextFrame++;
      callbacks.set(id, callback);
      return id;
    });
    const cancelFrame = vi.fn((id: number) => {
      callbacks.delete(id);
    });
    vi.stubGlobal("requestAnimationFrame", requestFrame);
    vi.stubGlobal("cancelAnimationFrame", cancelFrame);

    const { rerender, unmount } = render(<ScrollHarness streamRevision={0} />);
    rerender(<ScrollHarness streamRevision={1} />);
    rerender(<ScrollHarness streamRevision={2} />);

    expect(requestFrame).toHaveBeenCalledTimes(1);
    unmount();
    expect(cancelFrame).toHaveBeenCalledTimes(1);
    vi.unstubAllGlobals();
  });

  it("opens model source markers by source id rather than citation-card index", () => {
    const openPage = vi.fn(async () => {});
    useWikiStore.setState({ openPage: openPage as never });
    seedActiveSession([
      {
        id: "assistant-1",
        role: "assistant",
        content: "Grounded in the source [S2].",
        createdAt: "2026-07-07T00:00:00Z",
        citations: [
          {
            sourceId: "S2",
            pagePath: "wiki/concepts/react-pattern.md",
            title: "ReAct Pattern",
            score: 100,
          },
        ],
      },
    ]);

    render(<ChatView />);
    fireEvent.click(screen.getByRole("button", { name: /S2/i }));

    expect(openPage).toHaveBeenCalledWith(
      PROJECT.projectId,
      PROJECT.rootPath,
      "wiki/concepts/react-pattern.md",
    );
  });

  it("keeps a chat send failure technical message behind details", async () => {
    const failedTask: BackendTask = {
      id: "chat-task-1",
      taskType: "llm_request",
      projectId: "project-1",
      title: "Chat: hello",
      status: "failed",
      progress: null,
      startedAt: "2026-06-23T00:00:00Z",
      updatedAt: "2026-06-23T00:00:01Z",
      completedAt: "2026-06-23T00:00:01Z",
      cancellable: true,
      logPath: null,
      result: null,
      error: {
        code: "AGENT_UNAVAILABLE",
        message: "No usable Agent CLI is configured.",
        details: null,
        recoverable: true,
        userActionRequired: true,
      },
    };
    useChatStore.getState().reset();
    useChatStore.setState({
      sessions: [],
      activeSessionId: "session-1",
      activeSession: {
        id: "session-1",
        title: "Test",
        projectId: "project-1",
        createdAt: "2026-06-23T00:00:00Z",
        updatedAt: "2026-06-23T00:00:00Z",
        messages: [],
      },
      sendTaskId: failedTask.id,
      sendSessionId: "session-1",
      ensureSessions: async () => {},
      reloadActive: async () => true,
    });
    useTaskStore.getState().setTasks([failedTask]);

    render(<ChatView />);

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        "The message could not be sent or saved.",
      );
    });
    expect(screen.getByText(/No usable Agent CLI is configured/)).not.toBeVisible();
    fireEvent.click(screen.getByText("Technical details"));
    expect(screen.getByText(/No usable Agent CLI is configured/)).toBeVisible();
  });

  it("passes convenienceEnabled when authorized and no edit is pending", () => {
    const sendSpy = seedActiveSession();
    render(<ChatView />);

    fireEvent.change(screen.getByPlaceholderText(/Ask about this wiki/i), {
      target: { value: "Update the overview page" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(sendSpy).toHaveBeenCalledWith(
      PROJECT.projectId,
      PROJECT.rootPath,
      "session-1",
      "Update the overview page",
      "auto",
      { convenienceEnabled: true },
    );
  });

  it("suppresses convenienceEnabled while a soft violation is pending", () => {
    const sendSpy = seedActiveSession([
      {
        id: "assistant-1",
        role: "assistant",
        content: "Needs review",
        createdAt: "2026-07-05T00:00:00Z",
        convenienceEdit: {
          status: "soft_violation_pending",
          checkpointHash: "abc123",
          affectedPaths: ["wiki/a.md"],
          diffSummary: "1 file changed",
          diffText: "+hello",
          violationReason: "too many files",
        },
      },
    ]);
    render(<ChatView />);

    fireEvent.change(screen.getByPlaceholderText(/Ask about this wiki/i), {
      target: { value: "Update another page" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(sendSpy).toHaveBeenCalledWith(
      PROJECT.projectId,
      PROJECT.rootPath,
      "session-1",
      "Update another page",
      "auto",
      { convenienceEnabled: false },
    );
  });

  it("does not clear the composer when no session can accept the send", async () => {
    useChatStore.setState({
      activeSessionId: null,
      activeSession: null,
      sessions: [],
      createSession: vi.fn(async () => null) as never,
      ensureSessions: async () => {},
    });
    render(<ChatView />);

    const box = screen.getByPlaceholderText(/Ask about this wiki/i);
    fireEvent.change(box, { target: { value: "Will this vanish?" } });
    fireEvent.click(screen.getByRole("button", { name: /send/i }));

    await waitFor(() => expect(box).toHaveValue("Will this vanish?"));
  });

  it("does not request convenience edits for explicit BYOK sends", () => {
    const sendSpy = seedActiveSession();
    render(<ChatView />);

    fireEvent.click(screen.getByRole("button", { name: "BYOK" }));
    fireEvent.change(screen.getByPlaceholderText(/Ask about this wiki/i), {
      target: { value: "Update the overview page" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(sendSpy).toHaveBeenCalledWith(
      PROJECT.projectId,
      PROJECT.rootPath,
      "session-1",
      "Update the overview page",
      "byok",
      { convenienceEnabled: false },
    );
  });

  it("deletes the active session from the toolbar only after confirmation", () => {
    const deleteSession = vi.fn(async () => {});
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session(),
      sessions: [],
      deleteSession: deleteSession as never,
      ensureSessions: async () => {},
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<ChatView />);

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(deleteSession).not.toHaveBeenCalled();

    confirmSpy.mockReturnValue(true);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(deleteSession).toHaveBeenCalledWith(
      PROJECT.projectId,
      PROJECT.rootPath,
      "session-1",
    );
    confirmSpy.mockRestore();
  });

  it("gates session-list deletion behind confirmation", () => {
    const deleteSession = vi.fn(async () => {});
    useChatStore.setState({
      activeSessionId: null,
      activeSession: null,
      sessions: [
        {
          id: "s1",
          title: "Agent Memory",
          createdAt: "2026-07-07T00:00:00Z",
          updatedAt: "2026-07-07T00:00:00Z",
          messageCount: 1,
        },
      ],
      deleteSession: deleteSession as never,
      ensureSessions: async () => {},
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const { container } = render(<ChatView />);

    // The list's trash button is hover-revealed (display:none in jsdom), so
    // query it directly rather than via the accessible-tree role query.
    const trashButton = container.querySelector('button[aria-label="Delete"]');
    expect(trashButton).not.toBeNull();
    fireEvent.click(trashButton as Element);

    expect(deleteSession).toHaveBeenCalledWith(PROJECT.projectId, PROJECT.rootPath, "s1");
    confirmSpy.mockRestore();
  });
});

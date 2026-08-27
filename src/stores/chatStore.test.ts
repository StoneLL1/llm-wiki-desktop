import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ChatSession, ChatSessionSummary, SaveAnswerResult } from "../types/chat";
import { invalidateProjectScope } from "./projectScope";
import { useTaskStore } from "./taskStore";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import { useChatStore } from "./chatStore";

const sessionSummary = (overrides: Partial<ChatSessionSummary> = {}): ChatSessionSummary => ({
  id: "s1",
  title: "Session 1",
  createdAt: "2026-06-20T00:00:00Z",
  updatedAt: "2026-06-20T00:00:00Z",
  messageCount: 0,
  ...overrides,
});

const session = (overrides: Partial<ChatSession> = {}): ChatSession => ({
  id: "s1",
  title: "Session 1",
  projectId: "p",
  createdAt: "2026-06-20T00:00:00Z",
  updatedAt: "2026-06-20T00:00:00Z",
  messages: [],
  ...overrides,
});

const PROJECT = { projectId: "p", rootPath: "/x" };

// Drain the microtask queue so an async store action advances to its next
// await before the test continues.
const flushMicrotasks = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  invokeMock.mockReset();
  useChatStore.getState().reset();
  useTaskStore.getState().setTasks([]);
  // Pretend the desktop runtime is present so the hasTauri() guard passes.
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("chatStore", () => {
  it("single-flights session ensures without resetting the current selection", async () => {
    useChatStore.setState({ activeSessionId: "kept", activeSession: session({ id: "kept" }) });
    invokeMock.mockResolvedValue([sessionSummary({ id: "kept" })]);

    await Promise.all(Array.from({ length: 20 }, () =>
      useChatStore.getState().ensureSessions(PROJECT.projectId, PROJECT.rootPath)));

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(useChatStore.getState().activeSessionId).toBe("kept");
  });

  it("falls back to the newest session when the selected session disappeared", async () => {
    useChatStore.setState({ activeSessionId: "missing", activeSession: session({ id: "missing" }) });
    invokeMock
      .mockResolvedValueOnce([sessionSummary({ id: "new" })])
      .mockResolvedValueOnce(session({ id: "new" }));

    await useChatStore.getState().ensureSessions(PROJECT.projectId, PROJECT.rootPath);

    expect(useChatStore.getState().activeSessionId).toBe("new");
    expect(useChatStore.getState().activeSession?.id).toBe("new");
  });

  it("loads sessions and selects the first one via selectSession", async () => {
    invokeMock.mockResolvedValueOnce([sessionSummary(), sessionSummary({ id: "s2", title: "Two" })]);
    await useChatStore.getState().loadSessions(PROJECT.projectId, PROJECT.rootPath);
    expect(useChatStore.getState().sessions).toHaveLength(2);

    invokeMock.mockResolvedValueOnce(session());
    await useChatStore.getState().selectSession(PROJECT.projectId, PROJECT.rootPath, "s1");
    expect(useChatStore.getState().activeSessionId).toBe("s1");
    expect(useChatStore.getState().activeSession?.id).toBe("s1");
  });

  it("auto-selects the newest session when loading sessions without an active session", async () => {
    invokeMock
      .mockResolvedValueOnce([
        sessionSummary({ id: "new", title: "Newest", updatedAt: "2026-07-07T10:00:00Z" }),
        sessionSummary({ id: "old", title: "Old", updatedAt: "2026-07-06T10:00:00Z" }),
      ])
      .mockResolvedValueOnce(session({ id: "new", title: "Newest" }));

    await useChatStore.getState().loadSessions(PROJECT.projectId, PROJECT.rootPath);

    expect(useChatStore.getState().activeSessionId).toBe("new");
    expect(useChatStore.getState().activeSession?.id).toBe("new");
    // The second invoke must be the session load (selectSession), not a re-list.
    expect(invokeMock.mock.calls[1][0]).toBe("load_chat_session");
    expect(invokeMock.mock.calls[1][1].request.sessionId).toBe("new");
  });

  it("does not replace an already selected session during list refresh", async () => {
    useChatStore.setState({ activeSessionId: "old", activeSession: session({ id: "old" }) });
    invokeMock.mockResolvedValueOnce([
      sessionSummary({ id: "new", updatedAt: "2026-07-07T10:00:00Z" }),
      sessionSummary({ id: "old", updatedAt: "2026-07-06T10:00:00Z" }),
    ]);

    await useChatStore.getState().loadSessions(PROJECT.projectId, PROJECT.rootPath);

    expect(useChatStore.getState().activeSessionId).toBe("old");
    // Only the list call was made; no auto-select load.
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock.mock.calls[0][0]).toBe("list_chat_sessions");
  });

  it("does not let an older session load overwrite a newer selection", async () => {
    const resolvers: Array<(value: ChatSession) => void> = [];
    invokeMock.mockImplementation((command: string) => {
      if (command !== "load_chat_session") return Promise.resolve([]);
      return new Promise<ChatSession>((resolve) => resolvers.push(resolve));
    });

    const first = useChatStore.getState().selectSession(PROJECT.projectId, PROJECT.rootPath, "s1");
    const second = useChatStore.getState().selectSession(PROJECT.projectId, PROJECT.rootPath, "s2");
    resolvers[1]?.(session({ id: "s2", title: "Second" }));
    resolvers[0]?.(session({ id: "s1", title: "First" }));
    await Promise.all([first, second]);

    expect(useChatStore.getState().activeSessionId).toBe("s2");
    expect(useChatStore.getState().activeSession?.title).toBe("Second");
  });

  it("send stores the task id returned by the backend", async () => {
    invokeMock.mockResolvedValueOnce({ id: "task-1" });
    const taskId = await useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Hello",
      "auto",
    );
    expect(taskId).toBe("task-1");
    expect(useChatStore.getState().sendTaskId).toBe("task-1");
    expect(useChatStore.getState().pendingUserMessages["task-1"]?.content).toBe("Hello");
    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("send_chat_message");
    expect(call[1].request.route).toBe("auto");
    expect(call[1].request.content).toBe("Hello");
    expect(call[1].request.pinnedPagePath).toBeNull();
    expect(call[1].request.convenienceEnabled).toBe(false);
  });

  it("keeps returned task facts when the project scope changes before IPC resolves", async () => {
    let resolveTask: ((task: { id: string }) => void) | undefined;
    invokeMock.mockReturnValueOnce(new Promise((resolve) => { resolveTask = resolve; }));
    const pending = useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Hello",
      "auto",
    );

    invalidateProjectScope();
    resolveTask?.({ id: "task-after-switch" });
    await pending;

    expect(useTaskStore.getState().taskById["task-after-switch"]).toBeDefined();
  });

  it("replays stream deltas that arrive before the send response binds the task", async () => {
    useChatStore.getState().appendStreamDelta("task-early", "你好", "byok");
    invokeMock.mockResolvedValueOnce({ id: "task-early" });

    await useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Hello",
      "auto",
    );

    expect(useChatStore.getState().streamingText).toBe("你好");
    expect(useChatStore.getState().streamingRoute).toBe("byok");
    expect(useChatStore.getState().pendingStreamDeltas).toEqual({});
  });

  it("does not offer a fake retry when starting a send fails before a task exists", async () => {
    invokeMock.mockRejectedValueOnce({
      code: "LLM_RATE_LIMITED",
      message: "send start failed",
      details: null,
      recoverable: true,
      userActionRequired: false,
    });

    const taskId = await useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Keep this draft",
      "auto",
    );

    expect(taskId).toBeNull();
    expect(useChatStore.getState().sendTaskId).toBeNull();
    expect(useChatStore.getState().sendSessionId).toBeNull();
    expect(useChatStore.getState().sendStarting).toBe(false);
    expect(useChatStore.getState().error).toMatchObject({
      actionKind: null,
      summaryKey: "backendError.summary.provider",
    });
  });

  it("does not label a completed send failure as retry after its resend context is cleared", () => {
    useChatStore.setState({
      sendTaskId: "task-rate-limit",
      sendSessionId: "s1",
      pendingUserMessages: {
        "task-rate-limit": {
          id: "pending-user-task-rate-limit",
          role: "user",
          content: "Original prompt",
          createdAt: "2026-08-16T00:00:00Z",
          taskId: "task-rate-limit",
        },
      },
    });

    useChatStore.getState().clearSendTask({
      code: "LLM_RATE_LIMITED",
      message: "rate limited",
      details: null,
      recoverable: true,
      userActionRequired: false,
    });

    expect(useChatStore.getState().sendTaskId).toBeNull();
    expect(useChatStore.getState().pendingUserMessages).toEqual({});
    expect(useChatStore.getState().error).toMatchObject({
      code: "LLM_RATE_LIMITED",
      actionKind: null,
    });
  });

  it("publishes one exact active answer for an aggregated 256 KiB stream batch", () => {
    const text = "0123456789abcdef".repeat(16 * 1024);
    useChatStore.setState({ sendTaskId: "task-stream", sendSessionId: "s1" });
    let publications = 0;
    const unsubscribe = useChatStore.subscribe(() => { publications += 1; });

    useChatStore.getState().appendStreamDelta("task-stream", text, "byok");

    unsubscribe();
    expect(publications).toBe(1);
    expect(useChatStore.getState().streamingText).toBe(text);
  });

  it("does not truncate a pre-bind stream before replaying it to the returned task", async () => {
    const text = "界".repeat(300 * 1024);
    useChatStore.getState().appendStreamDelta("task-early-large", text, "byok");
    invokeMock.mockResolvedValueOnce({ id: "task-early-large" });

    await useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Hello",
      "auto",
    );

    expect(useChatStore.getState().streamingText).toBe(text);
  });

  it("send includes pinnedPagePath when provided", async () => {
    invokeMock.mockResolvedValueOnce({ id: "task-pinned" });
    const taskId = await useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Explain this page",
      "auto",
      { pinnedPagePath: "wiki/concepts/react-pattern.md" },
    );

    expect(taskId).toBe("task-pinned");
    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("send_chat_message");
    expect(call[1].request.pinnedPagePath).toBe("wiki/concepts/react-pattern.md");
  });

  it("send includes convenienceEnabled when requested", async () => {
    invokeMock.mockResolvedValueOnce({ id: "task-convenience" });
    await useChatStore.getState().send(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "Update this page",
      "agent",
      { convenienceEnabled: true },
    );

    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("send_chat_message");
    expect(call[1].request.convenienceEnabled).toBe(true);
  });

  it("resolves and rolls back convenience edits through backend commands", async () => {
    invokeMock
      .mockResolvedValueOnce(session({ messages: [{ id: "m1" } as never] }))
      .mockResolvedValueOnce([sessionSummary()])
      .mockResolvedValueOnce(session({ messages: [] }))
      .mockResolvedValueOnce([sessionSummary()]);

    await useChatStore
      .getState()
      .resolveConvenienceEdit(PROJECT.projectId, PROJECT.rootPath, "s1", "m1", true);
    expect(invokeMock.mock.calls[0][0]).toBe("resolve_chat_convenience_edit");
    expect(invokeMock.mock.calls[0][1].request).toMatchObject({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      sessionId: "s1",
      messageId: "m1",
      keep: true,
    });

    await useChatStore
      .getState()
      .rollbackLastConvenienceEdit(PROJECT.projectId, PROJECT.rootPath, "s1");
    expect(invokeMock.mock.calls[2][0]).toBe("rollback_last_chat_convenience_edit");
    expect(invokeMock.mock.calls[2][1].request).toMatchObject({
      projectId: PROJECT.projectId,
      projectRootPath: PROJECT.rootPath,
      sessionId: "s1",
    });
  });

  it("saveAnswer surfaces FILE_ALREADY_EXISTS as an overwrite request", async () => {
    invokeMock.mockRejectedValueOnce({
      code: "FILE_ALREADY_EXISTS",
      message: "exists",
      details: { path: "wiki/queries/foo.md", currentHash: "abc", actionId: "action-1" },
    });
    const result = await useChatStore.getState().saveAnswer(
      PROJECT.projectId,
      PROJECT.rootPath,
      "s1",
      "m1",
    );
    expect(result).toBeNull();
    expect(useChatStore.getState().overwriteRequest).toEqual({
      sessionId: "s1",
      messageId: "m1",
      path: "wiki/queries/foo.md",
      currentHash: "abc",
      actionId: "action-1",
    });
    expect(useChatStore.getState().saveStatus.m1).toBe("exists");
  });

  it("confirmOverwrite re-invokes with allowOverwrite and the expected hash", async () => {
    useChatStore.setState({
      overwriteRequest: {
        sessionId: "s1",
        messageId: "m1",
        path: "wiki/queries/foo.md",
        currentHash: "abc",
        actionId: "action-1",
      },
      activeSessionId: "s1",
      saveStatus: { m1: "exists" },
    });
    invokeMock.mockResolvedValueOnce({} as SaveAnswerResult);
    await useChatStore.getState().confirmOverwrite(PROJECT.projectId, PROJECT.rootPath);
    const call = invokeMock.mock.calls[0];
      expect(call[1].request.allowOverwrite).toBe(true);
      expect(call[1].request.sessionId).toBe("s1");
    expect(call[1].request.expectedHash).toBe("abc");
    expect(call[1].request.actionId).toBe("action-1");
    expect(useChatStore.getState().overwriteRequest).toBeNull();
    expect(useChatStore.getState().saveStatus.m1).toBe("saved");
  });

  it("does not replace an existing overwrite action when the same answer is saved again", async () => {
    const pending = {
      sessionId: "s1",
      messageId: "m1",
      path: "wiki/queries/foo.md",
      currentHash: "abc",
      actionId: "action-1",
    };
    useChatStore.setState({ overwriteRequest: pending });

    await useChatStore.getState().saveAnswer(PROJECT.projectId, PROJECT.rootPath, "s1", "m1");

    expect(invokeMock).not.toHaveBeenCalled();
    expect(useChatStore.getState().overwriteRequest).toEqual(pending);
  });

  describe("ensurePageSession (lazy / page-scoped)", () => {
    // Pre-set a non-null active session so loadSessions' auto-select branch
    // stays dormant and the ensurePageSession logic is what's under test.
    function seedActive(activeId: string) {
      useChatStore.setState({
        activeSessionId: activeId,
        activeSession: session({ id: activeId }),
      });
    }

    function createWasCalled(): boolean {
      return invokeMock.mock.calls.some((call) => call[0] === "create_chat_session");
    }

    it("reuses an existing page-scoped session instead of creating one", async () => {
      seedActive("pre");
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_chat_sessions") {
          return [
            sessionSummary({ id: "page-a", contextPagePath: "wiki/a.md" }),
            sessionSummary({ id: "other", contextPagePath: "wiki/other.md" }),
          ];
        }
        if (cmd === "load_chat_session") {
          return session({ id: "page-a", contextPagePath: "wiki/a.md" });
        }
        return undefined;
      });

      const result = await useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", false);

      expect(result?.id).toBe("page-a");
      expect(useChatStore.getState().activeSessionId).toBe("page-a");
      expect(createWasCalled()).toBe(false);
    });

    it("does not create a session when no page match exists, and clears the active session", async () => {
      seedActive("pre");
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_chat_sessions") {
          return [sessionSummary({ id: "other", contextPagePath: "wiki/other.md" })];
        }
        return undefined;
      });

      const result = await useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", false);

      expect(result).toBeNull();
      expect(createWasCalled()).toBe(false);
      // Clearing the stale active session stops a different page's thread from
      // bleeding into this one (the original bug #4).
      expect(useChatStore.getState().activeSessionId).toBeNull();
      expect(useChatStore.getState().activeSession).toBeNull();
    });

    it("clears the previous page session immediately while resolving the new page", async () => {
      seedActive("pre");
      invokeMock.mockResolvedValueOnce([]);

      const pending = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", false);

      expect(useChatStore.getState().activeSessionId).toBeNull();
      expect(useChatStore.getState().activeSession).toBeNull();

      await pending;
    });

    it("forceNew=true always creates a fresh page-scoped session", async () => {
      seedActive("pre");
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_chat_sessions") {
          return [sessionSummary({ id: "page-a", contextPagePath: "wiki/a.md" })];
        }
        if (cmd === "create_chat_session") {
          return session({ id: "fresh", contextPagePath: "wiki/a.md" });
        }
        if (cmd === "load_chat_session") {
          return session({ id: "fresh", contextPagePath: "wiki/a.md" });
        }
        return undefined;
      });

      const result = await useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", true);

      expect(result?.id).toBe("fresh");
      expect(createWasCalled()).toBe(true);
    });

    it("bails when a newer page focus supersedes an in-flight ensurePageSession", async () => {
      seedActive("pre");
      let listCall = 0;
      const listResolvers: Array<(value: ChatSessionSummary[]) => void> = [];
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_chat_sessions") {
          const idx = listCall++;
          return new Promise<ChatSessionSummary[]>((resolve) => {
            listResolvers[idx] = resolve;
          });
        }
        if (cmd === "load_chat_session") {
          return session({ id: "page-a", contextPagePath: "wiki/a.md" });
        }
        return undefined;
      });

      // A (page A) starts and awaits its session list.
      const callA = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", false);

      // User switches to page B before A's list resolves — B supersedes A.
      const callB = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/b.md", "B", false);

      // B's list resolves first: only wiki/a.md exists, so B has no match and
      // clears the stale active session.
      expect(listResolvers[1]).toBeDefined();
      listResolvers[1]([sessionSummary({ id: "page-a", contextPagePath: "wiki/a.md" })]);

      // A's list resolves last. A would normally reuse wiki/a.md, but B has
      // superseded it, so A must bail without selecting page-a onto page B.
      expect(listResolvers[0]).toBeDefined();
      listResolvers[0]([sessionSummary({ id: "page-a", contextPagePath: "wiki/a.md" })]);

      const [resultA, resultB] = await Promise.all([callA, callB]);

      expect(resultA).toBeNull();
      expect(resultB).toBeNull();
      // page-a's thread was never loaded onto the now-current page B.
      expect(invokeMock.mock.calls.some((c) => c[0] === "load_chat_session")).toBe(false);
      expect(useChatStore.getState().activeSessionId).toBeNull();
    });

    it("does not commit a reused session when superseded mid-load", async () => {
      seedActive("pre");
      const loadResolvers: Array<(value: ChatSession) => void> = [];
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_chat_sessions") {
          return [sessionSummary({ id: "page-a", contextPagePath: "wiki/a.md" })];
        }
        if (cmd === "load_chat_session") {
          return new Promise<ChatSession>((resolve) => {
            loadResolvers.push(resolve);
          });
        }
        return undefined;
      });

      // A (page A) finds a match and enters the session load.
      const callA = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", false);
      await flushMicrotasks();
      expect(loadResolvers).toHaveLength(1);

      // B (page B) supersedes A while A's load is still pending and clears
      // the stale active session.
      const callB = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/b.md", "B", false);
      await flushMicrotasks();
      expect(useChatStore.getState().activeSessionId).toBeNull();

      // A's load resolves last. A must NOT commit page-a's thread.
      loadResolvers[0]?.(session({ id: "page-a", contextPagePath: "wiki/a.md" }));

      const [resultA, resultB] = await Promise.all([callA, callB]);

      expect(resultA).toBeNull();
      expect(resultB).toBeNull();
      expect(useChatStore.getState().activeSession).toBeNull();
      expect(useChatStore.getState().activeSessionId).toBeNull();
    });

    it("does not commit a freshly created session when superseded mid-create", async () => {
      seedActive("pre");
      const createResolvers: Array<(value: ChatSession) => void> = [];
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "list_chat_sessions") {
          return [sessionSummary({ id: "other", contextPagePath: "wiki/other.md" })];
        }
        if (cmd === "create_chat_session") {
          return new Promise<ChatSession>((resolve) => {
            createResolvers.push(resolve);
          });
        }
        return undefined;
      });

      // A clicks "New Chat" on page A and reaches the create invoke.
      const callA = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/a.md", "A", true);
      await flushMicrotasks();
      expect(createResolvers).toHaveLength(1);

      // B (page B) supersedes A and clears the stale active session.
      const callB = useChatStore
        .getState()
        .ensurePageSession(PROJECT.projectId, PROJECT.rootPath, "wiki/b.md", "B", false);
      await flushMicrotasks();
      expect(useChatStore.getState().activeSessionId).toBeNull();

      // A's create resolves. The session is written to disk, but A must NOT
      // select it onto the now-current page B.
      createResolvers[0]?.(session({ id: "new-a", contextPagePath: "wiki/a.md" }));

      const [resultA, resultB] = await Promise.all([callA, callB]);

      expect(resultA).toBeNull();
      expect(resultB).toBeNull();
      expect(useChatStore.getState().activeSession).toBeNull();
      expect(useChatStore.getState().activeSessionId).toBeNull();
    });
  });
});

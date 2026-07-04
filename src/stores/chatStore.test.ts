import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ChatSession, ChatSessionSummary, SaveAnswerResult } from "../types/chat";

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

beforeEach(() => {
  invokeMock.mockReset();
  useChatStore.getState().reset();
  // Pretend the desktop runtime is present so the hasTauri() guard passes.
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("chatStore", () => {
  it("loads sessions and selects the first one via selectSession", async () => {
    invokeMock.mockResolvedValueOnce([sessionSummary(), sessionSummary({ id: "s2", title: "Two" })]);
    await useChatStore.getState().loadSessions(PROJECT.projectId, PROJECT.rootPath);
    expect(useChatStore.getState().sessions).toHaveLength(2);

    invokeMock.mockResolvedValueOnce(session());
    await useChatStore.getState().selectSession(PROJECT.projectId, PROJECT.rootPath, "s1");
    expect(useChatStore.getState().activeSessionId).toBe("s1");
    expect(useChatStore.getState().activeSession?.id).toBe("s1");
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
    const call = invokeMock.mock.calls[0];
    expect(call[0]).toBe("send_chat_message");
    expect(call[1].request.route).toBe("auto");
    expect(call[1].request.content).toBe("Hello");
    expect(call[1].request.pinnedPagePath).toBeNull();
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
    expect(call[1].request.expectedHash).toBe("abc");
    expect(call[1].request.actionId).toBe("action-1");
    expect(useChatStore.getState().overwriteRequest).toBeNull();
    expect(useChatStore.getState().saveStatus.m1).toBe("saved");
  });
});

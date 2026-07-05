import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useChatStore } from "../../stores/chatStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useTaskStore } from "../../stores/taskStore";
import type { ChatMessage, ChatSession } from "../../types/chat";
import type { BackendTask } from "../../types/task";
import { ChatView } from "./ChatView";

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
    loadChatConvenienceAuthorization: async () => ({
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
    loadSessions: async () => {},
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
      loadSessions: async () => {},
    });
    render(<ChatView />);
    expect(screen.getByPlaceholderText(/Ask about this wiki/i)).toBeInTheDocument();
    expect(screen.getByText(/No chat sessions yet/i)).toBeInTheDocument();
  });

  it("surfaces the backend error when a chat send task fails", async () => {
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
      loadSessions: async () => {},
      reloadActive: async () => {},
    });
    useTaskStore.getState().setTasks([failedTask]);

    render(<ChatView />);

    await waitFor(() => {
      expect(screen.getByText("No usable Agent CLI is configured.")).toBeInTheDocument();
    });
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
});

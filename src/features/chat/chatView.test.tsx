import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "../../i18n";
import { useChatStore } from "../../stores/chatStore";
import { useTaskStore } from "../../stores/taskStore";
import type { BackendTask } from "../../types/task";
import { ChatView } from "./ChatView";

describe("ChatView", () => {
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
});

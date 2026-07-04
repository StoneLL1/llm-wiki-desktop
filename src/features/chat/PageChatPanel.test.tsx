import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useChatStore } from "../../stores/chatStore";
import { useTaskStore } from "../../stores/taskStore";
import type { ChatSession } from "../../types/chat";
import type { WikiPageContent } from "../../types/wiki";
import { PageChatPanel } from "./PageChatPanel";

const page: WikiPageContent = {
  meta: {
    path: "wiki/concepts/react-pattern.md",
    title: "ReAct Pattern",
    pageType: "concept",
    tags: [],
    sources: [],
    aliases: [],
    created: null,
    updated: null,
    starred: false,
    bookmarked: false,
    wordCount: 12,
    fileSize: 120,
    modifiedTime: "2026-07-04T00:00:00Z",
    hash: "abc",
    wikilinks: [],
  },
  rawMarkdown: "# ReAct Pattern",
  bodyMarkdown: "# ReAct Pattern",
  frontmatterYaml: null,
};

const session = (overrides: Partial<ChatSession> = {}): ChatSession => ({
  id: "session-1",
  title: "Ask: ReAct Pattern",
  projectId: "project-1",
  createdAt: "2026-07-04T00:00:00Z",
  updatedAt: "2026-07-04T00:00:00Z",
  messages: [],
  ...overrides,
});

describe("PageChatPanel", () => {
  beforeEach(() => {
    useChatStore.getState().reset();
    useTaskStore.getState().setTasks([]);
  });

  it("renders current page title and path", () => {
    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    expect(screen.getByText("ReAct Pattern")).toBeInTheDocument();
    expect(screen.getByText("wiki/concepts/react-pattern.md")).toBeInTheDocument();
  });

  it("can return from page chat to related pages", () => {
    const showRelated = vi.fn();
    render(
      <PageChatPanel
        page={page}
        projectId="project-1"
        rootPath="/wiki"
        onShowRelatedPages={showRelated}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Page info" }));

    expect(showRelated).toHaveBeenCalledTimes(1);
  });

  it("creates a page chat session when no active session exists", async () => {
    const createSession = vi.fn(async () => session({ id: "created-session" }));
    const send = vi.fn(async () => "task-1");
    useChatStore.setState({ createSession, send, activeSessionId: null, activeSession: null });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);
    fireEvent.change(screen.getByPlaceholderText("Ask about this page..."), {
      target: { value: "How does this work?" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(createSession).toHaveBeenCalledWith("project-1", "/wiki", "Ask: ReAct Pattern");
      expect(send).toHaveBeenCalled();
    });
  });

  it("sends with pinnedPagePath", async () => {
    const send = vi.fn(async () => "task-1");
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session(),
      send,
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);
    fireEvent.change(screen.getByPlaceholderText("Ask about this page..."), {
      target: { value: "Summarize it" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(send).toHaveBeenCalledWith(
        "project-1",
        "/wiki",
        "session-1",
        "Summarize it",
        "auto",
        { pinnedPagePath: "wiki/concepts/react-pattern.md" },
      );
    });
  });

  it("shows the pinned citation label", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({
        messages: [
          {
            id: "a1",
            role: "assistant",
            content: "Answer",
            createdAt: "2026-07-04T00:00:00Z",
            route: "byok",
            citations: [
              {
                pagePath: "wiki/concepts/react-pattern.md",
                title: "ReAct Pattern",
                score: 10_000,
                isPinned: true,
              },
            ],
          },
        ],
      }),
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    expect(screen.getAllByText("Current page").length).toBeGreaterThan(0);
  });

  it("does not mark an old pinned citation as the current page after navigation", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({
        messages: [
          {
            id: "a1",
            role: "assistant",
            content: "Answer",
            createdAt: "2026-07-04T00:00:00Z",
            route: "byok",
            citations: [
              {
                pagePath: "wiki/concepts/old-page.md",
                title: "Old Page",
                score: 10_000,
                isPinned: true,
              },
            ],
          },
        ],
      }),
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    const header = screen
      .getByText("wiki/concepts/react-pattern.md")
      .closest(".page-chat__head");
    expect(header).not.toHaveTextContent("Current page");
  });

  it("displays the chat store error string", () => {
    useChatStore.setState({ error: "No usable Agent CLI is configured." });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    expect(screen.getByText("No usable Agent CLI is configured.")).toBeInTheDocument();
  });
});

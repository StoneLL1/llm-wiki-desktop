import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useChatStore } from "../../stores/chatStore";
import { useTaskStore } from "../../stores/taskStore";
import { normalizeBackendError } from "../../lib/backendError";
import type { ChatSession } from "../../types/chat";
import type { BackendTask } from "../../types/task";
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
    const ensurePageSession = vi.fn(async (_projectId, _rootPath, _path, _title, forceNew) =>
      forceNew ? session({ id: "created-session", contextPagePath: page.meta.path }) : null,
    );
    const send = vi.fn(async () => "task-1");
    useChatStore.setState({
      send,
      activeSessionId: null,
      activeSession: null,
      ensurePageSession: ensurePageSession as never,
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);
    fireEvent.change(screen.getByPlaceholderText("Ask about this page..."), {
      target: { value: "How does this work?" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(ensurePageSession).toHaveBeenCalledWith(
        "project-1",
        "/wiki",
        "wiki/concepts/react-pattern.md",
        "ReAct Pattern",
        true,
      );
      expect(send).toHaveBeenCalled();
    });
  });

  it("creates a distinct page-scoped session when the wiki page changes", async () => {
    const ensurePageSession = vi.fn(async () => session({ id: "react-session" }));
    useChatStore.setState({ ensurePageSession: ensurePageSession as never });

    const { rerender } = render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    const nextPage: WikiPageContent = {
      ...page,
      meta: { ...page.meta, path: "wiki/concepts/planning.md", title: "Planning" },
    };
    rerender(<PageChatPanel page={nextPage} projectId="project-1" rootPath="/wiki" />);

    await waitFor(() => {
      expect(ensurePageSession).toHaveBeenCalledWith(
        "project-1",
        "/wiki",
        "wiki/concepts/planning.md",
        "Planning",
        false,
      );
    });
  });

  it("offers a new page chat action", async () => {
    const ensurePageSession = vi.fn(async () => session({ id: "fresh-session" }));
    useChatStore.setState({ ensurePageSession: ensurePageSession as never });
    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    fireEvent.click(screen.getByRole("button", { name: /new chat/i }));

    await waitFor(() => {
      expect(ensurePageSession).toHaveBeenCalledWith(
        "project-1",
        "/wiki",
        page.meta.path,
        page.meta.title,
        true,
      );
    });
  });

  it("keeps the draft when page chat session creation fails", async () => {
    useChatStore.setState({
      activeSessionId: null,
      activeSession: null,
      createSession: vi.fn(async () => null) as never,
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    const box = screen.getByPlaceholderText("Ask about this page...");
    fireEvent.change(box, { target: { value: "Do not clear me" } });
    fireEvent.click(screen.getByRole("button", { name: /send/i }));

    await waitFor(() => expect(box).toHaveValue("Do not clear me"));
  });

  it("sends with pinnedPagePath", async () => {
    const send = vi.fn(async () => "task-1");
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({ contextPagePath: page.meta.path }),
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

  it("does not show or send through a stale page-scoped session from another page", async () => {
    const ensurePageSession = vi.fn(async (_projectId, _rootPath, _path, _title, forceNew) =>
      forceNew ? session({ id: "created-for-current", contextPagePath: page.meta.path }) : null,
    );
    const send = vi.fn(async () => "task-1");
    useChatStore.setState({
      activeSessionId: "old-session",
      activeSession: session({
        id: "old-session",
        contextPagePath: "wiki/concepts/old-page.md",
        messages: [
          {
            id: "old-answer",
            role: "assistant",
            content: "Old page answer",
            createdAt: "2026-07-04T00:00:00Z",
            citations: [],
          },
        ],
      }),
      send,
      ensurePageSession: ensurePageSession as never,
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    expect(screen.queryByText("Old page answer")).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Ask about this page..."), {
      target: { value: "Use current page only" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(ensurePageSession).toHaveBeenCalledWith(
        "project-1",
        "/wiki",
        page.meta.path,
        page.meta.title,
        true,
      );
      expect(send).toHaveBeenCalledWith(
        "project-1",
        "/wiki",
        "created-for-current",
        "Use current page only",
        "auto",
        { pinnedPagePath: page.meta.path },
      );
    });
  });

  it("shows the pinned citation label", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({
        contextPagePath: page.meta.path,
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
        contextPagePath: page.meta.path,
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

  it("presents a contextless chat error without a fake retry action", () => {
    useChatStore.setState({
      error: normalizeBackendError("No usable Agent CLI is configured.", {
        defaultSummaryKey: "backendError.summary.chat",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }),
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    expect(screen.getByRole("alert")).toHaveTextContent("The message could not be sent or saved.");
    expect(screen.getByText("No usable Agent CLI is configured.")).not.toBeVisible();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("keeps a terminal page-chat task until a failed reload is retried successfully", async () => {
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
      activeSessionId: "session-1",
      activeSession: session({ contextPagePath: page.meta.path }),
      sendTaskId: "task-retry",
      sendSessionId: "session-1",
      streamingText: "temporary answer",
      reloadActive: reloadActive as never,
      ensurePageSession: vi.fn(async () => session({ contextPagePath: page.meta.path })) as never,
    });
    useTaskStore.getState().setTasks([{
      id: "task-retry",
      taskType: "llm_request",
      projectId: "project-1",
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
    } satisfies BackendTask]);

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(
      "The message could not be sent or saved.",
    ));
    expect(useChatStore.getState().sendTaskId).toBe("task-retry");
    expect(useChatStore.getState().streamingText).toBe("temporary answer");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(useChatStore.getState().sendTaskId).toBeNull());
    expect(useChatStore.getState().error).toBeNull();
    expect(reloadActive).toHaveBeenCalledTimes(2);
  });

  it("shares the lightweight active-stream rendering contract", () => {
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({ contextPagePath: page.meta.path }),
      sendTaskId: "task-stream",
      sendSessionId: "session-1",
      streamingText: "**plain while streaming**\n\n```ts\nconst x = 1;\n```",
      ensurePageSession: vi.fn(async () => session({ contextPagePath: page.meta.path })) as never,
    });
    useTaskStore.getState().setTasks([{
      id: "task-stream",
      taskType: "llm_request",
      projectId: "project-1",
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
    }]);

    const { container } = render(
      <PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />,
    );

    expect(screen.getByText(/\*\*plain while streaming\*\*/)).toBeInTheDocument();
    expect(container.querySelector("pre")).toBeNull();
    expect(container.querySelector(".katex")).toBeNull();
    expect(container.querySelector(".hljs")).toBeNull();
  });

  it("opens cited wiki pages from page chat messages", () => {
    const openCitation = vi.fn();
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({
        contextPagePath: page.meta.path,
        messages: [
          {
            id: "a1",
            role: "assistant",
            content: "See [1]",
            createdAt: "2026-07-04T00:00:00Z",
            citations: [{ pagePath: "wiki/a.md", title: "A", score: 1 }],
          },
        ],
      }),
    });
    render(
      <PageChatPanel
        page={page}
        projectId="project-1"
        rootPath="/wiki"
        onOpenCitation={openCitation}
      />,
    );

    // Match on the citation path (unambiguous: only the citation card carries it).
    fireEvent.click(screen.getByRole("button", { name: /wiki\/a\.md/i }));

    expect(openCitation).toHaveBeenCalledWith("wiki/a.md");
  });

  it("saves an assistant answer through the chat store", () => {
    const saveAnswer = vi.fn(async () => null);
    useChatStore.setState({
      activeSessionId: "session-1",
      activeSession: session({
        contextPagePath: page.meta.path,
        messages: [
          {
            id: "a1",
            role: "assistant",
            content: "Answer",
            createdAt: "2026-07-04T00:00:00Z",
            citations: [],
          },
        ],
      }),
      saveAnswer: saveAnswer as never,
    });

    render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

    fireEvent.click(screen.getByRole("button", { name: /save to wiki/i }));

    expect(saveAnswer).toHaveBeenCalledWith("project-1", "/wiki", "session-1", "a1");
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import type { ChatSessionSummary } from "../../types/chat";
import { ChatSessionList } from "./ChatSessionList";

const session = (overrides: Partial<ChatSessionSummary> = {}): ChatSessionSummary => ({
  id: "s1",
  title: "Agent planning",
  createdAt: "2026-07-08T10:00:00Z",
  updatedAt: "2026-07-08T10:05:00Z",
  messageCount: 2,
  ...overrides,
});

describe("ChatSessionList", () => {
  it("keeps stale session rows visible during background revalidation", () => {
    render(
      <ChatSessionList
        sessions={[session()]}
        activeSessionId="s1"
        loading
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText("Agent planning")).toBeInTheDocument();
    expect(screen.queryByText("Loading chats…")).not.toBeInTheDocument();
  });

  it("shows an empty filter state when no session title matches the search", () => {
    render(
      <ChatSessionList
        sessions={[session()]}
        activeSessionId={null}
        loading={false}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByRole("textbox", { name: "Search chats" }), {
      target: { value: "missing" },
    });

    expect(screen.getByText("No matching chats.")).toBeInTheDocument();
    expect(screen.queryByText("Agent planning")).not.toBeInTheDocument();
  });
});

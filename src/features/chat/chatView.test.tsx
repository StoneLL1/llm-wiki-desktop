import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "../../i18n";
import { useChatStore } from "../../stores/chatStore";
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
});

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { AgentActivityTimeline } from "./AgentActivityTimeline";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        "agent.activity.thinkingDone": "Thought through",
        "agent.activity.thinkingActive": "Thinking",
        "agent.activity.toolDone": "Tool completed",
        "agent.activity.toolFailed": "Tool failed",
        "agent.activity.done": "Done",
        "agent.activity.active": "Active",
        "agent.activity.phase.sourceAiProvider": "AI provider response",
      })[key] ?? key,
  }),
}));

describe("AgentActivityTimeline", () => {
  it("groups safe thinking and tool lifecycle events without rendering raw output", () => {
    render(
      <AgentActivityTimeline
        activities={[
          { kind: "thinking", status: "started", summary: "hidden" },
          { kind: "thinking", status: "completed", summary: "safe summary" },
          { kind: "tool_call", callId: "read-1", name: "Read", detail: "wiki/page.md" },
          { kind: "tool_result", callId: "read-1", success: true, summary: "Tool completed" },
        ]}
      />,
    );

    expect(screen.getByText("Thought through")).toBeInTheDocument();
    expect(screen.getByText("Read")).toBeInTheDocument();
    expect(screen.getByText("wiki/page.md")).toBeInTheDocument();
    expect(screen.queryByText("hidden")).not.toBeInTheDocument();
  });

  it("pairs Source AI phases without hiding activities emitted between them", () => {
    render(
      <AgentActivityTimeline
        taskStatus="succeeded"
        activities={[
          {
            kind: "phase",
            name: "source-ai-provider",
            status: "started",
          },
          {
            kind: "tool_call",
            callId: "read-1",
            name: "Read",
            detail: "input.json",
          },
          {
            kind: "tool_result",
            callId: "read-1",
            success: true,
          },
          {
            kind: "phase",
            name: "source-ai-provider",
            status: "completed",
          },
        ]}
      />,
    );

    expect(screen.getAllByText("AI provider response")).toHaveLength(1);
    expect(screen.getByText("Read")).toBeInTheDocument();
    expect(screen.getAllByText("Done")).toHaveLength(1);
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import { ChatConveniencePanel } from "./ChatConveniencePanel";

describe("ChatConveniencePanel", () => {
  it("toggles convenience mode", () => {
    const onSetEnabled = vi.fn();
    render(<ChatConveniencePanel enabled={false} pending={false} onSetEnabled={onSetEnabled} />);

    fireEvent.click(screen.getByRole("button", { name: /convenience/i }));

    expect(onSetEnabled).toHaveBeenCalledWith(true);
  });

  it("renders soft violation actions", () => {
    const onKeep = vi.fn();
    const onRollback = vi.fn();
    render(
      <ChatConveniencePanel
        enabled
        pending
        onSetEnabled={() => {}}
        onKeep={onKeep}
        onRollback={onRollback}
        edit={{
          status: "soft_violation_pending",
          checkpointHash: "abc123",
          affectedPaths: ["wiki/a.md"],
          diffSummary: "1 file change",
          diffText: "+hello",
          violationReason: "too many files",
        }}
      />,
    );

    expect(screen.getByText("too many files")).toBeInTheDocument();
    expect(screen.getByText("wiki/a.md")).toBeInTheDocument();
    expect(screen.getByText("+hello")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /keep/i }));
    fireEvent.click(screen.getByRole("button", { name: /rollback/i }));

    expect(onKeep).toHaveBeenCalledTimes(1);
    expect(onRollback).toHaveBeenCalledTimes(1);
  });
});

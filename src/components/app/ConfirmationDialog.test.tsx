import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmationDialog } from "./ConfirmationDialog";
import type { PendingAction } from "../../types/backend";

const pendingAction: PendingAction = {
  id: "action-1",
  actionType: "overwrite_file",
  title: "Overwrite wiki page",
  message: "The generated page would replace an existing Markdown file.",
  riskLevel: "destructive",
  affectedPaths: ["wiki/concepts/agent.md", "raw/sources/report.pdf"],
  preview: {
    summary: "Two paths are affected.",
    before: "old",
    after: "new",
    diff: "- old\n+ new",
  },
  expiresAt: null,
};

describe("ConfirmationDialog", () => {
  it("moves focus into the modal and lets Escape cancel", () => {
    const onCancel = vi.fn();
    render(
      <ConfirmationDialog
        action={pendingAction}
        checkpointExists={false}
        onCancel={onCancel}
        onConfirm={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("shows risk, checkpoint state, affected paths, and keeps cancel available", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();

    render(
      <ConfirmationDialog
        action={pendingAction}
        checkpointExists={true}
        onCancel={onCancel}
        onConfirm={onConfirm}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Overwrite wiki page" })).toBeInTheDocument();
    expect(screen.getByText("Risk: destructive")).toBeInTheDocument();
    expect(screen.getByText("Checkpoint: available")).toBeInTheDocument();
    expect(screen.getByText("wiki/concepts/agent.md")).toBeInTheDocument();
    expect(screen.getByText("raw/sources/report.pdf")).toBeInTheDocument();
    expect(screen.getByText("- old")).toBeInTheDocument();
    expect(screen.getByText("+ new")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Confirm overwrite" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});

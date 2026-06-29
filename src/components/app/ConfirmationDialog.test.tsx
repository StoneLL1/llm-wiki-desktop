import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmationDialog } from "./ConfirmationDialog";
import type { PendingAction } from "../../types/backend";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) => {
      const labels: Record<string, string> = {
        "confirmation.risk": `Risk: ${values?.risk ?? ""}`,
        "confirmation.checkpoint.available": "Checkpoint: available",
        "confirmation.checkpoint.missing": "Checkpoint: not created yet",
        "confirmation.preview": "Preview",
        "confirmation.affectedPaths": "Affected paths",
        "confirmation.cancel": "Cancel",
        "confirmation.confirm.overwrite_file": "Confirm overwrite",
      };
      return labels[key] ?? key;
    },
  }),
}));

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

  it("renders the confirm button with the danger variant for destructive actions", () => {
    render(
      <ConfirmationDialog
        action={{ ...pendingAction, riskLevel: "destructive" }}
        checkpointExists={false}
        onCancel={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    const confirmButton = screen.getByRole("button", { name: "Confirm overwrite" });
    expect(confirmButton).toHaveClass("bg-[var(--danger)]");
    expect(confirmButton).toHaveClass("text-white");
  });

  it("renders the confirm button with the secondary variant for non-destructive actions", () => {
    render(
      <ConfirmationDialog
        action={{ ...pendingAction, riskLevel: "medium" }}
        checkpointExists={false}
        onCancel={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    const confirmButton = screen.getByRole("button", { name: "Confirm overwrite" });
    expect(confirmButton).not.toHaveClass("bg-[var(--danger)]");
    expect(confirmButton).toHaveClass("bg-[var(--surface-raised)]");
  });
});

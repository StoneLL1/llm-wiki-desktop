import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => ({
  "compileConflict.keepCurrent": "Keep current",
  "compileConflict.useGenerated": "Use generated",
  "compileConflict.manualMerge": "Manual merge",
  "compileConflict.applyManual": "Apply manual merge",
  "confirmation.cancel": "Cancel",
  "compileConflict.current": "Current",
  "compileConflict.generated": "Generated",
}[key] ?? key) }) }));

import { CompileConflictDialog } from "./CompileConflictDialog";
import type { PendingAction } from "../../types/backend";

const action: PendingAction = {
  id: "action-1",
  actionType: "merge_conflict",
  title: "Resolve conflicts",
  message: "Choose a resolution.",
  riskLevel: "high",
  affectedPaths: ["wiki/index.md"],
  preview: null,
  expiresAt: null,
};

describe("CompileConflictDialog", () => {
  it("offers keep, generated, and editable manual merge paths", async () => {
    invokeMock
      .mockResolvedValueOnce([
        { path: "wiki/index.md", currentContent: "# Current", generatedContent: "# Generated" },
      ])
      .mockResolvedValueOnce({ id: "task-1", status: "succeeded" });
    render(<CompileConflictDialog action={action} onCancel={vi.fn()} onResolved={vi.fn()} />);

    expect(await screen.findByText("# Current")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep current" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use generated" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Manual merge" }));
    const editor = screen.getByRole("textbox", { name: "wiki/index.md" });
    expect(editor).toHaveValue("# Generated");
    fireEvent.change(editor, { target: { value: "# Merged" } });
    expect(screen.getByRole("button", { name: "Apply manual merge" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Apply manual merge" }));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("resolve_compile_conflict", {
        request: {
          actionId: "action-1",
          resolution: "manual_merge",
          manualFiles: [{ path: "wiki/index.md", content: "# Merged" }],
        },
      }),
    );
  });
});

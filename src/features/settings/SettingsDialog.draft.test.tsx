import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { defaultProject } from "../../stores/projectStore";
import { getSettingsDraftDirty } from "../../lib/settingsDraftGuard";

vi.mock("./SettingsView", () => ({
  SettingsView: ({ onDirtyChange }: { onDirtyChange?: (dirty: boolean) => void }) => <>
    <input aria-label="Connection model" defaultValue="saved-model" onChange={() => onDirtyChange?.(true)} />
    <button type="button" onClick={() => onDirtyChange?.(false)}>Save connection</button>
  </>,
}));

import { SettingsDialog } from "./SettingsDialog";

function openDialog(onClose = vi.fn()) {
  return { onClose, ...render(<SettingsDialog open onClose={onClose} project={{ ...defaultProject, projectId: "p1", rootPath: "/p1" }} providers={[]} agents={[]} onRefreshCapabilities={vi.fn()} onSaveProvider={vi.fn()} onSaveSecret={vi.fn()} onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />) };
}

async function editModel() {
  const input = await screen.findByRole("textbox", { name: "Connection model" });
  fireEvent.change(input, { target: { value: "unsaved-model" } });
  return input;
}

describe("SettingsDialog unsaved connection protection", () => {
  it("closes immediately when no connection draft has changed", async () => {
    const { onClose } = openDialog();
    await screen.findByRole("textbox", { name: "Connection model" });
    fireEvent.click(screen.getByRole("button", { name: "Close settings" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it.each(["close button", "backdrop", "Escape"] as const)("requires a concrete discard choice for a dirty draft closed using %s", async (method) => {
    const { onClose } = openDialog();
    await editModel();
    expect(getSettingsDraftDirty()).toBe(true);
    if (method === "close button") fireEvent.click(screen.getByRole("button", { name: "Close settings" }));
    if (method === "backdrop") fireEvent.click(screen.getByRole("dialog", { name: "Settings" }));
    if (method === "Escape") fireEvent.keyDown(document, { key: "Escape" });
    const confirmation = screen.getByRole("alertdialog", { name: "Discard unsaved connection changes?" });
    expect(confirmation).toHaveTextContent("Unsaved models, addresses, and credentials will be discarded.");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Keep editing" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Discard and close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(getSettingsDraftDirty()).toBe(false);
  });

  it("keeps the draft when the nested confirmation is dismissed with Escape", async () => {
    const { onClose, unmount } = openDialog();
    const input = await editModel();
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    expect(input).toHaveValue("unsaved-model");
    expect(onClose).not.toHaveBeenCalled();
    expect(getSettingsDraftDirty()).toBe(true);
    unmount();
    expect(getSettingsDraftDirty()).toBe(false);
  });

  it("allows closing once the connection reports a successful save", async () => {
    const { onClose } = openDialog();
    await editModel();
    fireEvent.click(screen.getByRole("button", { name: "Save connection" }));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});

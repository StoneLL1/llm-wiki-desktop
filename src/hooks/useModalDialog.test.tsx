import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { useState } from "react";

import { GenerateHtmlDialog } from "../features/wiki/GenerateHtmlDialog";
import "../i18n";

function ModalHarness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>Open export dialog</button>
      <input aria-label="Background search" />
      {open ? (
        <GenerateHtmlDialog
          pagePath="wiki/agent-memory.md"
          onCancel={() => setOpen(false)}
          onGenerate={() => setOpen(false)}
        />
      ) : null}
    </>
  );
}

afterEach(cleanup);

describe("modal keyboard behavior", () => {
  it("moves focus inside, closes on Escape, and restores the trigger", () => {
    render(<ModalHarness />);
    const trigger = screen.getByRole("button", { name: "Open export dialog" });

    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    screen.getByRole("textbox", { name: "Background search" }).focus();
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    const dialogButtons = within(dialog).getAllByRole("button");
    dialogButtons.at(-1)?.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(dialogButtons[0]);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});

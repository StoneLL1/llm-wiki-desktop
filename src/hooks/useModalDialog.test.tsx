import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";

import { GenerateHtmlDialog } from "../features/wiki/GenerateHtmlDialog";
import { useModalDialog } from "./useModalDialog";
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

function OverflowModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const ref = useModalDialog<HTMLDivElement>({ open, onClose });
  if (!open) return null;
  return (
    <div ref={ref} role="dialog" aria-modal="true" tabIndex={-1}>
      <button type="button">close</button>
    </div>
  );
}

describe("useModalDialog body scroll lock", () => {
  afterEach(() => {
    document.body.style.overflow = "";
  });

  it("locks body scroll while open and restores the prior value on close", () => {
    document.body.style.overflow = "";

    const { unmount } = render(<OverflowModal open={true} onClose={() => {}} />);
    expect(document.body.style.overflow).toBe("hidden");

    unmount();
    expect(document.body.style.overflow).toBe("");
  });

  it("keeps the body locked while a nested modal closes first", () => {
    document.body.style.overflow = "";

    const { rerender } = render(
      <>
        <OverflowModal open={true} onClose={() => {}} />
        <OverflowModal open={true} onClose={() => {}} />
      </>,
    );
    expect(document.body.style.overflow).toBe("hidden");

    rerender(
      <>
        <OverflowModal open={true} onClose={() => {}} />
        <OverflowModal open={false} onClose={() => {}} />
      </>,
    );
    expect(document.body.style.overflow).toBe("hidden");
  });
});

describe("useModalDialog Escape cascade", () => {
  afterEach(() => {
    document.body.style.overflow = "";
  });

  it("closes only the topmost modal so a nested dialog does not cascade-close the one beneath", () => {
    const outerClose = vi.fn();
    const innerClose = vi.fn();

    render(
      <>
        <OverflowModal open={true} onClose={outerClose} />
        <OverflowModal open={true} onClose={innerClose} />
      </>,
    );

    fireEvent.keyDown(document, { key: "Escape" });

    expect(innerClose).toHaveBeenCalledTimes(1);
    expect(outerClose).not.toHaveBeenCalled();
  });

  it("lets the topmost nested modal own the focus trap", () => {
    render(
      <>
        <OverflowModal open={true} onClose={() => {}} />
        <OverflowModal open={true} onClose={() => {}} />
      </>,
    );

    const [outerButton, innerButton] = screen.getAllByRole("button", { name: "close" });
    outerButton.focus();

    expect(innerButton).toHaveFocus();
  });
});

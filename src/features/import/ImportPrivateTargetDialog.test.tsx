import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { ImportPrivateTargetDialog } from "./ImportPrivateTargetDialog";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportPrivateTargetDialog", () => {
  it("shows the exact one-item target scope and defaults to cancel", () => {
    const onCancel = vi.fn();
    render(
      <ImportPrivateTargetDialog
        open
        itemId="item-1"
        target="https://private.example.com/article"
        addressCategory="private_network"
        reason="Redirect resolved to a private address"
        onAuthorize={vi.fn()}
        onCancel={onCancel}
      />,
    );

    expect(screen.getByText("https://private.example.com/article")).toBeInTheDocument();
    expect(screen.getByText(/private_network/i)).toBeInTheDocument();
    expect(screen.getByText(/one item.*one target/i)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /cancel/i }).length).toBeGreaterThan(0);
  });

  it("authorizes only the displayed target and does not offer an editable redirect", async () => {
    const onAuthorize = vi.fn().mockResolvedValue("grant-opaque");
    render(
      <ImportPrivateTargetDialog
        open
        itemId="item-1"
        target="https://private.example.com/article"
        addressCategory="private_network"
        reason="Redirect resolved to a private address"
        onAuthorize={onAuthorize}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /authorize this target/i }));
    await waitFor(() => expect(onAuthorize).toHaveBeenCalledWith("item-1", "https://private.example.com/article"));
  });
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportPreviewContent } from "../../types/importV2Presentation";
import { ImportMarkdownPreviewDialog, type ImportPreviewIdentity } from "./ImportMarkdownPreviewDialog";

function content(identity: ImportPreviewIdentity, markdown: string, truncated = false): ImportPreviewContent {
  return {
    ...identity,
    title: identity.itemId,
    markdown,
    truncated,
    totalBytes: markdown.length + (truncated ? 100 : 0),
    sha256: `hash-${identity.itemId}`,
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportMarkdownPreviewDialog", () => {
  it("loads by session/item/candidate identity, renders the bounded preview, and exposes hash/copy metadata", async () => {
    const identity = { sessionId: "session-a", itemId: "item-a", candidateId: null } as const;
    const loadContent = vi.fn().mockResolvedValue(content(identity, "# Safe\n\n- one", true));
    const onClose = vi.fn();
    const onCopyMarkdown = vi.fn().mockResolvedValue(undefined);

    render(
      <ImportMarkdownPreviewDialog
        open
        identity={identity}
        loadContent={loadContent}
        onClose={onClose}
        onCopyMarkdown={onCopyMarkdown}
      />,
    );

    await waitFor(() => expect(loadContent).toHaveBeenCalledWith(identity));
    expect(await screen.findByRole("heading", { name: "item-a" })).toBeInTheDocument();
    expect(screen.getByText(/preview is truncated/i)).toBeInTheDocument();
    expect(screen.getByText(/hash-item-a/)).toBeInTheDocument();
    expect(screen.getByText(/bytes/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy markdown/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /copy markdown/i }));
    expect(onCopyMarkdown).toHaveBeenCalledWith("# Safe\n\n- one");
  });

  it("rejects unsafe link and image targets in the rendered preview", async () => {
    const identity = { sessionId: "session-a", itemId: "item-a", candidateId: "candidate-a" } as const;
    const loadContent = vi.fn().mockResolvedValue(
      content(identity, "[unsafe](javascript:alert(1))\n\n![remote](https://evil.example/x.png)"),
    );

    render(<ImportMarkdownPreviewDialog open identity={identity} loadContent={loadContent} onClose={vi.fn()} />);

    expect(await screen.findByText("unsafe")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "unsafe" })).not.toBeInTheDocument();
    expect(screen.getByText(/image omitted/i)).toBeInTheDocument();
  });

  it("keeps the newest item when an earlier preview resolves late and exposes fetch errors", async () => {
    const identityA = { sessionId: "session-a", itemId: "item-a", candidateId: null } as const;
    const identityB = { sessionId: "session-a", itemId: "item-b", candidateId: null } as const;
    let resolveA!: (value: ImportPreviewContent) => void;
    const loadContent = vi.fn((identity: ImportPreviewIdentity) => {
      if (identity.itemId === "item-a") {
        return new Promise<ImportPreviewContent>((resolve) => {
          resolveA = resolve;
        });
      }
      return Promise.reject(new Error("preview unavailable"));
    });

    const view = render(
      <ImportMarkdownPreviewDialog open identity={identityA} loadContent={loadContent} onClose={vi.fn()} />,
    );
    view.rerender(
      <ImportMarkdownPreviewDialog open identity={identityB} loadContent={loadContent} onClose={vi.fn()} />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(/preview unavailable/i);
    resolveA(content(identityA, "# stale"));
    await waitFor(() => expect(screen.queryByText("stale")).not.toBeInTheDocument());
    expect(screen.getAllByText(/item-b/i).length).toBeGreaterThan(0);
  });

  it("closes on Escape and restores focus to the trigger", async () => {
    const identity = { sessionId: "session-a", itemId: "item-a", candidateId: null } as const;
    const trigger = document.createElement("button");
    trigger.textContent = "Preview trigger";
    document.body.appendChild(trigger);
    trigger.focus();
    const onClose = vi.fn();

    const view = render(
      <ImportMarkdownPreviewDialog
        open
        identity={identity}
        loadContent={vi.fn().mockResolvedValue(content(identity, "# Safe"))}
        onClose={onClose}
      />,
    );
    fireEvent.keyDown(document, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
    view.rerender(
      <ImportMarkdownPreviewDialog
        open={false}
        identity={identity}
        loadContent={vi.fn().mockResolvedValue(content(identity, "# Safe"))}
        onClose={onClose}
      />,
    );
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    trigger.remove();
  });
});

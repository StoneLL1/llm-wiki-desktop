import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportPreviewContent } from "../../types/importV2Presentation";
import { ImportMarkdownPreviewDialog, type ImportPreviewIdentity } from "./ImportMarkdownPreviewDialog";

function content(identity: ImportPreviewIdentity, markdown: string, truncated = false): ImportPreviewContent {
  return {
    ...identity,
    title: "Readable preview",
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
  it("renders readable content while keeping internal identity and hash collapsed", async () => {
    const identity = { sessionId: "session-a", itemId: "item-a", candidateId: null } as const;
    const loadContent = vi.fn().mockResolvedValue({
      ...content(identity, "# Safe\n\n- one", true),
      target: {
        disposition: "update",
        sourceId: "source-internal-a",
        versionId: "version-internal-b",
        wikiPath: "wiki/sources/local/研究笔记.md",
      },
      comparison: {
        currentMarkdown: "# Existing Source\n\nUser-maintained note.",
        mergedMarkdown: "# Merged result\n\nCombined note.",
      },
    });
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
    expect(await screen.findByRole("heading", { name: "Readable preview" })).toBeInTheDocument();
    expect(screen.getByText(/preview is truncated/i)).toBeInTheDocument();
    expect(screen.getByText(/bytes/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy markdown/i })).toBeInTheDocument();
    const technical = screen.getByText("Technical details").closest("details");
    expect(technical).not.toHaveAttribute("open");
    expect(technical).toHaveTextContent("hash-item-a");
    expect(technical).toHaveTextContent("session-a");
    expect(screen.getByText("A new version will be created when you commit")).toBeVisible();
    expect(screen.getByText("wiki/sources/local/研究笔记.md")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Update comparison" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Existing Source" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Imported update" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Merged result" })).toBeVisible();
    expect(screen.getByText(/User-maintained note/)).toBeVisible();
    expect(screen.getByText(/Combined note/)).toBeVisible();
    expect(screen.getByText("source-internal-a")).not.toBeVisible();
    expect(screen.getByText("version-internal-b")).not.toBeVisible();

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
    expect(screen.getByText(/image preview unavailable/i)).toBeInTheDocument();
  });

  it("localizes normalized preview failures, keeps diagnostics, and retries", async () => {
    const identity = { sessionId: "session-a", itemId: "item-a", candidateId: "candidate-a" } as const;
    const loadContent = vi.fn()
      .mockRejectedValueOnce({
        code: "IMPORT_V2_PREVIEW_FAILED",
        summaryKey: "backendError.summary.import",
        technicalDetails: "preview parser failed",
        recoverable: true,
        userActionRequired: false,
        actionKind: "retry",
      })
      .mockResolvedValueOnce(content(identity, "# Recovered"));

    render(<ImportMarkdownPreviewDialog open identity={identity} loadContent={loadContent} onClose={vi.fn()} />);

    expect(await screen.findByText(/import could not continue/i)).toBeVisible();
    expect(screen.queryByText("[object Object]")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Technical details"));
    expect(screen.getByText(/preview parser failed/i)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /retry/i }));
    expect(await screen.findByRole("heading", { name: "Recovered" })).toBeVisible();
    expect(loadContent).toHaveBeenCalledTimes(2);
  });

  it("renders verified local image resources and never renders a remote URL", async () => {
    const identity = { sessionId: "session-a", itemId: "item-a", candidateId: null } as const;
    const preview = {
      ...content(identity, "![figure](assets/figure.png)\n\n![remote](https://evil.example/x.png)"),
      resources: [{
        source: "assets/figure.png",
        name: "figure.png",
        kind: "image" as const,
        sizeBytes: 4,
        dataUrl: "data:image/png;base64,iVBORw==",
      }],
    };

    render(
      <ImportMarkdownPreviewDialog
        open
        identity={identity}
        loadContent={vi.fn().mockResolvedValue(preview)}
        onClose={vi.fn()}
      />,
    );

    expect(await screen.findByRole("img", { name: "figure" })).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw==",
    );
    expect(screen.queryByRole("img", { name: "remote" })).not.toBeInTheDocument();
    expect(screen.getByText(/image preview unavailable: remote/i)).toBeInTheDocument();
  });

  it("changes workbench copy without translating or replacing Source content", async () => {
    const identity = { sessionId: "session-language", itemId: "item-language", candidateId: null } as const;
    const originalMarkdown = "# 原始标题\n\n未经翻译的中文正文。";
    const loadContent = vi.fn().mockResolvedValue(content(identity, originalMarkdown));

    render(
      <ImportMarkdownPreviewDialog
        open
        identity={identity}
        loadContent={loadContent}
        onClose={vi.fn()}
      />,
    );

    expect(await screen.findByRole("heading", { name: "原始标题" })).toBeInTheDocument();
    expect(screen.getByText("未经翻译的中文正文。")).toBeInTheDocument();

    await act(async () => {
      await i18next.changeLanguage("zh-CN");
    });

    expect(screen.getByRole("heading", { name: "原始标题" })).toBeInTheDocument();
    expect(screen.getByText("未经翻译的中文正文。")).toBeInTheDocument();
    expect(loadContent).toHaveBeenCalledTimes(1);
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

    expect(await screen.findByRole("alert")).toHaveTextContent(/import could not continue/i);
    expect(screen.getByText("Technical details").closest("details")).toHaveTextContent("preview unavailable");
    resolveA(content(identityA, "# stale"));
    await waitFor(() => expect(screen.queryByText("stale")).not.toBeInTheDocument());
    expect(screen.getByRole("heading", { name: "Markdown preview" })).toBeInTheDocument();
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

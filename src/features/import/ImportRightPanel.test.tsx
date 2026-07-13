import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem } from "../../types/importV2";
import { ImportRightPanel } from "./ImportRightPanel";

function item(overrides: Partial<ImportItem> = {}): ImportItem {
  return {
    itemId: "item-a",
    input: {
      kind: "file",
      displayName: "研究笔记.md",
      locator: "D:\\资料\\研究笔记.md",
      normalizedLocator: null,
    },
    status: "preview_ready",
    selected: true,
    taskId: "task-a",
    progress: null,
    attempts: [
      {
        route: "native_markdown",
        engineId: "core-markdown",
        engineVersion: "2.0.0",
        stage: "extract",
        startedAt: "2026-07-13T09:00:00Z",
        completedAt: "2026-07-13T09:00:01Z",
        outcome: "succeeded",
        warnings: [],
      },
    ],
    preview: {
      title: "研究笔记",
      markdown: {
        kind: "markdown",
        relativePath: "raw/extracted/研究笔记.md",
        sha256: "markdown-hash",
        sizeBytes: 120,
      },
      assets: [
        {
          kind: "image",
          relativePath: "raw/assets/figure.png",
          sha256: "asset-hash",
          sizeBytes: 2048,
        },
      ],
      sourceSnapshot: {
        kind: "source_snapshot",
        relativePath: "raw/sources/研究笔记.md",
        sha256: "source-hash",
        sizeBytes: 120,
      },
      quality: {
        level: "warning",
        metrics: [
          { code: "TEXT_COVERAGE", actual: 0.91, minimum: 0.98, passed: false },
          { code: "TABLE_CELL_ACCURACY", actual: 1, minimum: 0.95, passed: true },
        ],
        warnings: ["LOW_TEXT_COVERAGE"],
      },
    },
    issue: null,
    ...overrides,
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportRightPanel", () => {
  it("shows an explicit empty inspector when nothing is selected", () => {
    render(<ImportRightPanel selectedItem={null} onPreviewMarkdown={vi.fn()} />);

    expect(screen.getByText(/select an item/i)).toBeInTheDocument();
  });

  it("inspects local source identity, quality, attempts, provenance, and assets", () => {
    const onPreviewMarkdown = vi.fn();
    render(<ImportRightPanel selectedItem={item()} onPreviewMarkdown={onPreviewMarkdown} />);

    expect(screen.getByText("研究笔记.md")).toBeInTheDocument();
    expect(screen.getAllByText(/source/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText("core-markdown 2.0.0").length).toBeGreaterThan(0);
    expect(screen.getByText("TEXT_COVERAGE")).toBeInTheDocument();
    expect(screen.getByText("LOW_TEXT_COVERAGE")).toBeInTheDocument();
    expect(screen.getAllByText("native_markdown").length).toBeGreaterThan(0);
    expect(screen.getByText("raw/extracted/研究笔记.md")).toHaveAttribute("title", "raw/extracted/研究笔记.md");
    expect(screen.getByText(/1 asset/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /preview markdown/i }));
    expect(onPreviewMarkdown).toHaveBeenCalledWith("item-a");
  });

  it("distinguishes generic URLs and platform video routes without hardcoded archive rules", () => {
    render(
      <ImportRightPanel
        selectedItem={item({
          input: {
            kind: "url",
            displayName: "Example video",
            locator: "https://www.bilibili.com/video/BV1xx",
            normalizedLocator: "https://www.bilibili.com/video/BV1xx",
          },
          preview: {
            ...item().preview!,
            title: "Example video",
          },
        })}
        onPreviewMarkdown={vi.fn()}
      />,
    );

    expect(screen.getByText("www.bilibili.com")).toBeInTheDocument();
    expect(screen.getAllByText(/generic_http|bilibili/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/archive rules/i)).not.toBeInTheDocument();
  });

  it("shows fail state and preserves all attempt history", () => {
    render(
      <ImportRightPanel
        selectedItem={item({
          status: "failed",
          attempts: [
            ...item().attempts,
            {
              route: "browser_runtime",
              engineId: "browser-lite",
              engineVersion: "1.4.0",
              stage: "validate",
              startedAt: "2026-07-13T09:01:00Z",
              completedAt: "2026-07-13T09:01:01Z",
              outcome: "failed",
              warnings: ["CHALLENGE"],
            },
          ],
          issue: {
            code: "LOGIN_REQUIRED",
            message: "Login is required.",
            stage: "extract",
            retryable: true,
            userActionRequired: true,
            recoveryActions: ["begin_login"],
            availableActions: [],
          },
        })}
        onPreviewMarkdown={vi.fn()}
      />,
    );

    expect(screen.getByText(/login is required/i)).toBeInTheDocument();
    expect(screen.getAllByText("browser_runtime").length).toBeGreaterThan(0);
    expect(screen.getByText("CHALLENGE")).toBeInTheDocument();
    expect(screen.getAllByText(/failed/i).length).toBeGreaterThan(0);
  });
});

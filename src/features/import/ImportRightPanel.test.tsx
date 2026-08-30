import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { importV2Api } from "../../services/importV2Api";
import type { ImportItem } from "../../types/importV2";
import type { ImportPreviewContent } from "../../types/importV2Presentation";
import { ImportRightPanel } from "./ImportRightPanel";

vi.mock("../../services/importV2Api", () => ({
  importV2Api: {
    getPreviewContent: vi.fn(),
  },
}));

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
      assets: [],
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
        ],
        warnings: ["LOW_TEXT_COVERAGE"],
      },
    },
    issue: null,
    ...overrides,
  };
}

function preview(): ImportPreviewContent {
  return {
    sessionId: "session-a",
    itemId: "item-a",
    candidateId: null,
    title: "研究笔记",
    markdown: "# 最终候选\n\n可读正文",
    truncated: false,
    totalBytes: 22,
    sha256: "markdown-hash",
    target: {
      disposition: "update",
      sourceId: "source-a",
      versionId: "version-b",
      wikiPath: "wiki/sources/local/研究笔记.md",
    },
    quality: {
      level: "warning",
      metrics: [],
      warnings: ["LOW_TEXT_COVERAGE"],
    },
    rawLabel: "研究笔记.md",
    resources: [],
  };
}

function renderPanel(
  selectedItem: ImportItem,
  onPreviewMarkdown = vi.fn(),
  onPrimaryAction = vi.fn(),
) {
  return render(
    <ImportRightPanel
      selectedItem={selectedItem}
      sessionId="session-a"
      projectId="project-a"
      projectRootPath="D:\\Wiki"
      onPreviewMarkdown={onPreviewMarkdown}
      onPrimaryAction={onPrimaryAction}
    />,
  );
}

beforeEach(async () => {
  vi.mocked(importV2Api.getPreviewContent).mockReset();
  vi.mocked(importV2Api.getPreviewContent).mockResolvedValue(preview());
  await i18next.changeLanguage("en");
});

describe("ImportRightPanel", () => {
  it("shows an explicit empty inspector when nothing is selected", () => {
    render(<ImportRightPanel selectedItem={null} onPreviewMarkdown={vi.fn()} />);

    expect(screen.getByText(/select an item/i)).toBeInTheDocument();
  });

  it("orders the next step, quick preview, target, quality, raw source, and collapsed diagnostics", async () => {
    const onPreviewMarkdown = vi.fn();
    const onPrimaryAction = vi.fn();
    renderPanel(item(), onPreviewMarkdown, onPrimaryAction);

    expect(await screen.findByText("最终候选")).toBeInTheDocument();
    expect(screen.getByText("可读正文")).toBeInTheDocument();
    expect(screen.getByText("Update an existing Source")).toBeInTheDocument();
    expect(screen.getByText("wiki/sources/local/研究笔记.md")).toBeInTheDocument();
    expect(screen.getByText("Raw source")).toBeInTheDocument();
    expect(screen.getByText("Text coverage")).toBeVisible();
    expect(screen.getByText("Low text coverage")).toBeVisible();
    expect(screen.queryByText("TEXT_COVERAGE")).not.toBeInTheDocument();
    expect(screen.queryByText("LOW_TEXT_COVERAGE")).not.toBeInTheDocument();

    const technical = screen.getByText("Technical details and attempts").closest("details");
    expect(technical).not.toHaveAttribute("open");
    expect(technical).toHaveTextContent("item-a");
    expect(technical).toHaveTextContent("native_markdown");
    expect(technical).toHaveTextContent("markdown-hash");
    expect(screen.getByText("source-a")).not.toBeVisible();
    expect(screen.getByText("version-b")).not.toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: /preview markdown/i }));
    expect(onPrimaryAction).toHaveBeenCalledWith("preview_markdown", "item-a");
    expect(onPreviewMarkdown).not.toHaveBeenCalled();
  });

  it("shows stable user copy and keeps raw errors, routes, and engines in technical details", async () => {
    const failedAttempt = {
      ...item().attempts[0],
      outcome: "failed" as const,
      errorCode: "IMPORT_V2_PRIVATE_TARGET_BLOCKED",
    };
    renderPanel(item({
      status: "failed",
      attempts: [failedAttempt],
      issue: {
        code: "LOGIN_REQUIRED",
        message: "raw connector message",
        stage: "extract",
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["begin_login"],
        availableActions: [],
      },
    }));

    expect(await screen.findByText("Sign in to continue")).toBeInTheDocument();
    expect(screen.getByText(/nothing has been committed/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue login" })).toBeInTheDocument();
    const technical = screen.getByText("Technical details and attempts").closest("details");
    expect(technical).not.toHaveAttribute("open");
    expect(technical).toHaveTextContent("LOGIN_REQUIRED");
    expect(technical).toHaveTextContent("raw connector message");
    expect(technical).toHaveTextContent("native_markdown");
    expect(technical).toHaveTextContent("core-markdown 2.0.0");
    expect(technical).toHaveTextContent("IMPORT_V2_PRIVATE_TARGET_BLOCKED");
  });

  it("does not let a stale preview replace the newly selected item", async () => {
    let resolveFirst!: (value: ImportPreviewContent) => void;
    vi.mocked(importV2Api.getPreviewContent)
      .mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValueOnce({ ...preview(), itemId: "item-b", title: "第二项", markdown: "# 第二项" });

    const view = renderPanel(item());
    view.rerender(
      <ImportRightPanel
        selectedItem={item({ itemId: "item-b", input: { ...item().input, displayName: "第二项.md" } })}
        sessionId="session-a"
        projectId="project-a"
        projectRootPath="D:\\Wiki"
        onPreviewMarkdown={vi.fn()}
      />,
    );

    expect(await screen.findByText("第二项")).toBeInTheDocument();
    resolveFirst(preview());
    await waitFor(() => expect(screen.queryByText("最终候选")).not.toBeInTheDocument());
  });

  it("localizes preview failures, preserves diagnostics, and retries", async () => {
    vi.mocked(importV2Api.getPreviewContent)
      .mockRejectedValueOnce({ code: "IMPORT_PREVIEW_OFFLINE", message: "raw offline transport failure" })
      .mockResolvedValueOnce(preview());
    renderPanel(item());

    expect(await screen.findByText("The import could not continue.")).toBeInTheDocument();
    expect(screen.queryByText("raw offline transport failure")).not.toBeInTheDocument();
    const details = screen.getByText("Technical details").closest("details");
    expect(details).toHaveTextContent("raw offline transport failure");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("最终候选")).toBeInTheDocument();
    expect(importV2Api.getPreviewContent).toHaveBeenCalledTimes(2);
  });

  it("shows live local recognition percentage and stage before a preview exists", () => {
    renderPanel(item({
      status: "extracting",
      preview: null,
      progress: { current: 48, total: 100, label: "asr.recognizing" },
    }));

    const progressbar = screen.getByRole("progressbar", { name: /recognizing audio segments/i });
    expect(progressbar).toHaveAttribute("aria-valuenow", "48");
    expect(screen.getByText("48%")).toBeInTheDocument();
    expect(screen.getByText(/preview appears here/i)).toBeInTheDocument();
    expect(importV2Api.getPreviewContent).not.toHaveBeenCalled();
  });
});

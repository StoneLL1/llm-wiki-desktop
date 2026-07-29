import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportCompletion } from "../../types/importV2";
import {
  completionCountRows,
  ImportCompletionSummary,
} from "./ImportCompletionSummary";

const completion: ImportCompletion = {
  sessionId: "session-a",
  batchId: "batch-a",
  newSources: [{
    sourceId: "internal-source-id",
    versionId: "internal-version-id",
    wikiPath: "wiki/sources/本地/研究资料.md",
    contentHash: "a".repeat(64),
  }],
  updatedSources: [{
    sourceId: "updated-source-id",
    versionId: "updated-version-id",
    wikiPath: "wiki/sources/web/更新记录.md",
    contentHash: "b".repeat(64),
  }],
  duplicateSkips: [{
    itemId: "duplicate-item",
    sourceId: "duplicate-source-id",
    versionId: "duplicate-version-id",
    contentHash: "c".repeat(64),
  }],
  warnings: [{
    code: "QUALITY_WARNING",
    title: "One warning",
    dataSafety: "Source saved",
    primaryAction: null,
  }],
  failures: [{
    itemId: "failed-item",
    inputLabel: "失败文档.pdf",
    issue: {
      code: "IMPORT_FAILED",
      title: "Failed",
      dataSafety: "Other Sources kept",
      primaryAction: "retry",
    },
  }],
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCompletionSummary", () => {
  it.each([
    ["new", 1],
    ["updated", 1],
    ["duplicate", 1],
    ["warning", 1],
    ["failure", 1],
  ] as const)("maps the %s result count without changing the row structure", (key, count) => {
    expect(completionCountRows(completion).find((row) => row.key === key)?.count).toBe(count);
  });

  it("shows human filenames, keeps internal identity hidden, and exposes independent actions", () => {
    const onViewSources = vi.fn();
    const onViewSource = vi.fn();
    const onUpdateWiki = vi.fn();
    const onRetryFailure = vi.fn();
    render(
      <ImportCompletionSummary
        completion={completion}
        onViewSources={onViewSources}
        onViewSource={onViewSource}
        onUpdateWiki={onUpdateWiki}
        onRetryFailure={onRetryFailure}
      />,
    );

    expect(screen.getByText("研究资料.md")).toBeInTheDocument();
    expect(screen.getByText("更新记录.md")).toBeInTheDocument();
    expect(screen.queryByText("internal-source-id")).not.toBeInTheDocument();
    expect(screen.queryByText("internal-version-id")).not.toBeInTheDocument();
    expect(screen.queryByText("a".repeat(64))).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("link", { name: "研究资料.md" }));
    expect(onViewSource).toHaveBeenCalledWith("wiki/sources/本地/研究资料.md");
    fireEvent.click(screen.getByRole("button", { name: "View imported Sources" }));
    expect(onViewSources).toHaveBeenCalledTimes(1);
    expect(onUpdateWiki).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Update Wiki with these Sources" }));
    expect(onUpdateWiki).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetryFailure).toHaveBeenCalledWith("failed-item");
  });
});

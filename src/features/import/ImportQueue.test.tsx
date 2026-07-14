import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem } from "../../types/importV2";
import { ImportQueue } from "./ImportQueue";

function item(itemId: string, status: ImportItem["status"], selected = false): ImportItem {
  return {
    itemId,
    input: {
      kind: itemId === "url" ? "url" : "file",
      displayName: itemId === "url" ? "Public article" : `${itemId}.md`,
      locator: itemId === "url" ? "https://example.com/article" : `C:\\sources\\${itemId}.md`,
      normalizedLocator: itemId === "url" ? "https://example.com/article" : null,
    },
    status,
    selected,
    taskId: null,
    progress: null,
    attempts: [],
    preview: status === "preview_ready" ? {
      title: itemId,
      markdown: { kind: "markdown", relativePath: "raw/extracted/a.md", sha256: "a", sizeBytes: 1 },
      assets: [],
      sourceSnapshot: { kind: "source_snapshot", relativePath: "raw/sources/a", sha256: "b", sizeBytes: 1 },
      quality: { level: "pass", metrics: [], warnings: [] },
    } : null,
    issue: null,
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportQueue", () => {
  it("renders counts, filters, measurable progress, and stable row identities", () => {
    const onFilterChange = vi.fn();
    const items = [item("ready", "preview_ready", true), item("failed", "failed"), item("url", "completed")];
    render(
      <ImportQueue
        items={items}
        counts={{ all: 3, active: 0, ready: 1, needsAction: 0, failed: 1, completed: 1 }}
        progress={{ completed: 1, total: 3, active: 0 }}
        selectedItemId="ready"
        filter="all"
        onFilterChange={onFilterChange}
        onSelectItem={vi.fn()}
        onSetItemSelected={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByText("3 items")).toBeInTheDocument();
    expect(screen.getByText("33% complete")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ready 1/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /failed 1/i }));
    expect(onFilterChange).toHaveBeenCalledWith("failed");
    expect(screen.getByTestId("import-item-ready")).toBeInTheDocument();
    expect(screen.getByTestId("import-item-failed")).toBeInTheDocument();
  });

  it("selects rows and only exposes checkboxes for preview-ready or merge items", () => {
    const onSelectItem = vi.fn();
    const onSetItemSelected = vi.fn();
    render(
      <ImportQueue
        items={[item("ready", "preview_ready"), item("running", "extracting"), item("merge", "needs_merge")]}
        counts={{ all: 3, active: 1, ready: 1, needsAction: 1, failed: 0, completed: 0 }}
        progress={{ completed: 0, total: 3, active: 1 }}
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={onSelectItem}
        onSetItemSelected={onSetItemSelected}
        onAction={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId("import-item-ready"));
    expect(onSelectItem).toHaveBeenCalledWith("ready");
    expect(screen.getByRole("checkbox", { name: "Select ready.md" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Select merge.md" })).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Select running.md" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "Select ready.md" }));
    expect(onSetItemSelected).toHaveBeenCalledWith("ready", true);
  });

  it("routes row actions without calling backend code from the component", () => {
    const onAction = vi.fn();
    render(
      <ImportQueue
        items={[item("failed", "failed"), item("ready", "preview_ready")]}
        counts={{ all: 2, active: 0, ready: 1, needsAction: 0, failed: 1, completed: 0 }}
        progress={{ completed: 0, total: 2, active: 0 }}
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={vi.fn()}
        onSetItemSelected={vi.fn()}
        onAction={onAction}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Retry failed/ }));
    expect(onAction).toHaveBeenCalledWith("retry", "failed");
  });
});

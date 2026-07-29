import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem } from "../../types/importV2";
import type { BackendTask } from "../../types/task";
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
    expect(screen.getByText("1/3 processed · 33%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ready 1/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /failed 1/i }));
    expect(onFilterChange).toHaveBeenCalledWith("failed");
    expect(screen.getByTestId("import-item-ready")).toBeInTheDocument();
    expect(screen.getByTestId("import-item-failed")).toBeInTheDocument();
  });

  it("shows discovery progress instead of an empty 0/0 session progress", () => {
    const discoveryTask: BackendTask = {
      id: "scan-task",
      taskType: "import",
      projectId: "project-a",
      title: "Scan sources",
      status: "running",
      progress: { current: 12, total: null, label: "Discovering files" },
      startedAt: "2026-07-15T00:00:00Z",
      updatedAt: "2026-07-15T00:00:01Z",
      completedAt: null,
      cancellable: true,
      logPath: null,
      result: null,
      error: null,
    };
    render(
      <ImportQueue
        items={[]}
        counts={{ all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0 }}
        progress={{ completed: 0, total: 0, active: 0 }}
        discoveryTask={discoveryTask}
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={vi.fn()}
        onSetItemSelected={vi.fn()}
        onAction={vi.fn()}
      />,
    );
    expect(screen.getByText(/scanning.*12 discovered/i)).toBeInTheDocument();
    expect(screen.getByText(/building the queue/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /choose files/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/0\/0 processed/i)).not.toBeInTheDocument();
  });

  it("selects rows and keeps unresolved merge items out of commit selection", () => {
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
    expect(screen.getByRole("checkbox", { name: "Select merge.md" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "Select running.md" })).toBeDisabled();

    fireEvent.click(screen.getByRole("checkbox", { name: "Select ready.md" }));
    expect(onSetItemSelected).toHaveBeenCalledWith("ready", true);
  });

  it("does not let row keyboard handling steal checkbox or action keys", () => {
    const onSelectItem = vi.fn();
    render(
      <ImportQueue
        items={[item("ready", "preview_ready"), item("failed", "failed")]}
        counts={{ all: 2, active: 0, ready: 1, needsAction: 0, failed: 1, completed: 0 }}
        progress={{ completed: 0, total: 2, active: 0 }}
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={onSelectItem}
        onSetItemSelected={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    fireEvent.keyDown(screen.getByRole("checkbox", { name: "Select ready.md" }), { key: " " });
    fireEvent.keyDown(screen.getByRole("button", { name: "Retry" }), { key: "Enter" });
    expect(onSelectItem).not.toHaveBeenCalled();
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

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onAction).toHaveBeenCalledWith("retry", "failed");
  });

  it("offers a keyboard-accessible copy action for long source locators", () => {
    const onCopyLocator = vi.fn();
    render(
      <ImportQueue
        items={[item("very-long-source-name", "failed")]}
        counts={{ all: 1, active: 0, ready: 0, needsAction: 0, failed: 1, completed: 0 }}
        progress={{ completed: 0, total: 1, active: 0 }}
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={vi.fn()}
        onSetItemSelected={vi.fn()}
        onAction={vi.fn()}
        onCopyLocator={onCopyLocator}
      />,
    );

    const more = screen.getByRole("button", { name: "More actions for very-long-source-name.md" });
    fireEvent.keyDown(more, { key: "Enter" });
    fireEvent.click(more);
    const copy = screen.getByRole("menuitem", { name: "Copy original location" });
    fireEvent.click(copy);
    expect(onCopyLocator).toHaveBeenCalledWith("C:\\sources\\very-long-source-name.md");
  });

  it("labels the short interval between task completion and session refresh", () => {
    render(
      <ImportQueue
        items={[item("syncing", "extracting")]}
        counts={{ all: 1, active: 1, ready: 0, needsAction: 0, failed: 0, completed: 0 }}
        progress={{ completed: 0, total: 1, active: 1 }}
        sessionSyncing
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={vi.fn()}
        onSetItemSelected={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    expect(screen.getByText("Updating queue…")).toBeInTheDocument();
  });

  it("announces one compact summary without making the whole queue live", () => {
    render(
      <ImportQueue
        items={[item("ready", "preview_ready"), item("failed", "failed")]}
        counts={{ all: 2, active: 0, ready: 1, needsAction: 0, failed: 1, completed: 0 }}
        progress={{ completed: 0, total: 2, active: 0 }}
        selectedItemId={null}
        filter="all"
        onFilterChange={vi.fn()}
        onSelectItem={vi.fn()}
        onSetItemSelected={vi.fn()}
        onAction={vi.fn()}
      />,
    );

    const liveRegions = document.querySelectorAll("[aria-live]");
    expect(liveRegions).toHaveLength(1);
    expect(liveRegions[0]).toHaveTextContent(
      "Queue updated: 2 items, 0 need action, 1 ready.",
    );
    expect(screen.getByRole("list", { name: "Sources" })).not.toHaveAttribute("aria-live");
  });
});

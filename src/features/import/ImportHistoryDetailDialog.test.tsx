import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { ImportHistoryDetailPage, ImportHistoryEntry } from "../../types/importV2Presentation";
import { ImportHistoryDetailDialog } from "./ImportHistoryDetailDialog";

function item(overrides: Partial<ImportItem> = {}): ImportItem {
  return {
    itemId: "item-1",
    input: { kind: "file", displayName: "notes.md", locator: "notes.md", normalizedLocator: null },
    status: "completed",
    selected: true,
    taskId: "task-1",
    progress: null,
    attempts: [{
      route: "native_markdown",
      engineId: "core-markdown",
      engineVersion: "2.0.0",
      stage: "extract",
      startedAt: "2026-07-15T00:00:00Z",
      completedAt: "2026-07-15T00:00:01Z",
      outcome: "succeeded",
      warnings: ["Recovered a missing heading."],
    }],
    preview: {
      title: "notes",
      markdown: { kind: "markdown", relativePath: "wiki/notes.md", sha256: "hash", sizeBytes: 10 },
      assets: [],
      sourceSnapshot: { kind: "source_snapshot", relativePath: "raw/notes.md", sha256: "source", sizeBytes: 10 },
      quality: { level: "pass", metrics: [], warnings: [] },
    },
    issue: null,
    ...overrides,
  };
}

const session: ImportSession = {
  schemaVersion: 2,
  sessionId: "session-1",
  projectId: "project-1",
  status: "completed",
  resourceMode: "balanced",
  createdAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:01:00Z",
  items: [item()],
};

const entry: ImportHistoryEntry = {
  id: "batch-1",
  title: "Import · notes.md",
  status: "completed",
  sessionId: "session-1",
  batchId: "batch-1",
  taskId: null,
  startedAt: session.createdAt,
  updatedAt: session.updatedAt,
  completedAt: session.updatedAt,
  legacyReadOnly: false,
  itemCount: 1,
  committedCount: 1,
  failedCount: 0,
  sampleLabels: ["notes.md"],
  availableActions: ["open_result", "view_logs"],
};

const page: ImportHistoryDetailPage = {
  entry,
  items: session.items,
  nextCursor: null,
  total: 1,
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportHistoryDetailDialog", () => {
  it("lets the user choose a historical item preview or its task log", () => {
    const onPreview = vi.fn();
    const onViewLogs = vi.fn();
    render(
      <ImportHistoryDetailDialog
        open
        page={page}
        onClose={vi.fn()}
        onPreview={onPreview}
        canViewLogs={() => true}
        onViewLogs={onViewLogs}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
    fireEvent.click(screen.getByRole("button", { name: "View logs for notes.md" }));
    expect(onPreview).toHaveBeenCalledWith("item-1");
    expect(onViewLogs).toHaveBeenCalledWith("task-1");
  });

  it("does not present a dead log action when the task is no longer available", () => {
    render(
      <ImportHistoryDetailDialog
        open
        page={page}
        onClose={vi.fn()}
        onPreview={vi.fn()}
        canViewLogs={() => false}
        onViewLogs={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: /view item logs/i })).not.toBeInTheDocument();
  });

  it("keeps a missing historical result explanation visible in the detail dialog", () => {
    render(
      <ImportHistoryDetailDialog
        open
        page={page}
        resultUnavailable
        onClose={vi.fn()}
        onPreview={vi.fn()}
        canViewLogs={() => false}
        onViewLogs={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(/committed result artifact is missing/i);
  });

  it("keeps attempt route, duration, and warnings inspectable without crowding the item row", () => {
    render(
      <ImportHistoryDetailDialog
        open
        page={page}
        onClose={vi.fn()}
        onPreview={vi.fn()}
        canViewLogs={() => false}
        onViewLogs={vi.fn()}
      />,
    );

    const attempts = screen.getByText("Attempts (1)");
    expect(attempts).toBeInTheDocument();
    fireEvent.click(attempts);
    expect(screen.getByText(/native_markdown · core-markdown 2\.0\.0 · extract/i)).toBeInTheDocument();
    expect(screen.getByText("Duration: 1.0 s")).toBeInTheDocument();
    expect(screen.getAllByText("Recovered a missing heading.").length).toBeGreaterThan(0);
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportHistoryPage } from "../../types/importV2Presentation";
import { ImportHistoryPanel } from "./ImportHistoryPanel";

const page: ImportHistoryPage = {
  entries: [{ id: "v2-1", title: "New import", status: "completed", sessionId: "session-1", batchId: "batch-1", taskId: null, startedAt: "2026-07-13T00:00:00Z", updatedAt: "2026-07-13T00:01:00Z", completedAt: "2026-07-13T00:01:00Z", legacyReadOnly: false, itemCount: 1, committedCount: 1, failedCount: 0, sampleLabels: ["item-1"], availableActions: ["open_result", "view_logs", "update_wiki"] }],
  legacyReadOnly: [{ id: "legacy-1", title: "Old import", status: "completed", startedAt: null, updatedAt: null, completedAt: null, evidencePath: ".app/legacy-import.json", legacyReadOnly: true, availableActions: [], canRetry: false, canDelete: false, canReplaceSource: false }],
  nextCursor: "next-1",
  warnings: [],
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportHistoryPanel", () => {
  it("labels legacy entries read-only and removes mutation actions", () => {
    render(<ImportHistoryPanel page={page} onOpenEntry={vi.fn()} onLoadMore={vi.fn()} />);

    expect(screen.getByText(/read-only legacy/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /retry|delete|replace|commit/i })).not.toBeInTheDocument();
  });

  it("exposes V2 details and cursor pagination only through callbacks", () => {
    const onOpenEntry = vi.fn();
    const onLoadMore = vi.fn();
    render(<ImportHistoryPanel page={page} onOpenEntry={onOpenEntry} onLoadMore={onLoadMore} />);

    fireEvent.click(screen.getByRole("button", { name: /open result/i }));
    fireEvent.click(screen.getByRole("button", { name: /update wiki/i }));
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    expect(onOpenEntry).toHaveBeenCalledWith("v2-1", "open_result");
    expect(onOpenEntry).toHaveBeenCalledWith("v2-1", "update_wiki");
    expect(onLoadMore).toHaveBeenCalledWith("next-1");
  });

  it("locks pagination while the next page is loading", () => {
    render(<ImportHistoryPanel page={page} loadingMore onLoadMore={vi.fn()} />);
    expect(screen.getByRole("button", { name: /loading history/i })).toBeDisabled();
  });

  it("falls back to details for a non-committed entry even if stale actions mention a result", () => {
    const stale: ImportHistoryPage = {
      ...page,
      entries: [{ ...page.entries[0], status: "processing", availableActions: ["open_result", "open_detail"] }],
    };
    const onOpenEntry = vi.fn();
    render(<ImportHistoryPanel page={stale} onOpenEntry={onOpenEntry} onLoadMore={vi.fn()} />);

    expect(screen.queryByRole("button", { name: /open result/i })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /open details/i }));
    expect(onOpenEntry).toHaveBeenCalledWith("v2-1", "open_detail");
  });

  it("distinguishes a history load failure from an empty history", () => {
    const onRetry = vi.fn();
    render(<ImportHistoryPanel page={null} error onRetry={onRetry} onLoadMore={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent(/could not be loaded/i);
    fireEvent.click(screen.getByRole("button", { name: /retry/i }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});

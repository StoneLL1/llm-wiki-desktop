import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { ImportBatchStatus } from "./ImportBatchStatus";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportBatchStatus", () => {
  const tasks = [{ id: "task-1", itemId: "notes.md", title: "Import notes.md", status: "running" as const, cancellable: true }];

  it("shows aggregate progress and exposes batch cancellation", () => {
    const onCancel = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ImportBatchStatus
        batch={{ id: "batch-1", sessionId: "session-1", taskIds: ["task-1"], total: 3, processed: 1, active: 2, completed: 1, failed: 0, cancelled: 0, cancelling: 0, unknown: 0, nonCancellable: 0, failedItemIds: [], tasks }}
        onCancel={onCancel}
        onDismiss={onDismiss}
      />,
    );

    expect(screen.getByText("1/3 tasks finished")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /cancel batch/i }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: /dismiss batch status/i })).not.toBeInTheDocument();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("keeps a completed batch dismissible without a cancel action", () => {
    render(
      <ImportBatchStatus
        batch={{ id: "batch-2", sessionId: "session-1", taskIds: [], total: 2, processed: 2, active: 0, completed: 1, failed: 1, cancelled: 0, cancelling: 0, unknown: 0, nonCancellable: 0, failedItemIds: ["item-1"], tasks: [] }}
        onCancel={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: /cancel batch/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /dismiss batch status/i })).toBeInTheDocument();
  });

  it("keeps task logs behind an explicit batch detail disclosure", () => {
    const onViewTask = vi.fn();
    render(
      <ImportBatchStatus
        batch={{ id: "batch-1", sessionId: "session-1", taskIds: ["task-1"], total: 1, processed: 0, active: 1, completed: 0, failed: 0, cancelled: 0, cancelling: 0, unknown: 0, nonCancellable: 0, failedItemIds: [], tasks }}
        onCancel={vi.fn()}
        onDismiss={vi.fn()}
        onViewTask={onViewTask}
      />,
    );

    const disclosure = screen.getByText("View 1 task details").closest("details");
    expect(disclosure).not.toHaveAttribute("open");
    fireEvent.click(screen.getByText("View 1 task details"));
    expect(disclosure).toHaveAttribute("open");
    fireEvent.click(screen.getByRole("button", { name: /view log for import notes\.md/i }));
    expect(onViewTask).toHaveBeenCalledWith("task-1");
  });

  it("does not trap the user in an active state when a task is no longer present", () => {
    render(
      <ImportBatchStatus
        batch={{ id: "batch-3", sessionId: "session-1", taskIds: ["task-1", "task-2"], total: 2, processed: 1, active: 0, completed: 1, failed: 0, cancelled: 0, cancelling: 0, unknown: 1, nonCancellable: 0, failedItemIds: [], tasks: [{ id: "task-2", itemId: "lost.md", title: "lost.md", status: "unknown", cancellable: false }] }}
        onCancel={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.getByText(/1 unavailable/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /dismiss batch status/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /cancel batch/i })).not.toBeInTheDocument();
  });

  it("does not offer cancellation for a running task that cannot be cancelled", () => {
    render(
      <ImportBatchStatus
        batch={{ id: "batch-4", sessionId: "session-1", taskIds: ["task-4"], total: 1, processed: 0, active: 1, completed: 0, failed: 0, cancelled: 0, cancelling: 0, unknown: 0, nonCancellable: 1, failedItemIds: [], tasks: [{ id: "task-4", itemId: "large.bin", title: "large.bin", status: "running", cancellable: false }] }}
        onCancel={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.getByText(/1 cannot be cancelled/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /cancel batch/i })).not.toBeInTheDocument();
  });

  it("treats preview-ready tasks as waiting for the user, not as still running", () => {
    const onCancel = vi.fn();
    render(
      <ImportBatchStatus
        batch={{ id: "batch-5", sessionId: "session-1", taskIds: ["task-5"], total: 1, processed: 1, active: 0, completed: 0, waitingForConfirmation: 1, reviewReady: 1, failed: 0, cancelled: 0, cancelling: 0, unknown: 0, nonCancellable: 0, failedItemIds: [], tasks: [{ id: "task-5", itemId: "notes.md", title: "Import notes.md", status: "waiting_for_confirmation", cancellable: true }] }}
        onCancel={onCancel}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.getByText("1/1 tasks finished")).toBeInTheDocument();
    expect(screen.getByText(/ready to review/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /cancel batch/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /dismiss batch status/i })).toBeInTheDocument();
    expect(onCancel).not.toHaveBeenCalled();
  });
});

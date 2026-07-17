import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { BackendTask } from "../../types/task";
import { ImportDiscoveryStatus } from "./ImportDiscoveryStatus";

function task(status: BackendTask["status"]): BackendTask {
  return {
    id: "scan-task",
    taskType: "import",
    projectId: "project-a",
    title: "Scan sources",
    status,
    progress: { current: 12, total: null, label: "Discovering files" },
    startedAt: "2026-07-15T00:00:00Z",
    updatedAt: "2026-07-15T00:00:01Z",
    completedAt: status === "succeeded" ? "2026-07-15T00:00:02Z" : null,
    cancellable: status === "running",
    logPath: null,
    result: status === "succeeded" ? { summary: "Added 12 files", affectedPaths: [] } : null,
    error: null,
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportDiscoveryStatus", () => {
  it("shows discovered count and an indeterminate bar while scanning", () => {
    const onCancel = vi.fn();
    render(<ImportDiscoveryStatus task={task("running")} onCancel={onCancel} onDismiss={vi.fn()} />);
    expect(screen.getByText(/12 discovered/i)).toBeInTheDocument();
    expect(screen.getByText(/12 added/i)).toBeInTheDocument();
    expect(screen.getByText(/0 skipped/i)).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /cancel scan/i }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("locks the cancel control while cancellation is requested", () => {
    render(<ImportDiscoveryStatus task={task("running")} cancelling onCancel={vi.fn()} onDismiss={vi.fn()} />);
    expect(screen.getByRole("button", { name: /cancelling/i })).toBeDisabled();
  });

  it("shows a localized completion summary without the backend raw summary", () => {
    render(<ImportDiscoveryStatus task={task("succeeded")} onCancel={vi.fn()} onDismiss={vi.fn()} />);
    expect(screen.getByText(/source scan complete/i)).toBeInTheDocument();
    expect(screen.getByText(/12 discovered/i)).toBeInTheDocument();
    expect(screen.queryByText(/Added 12 files/i)).not.toBeInTheDocument();
  });

  it("keeps the recovery notice compact when a scan cannot be restored", () => {
    render(
      <ImportDiscoveryStatus
        task={null}
        unavailable
        onCancel={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: /rescan files/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /rescan folder/i })).not.toBeInTheDocument();
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { FileScanResult } from "../../types/importV2File";
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

  it("reveals the path and localized reason for skipped files", () => {
    const scan: FileScanResult = {
      files: [],
      skipped: [{
        sourcePath: "D:/Wiki/project/wiki/internal.md",
        relativePath: "wiki/internal.md",
        reason: "project_internal",
        detail: "Current project content cannot be imported into itself.",
      }],
      truncated: false,
    };

    render(<ImportDiscoveryStatus task={task("succeeded")} scan={scan} onCancel={vi.fn()} onDismiss={vi.fn()} />);

    fireEvent.click(screen.getByText(/view 1 skipped item/i));
    expect(screen.getByText("wiki/internal.md")).toBeInTheDocument();
    expect(screen.getByText(/inside the current Wiki project/i)).toBeInTheDocument();
  });

  it("shows content-detected extension mismatches", () => {
    const scan: FileScanResult = {
      files: [{
        sourcePath: "D:/sources/report.txt",
        relativePath: "nested/report.txt",
        displayName: "report.txt",
        format: "pdf",
        contentKind: "document",
        sizeBytes: 128,
        identity: {
          extension: "txt",
          magic: "pdf",
          mime: "application/pdf",
          detectionMethod: "magic",
          extensionMismatch: true,
        },
        sourceIdentity: {
          canonicalPath: "D:/sources/report.txt",
          sizeBytes: 128,
          modifiedNanos: null,
          fileId: null,
          sha256: "a".repeat(64),
          magic: "b".repeat(64),
        },
      }],
      skipped: [],
      truncated: false,
    };

    render(<ImportDiscoveryStatus task={task("succeeded")} scan={scan} onCancel={vi.fn()} onDismiss={vi.fn()} />);
    fireEvent.click(screen.getByText(/different detected format/i));

    expect(screen.getByText("nested/report.txt")).toBeInTheDocument();
    expect(screen.getByText(".txt detected as PDF")).toBeInTheDocument();
  });

  it("requires explicit confirmation before adding every row of a large CSV", () => {
    const onConfirmLargeData = vi.fn();
    const sourcePath = "D:/sources/large.csv";
    const scan: FileScanResult = {
      files: [{
        sourcePath,
        relativePath: "large.csv",
        displayName: "large.csv",
        format: "csv",
        contentKind: "document",
        sizeBytes: 9_000_000,
        identity: {
          extension: "csv",
          magic: "delimited-text",
          mime: "text/csv",
          detectionMethod: "structured_text",
          extensionMismatch: false,
        },
        sourceIdentity: {
          canonicalPath: sourcePath,
          sizeBytes: 9_000_000,
          modifiedNanos: null,
          fileId: null,
          sha256: "a".repeat(64),
          magic: "b".repeat(64),
        },
        largeData: {
          rowCount: 12_345,
          estimatedOutputFiles: 4,
          totalBytes: 9_000_000,
          requiresConfirmation: true,
        },
      }],
      skipped: [{
        sourcePath,
        relativePath: "large.csv",
        reason: "large_data_confirmation_required",
      }],
      truncated: false,
    };

    render(
      <ImportDiscoveryStatus
        task={task("succeeded")}
        scan={scan}
        onCancel={vi.fn()}
        onDismiss={vi.fn()}
        onConfirmLargeData={onConfirmLargeData}
      />,
    );
    expect(screen.getByText(/12,345 rows/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /continue with all rows/i }));
    expect(onConfirmLargeData).toHaveBeenCalledWith([sourcePath]);
  });
});

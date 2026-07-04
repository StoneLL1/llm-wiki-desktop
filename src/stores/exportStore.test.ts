import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useExportStore } from "./exportStore";
import type { ExportRecord } from "../types/export";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function record(overrides: Partial<ExportRecord> = {}): ExportRecord {
  return {
    id: "export-1",
    exportType: "beautiful_read",
    title: "Agent",
    sourcePath: "wiki/concepts/agent.md",
    outputPath: "exports/html/agent.html",
    createdAt: "2026-07-04T00:00:00Z",
    route: "byok",
    status: "succeeded",
    bookmarked: false,
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  useExportStore.getState().reset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("exportStore", () => {
  it("toggles an export bookmark and updates the matching record", async () => {
    useExportStore.setState({ records: [record()] });
    invokeMock.mockResolvedValueOnce({
      exportRecordId: "export-1",
      bookmarked: true,
    });

    await useExportStore.getState().toggleBookmark("p", "/x", "export-1");

    expect(invokeMock).toHaveBeenCalledWith("toggle_export_bookmark", {
      request: {
        projectId: "p",
        projectRootPath: "/x",
        exportRecordId: "export-1",
      },
    });
    expect(useExportStore.getState().records[0]?.bookmarked).toBe(true);
  });
});

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
  it("rolls back owned loading state when an identity commit guard expires", async () => {
    let resolve!: (records: ExportRecord[]) => void;
    invokeMock.mockReturnValueOnce(new Promise<ExportRecord[]>((done) => { resolve = done; }));
    let current = true;
    const loading = useExportStore.getState().loadExports("p", "/x", () => current);
    expect(useExportStore.getState().loading).toBe(true);
    current = false;
    resolve([record()]);
    await loading;

    expect(useExportStore.getState()).toMatchObject({ loading: false, records: [] });
  });

  it("does not inherit loading from an overlapping superseded list request", async () => {
    let resolveA!: (records: ExportRecord[]) => void;
    let resolveB!: (records: ExportRecord[]) => void;
    invokeMock
      .mockReturnValueOnce(new Promise<ExportRecord[]>((done) => { resolveA = done; }))
      .mockReturnValueOnce(new Promise<ExportRecord[]>((done) => { resolveB = done; }));
    let current = true;
    const first = useExportStore.getState().loadExports("p", "/x", () => current);
    const second = useExportStore.getState().loadExports("p", "/x", () => current);
    current = false;
    resolveA([]);
    resolveB([]);
    await Promise.all([first, second]);

    expect(useExportStore.getState().loading).toBe(false);
  });

  it("defaults to inline preview mode and allows switching modes", () => {
    expect(useExportStore.getState().previewMode).toBe("inline");

    useExportStore.getState().setPreviewMode("source");

    expect(useExportStore.getState().previewMode).toBe("source");
  });

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

  it("opens an export in the browser through the backend command", async () => {
    const request = { projectId: "p", projectRootPath: "/x", outputPath: "exports/html/agent.html" };
    useExportStore.setState({ error: "stale error" });
    invokeMock.mockResolvedValueOnce(undefined);

    await useExportStore.getState().openInBrowser(request);

    expect(invokeMock).toHaveBeenCalledWith("open_export_in_browser", { request });
    expect(useExportStore.getState().error).toBeNull();
  });
});

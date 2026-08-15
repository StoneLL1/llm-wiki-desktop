import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useExportStore } from "./exportStore";
import type { ExportRecord } from "../types/export";
import { invalidateProjectResources, invalidateProjectScope } from "./projectScope";

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
  it("single-flights ensure calls and preserves records after a failed revalidation", async () => {
    invokeMock.mockResolvedValueOnce([record()]).mockRejectedValueOnce(new Error("offline"));
    await Promise.all(Array.from({ length: 20 }, () =>
      useExportStore.getState().ensureExports("p", "/x")));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    const records = useExportStore.getState().records;

    invalidateProjectResources({ projectId: "p", rootPath: "/x" }, ["exports"]);
    await useExportStore.getState().ensureExports("p", "/x");
    expect(useExportStore.getState().records).toBe(records);
    expect(useExportStore.getState().error).toBe("offline");
  });

  it("rejects an old A response after A to B to A project switches", async () => {
    let resolveOldA!: (records: ExportRecord[]) => void;
    invokeMock
      .mockReturnValueOnce(new Promise<ExportRecord[]>((resolve) => { resolveOldA = resolve; }))
      .mockResolvedValueOnce([record({ id: "project-b" })])
      .mockResolvedValueOnce([record({ id: "project-a-current" })]);

    const oldA = useExportStore.getState().ensureExports("a", "D:/a");
    invalidateProjectScope();
    useExportStore.getState().reset();
    await useExportStore.getState().ensureExports("b", "D:/b");
    invalidateProjectScope();
    useExportStore.getState().reset();
    await useExportStore.getState().ensureExports("a", "D:/a");
    resolveOldA([record({ id: "project-a-stale" })]);
    await oldA;

    expect(useExportStore.getState().records[0]?.id).toBe("project-a-current");
  });

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

  it("does not let an older list refresh overwrite a bookmark mutation", async () => {
    let resolveList!: (records: ExportRecord[]) => void;
    useExportStore.setState({ records: [record()] });
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_exports") {
        return new Promise<ExportRecord[]>((resolve) => { resolveList = resolve; });
      }
      if (command === "toggle_export_bookmark") {
        return Promise.resolve({ exportRecordId: "export-1", bookmarked: true });
      }
      return Promise.resolve(null);
    });

    const refreshing = useExportStore.getState().ensureExports("p", "/x");
    await useExportStore.getState().toggleBookmark("p", "/x", "export-1");
    resolveList([record({ bookmarked: false })]);
    await refreshing;

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

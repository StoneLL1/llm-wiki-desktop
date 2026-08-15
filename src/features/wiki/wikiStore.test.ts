import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  invalidateProjectResources,
  invalidateProjectScope,
} from "../../stores/projectScope";
import { useWikiStore } from "./wikiStore";

const emptyTree = {
  root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 0, children: [] },
  pages: [],
  totalPages: 0,
};

beforeEach(() => {
  invokeMock.mockReset();
  useWikiStore.getState().reset();
});

describe("wikiStore identity commit guard", () => {
  it("single-flights repeated ensures and refreshes once after invalidation", async () => {
    invokeMock.mockResolvedValue(emptyTree);
    await Promise.all(Array.from({ length: 20 }, () =>
      useWikiStore.getState().ensureScanned("p", "/x")));
    expect(invokeMock).toHaveBeenCalledTimes(1);

    invalidateProjectResources({ projectId: "p", rootPath: "/x" }, ["wiki"]);
    await useWikiStore.getState().ensureScanned("p", "/x");
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("keeps a usable tree when a stale background refresh fails", async () => {
    invokeMock.mockResolvedValueOnce(emptyTree).mockRejectedValueOnce(new Error("offline"));
    await useWikiStore.getState().ensureScanned("p", "/x");
    const tree = useWikiStore.getState().tree;
    invalidateProjectResources({ projectId: "p", rootPath: "/x" }, ["wiki"]);
    await useWikiStore.getState().ensureScanned("p", "/x");

    expect(useWikiStore.getState().tree).toBe(tree);
    expect(useWikiStore.getState().error).toBe("offline");
  });

  it("does not reopen an old project page after an A to B switch", async () => {
    const pagePath = "wiki/same.md";
    const tree = {
      ...emptyTree,
      pages: [{ path: pagePath }],
      totalPages: 1,
    } as never;
    let resolveA!: (tree: never) => void;
    let scanCount = 0;
    invokeMock.mockImplementation((command: string, args: { request: { projectId: string } }) => {
      if (command === "scan_wiki") {
        scanCount += 1;
        if (scanCount === 1) return new Promise((resolve) => { resolveA = resolve; });
        return Promise.resolve(tree);
      }
      if (command === "read_wiki_page") {
        return Promise.resolve({ path: pagePath, title: "Same", content: args.request.projectId });
      }
      return Promise.resolve(null);
    });
    useWikiStore.setState({
      tree,
      selectedPath: pagePath,
      mode: "read",
    });

    const oldA = useWikiStore.getState().ensureScanned("a", "/a");
    invalidateProjectScope();
    useWikiStore.getState().reset();
    await useWikiStore.getState().ensureScanned("b", "/b");
    resolveA(tree);
    await oldA;

    const pageReads = invokeMock.mock.calls.filter(([command]) => command === "read_wiki_page");
    expect(pageReads).toHaveLength(1);
    expect(pageReads[0][1].request.projectId).toBe("b");
  });

  it("rolls back scan loading when the guard expires", async () => {
    let resolve!: (tree: never) => void;
    invokeMock.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    let current = true;
    const scanning = useWikiStore.getState().scan("p", "/x", () => current);
    expect(useWikiStore.getState().loadingTree).toBe(true);
    current = false;
    resolve({ pages: [] } as never);
    await scanning;

    expect(useWikiStore.getState()).toMatchObject({ loadingTree: false, tree: null });
  });

  it("restores selection when an open-page guard expires", async () => {
    useWikiStore.setState({ selectedPath: "wiki/current.md", loadingPage: false });
    let resolve!: (page: never) => void;
    invokeMock.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    let current = true;
    const opening = useWikiStore.getState().openPage("p", "/x", "wiki/stale.md", () => current);
    expect(useWikiStore.getState()).toMatchObject({ selectedPath: "wiki/stale.md", loadingPage: true });
    current = false;
    resolve({} as never);
    await opening;

    expect(useWikiStore.getState()).toMatchObject({
      selectedPath: "wiki/current.md",
      loadingPage: false,
    });
  });

  it("rolls overlapping opens back to the last stable presentation", async () => {
    useWikiStore.setState({ selectedPath: "wiki/current.md", loadingPage: false });
    let resolveA!: (page: never) => void;
    let resolveB!: (page: never) => void;
    invokeMock
      .mockReturnValueOnce(new Promise((done) => { resolveA = done; }))
      .mockReturnValueOnce(new Promise((done) => { resolveB = done; }));
    let current = true;
    const first = useWikiStore.getState().openPage("p", "/x", "wiki/a.md", () => current);
    const second = useWikiStore.getState().openPage("p", "/x", "wiki/b.md", () => current);
    current = false;
    resolveA({} as never);
    resolveB({} as never);
    await Promise.all([first, second]);

    expect(useWikiStore.getState()).toMatchObject({
      selectedPath: "wiki/current.md",
      loadingPage: false,
    });
  });
});

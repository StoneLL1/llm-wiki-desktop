import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useWikiStore } from "./wikiStore";

beforeEach(() => {
  invokeMock.mockReset();
  useWikiStore.getState().reset();
});

describe("wikiStore identity commit guard", () => {
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

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import type {
  SaveWikiPageResponse,
  WikiPageContent,
  WikiPageMeta,
  WikiTree,
} from "../../types/wiki";
import { MarkdownReader } from "./MarkdownReader";
import { useWikiStore } from "./wikiStore";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function pageMeta(overrides: Partial<WikiPageMeta> = {}): WikiPageMeta {
  return {
    path: "wiki/concepts/transformer.md",
    title: "Transformer",
    pageType: "concept",
    tags: ["nlp"],
    sources: [],
    aliases: ["Transformers"],
    created: null,
    updated: null,
    starred: false,
    bookmarked: false,
    wordCount: 100,
    fileSize: 2048,
    modifiedTime: "2024-01-01T00:00:00Z",
    hash: "hash-1",
    wikilinks: ["attention"],
    ...overrides,
  };
}

function pageContent(overrides: Partial<WikiPageContent> = {}): WikiPageContent {
  return {
    meta: pageMeta(),
    rawMarkdown: "# Transformer\n\nSee [[attention]].",
    bodyMarkdown: "# Transformer\n\nSee [[attention]].",
    frontmatterYaml: null,
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  useWikiStore.getState().reset();
  void i18next.changeLanguage("en");
});

afterEach(() => {
  cleanup();
});

describe("wikiStore", () => {
  it("scans the tree and opens the first page by default", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const content = pageContent();
    invokeMock.mockResolvedValueOnce(tree).mockResolvedValueOnce(content);

    await useWikiStore.getState().scan("proj-1", "D:/wiki");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "scan_wiki", {
      request: { projectId: "proj-1", projectRootPath: "D:/wiki" },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "read_wiki_page", {
      request: { projectId: "proj-1", projectRootPath: "D:/wiki", relativePath: "wiki/concepts/transformer.md" },
    });
    expect(useWikiStore.getState().tree).toEqual(tree);
    expect(useWikiStore.getState().page).toEqual(content);
    expect(useWikiStore.getState().selectedPath).toBe("wiki/concepts/transformer.md");
  });

  it("opens a page and seeds the draft from raw markdown", async () => {
    const content = pageContent({ rawMarkdown: "---\ntitle: X\n---\nbody" });
    invokeMock.mockResolvedValueOnce(content);

    await useWikiStore.getState().openPage("proj-1", "D:/wiki", "wiki/a.md");

    expect(useWikiStore.getState().draft).toBe("---\ntitle: X\n---\nbody");
    expect(useWikiStore.getState().mode).toBe("read");
  });

  it("marks saveState as conflict when the backend reports FILE_HASH_MISMATCH", async () => {
    useWikiStore.setState({
      page: pageContent(),
      draft: "# Edited",
      mode: "edit",
    });
    invokeMock.mockRejectedValueOnce({ code: "FILE_HASH_MISMATCH", message: "changed" });

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    expect(useWikiStore.getState().saveState).toBe("conflict");
  });

  it("marks saveState as error for generic backend failures and keeps the draft", async () => {
    const draft = "# Edited";
    useWikiStore.setState({ page: pageContent(), draft, mode: "edit" });
    invokeMock.mockRejectedValueOnce({ code: "PATH_TRAVERSAL", message: "bad path" });

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    expect(useWikiStore.getState().saveState).toBe("error");
    expect(useWikiStore.getState().draft).toBe(draft);
  });

  it("sends expectedHash from the current page on save and returns to read mode", async () => {
    useWikiStore.setState({ page: pageContent(), draft: "# New", mode: "edit" });
    const saveResponse: SaveWikiPageResponse = {
      relativePath: "wiki/concepts/transformer.md",
      hash: "hash-2",
      savedAt: "2024-01-02T00:00:00Z",
      graphCacheInvalidated: true,
    };
    invokeMock.mockResolvedValueOnce(saveResponse).mockResolvedValueOnce(
      pageContent({ meta: pageMeta({ hash: "hash-2" }), rawMarkdown: "# New", bodyMarkdown: "# New" }),
    );

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    expect(invokeMock).toHaveBeenCalledWith("save_wiki_page", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        relativePath: "wiki/concepts/transformer.md",
        contents: "# New",
        expectedHash: "hash-1",
      },
    });
    expect(useWikiStore.getState().mode).toBe("read");
    expect(useWikiStore.getState().saveState).toBe("saved");
    expect(useWikiStore.getState().page?.meta.hash).toBe("hash-2");
  });

  it("refreshes the tree page meta after a save so backlinks stay fresh", async () => {
    const oldMeta = pageMeta({ title: "Old Title", tags: ["old"] });
    useWikiStore.setState({
      page: pageContent({ meta: oldMeta }),
      draft: "# Edited",
      mode: "edit",
      tree: {
        root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
        pages: [oldMeta],
        totalPages: 1,
      },
    });
    invokeMock
      .mockResolvedValueOnce({
        relativePath: "wiki/concepts/transformer.md",
        hash: "hash-2",
        savedAt: "2024-01-02T00:00:00Z",
        graphCacheInvalidated: true,
      })
      .mockResolvedValueOnce(
        pageContent({
          meta: pageMeta({ hash: "hash-2", title: "New Title", tags: ["new"] }),
          rawMarkdown: "# Edited",
          bodyMarkdown: "# Edited",
        }),
      );

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    const saved = useWikiStore.getState().tree?.pages[0];
    expect(saved?.title).toBe("New Title");
    expect(saved?.tags).toEqual(["new"]);
    expect(saved?.hash).toBe("hash-2");
  });

  it("reports saved even when the post-save re-read fails", async () => {
    useWikiStore.setState({ page: pageContent(), draft: "# New", mode: "edit" });
    invokeMock
      .mockResolvedValueOnce({
        relativePath: "wiki/concepts/transformer.md",
        hash: "hash-2",
        savedAt: "2024-01-02T00:00:00Z",
        graphCacheInvalidated: true,
      })
      .mockRejectedValueOnce({ code: "FILE_NOT_FOUND", message: "gone" });

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    expect(useWikiStore.getState().saveState).toBe("saved");
  });

  it("does not clobber a page opened mid-save", async () => {
    useWikiStore.setState({ page: pageContent(), draft: "# New", mode: "edit" });
    const otherPage = pageContent({
      meta: pageMeta({ path: "wiki/concepts/attention.md", hash: "hash-att", title: "Attention" }),
      rawMarkdown: "# Attention",
      bodyMarkdown: "# Attention",
    });
    invokeMock
      .mockResolvedValueOnce({
        relativePath: "wiki/concepts/transformer.md",
        hash: "hash-2",
        savedAt: "2024-01-02T00:00:00Z",
        graphCacheInvalidated: true,
      })
      .mockImplementationOnce(async () => {
        // Simulate the user navigating away while the re-read is in flight.
        useWikiStore.setState({ selectedPath: "wiki/concepts/attention.md", page: otherPage });
        return pageContent({ meta: pageMeta({ hash: "hash-2" }) });
      });

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    expect(useWikiStore.getState().page?.meta.path).toBe("wiki/concepts/attention.md");
  });
});

describe("MarkdownReader", () => {
  it("renders an existing wikilink as clickable and invokes onOpenPage", async () => {
    const onOpenPage = vi.fn();
    const pages = [
      pageMeta({ path: "wiki/concepts/attention.md", title: "Attention" }),
    ];

    render(
      <MarkdownReader
        bodyMarkdown="See [[attention]] for details."
        frontmatterYaml={null}
        pages={pages}
        onOpenPage={onOpenPage}
      />,
    );

    const link = await screen.findByText("attention");
    expect(link.className).toContain("wikilink");
    expect(link.className).not.toContain("wikilink--missing");

    fireEvent.click(link);
    await waitFor(() => expect(onOpenPage).toHaveBeenCalledWith("wiki/concepts/attention.md"));
  });

  it("flags a wikilink with no matching page as missing", async () => {
    render(
      <MarkdownReader
        bodyMarkdown="Broken [[does-not-exist]] link."
        frontmatterYaml={null}
        pages={[]}
        onOpenPage={vi.fn()}
      />,
    );

    const link = await screen.findByText("does-not-exist");
    expect(link.className).toContain("wikilink--missing");
  });

  it("resolves wikilinks by alias and display label", async () => {
    const onOpenPage = vi.fn();
    const pages = [
      pageMeta({ path: "wiki/concepts/transformer.md", title: "Transformer", aliases: ["Transformers"] }),
    ];

    render(
      <MarkdownReader
        bodyMarkdown="The [[Transformers|TF architecture]] is key."
        frontmatterYaml={null}
        pages={pages}
        onOpenPage={onOpenPage}
      />,
    );

    const link = await screen.findByText("TF architecture");
    fireEvent.click(link);
    await waitFor(() => expect(onOpenPage).toHaveBeenCalledWith("wiki/concepts/transformer.md"));
  });
});

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
import { WikiEditor } from "./WikiEditor";
import { WikiTree as WikiTreeView } from "./WikiTree";
import { WikiPageFormDialog } from "./WikiPageFormDialog";
import { useWikiStore } from "./wikiStore";
import { invalidateProjectScope } from "../../stores/projectScope";

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
  it("ignores a page response that arrives after the project scope changed", async () => {
    let resolvePage!: (value: WikiPageContent) => void;
    invokeMock.mockReturnValueOnce(
      new Promise<WikiPageContent>((resolve) => {
        resolvePage = resolve;
      }),
    );

    const opening = useWikiStore.getState().openPage("project-a", "D:/a", "wiki/a.md");
    invalidateProjectScope();
    useWikiStore.getState().reset();
    resolvePage(pageContent());
    await opening;

    expect(useWikiStore.getState().page).toBeNull();
    expect(useWikiStore.getState().selectedPath).toBeNull();
  });

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

  it("tracks opened pages in recentPages (deduped, newest first, capped)", async () => {
    useWikiStore.setState({ recentPages: [] });
    invokeMock.mockResolvedValue(pageContent({ meta: pageMeta({ path: "wiki/a.md", title: "A" }) }));
    await useWikiStore.getState().openPage("proj-1", "D:/wiki", "wiki/a.md");
    invokeMock.mockResolvedValue(pageContent({ meta: pageMeta({ path: "wiki/b.md", title: "B" }) }));
    await useWikiStore.getState().openPage("proj-1", "D:/wiki", "wiki/b.md");
    // Reopening A promotes it to the front without duplicating.
    invokeMock.mockResolvedValue(pageContent({ meta: pageMeta({ path: "wiki/a.md", title: "A" }) }));
    await useWikiStore.getState().openPage("proj-1", "D:/wiki", "wiki/a.md");

    const recent = useWikiStore.getState().recentPages.map((entry) => entry.path);
    expect(recent).toEqual(["wiki/a.md", "wiki/b.md"]);
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

  it("creates a page through wiki-BE and opens the returned path", async () => {
    const created = {
      relativePath: "wiki/concepts/新页面.md",
      hash: "hash-new",
      savedAt: "2026-06-21T00:00:00Z",
      graphCacheInvalidated: false,
    };
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta({ path: created.relativePath, title: "新页面" })],
      totalPages: 1,
    };
    invokeMock
      .mockResolvedValueOnce(created)
      .mockResolvedValueOnce(tree)
      .mockResolvedValue(pageContent({ meta: pageMeta({ path: created.relativePath, title: "新页面" }) }));

    await useWikiStore.getState().createPage("proj-1", "D:/wiki", {
      relativePath: created.relativePath,
      title: "新页面",
      pageType: "concept",
    });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "create_wiki_page", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        relativePath: created.relativePath,
        title: "新页面",
        pageType: "concept",
      },
    });
    expect(useWikiStore.getState().selectedPath).toBe(created.relativePath);
  });

  it("renames a page through wiki-BE and follows the new path", async () => {
    const renamedPath = "wiki/concepts/reasoning.md";
    invokeMock
      .mockResolvedValueOnce({
        relativePath: renamedPath,
        hash: "hash-2",
        savedAt: "2026-06-21T00:00:00Z",
        updatedReferences: ["wiki/index.md"],
        graphCacheInvalidated: true,
      })
      .mockResolvedValueOnce({
        root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
        pages: [pageMeta({ path: renamedPath })],
        totalPages: 1,
      })
      .mockResolvedValue(pageContent({ meta: pageMeta({ path: renamedPath }) }));

    await useWikiStore.getState().renamePage(
      "proj-1",
      "D:/wiki",
      "wiki/concepts/transformer.md",
      renamedPath,
    );

    expect(invokeMock).toHaveBeenNthCalledWith(1, "rename_wiki_page", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        relativePath: "wiki/concepts/transformer.md",
        newRelativePath: renamedPath,
      },
    });
    expect(useWikiStore.getState().selectedPath).toBe(renamedPath);
  });

  it("requests deletion and confirms the backend PendingAction", async () => {
    const action = {
      id: "delete-1",
      actionType: "delete_file" as const,
      title: "Delete wiki page",
      message: "Delete after checkpoint",
      riskLevel: "destructive" as const,
      affectedPaths: ["wiki/concepts/transformer.md"],
      preview: null,
      expiresAt: null,
      checkpointHash: null,
    };
    invokeMock
      .mockResolvedValueOnce(action)
      .mockResolvedValueOnce({ action, status: "confirmed", checkpointExists: true, projectSummary: null })
      .mockResolvedValueOnce({
        root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 0, children: [] },
        pages: [],
        totalPages: 0,
      });

    const pending = await useWikiStore.getState().requestDeletePage(
      "proj-1",
      "D:/wiki",
      "wiki/concepts/transformer.md",
    );
    await useWikiStore.getState().confirmDeletePage("proj-1", "D:/wiki", pending!);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "request_delete_wiki_page", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        relativePath: "wiki/concepts/transformer.md",
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "confirm_pending_action", {
      request: { actionId: "delete-1", status: "confirmed" },
    });
  });
});

describe("MarkdownReader", () => {
  it("renders frontmatter as ordered key-value rows instead of raw YAML", () => {
    const { container } = render(
      <MarkdownReader
        bodyMarkdown="# Memory"
        frontmatterYaml={[
          "type: concept",
          "tags: [memory, context]",
          "schema_version: 3",
        ].join("\n")}
        pages={[]}
        onOpenPage={vi.fn()}
      />,
    );

    const card = container.querySelector(".frontmatter");
    expect(card).not.toBeNull();
    expect(card?.querySelectorAll(".frontmatter__row")).toHaveLength(3);
    expect(card?.textContent).toContain("type:");
    expect(card?.textContent).toContain("[memory, context]");
    expect(container.querySelector(".wiki-frontmatter")).toBeNull();
  });

  it("keeps unknown and continuation frontmatter content readable", () => {
    const { container } = render(
      <MarkdownReader
        bodyMarkdown="Body"
        frontmatterYaml={"custom_field:\n  nested: value\nmalformed line"}
        pages={[]}
        onOpenPage={vi.fn()}
      />,
    );

    const rows = container.querySelectorAll(".frontmatter__row");
    expect(rows).toHaveLength(2);
    expect(rows[0]?.textContent).toContain("nested: value");
    expect(rows[1]?.textContent).toContain("malformed line");
  });

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

describe("WikiTree page lifecycle actions", () => {
  it("exposes create, rename, and delete entry points", () => {
    const onCreate = vi.fn();
    const onRename = vi.fn();
    const onDelete = vi.fn();
    const page = pageMeta();
    const root = {
      name: "wiki",
      kind: "folder" as const,
      path: "wiki",
      starred: false,
      bookmarked: false,
      fileCount: 1,
      children: [{
        name: "transformer.md",
        kind: "file" as const,
        path: page.path,
        starred: false,
        bookmarked: false,
        fileCount: 1,
        children: [],
      }],
    };

    render(
      <WikiTreeView
        root={root}
        pages={[page]}
        selectedPath={page.path}
        onSelect={vi.fn()}
        onRefresh={vi.fn()}
        onCreate={onCreate}
        onRename={onRename}
        onDelete={onDelete}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New page" }));
    expect(onCreate).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Page actions: transformer.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(onRename).toHaveBeenCalledWith(page.path);

    fireEvent.click(screen.getByRole("button", { name: "Page actions: transformer.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onDelete).toHaveBeenCalledWith(page.path);
  });
});

describe("WikiPageFormDialog", () => {
  it("submits a normalized create-page payload", () => {
    const onSubmit = vi.fn();
    render(
      <WikiPageFormDialog
        mode="create"
        initialPath="wiki/concepts/"
        onCancel={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    fireEvent.change(screen.getByLabelText("Page path"), {
      target: { value: "wiki/concepts/agent-memory" },
    });
    fireEvent.change(screen.getByLabelText("Title"), {
      target: { value: "Agent Memory" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create page" }));

    expect(onSubmit).toHaveBeenCalledWith({
      relativePath: "wiki/concepts/agent-memory.md",
      title: "Agent Memory",
      pageType: "concept",
    });
  });
});

describe("WikiEditor (Milkdown)", () => {
  it("renders the complete Milkdown formatting toolbar", () => {
    render(
      <WikiEditor
        draft="Select me"
        saveState="idle"
        onDraftChange={vi.fn()}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        onReload={vi.fn()}
      />,
    );

    for (const name of [
      "Bold",
      "Italic",
      "Heading",
      "Link",
      "Inline code",
      "Blockquote",
      "Undo",
      "Redo",
    ]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
  });

  it("mounts the WYSIWYG surface and renders the save toolbar", async () => {
    const onDraftChange = vi.fn();
    const onSave = vi.fn();
    render(
      <WikiEditor
        draft={"# Hello\n\nworld"}
        saveState="idle"
        onDraftChange={onDraftChange}
        onSave={onSave}
        onCancel={vi.fn()}
        onReload={vi.fn()}
      />,
    );

    // The Milkdown container mounts a ProseMirror editor under .milkdown.
    // jsdom cannot fully boot ProseMirror, so we assert the stable toolbar
    // surface + the editor mount point render without throwing.
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
    const mount = document.querySelector(".wiki-editor");
    expect(mount).not.toBeNull();
  });
});

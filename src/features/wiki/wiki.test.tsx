import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import type {
  SaveWikiPageResponse,
  WikiPageContent,
  WikiPageMeta,
  WikiTree,
  WikiTreeNode,
} from "../../types/wiki";
import { SINGLE_PAGE_EXPORT_TYPES, type ExportRecord } from "../../types/export";
import type { BackendTask } from "../../types/task";
import { RightContextPanel } from "../../components/app/RightContextPanel";
import { MarkdownReader } from "./MarkdownReader";
import { WikiEditor } from "./WikiEditor";
import { WikiTree as WikiTreeView } from "./WikiTree";
import { WikiPageFormDialog } from "./WikiPageFormDialog";
import { ConflictDiffDialog } from "./ConflictDiffDialog";
import { GenerateHtmlDialog } from "./GenerateHtmlDialog";
import { HtmlPreviewPane as WikiHtmlPreviewPane } from "./HtmlPreviewPane";
import { RelatedPagesPanel } from "./RelatedPagesPanel";
import { selectWikiPreviewRecord, WikiView } from "./WikiView";
import { updateTreeNodeBookmark, useWikiStore } from "./wikiStore";
import { invalidateProjectScope } from "../../stores/projectScope";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useExportStore } from "../../stores/exportStore";
import { useTaskStore } from "../../stores/taskStore";
import { useUpdateStore } from "../../stores/updateStore";

const invokeMock = vi.hoisted(() => vi.fn());
const emptyAiCapabilities = { agents: [], providers: [] };

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

function exportTask(overrides: Partial<BackendTask> = {}): BackendTask {
  return {
    id: "task-export",
    taskType: "export",
    projectId: "proj-1",
    title: "Export Transformer",
    status: "running",
    progress: { current: 0, total: 1, label: null },
    startedAt: "2026-08-13T10:00:00Z",
    updatedAt: "2026-08-13T10:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, reject, resolve };
}

beforeEach(() => {
  invokeMock.mockReset();
  useWikiStore.getState().reset();
  useExportStore.getState().reset();
  useUpdateStore.getState().resetForTests();
  useTaskStore.setState({
    activeProjectId: null,
    activeProjectRootPath: null,
    taskFacts: {},
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
    tasksHydrated: false,
    projectPersistence: null,
    projectPersistenceReason: null,
  });
  useNavigationStore.setState({
    activeView: "dashboard",
    rightPanelOpen: true,
    rightPanelMode: "default",
    wikiAssistantPagePath: null,
  });
  void i18next.changeLanguage("en");
});

afterEach(() => {
  cleanup();
});

describe("wikiStore", () => {
  it("updates bookmark state in flat pages and recursive tree nodes", async () => {
    const page = pageMeta({ bookmarked: false });
    const tree: WikiTree = {
      root: {
        name: "wiki",
        kind: "folder",
        path: "wiki",
        starred: false,
        bookmarked: false,
        fileCount: 1,
        children: [{
          name: "concepts",
          kind: "folder",
          path: "wiki/concepts",
          starred: false,
          bookmarked: false,
          fileCount: 1,
          children: [{
            name: "transformer.md",
            kind: "file",
            path: page.path,
            starred: false,
            bookmarked: false,
            fileCount: 1,
            children: [],
          }],
        }],
      },
      pages: [page],
      totalPages: 1,
    };
    useWikiStore.setState({
      page: pageContent({ meta: page }),
      tree,
      selectedPath: page.path,
    });
    invokeMock.mockResolvedValueOnce({ relativePath: page.path, bookmarked: true });

    await useWikiStore.getState().toggleBookmark("proj-1", "D:/wiki");

    expect(useWikiStore.getState().tree?.pages[0]?.bookmarked).toBe(true);
    expect(
      useWikiStore.getState().tree?.root.children[0]?.children[0]?.bookmarked,
    ).toBe(true);
  });

  it("does not apply a delayed bookmark response to a newly opened page", async () => {
    const firstPage = pageMeta({ path: "wiki/concepts/first.md", title: "First", bookmarked: false });
    const secondPage = pageMeta({ path: "wiki/concepts/second.md", title: "Second", bookmarked: false });
    const tree: WikiTree = {
      root: {
        name: "wiki",
        kind: "folder",
        path: "wiki",
        starred: false,
        bookmarked: false,
        fileCount: 2,
        children: [],
      },
      pages: [firstPage, secondPage],
      totalPages: 2,
    };
    const toggleResponse = deferred<{ relativePath: string; bookmarked: boolean }>();
    invokeMock.mockReturnValueOnce(toggleResponse.promise);
    useWikiStore.setState({
      page: pageContent({ meta: firstPage }),
      tree,
      selectedPath: firstPage.path,
    });

    const toggling = useWikiStore.getState().toggleBookmark("proj-1", "D:/wiki");
    useWikiStore.setState({
      page: pageContent({ meta: secondPage }),
      selectedPath: secondPage.path,
    });
    toggleResponse.resolve({ relativePath: firstPage.path, bookmarked: true });
    await toggling;

    expect(useWikiStore.getState().page?.meta.path).toBe(secondPage.path);
    expect(useWikiStore.getState().page?.meta.bookmarked).toBe(false);
    expect(
      useWikiStore.getState().tree?.pages.find((page) => page.path === firstPage.path)?.bookmarked,
    ).toBe(true);
    expect(
      useWikiStore.getState().tree?.pages.find((page) => page.path === secondPage.path)?.bookmarked,
    ).toBe(false);
  });

  it("updates a matching nested tree node bookmark flag", () => {
    const root: WikiTreeNode = {
      name: "wiki",
      kind: "folder",
      path: "wiki",
      starred: false,
      bookmarked: false,
      fileCount: 1,
      children: [{
        name: "concepts",
        kind: "folder",
        path: "wiki/concepts",
        starred: false,
        bookmarked: false,
        fileCount: 1,
        children: [{
          name: "transformer.md",
          kind: "file",
          path: "wiki/concepts/transformer.md",
          starred: false,
          bookmarked: false,
          fileCount: 1,
          children: [],
        }],
      }],
    };

    const next = updateTreeNodeBookmark(root, "wiki/concepts/transformer.md", true);

    expect(next.children[0]?.children[0]?.bookmarked).toBe(true);
  });

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

  it("keeps the latest page when same-project responses arrive out of order", async () => {
    const first = deferred<WikiPageContent>();
    const second = deferred<WikiPageContent>();
    invokeMock
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const openingFirst = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/a.md");
    const openingSecond = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/b.md");
    const secondPage = pageContent({
      meta: pageMeta({ path: "wiki/b.md", title: "B" }),
      rawMarkdown: "# B",
      bodyMarkdown: "# B",
    });
    second.resolve(secondPage);
    await openingSecond;

    first.resolve(
      pageContent({
        meta: pageMeta({ path: "wiki/a.md", title: "A" }),
        rawMarkdown: "# A",
        bodyMarkdown: "# A",
      }),
    );
    await openingFirst;

    const state = useWikiStore.getState();
    expect(state.selectedPath).toBe("wiki/b.md");
    expect(state.page).toEqual(secondPage);
    expect(state.draft).toBe("# B");
    expect(state.loadingPage).toBe(false);
    expect(state.error).toBeNull();
  });

  it("ignores stale same-project failures without clearing the latest page state", async () => {
    const first = deferred<WikiPageContent>();
    const second = deferred<WikiPageContent>();
    invokeMock
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const openingFirst = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/a.md");
    const openingSecond = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/b.md");
    const secondPage = pageContent({
      meta: pageMeta({ path: "wiki/b.md", title: "B" }),
      rawMarkdown: "# B",
      bodyMarkdown: "# B",
    });
    second.resolve(secondPage);
    await openingSecond;
    first.reject(new Error("stale A failed"));
    await openingFirst;

    const state = useWikiStore.getState();
    expect(state.selectedPath).toBe("wiki/b.md");
    expect(state.page).toEqual(secondPage);
    expect(state.loadingPage).toBe(false);
    expect(state.error).toBeNull();
  });

  it("distinguishes repeated paths in an A-B-A request sequence", async () => {
    const firstA = deferred<WikiPageContent>();
    const middleB = deferred<WikiPageContent>();
    const finalA = deferred<WikiPageContent>();
    invokeMock
      .mockReturnValueOnce(firstA.promise)
      .mockReturnValueOnce(middleB.promise)
      .mockReturnValueOnce(finalA.promise);

    const openingFirstA = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/a.md");
    const openingB = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/b.md");
    const openingFinalA = useWikiStore
      .getState()
      .openPage("proj-1", "D:/wiki", "wiki/a.md");
    const latestPage = pageContent({
      meta: pageMeta({ path: "wiki/a.md", title: "Latest A", hash: "hash-latest" }),
      rawMarkdown: "# Latest A",
      bodyMarkdown: "# Latest A",
    });
    finalA.resolve(latestPage);
    await openingFinalA;
    middleB.resolve(
      pageContent({ meta: pageMeta({ path: "wiki/b.md", title: "B" }) }),
    );
    firstA.resolve(
      pageContent({ meta: pageMeta({ path: "wiki/a.md", title: "Stale A" }) }),
    );
    await Promise.all([openingFirstA, openingB]);

    const state = useWikiStore.getState();
    expect(state.page).toEqual(latestPage);
    expect(state.draft).toBe("# Latest A");
    expect(state.recentPages.map((entry) => entry.title)).toEqual(["Latest A"]);
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
    invokeMock.mockRejectedValueOnce({
      code: "FILE_HASH_MISMATCH",
      message: "changed",
      details: {
        baselineContent: "# External edit",
        currentHash: "hash-external",
      },
    });

    await useWikiStore.getState().save("proj-1", "D:/wiki");

    expect(useWikiStore.getState().saveState).toBe("conflict");
    expect(useWikiStore.getState().conflict).toEqual({
      path: "wiki/concepts/transformer.md",
      originalContent: "# Transformer\n\nSee [[attention]].",
      currentContent: "# External edit",
      incomingContent: "# Edited",
      currentHash: "hash-external",
    });
  });

  it("resolves a conflict with the current backend hash", async () => {
    useWikiStore.setState({
      page: pageContent(),
      draft: "# Incoming",
      mode: "edit",
      saveState: "conflict",
      conflict: {
        path: "wiki/concepts/transformer.md",
        originalContent: "# Original",
        currentContent: "# External",
        incomingContent: "# Incoming",
        currentHash: "hash-external",
      },
    });
    invokeMock
      .mockResolvedValueOnce({ created: true, commitHash: "checkpoint-1", message: "Before conflict merge", purpose: "high_risk_operation", affectedPaths: ["wiki/concepts/transformer.md"] })
      .mockResolvedValueOnce({ relativePath: "wiki/concepts/transformer.md", hash: "hash-3", savedAt: "2026-06-21", graphCacheInvalidated: true })
      .mockResolvedValueOnce(pageContent({ rawMarkdown: "# Incoming", bodyMarkdown: "# Incoming", meta: pageMeta({ hash: "hash-3" }) }));

    await useWikiStore.getState().resolveConflict("proj-1", "D:/wiki", "use_incoming");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "create_git_checkpoint", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        purpose: "high_risk_operation",
        message: "Before resolving wiki conflict: wiki/concepts/transformer.md",
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "save_wiki_page", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        relativePath: "wiki/concepts/transformer.md",
        contents: "# Incoming",
        expectedHash: "hash-external",
      },
    });
    expect(useWikiStore.getState().saveState).toBe("saved");
    expect(useWikiStore.getState().conflict).toBeNull();
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

  it("resolves imported local images through the project-scoped backend command", async () => {
    invokeMock.mockResolvedValueOnce({
      contentType: "image/jpeg",
      bytes: [255, 216, 255],
    });

    render(
      <MarkdownReader
        bodyMarkdown="![cover](assets/cover.jpg)"
        frontmatterYaml={null}
        pages={[]}
        onOpenPage={vi.fn()}
        projectId="proj-1"
        projectRootPath="D:/wiki"
        pagePath="wiki/sources/web/article.md"
      />,
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("read_wiki_asset", {
        request: {
          projectId: "proj-1",
          projectRootPath: "D:/wiki",
          pagePath: "wiki/sources/web/article.md",
          assetPath: "assets/cover.jpg",
        },
      }),
    );
  });

  it("renders remote images immediately with native lazy loading and no IPC", () => {
    render(
      <MarkdownReader
        bodyMarkdown="![remote cover](https://example.com/cover.jpg)"
        frontmatterYaml={null}
        pages={[]}
        onOpenPage={vi.fn()}
      />,
    );

    const image = screen.getByRole("img", { name: "remote cover" });
    expect(image).toHaveAttribute("src", "https://example.com/cover.jpg");
    expect(image).toHaveAttribute("loading", "lazy");
    expect(image).toHaveAttribute("decoding", "async");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("defers local asset IPC until the image is near the viewport", async () => {
    let reveal!: () => void;
    class MockIntersectionObserver implements IntersectionObserver {
      readonly root = null;
      readonly rootMargin = "600px";
      readonly thresholds = [0];
      readonly disconnect = vi.fn();
      readonly observe = vi.fn();
      readonly unobserve = vi.fn();

      constructor(callback: IntersectionObserverCallback) {
        reveal = () =>
          callback(
            [{ isIntersecting: true } as IntersectionObserverEntry],
            this,
          );
      }

      takeRecords(): IntersectionObserverEntry[] {
        return [];
      }
    }
    vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
    invokeMock.mockResolvedValue({
      contentType: "image/png",
      bytes: [137, 80, 78, 71],
    });

    try {
      render(
        <MarkdownReader
          bodyMarkdown="![cover](assets/cover.png)"
          frontmatterYaml={null}
          pages={[]}
          onOpenPage={vi.fn()}
          projectId="proj-1"
          projectRootPath="D:/wiki"
          pagePath="wiki/sources/local/article.md"
        />,
      );

      expect(invokeMock).not.toHaveBeenCalled();
      act(() => reveal());
      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("read_wiki_asset", {
          request: {
            projectId: "proj-1",
            projectRootPath: "D:/wiki",
            pagePath: "wiki/sources/local/article.md",
            assetPath: "assets/cover.png",
          },
        }),
      );
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("deduplicates a local image request across StrictMode effect replay", async () => {
    invokeMock.mockResolvedValue({
      contentType: "image/png",
      bytes: [137, 80, 78, 71],
    });
    const onOpenPage = vi.fn();
    const pages: WikiPageMeta[] = [];
    const reader = () => (
      <StrictMode>
        <MarkdownReader
          bodyMarkdown="![cover](assets/cover.png)"
          frontmatterYaml={null}
          pages={pages}
          onOpenPage={onOpenPage}
          projectId="proj-1"
          projectRootPath="D:/wiki"
          pagePath="wiki/sources/local/article.md"
        />
      </StrictMode>
    );

    const { rerender } = render(reader());

    await waitFor(() => {
      const calls = invokeMock.mock.calls.filter(
        ([command]) => command === "read_wiki_asset",
      );
      expect(calls).toHaveLength(1);
    });
    await waitFor(() =>
      expect(screen.getByRole("img", { name: "cover" })).toHaveAttribute(
        "data-wiki-asset-state",
        "ready",
      ),
    );

    rerender(reader());
    await act(async () => undefined);
    const calls = invokeMock.mock.calls.filter(
      ([command]) => command === "read_wiki_asset",
    );
    expect(calls).toHaveLength(1);
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

  it("renders numbered citations as linked circular references", () => {
    const { container } = render(
      <MarkdownReader
        bodyMarkdown="Evidence [1] and [^2]."
        frontmatterYaml={null}
        pages={[]}
        onOpenPage={vi.fn()}
      />,
    );

    const citations = container.querySelectorAll(".citation-ref");
    expect(citations).toHaveLength(2);
    expect(citations[0]).toHaveAttribute("href", "#citation-1");
  });

  it("does not rewrite Markdown reference definitions as citations", () => {
    const { container } = render(
      <MarkdownReader
        bodyMarkdown={"See [source][1].\n\n[1]: https://example.com"}
        frontmatterYaml={null}
        pages={[]}
        onOpenPage={vi.fn()}
      />,
    );

    expect(container.querySelectorAll(".citation-ref")).toHaveLength(0);
    expect(screen.getByRole("link", { name: "source" })).toHaveAttribute(
      "href",
      "https://example.com",
    );
  });
});

describe("WikiTree page lifecycle actions", () => {
  it("renders a star for bookmarked file nodes", () => {
    const page = pageMeta({ bookmarked: true, starred: false });
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
        bookmarked: true,
        fileCount: 1,
        children: [],
      }],
    };

    const { container } = render(
      <WikiTreeView
        root={root}
        pages={[page]}
        selectedPath={page.path}
        onSelect={vi.fn()}
        onRefresh={vi.fn()}
        onCreate={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(container.querySelector(".lucide-star")).not.toBeNull();
  });

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

  it("infers irregular page-type folders without truncating their names", () => {
    const onSubmit = vi.fn();
    render(
      <WikiPageFormDialog
        mode="create"
        initialPath="wiki/entities/"
        onCancel={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    fireEvent.change(screen.getByLabelText("Page path"), {
      target: { value: "wiki/entities/张三" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create page" }));

    expect(onSubmit).toHaveBeenCalledWith({
      relativePath: "wiki/entities/张三.md",
      title: null,
      pageType: "entity",
    });
  });
});

describe("ConflictDiffDialog", () => {
  it("shows all three versions and exposes the three resolution choices", () => {
    render(
      <ConflictDiffDialog
        conflict={{
          path: "wiki/concepts/agent.md",
          originalContent: "baseline text",
          currentContent: "external text",
          incomingContent: "agent text",
          currentHash: "hash-current",
        }}
        onKeepCurrent={vi.fn()}
        onUseIncoming={vi.fn()}
        onManualMerge={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("baseline text")).toBeInTheDocument();
    expect(screen.getByText("external text")).toBeInTheDocument();
    expect(screen.getByText("agent text")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep current" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use incoming" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Manual merge" })).toBeInTheDocument();
  });
});

describe("Wiki HTML preview", () => {
  it("keeps the Wiki toolbar quick export on Wiki instead of launching Workflows", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useNavigationStore.setState({ activeView: "wiki", workflowLaunchIntent: null });
    const requestWorkflowLaunch = vi
      .spyOn(useNavigationStore.getState(), "requestWorkflowLaunch")
      .mockImplementation(() => undefined);

    try {
      render(<WikiView capabilities={emptyAiCapabilities} />);
      fireEvent.click(await screen.findByRole("button", { name: "Generate HTML" }));

      expect(requestWorkflowLaunch).not.toHaveBeenCalled();
      expect(useNavigationStore.getState().activeView).toBe("wiki");
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    } finally {
      requestWorkflowLaunch.mockRestore();
    }
  });

  it("starts a direct single-page export with default options and opens its task", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const task = exportTask();
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "start_export") return Promise.resolve(task);
      if (command === "get_task") return Promise.resolve(task);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useTaskStore.setState({
      activeProjectId: "proj-1",
      activeProjectRootPath: "D:/wiki",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));

    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([command]) => command === "start_export")).toHaveLength(1),
    );
    const startCall = invokeMock.mock.calls.find(([command]) => command === "start_export");
    expect(startCall?.[1]).toEqual({
      request: expect.objectContaining({
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        exportType: "beautiful_read",
        sourcePath: "wiki/concepts/transformer.md",
        route: "auto",
        options: {
          includeFrontmatter: true,
          embedCss: true,
          embedImages: false,
        },
        acknowledgeRestrictedContent: false,
      }),
    });
    expect(useTaskStore.getState()).toMatchObject({
      drawerOpen: true,
      selectedTaskId: task.id,
    });
    expect(useWikiStore.getState().mode).toBe("preview");
  });

  it.each(["page", "project"] as const)(
    "keeps a deferred start task fact without stale %s presentation",
    async (switchKind) => {
      const firstPage = pageMeta();
      const secondPage: WikiPageMeta = {
        ...firstPage,
        path: "wiki/concepts/attention.md",
        title: "Attention",
      };
      const tree: WikiTree = {
        root: {
          name: "wiki",
          kind: "folder",
          path: "wiki",
          starred: false,
          bookmarked: false,
          fileCount: 2,
          children: [],
        },
        pages: [firstPage, secondPage],
        totalPages: 2,
      };
      const task = exportTask({ id: `task-deferred-${switchKind}` });
      const startResponse = deferred<BackendTask>();
      invokeMock.mockImplementation((command: string) => {
        if (command === "scan_wiki") return Promise.resolve(tree);
        if (command === "read_wiki_page") return Promise.resolve(pageContent());
        if (command === "list_exports") return Promise.resolve([]);
        if (command === "get_export_restricted_content_status") {
          return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
        }
        if (command === "start_export") return startResponse.promise;
        if (command === "get_task") return Promise.resolve(task);
        return Promise.resolve(null);
      });
      useProjectStore.getState().setCurrentProject({
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      });
      useTaskStore.setState({
        activeProjectId: "proj-1",
        activeProjectRootPath: "D:/wiki",
      });
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        value: {},
        configurable: true,
      });

      render(<WikiView capabilities={emptyAiCapabilities} />);
      await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
      fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
      fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));
      await waitFor(() =>
        expect(
          invokeMock.mock.calls.filter(([command]) => command === "start_export"),
        ).toHaveLength(1),
      );
      expect(screen.getByRole("dialog")).toHaveAttribute("aria-busy", "true");
      expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "Generate and preview" })).toBeDisabled();

      act(() => {
        if (switchKind === "page") {
          useWikiStore.setState({
            selectedPath: secondPage.path,
            page: pageContent({ meta: secondPage }),
            mode: "read",
          });
        } else {
          useProjectStore.getState().setCurrentProject({
            ...defaultProject,
            projectId: "proj-2",
            rootPath: "D:/wiki-two",
            name: "Wiki Two",
          });
          useTaskStore.setState({
            activeProjectId: "proj-2",
            activeProjectRootPath: "D:/wiki-two",
            tasks: [],
            drawerOpen: false,
            selectedTaskId: null,
          });
        }
        startResponse.resolve(task);
      });

      await waitFor(() =>
        expect(useTaskStore.getState().taskFacts[task.id]).toMatchObject({ id: task.id }),
      );
      expect(useTaskStore.getState()).toMatchObject({
        drawerOpen: false,
        selectedTaskId: null,
      });
      expect(useExportStore.getState().runningTaskId).toBeNull();
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(
        invokeMock.mock.calls.filter(([command]) => command === "read_export_preview"),
      ).toHaveLength(0);
      if (switchKind === "page") {
        expect(useTaskStore.getState().tasks.some((candidate) => candidate.id === task.id)).toBe(true);
        expect(useWikiStore.getState().selectedPath).toBe(secondPage.path);
      } else {
        expect(useTaskStore.getState()).toMatchObject({ tasks: [], runningCount: 0 });
        expect(useProjectStore.getState().currentProject.projectId).toBe("proj-2");
      }
    },
  );

  it.each(["failed", "cancelled"] as const)(
    "cleans a %s quick export before a delayed record refresh resolves",
    async (terminalStatus) => {
      const tree: WikiTree = {
        root: {
          name: "wiki",
          kind: "folder",
          path: "wiki",
          starred: false,
          bookmarked: false,
          fileCount: 1,
          children: [],
        },
        pages: [pageMeta()],
        totalPages: 1,
      };
      const task = exportTask({ id: `task-${terminalStatus}` });
      const terminalRefresh = deferred<ExportRecord[]>();
      let listExportsCalls = 0;
      invokeMock.mockImplementation((command: string) => {
        if (command === "scan_wiki") return Promise.resolve(tree);
        if (command === "read_wiki_page") return Promise.resolve(pageContent());
        if (command === "list_exports") {
          listExportsCalls += 1;
          return listExportsCalls === 1 ? Promise.resolve([]) : terminalRefresh.promise;
        }
        if (command === "get_export_restricted_content_status") {
          return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
        }
        if (command === "start_export") return Promise.resolve(task);
        if (command === "get_task") return Promise.resolve(task);
        return Promise.resolve(null);
      });
      useProjectStore.getState().setCurrentProject({
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      });
      useTaskStore.setState({
        activeProjectId: "proj-1",
        activeProjectRootPath: "D:/wiki",
      });
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        value: {},
        configurable: true,
      });

      render(<WikiView capabilities={emptyAiCapabilities} />);
      await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
      fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
      fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));
      await waitFor(() => expect(useWikiStore.getState().mode).toBe("preview"));

      act(() => {
        useTaskStore.setState({
          tasks: [
            exportTask({
              id: task.id,
              status: terminalStatus,
              updatedAt: "2026-08-13T10:05:00Z",
              completedAt: "2026-08-13T10:05:00Z",
            }),
          ],
        });
      });

      await waitFor(() => expect(useExportStore.getState().runningTaskId).toBeNull());
      expect(useWikiStore.getState().mode).toBe("read");
      expect(screen.queryByTitle("HTML preview")).not.toBeInTheDocument();
      expect(listExportsCalls).toBe(2);

      act(() => terminalRefresh.resolve([]));
    },
  );

  it("previews the ExportRecord produced by the terminal task instead of the newest same-page export", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const task = exportTask();
    const matchingRecord: ExportRecord = {
      id: "export-for-task",
      exportType: "beautiful_read",
      title: "Transformer from task",
      sourcePath: pageMeta().path,
      outputPath: "exports/html/transformer-task.html",
      createdAt: "2026-08-13T09:00:00Z",
      route: "byok",
      status: "succeeded",
      bookmarked: false,
      taskId: task.id,
    };
    const newerUnrelatedRecord: ExportRecord = {
      ...matchingRecord,
      id: "newer-unrelated-export",
      title: "Transformer from another task",
      outputPath: "exports/html/transformer-other.html",
      createdAt: "2026-08-13T11:00:00Z",
      taskId: "another-task",
    };
    let listExportsCalls = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") {
        listExportsCalls += 1;
        return Promise.resolve(listExportsCalls === 1 ? [] : [newerUnrelatedRecord, matchingRecord]);
      }
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "start_export") return Promise.resolve(task);
      if (command === "get_task") return Promise.resolve(task);
      if (command === "read_export_preview") return Promise.resolve("<h1>Transformer from task</h1>");
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useTaskStore.setState({
      activeProjectId: "proj-1",
      activeProjectRootPath: "D:/wiki",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));
    await waitFor(() => expect(useExportStore.getState().runningTaskId).toBe(task.id));

    act(() => {
      useTaskStore.setState({
        tasks: [
          exportTask({
            status: "succeeded",
            updatedAt: "2026-08-13T10:05:00Z",
            completedAt: "2026-08-13T10:05:00Z",
          }),
        ],
      });
    });

    await waitFor(() => expect(useExportStore.getState().previewId).toBe(matchingRecord.id));
    expect(useExportStore.getState().records).toEqual([newerUnrelatedRecord, matchingRecord]);
    expect(invokeMock).toHaveBeenCalledWith("read_export_preview", {
      request: {
        projectId: "proj-1",
        projectRootPath: "D:/wiki",
        outputPath: matchingRecord.outputPath,
      },
    });
  });

  it("does not reload or preview twice when the same terminal task event arrives twice", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const task = exportTask();
    const record: ExportRecord = {
      id: "export-once",
      exportType: "beautiful_read",
      title: "Transformer",
      sourcePath: pageMeta().path,
      outputPath: "exports/html/transformer-once.html",
      createdAt: "2026-08-13T10:00:00Z",
      route: "byok",
      status: "succeeded",
      bookmarked: false,
      taskId: task.id,
    };
    const refresh = deferred<ExportRecord[]>();
    let listExportsCalls = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") {
        listExportsCalls += 1;
        return listExportsCalls === 1 ? Promise.resolve([]) : refresh.promise;
      }
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "start_export") return Promise.resolve(task);
      if (command === "get_task") return Promise.resolve(task);
      if (command === "read_export_preview") return Promise.resolve("<h1>Once</h1>");
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useTaskStore.setState({
      activeProjectId: "proj-1",
      activeProjectRootPath: "D:/wiki",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    await waitFor(() => expect(listExportsCalls).toBe(1));
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));
    await waitFor(() => expect(useExportStore.getState().runningTaskId).toBe(task.id));

    const terminalTask = exportTask({
      status: "succeeded",
      updatedAt: "2026-08-13T10:05:00Z",
      completedAt: "2026-08-13T10:05:00Z",
    });
    act(() => useTaskStore.setState({ tasks: [terminalTask] }));
    await waitFor(() => expect(listExportsCalls).toBe(2));

    act(() => {
      useTaskStore.setState({
        tasks: [
          exportTask({
            status: "succeeded",
            updatedAt: "2026-08-13T10:06:00Z",
            completedAt: "2026-08-13T10:05:00Z",
          }),
        ],
      });
    });
    expect(listExportsCalls).toBe(2);

    refresh.resolve([record]);
    await waitFor(() => expect(useExportStore.getState().previewId).toBe(record.id));
    expect(listExportsCalls).toBe(2);
    expect(invokeMock.mock.calls.filter(([command]) => command === "read_export_preview")).toHaveLength(1);
  });

  it("keeps a terminal quick export from taking over after the Wiki page changes", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const task = exportTask();
    let listExportsCalls = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") {
        listExportsCalls += 1;
        return Promise.resolve([]);
      }
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "start_export") return Promise.resolve(task);
      if (command === "get_task") return Promise.resolve(task);
      if (command === "read_export_preview") return Promise.resolve("<h1>Should not load</h1>");
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useTaskStore.setState({
      activeProjectId: "proj-1",
      activeProjectRootPath: "D:/wiki",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));
    await waitFor(() => expect(useExportStore.getState().runningTaskId).toBe(task.id));

    act(() => {
      useWikiStore.setState({
        page: pageContent({ meta: pageMeta({ path: "wiki/other.md", title: "Other" }) }),
        selectedPath: "wiki/other.md",
        mode: "read",
      });
    });
    await waitFor(() => expect(useExportStore.getState().runningTaskId).toBeNull());

    act(() => {
      useTaskStore.setState({
        tasks: [
          exportTask({
            status: "succeeded",
            updatedAt: "2026-08-13T10:05:00Z",
            completedAt: "2026-08-13T10:05:00Z",
          }),
        ],
      });
    });
    await waitFor(() => expect(useTaskStore.getState().tasks[0]?.status).toBe("succeeded"));
    expect(listExportsCalls).toBe(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "read_export_preview")).toHaveLength(0);
    expect(useWikiStore.getState().mode).toBe("read");
  });

  it("does not let a terminal quick export from the old project preview in the new project", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const task = exportTask();
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "start_export") return Promise.resolve(task);
      if (command === "get_task") return Promise.resolve(task);
      if (command === "read_export_preview") return Promise.resolve("<h1>Should not load</h1>");
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useTaskStore.setState({
      activeProjectId: "proj-1",
      activeProjectRootPath: "D:/wiki",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));
    await waitFor(() => expect(useExportStore.getState().runningTaskId).toBe(task.id));

    act(() => {
      useProjectStore.getState().setCurrentProject({
        ...defaultProject,
        projectId: "proj-2",
        rootPath: "D:/other-wiki",
        name: "Other Wiki",
      });
      useTaskStore.setState({
        activeProjectId: "proj-2",
        activeProjectRootPath: "D:/other-wiki",
        tasks: [
          exportTask({
            status: "succeeded",
            updatedAt: "2026-08-13T10:05:00Z",
            completedAt: "2026-08-13T10:05:00Z",
          }),
        ],
      });
    });

    await waitFor(() => expect(useExportStore.getState().runningTaskId).toBeNull());
    expect(invokeMock.mock.calls.filter(([command]) => command === "read_export_preview")).toHaveLength(0);
    expect(useExportStore.getState().previewId).toBeNull();
  });

  it("keeps the old preview and switches to the new direct-regenerate record on completion", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const record: ExportRecord = {
      id: "export-existing",
      exportType: "knowledge_card",
      title: "Transformer card",
      sourcePath: pageMeta().path,
      outputPath: "exports/html/transformer-card.html",
      createdAt: "2026-08-13T09:00:00Z",
      route: "agent",
      status: "succeeded",
      bookmarked: false,
    };
    const regeneratedTask = exportTask({ id: "task-regenerate" });
    const regeneratedRecord: ExportRecord = {
      ...record,
      id: "export-regenerated",
      title: "Transformer card v2",
      outputPath: "exports/html/transformer-card-v2.html",
      createdAt: "2026-08-13T10:05:00Z",
      route: "byok",
      taskId: regeneratedTask.id,
    };
    let listExportsCalls = 0;
    invokeMock.mockImplementation((
      command: string,
      payload?: { request?: { outputPath?: string } },
    ) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") {
        listExportsCalls += 1;
        return Promise.resolve(
          listExportsCalls === 1 ? [record] : [regeneratedRecord, record],
        );
      }
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "regenerate_export") return Promise.resolve(regeneratedTask);
      if (command === "get_task") return Promise.resolve(regeneratedTask);
      if (command === "read_export_preview") {
        return Promise.resolve(
          payload?.request?.outputPath === record.outputPath
            ? "<h1>Transformer card v1</h1>"
            : "<h1>Transformer card v2</h1>",
        );
      }
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useTaskStore.setState({
      activeProjectId: "proj-1",
      activeProjectRootPath: "D:/wiki",
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    const requestWorkflowLaunch = vi
      .spyOn(useNavigationStore.getState(), "requestWorkflowLaunch")
      .mockImplementation(() => undefined);

    try {
      render(<WikiView capabilities={emptyAiCapabilities} />);
      await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
      await waitFor(() => expect(useExportStore.getState().records).toEqual([record]));
      fireEvent.click(screen.getByRole("tab", { name: "HTML preview" }));
      await waitFor(() =>
        expect(screen.getByTitle("HTML preview")).toHaveAttribute(
          "srcdoc",
          "<h1>Transformer card v1</h1>",
        ),
      );
      expect(screen.getAllByText(record.outputPath)).toHaveLength(2);
      fireEvent.click(screen.getByRole("button", { name: "Regenerate" }));

      await waitFor(() =>
        expect(
          invokeMock.mock.calls.filter(([command]) => command === "regenerate_export"),
        ).toHaveLength(1),
      );
      const regenerateCall = invokeMock.mock.calls.find(
        ([command]) => command === "regenerate_export",
      );
      expect(regenerateCall?.[1]).toEqual({
        request: expect.objectContaining({
          projectId: "proj-1",
          projectRootPath: "D:/wiki",
          exportType: "knowledge_card",
          sourcePath: pageMeta().path,
          route: "auto",
          options: {
            includeFrontmatter: true,
            embedCss: true,
            embedImages: false,
          },
          acknowledgeRestrictedContent: false,
        }),
      });
      await waitFor(() =>
        expect(useExportStore.getState().runningTaskId).toBe(regeneratedTask.id),
      );
      expect(screen.getAllByText(record.outputPath)).toHaveLength(2);
      expect(screen.getByTitle("HTML preview")).toHaveAttribute(
        "srcdoc",
        "<h1>Transformer card v1</h1>",
      );
      expect(requestWorkflowLaunch).not.toHaveBeenCalled();

      act(() => {
        useTaskStore.setState({
          tasks: [
            exportTask({
              id: regeneratedTask.id,
              status: "succeeded",
              updatedAt: "2026-08-13T10:05:00Z",
              completedAt: "2026-08-13T10:05:00Z",
            }),
          ],
        });
      });

      await waitFor(() =>
        expect(useExportStore.getState().previewId).toBe(regeneratedRecord.id),
      );
      expect(useExportStore.getState().records).toEqual([regeneratedRecord, record]);
      expect(screen.getAllByText(regeneratedRecord.outputPath)).toHaveLength(2);
      expect(screen.queryByText(record.outputPath)).not.toBeInTheDocument();
      expect(screen.getByTitle("HTML preview")).toHaveAttribute(
        "srcdoc",
        "<h1>Transformer card v2</h1>",
      );
      expect(invokeMock).toHaveBeenCalledWith("read_export_preview", {
        request: {
          projectId: "proj-1",
          projectRootPath: "D:/wiki",
          outputPath: regeneratedRecord.outputPath,
        },
      });
    } finally {
      requestWorkflowLaunch.mockRestore();
    }
  });

  it("reopens the single-page dialog when preview regeneration has no valid Wiki record", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    const projectReport: ExportRecord = {
      id: "project-report",
      exportType: "project_report",
      title: "Project report",
      outputPath: "exports/html/project-report.html",
      createdAt: "2026-08-13T09:00:00Z",
      route: "agent",
      status: "succeeded",
      bookmarked: false,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([projectReport]);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    const requestWorkflowLaunch = vi
      .spyOn(useNavigationStore.getState(), "requestWorkflowLaunch")
      .mockImplementation(() => undefined);

    try {
      render(<WikiView capabilities={emptyAiCapabilities} />);
      await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
      await waitFor(() => expect(useExportStore.getState().records).toEqual([projectReport]));
      act(() => useWikiStore.setState({ mode: "preview" }));
      fireEvent.click(screen.getByRole("button", { name: "Regenerate" }));

      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Beautiful read" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(requestWorkflowLaunch).not.toHaveBeenCalled();
      expect(
        invokeMock.mock.calls.filter(([command]) => command === "regenerate_export"),
      ).toHaveLength(0);
    } finally {
      requestWorkflowLaunch.mockRestore();
    }
  });

  it("pauses for restricted-content confirmation before starting the direct export", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: true, restrictedSourceCount: 2 });
      }
      if (command === "start_export") {
        return Promise.resolve(exportTask({ id: "task-restricted" }));
      }
      if (command === "get_task") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));

    expect(await screen.findByText(/This export contains 2 restricted source/)).toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "start_export")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Export anyway" }));

    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([command]) => command === "start_export")).toHaveLength(1),
    );
    const startCall = invokeMock.mock.calls.find(([command]) => command === "start_export");
    expect(startCall?.[1]).toEqual({
      request: expect.objectContaining({ acknowledgeRestrictedContent: true }),
    });
  });

  it("keeps the quick-export dialog open and shows the backend error when start fails", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_export_restricted_content_status") {
        return Promise.resolve({ containsRestrictedContent: false, restrictedSourceCount: 0 });
      }
      if (command === "start_export") return Promise.reject(new Error("permission denied"));
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Generate HTML" }));
    fireEvent.click(await screen.findByRole("button", { name: "Generate and preview" }));

    expect(await screen.findByText("permission denied")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(useWikiStore.getState().mode).toBe("read");
    expect(useTaskStore.getState().drawerOpen).toBe(false);
  });

  it("keeps Wiki quick export types limited to single-page artifacts", () => {
    expect(SINGLE_PAGE_EXPORT_TYPES).toEqual([
      "beautiful_read",
      "knowledge_card",
      "concept_map",
    ]);
    expect(SINGLE_PAGE_EXPORT_TYPES).not.toContain("project_report");
  });

  it("routes Wiki reading-page and knowledge-card actions through wikiStore.requestExport", async () => {
    const page = pageContent();
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useNavigationStore.setState({ activeView: "wiki", rightPanelMode: "default" });
    const requestWorkflowLaunch = vi
      .spyOn(useNavigationStore.getState(), "requestWorkflowLaunch")
      .mockImplementation(() => undefined);
    useWikiStore.setState({ page, selectedPath: page.meta.path });
    const requestExport = vi.spyOn(useWikiStore.getState(), "requestExport");

    try {
      render(<RightContextPanel />);
      fireEvent.click(
        await screen.findByRole("button", { name: "Generate HTML reading page" }),
      );
      fireEvent.click(screen.getByRole("button", { name: "Generate knowledge card" }));

      expect(requestExport).toHaveBeenNthCalledWith(1, "beautiful_read");
      expect(requestExport).toHaveBeenNthCalledWith(2, "knowledge_card");
      expect(requestWorkflowLaunch).not.toHaveBeenCalled();
    } finally {
      requestExport.mockRestore();
      requestWorkflowLaunch.mockRestore();
    }
  });

  it("consumes a Wiki export request and opens the dialog with its selected type", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });
    useNavigationStore.setState({ activeView: "wiki" });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    act(() => useWikiStore.getState().requestExport("knowledge_card"));

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Knowledge card" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(useWikiStore.getState().requestedExportType).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("exposes a resizable wiki tree splitter", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);

    expect(await screen.findByRole("separator", { name: "Resize wiki tree" })).toHaveAttribute("aria-valuemin", "220");
  });

  it("opens the wiki assistant mode from the Ask AI button", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    fireEvent.click(await screen.findByRole("button", { name: "Ask AI" }));

    expect(useNavigationStore.getState().rightPanelMode).toBe("wikiAssistant");
    expect(useNavigationStore.getState().wikiAssistantPagePath).toBe(
      "wiki/concepts/transformer.md",
    );
  });

  it("updates the wiki assistant page path when the selected page changes", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") {
        return Promise.resolve({ root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 0, children: [] }, pages: [], totalPages: 0 });
      }
      if (command === "list_exports") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    useNavigationStore.setState({
      activeView: "wiki",
      rightPanelOpen: true,
      rightPanelMode: "wikiAssistant",
      wikiAssistantPagePath: "wiki/old.md",
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    act(() => {
      useWikiStore.setState({
        page: pageContent({ meta: pageMeta({ path: "wiki/concepts/updated.md" }) }),
      });
    });

    await waitFor(() => {
      expect(useNavigationStore.getState().wikiAssistantPagePath).toBe(
        "wiki/concepts/updated.md",
      );
    });
  });

  it("does not reuse a preview generated for another page", () => {
    const records: ExportRecord[] = [
      {
        id: "old-preview",
        exportType: "beautiful_read",
        title: "Old",
        sourcePath: "wiki/old.md",
        outputPath: "exports/html/old.html",
        createdAt: "2026-06-21T00:00:00Z",
        route: "agent",
        status: "succeeded",
        bookmarked: false,
      },
    ];

    expect(
      selectWikiPreviewRecord(records, "old-preview", "wiki/new.md"),
    ).toBeNull();
  });

  it("offers only single-page export templates", () => {
    render(
      <GenerateHtmlDialog
        pagePath="wiki/concepts/agent.md"
        initialType="project_report"
        onCancel={vi.fn()}
        onGenerate={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /Beautiful read/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Knowledge card/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Concept map/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Project report/i })).not.toBeInTheDocument();
  });

  it("renders generated HTML in a sandboxed iframe", () => {
    render(
      <WikiHtmlPreviewPane
        html="<h1>Preview</h1>"
        outputPath="exports/html/agent.html"
        templateLabel="Beautiful read"
        busy={false}
        onBack={vi.fn()}
        onRegenerate={vi.fn()}
        onOpenFolder={vi.fn()}
        onCopyPath={vi.fn()}
      />,
    );

    const frame = screen.getByTitle("HTML preview");
    expect(frame).toHaveAttribute("sandbox", "");
    expect(frame).toHaveAttribute("srcdoc", "<h1>Preview</h1>");
    expect(screen.getAllByText("exports/html/agent.html")).toHaveLength(2);
  });
});

describe("RelatedPagesPanel P1 details", () => {
  it("numbers citations, counts backlinks, and exposes page actions", () => {
    const page = pageMeta({ sources: ["source-a", "source-b"] });
    const backlink = pageMeta({
      path: "wiki/concepts/attention.md",
      title: "Attention",
      aliases: [],
      wikilinks: ["transformer", "Transformers"],
    });
    const onViewAllBacklinks = vi.fn();
    const onGenerateHtml = vi.fn();
    const onGenerateCard = vi.fn();
    const onViewInGraph = vi.fn();
    const onCopyWikilink = vi.fn();
    const { container } = render(
      <RelatedPagesPanel
        page={page}
        pages={[page, backlink]}
        onOpenPage={vi.fn()}
        onViewAllBacklinks={onViewAllBacklinks}
        onGenerateHtml={onGenerateHtml}
        onGenerateCard={onGenerateCard}
        onViewInGraph={onViewInGraph}
        onCopyWikilink={onCopyWikilink}
      />,
    );

    expect(container.querySelectorAll(".citation__idx")).toHaveLength(2);
    expect(container.querySelector(".relpage__count")).toHaveTextContent("2");
    fireEvent.click(screen.getByRole("button", { name: "View all 2 backlinks" }));
    expect(onViewAllBacklinks).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Generate HTML reading page" }));
    fireEvent.click(screen.getByRole("button", { name: "Generate knowledge card" }));
    fireEvent.click(screen.getByRole("button", { name: "View in graph" }));
    fireEvent.click(screen.getByRole("button", { name: "Copy wikilink" }));
    expect(onGenerateHtml).toHaveBeenCalledOnce();
    expect(onGenerateCard).toHaveBeenCalledOnce();
    expect(onViewInGraph).toHaveBeenCalledOnce();
    expect(onCopyWikilink).toHaveBeenCalledOnce();
  });
});

describe("WikiEditor (Milkdown)", () => {
  it("locks the live editor while update installation owns the restart barrier", async () => {
    const tree: WikiTree = {
      root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 1, children: [] },
      pages: [pageMeta()],
      totalPages: 1,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(pageContent());
      if (command === "list_exports") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "proj-1",
        rootPath: "D:/wiki",
        name: "Wiki",
      },
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await waitFor(() => expect(useWikiStore.getState().page).not.toBeNull());
    act(() => useWikiStore.getState().startEdit());
    act(() => useUpdateStore.setState({ uiStatus: "installing" }));

    expect(await screen.findByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByTestId("wiki-editor-scroll")).toHaveAttribute("aria-disabled", "true");
  });

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

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useExportStore } from "../../stores/exportStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import type { ExportRecord } from "../../types/export";
import type { WikiPageMeta, WikiTree } from "../../types/wiki";
import { LeftSidebar } from "./LeftSidebar";

const { invokeMock, preloadWorkspaceViewMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  preloadWorkspaceViewMock: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("./workspaceViewLoaders", () => ({
  preloadWorkspaceView: preloadWorkspaceViewMock,
}));

const wikiPage: WikiPageMeta = {
  path: "wiki/concepts/transformer.md",
  title: "Transformer",
  pageType: "concept",
  tags: [],
  sources: [],
  aliases: [],
  created: null,
  updated: null,
  starred: false,
  bookmarked: true,
  wordCount: 10,
  fileSize: 100,
  modifiedTime: "2026-07-04T00:00:00Z",
  hash: "hash",
  wikilinks: [],
};

const wikiTree: WikiTree = {
  root: {
    name: "wiki",
    kind: "folder",
    path: "wiki",
    starred: false,
    bookmarked: false,
    fileCount: 1,
    children: [],
  },
  pages: [wikiPage],
  totalPages: 1,
};

const exportRecord: ExportRecord = {
  id: "export-1",
  exportType: "beautiful_read",
  title: "Transformer HTML",
  sourcePath: wikiPage.path,
  outputPath: "exports/html/transformer.html",
  createdAt: "2026-07-04T00:00:00Z",
  route: "agent",
  status: "succeeded",
  bookmarked: true,
};

const originalExportActions = {
  loadExports: useExportStore.getState().loadExports,
  loadPreview: useExportStore.getState().loadPreview,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("LeftSidebar favorites", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    invokeMock.mockResolvedValue([]);
    preloadWorkspaceViewMock.mockClear();
    useNavigationStore.setState({ activeView: "dashboard" });
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "project-1",
      rootPath: "/wiki",
    });
    useWikiStore.getState().reset();
    useExportStore.getState().reset();
    useExportStore.setState(originalExportActions);
    useWikiStore.setState({ tree: wikiTree, recentPages: [wikiPage] });
    useExportStore.setState({ records: [exportRecord] });
  });

  it("preloads a route from the shared loader on hover and keyboard focus", () => {
    render(<LeftSidebar />);
    const graph = screen.getByRole("button", { name: "Graph" });

    fireEvent.pointerEnter(graph);
    fireEvent.focus(graph);

    expect(preloadWorkspaceViewMock).toHaveBeenNthCalledWith(1, "graph");
    expect(preloadWorkspaceViewMock).toHaveBeenNthCalledWith(2, "graph");
  });

  it("renders favorites between workflow and recent pages", () => {
    render(<LeftSidebar />);

    const workflow = screen.getByText("Knowledge Processing");
    const favorites = screen.getByText("Favorites");
    const recent = screen.getByText("Recent pages");

    expect(workflow.compareDocumentPosition(favorites)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(favorites.compareDocumentPosition(recent)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
  });

  it("scrolls all upper sections together while keeping the Agent footer fixed", () => {
    render(<LeftSidebar />);

    const sectionLabels = [
      screen.getByText("Main views"),
      screen.getByText("Knowledge Processing"),
      screen.getByText("Favorites"),
      screen.getByText("Recent pages"),
    ];
    const scrollRegion = sectionLabels[0].closest(".app-sidebar__scroll-region");

    expect(scrollRegion).not.toBeNull();
    sectionLabels.forEach((label) => {
      expect(label.closest(".app-sidebar__scroll-region")).toBe(scrollRegion);
    });
    const agentButton = screen.getByTitle("Agent settings");
    const agentFooter = agentButton.closest("div");
    expect(scrollRegion as HTMLElement).not.toContainElement(agentButton);
    expect(agentFooter).toHaveClass("shrink-0");
  });

  it("opens Settings from the preserved Agent status footer", () => {
    render(<LeftSidebar />);

    fireEvent.click(screen.getByRole("button", { name: "Agent settings" }));

    expect(useNavigationStore.getState().settingsOpen).toBe(true);
    expect(useNavigationStore.getState().settingsSection).toBe("ai");
    expect(useNavigationStore.getState().activeView).toBe("dashboard");
  });

  it("keeps favorite and recent page icons from shrinking beside long titles", () => {
    const longTitle =
      "A very long page title that should truncate without squeezing the leading file icon";
    const longWikiPage = { ...wikiPage, path: "wiki/concepts/long-title.md", title: longTitle };
    const longExportRecord = {
      ...exportRecord,
      id: "export-long-title",
      title: "A very long exported favorite title that should not squeeze its icon",
    };
    useWikiStore.setState({
      tree: { ...wikiTree, pages: [longWikiPage] },
      recentPages: [longWikiPage],
    });
    useExportStore.setState({ records: [longExportRecord] });

    render(<LeftSidebar />);

    const wikiFavoriteIcon = screen
      .getByRole("button", { name: `Open wiki favorite: ${longTitle}` })
      .querySelector("svg");
    const exportFavoriteIcon = screen
      .getByRole("button", { name: `Open export favorite: ${longExportRecord.title}` })
      .querySelector("svg");
    const recentIcon = screen
      .getByRole("button", { name: longTitle })
      .querySelector("svg");

    expect(wikiFavoriteIcon).toHaveClass("shrink-0");
    expect(exportFavoriteIcon).toHaveClass("shrink-0");
    expect(recentIcon).toHaveClass("shrink-0");
  });

  it("opens a wiki favorite in the wiki view", () => {
    const openPage = vi.fn(async () => {});
    useWikiStore.setState({ openPage });
    render(<LeftSidebar />);

    fireEvent.click(screen.getByRole("button", { name: "Open wiki favorite: Transformer" }));

    expect(useNavigationStore.getState().activeView).toBe("wiki");
    expect(openPage).toHaveBeenCalledWith("project-1", "/wiki", wikiPage.path);
  });

  it("opens an export favorite preview in the exports view", async () => {
    const loadExports = vi.fn(async () => {});
    const loadPreview = vi.fn(async () => {});
    useExportStore.setState({ loadExports, loadPreview });
    render(<LeftSidebar />);

    fireEvent.click(screen.getByRole("button", { name: "Open export favorite: Transformer HTML" }));

    await waitFor(() => {
      expect(useNavigationStore.getState().activeView).toBe("exports");
      expect(loadExports).toHaveBeenCalledWith("project-1", "/wiki");
      expect(loadPreview).toHaveBeenCalledWith(
        {
          projectId: "project-1",
          projectRootPath: "/wiki",
          outputPath: exportRecord.outputPath,
        },
        exportRecord.id,
      );
    });
  });

  it("loads export favorites on cold project load", async () => {
    useExportStore.getState().reset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_exports") return Promise.resolve([exportRecord]);
      return Promise.resolve([]);
    });

    render(<LeftSidebar />);

    expect(
      await screen.findByRole("button", { name: "Open export favorite: Transformer HTML" }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("list_exports", {
      request: { projectId: "project-1", projectRootPath: "/wiki" },
    });
  });

  it("does not open an export preview if the project changes while loading favorites", async () => {
    const loading = deferred<void>();
    const loadExports = vi.fn(() => loading.promise);
    const loadPreview = vi.fn(async () => {});
    useExportStore.setState({ loadExports, loadPreview });
    render(<LeftSidebar />);

    fireEvent.click(screen.getByRole("button", { name: "Open export favorite: Transformer HTML" }));
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "project-2",
      rootPath: "/other-wiki",
    });
    loading.resolve();

    await waitFor(() => {
      expect(loadExports).toHaveBeenCalledWith("project-1", "/wiki");
      expect(loadPreview).not.toHaveBeenCalled();
    });
  });
});

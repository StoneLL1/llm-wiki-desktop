import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../i18n";
import { useWikiStore } from "../features/wiki/wikiStore";
import { useGraphStore } from "../stores/graphStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";
import type { ProjectSummary } from "../types/project";
import type { BackendTask } from "../types/task";
import type { WikiPageContent } from "../types/wiki";
import { App } from "./App";
import { PANE_WIDTH_LIMITS } from "../hooks/useResizablePane";

const invokeMock = vi.hoisted(() => vi.fn());
const openDialogMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openDialogMock,
}));

function mockTask(overrides: Partial<BackendTask> = {}): BackendTask {
  return {
    id: "task-1",
    taskType: "import",
    projectId: null,
    title: "Default task",
    status: "queued",
    progress: null,
    startedAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
    ...overrides,
  };
}

const sampleProject = (overrides: Partial<ProjectSummary> = {}): ProjectSummary => ({
  projectId: "sample",
  name: "Agent Knowledge Base",
  rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
  template: "general",
  wikiPageCount: 237,
  sourceCount: 18,
  taskCount: 2,
  indexState: "indexed",
  graphState: "cached",
  agentRoute: "agent",
  health: {
    isWikiProject: true,
    hasPurpose: true,
    hasSchema: true,
    hasAppState: true,
    hasObsidian: false,
    missingPaths: [],
  },
  ...overrides,
});

beforeEach(() => {
  invokeMock.mockReset();
  openDialogMock.mockReset();
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  window.localStorage.clear();
  useToastStore.setState({ toasts: [] });
  useGraphStore.getState().reset();
  useNavigationStore.getState().setActiveView("dashboard");
  useNavigationStore.getState().setRightPanelOpen(true);
  useNavigationStore.getState().setSidebarCollapsed(false);
  useNavigationStore.getState().resetPaneSize("sidebar");
  useNavigationStore.getState().resetPaneSize("rightPanel");
  useNavigationStore.getState().resetPaneSize("wikiTree");
  useNavigationStore.getState().resetPaneSize("exportsList");
  useNavigationStore.getState().resetPaneSize("lintList");
  useProjectStore.getState().setCurrentProject(
    sampleProject({
      rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
      agentRoute: "agent",
    }),
  );
  useProjectStore.getState().setRecentProjects([]);
  useProjectStore.getState().setPendingAction(undefined);
  useTaskStore.getState().setTasks([
    mockTask({ id: "task-graph-refresh", title: "Refreshing graph cache", status: "running" }),
  ]);
  void i18next.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("App", () => {
  it("shows the project start flow instead of a fabricated workspace when no project is active", () => {
    useProjectStore.getState().setCurrentProject(
      sampleProject({
        projectId: "",
        name: "",
        rootPath: "",
        wikiPageCount: 0,
        sourceCount: 0,
        taskCount: 0,
        indexState: "missing",
        graphState: "missing",
        agentRoute: "unconfigured",
      }),
    );

    render(<App />);

    expect(screen.getByRole("heading", { name: "Choose a project to start working" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /New empty project/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open folder as project/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open existing project/i })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /project path|open path|local file/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/Import materials into existing project/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Primary" })).not.toBeInTheDocument();
  });

  it("opens existing projects through the native directory picker", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    openDialogMock.mockResolvedValue("D:\\知识库\\agent");
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "open_project") {
        return Promise.resolve({
          kind: "opened",
          summary: sampleProject({ rootPath: "D:/知识库/agent" }),
        });
      }
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Open existing project/i }));

    await waitFor(() =>
      expect(openDialogMock).toHaveBeenCalledWith(
        expect.objectContaining({ directory: true, multiple: false }),
      ),
    );
    expect(invokeMock).toHaveBeenCalledWith("open_project", {
      request: { path: "D:\\知识库\\agent" },
    });
  });

  it("shows launch picker errors instead of leaving an unhandled rejection", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    openDialogMock.mockRejectedValue(new Error("dialog unavailable"));
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Open existing project/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("dialog unavailable");
    expect(invokeMock).not.toHaveBeenCalledWith("open_project", expect.anything());
  });

  it("creates a project from a parent folder and project name", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    openDialogMock.mockResolvedValue("D:\\资料库");
    invokeMock.mockImplementation((command: string) => {
      if (command === "create_project") {
        return Promise.resolve(
          sampleProject({ rootPath: "D:/资料库/中文知识库", name: "中文知识库" }),
        );
      }
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /New empty project/i }));
    expect(screen.queryByRole("checkbox", { name: /Initialize a Git repo/i })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), {
      target: { value: "中文知识库" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("D:\\资料库\\中文知识库");
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_project", {
        request: {
          rootPath: "D:\\资料库\\中文知识库",
          name: "中文知识库",
          template: "general",
        },
      }),
    );
  });

  it("collapses the launch setup panel without placeholder controls", () => {
    useProjectStore.getState().setCurrentProject(
      sampleProject({ projectId: "", name: "", rootPath: "" }),
    );

    render(<App />);

    expect(screen.queryByRole("button", { name: "Settings" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Help" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Collapse setup panel" }));
    expect(screen.queryByRole("complementary", { name: "Launch side panel" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open setup panel" }));
    expect(screen.getByRole("complementary", { name: "Launch side panel" })).toBeInTheDocument();
  });

  it("starts the launch setup panel closed on narrow viewports", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    useProjectStore.getState().setCurrentProject(
      sampleProject({ projectId: "", name: "", rootPath: "" }),
    );

    render(<App />);

    expect(screen.queryByRole("complementary", { name: "Launch side panel" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open setup panel" })).toBeInTheDocument();
  });

  it("opens the setup panel before navigating to templates", () => {
    useProjectStore.getState().setCurrentProject(
      sampleProject({ projectId: "", name: "", rootPath: "" }),
    );
    const scrollIntoView = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoView,
    });
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Collapse setup panel" }));
    fireEvent.click(screen.getByRole("button", { name: "Templates" }));

    expect(screen.getByRole("complementary", { name: "Launch side panel" })).toBeInTheDocument();
    expect(scrollIntoView).toHaveBeenCalled();
  });

  it("returns to project selection when the project switcher is used", () => {
    useWikiStore.setState({ selectedPath: "wiki/old.md" });
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Switch project" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Back to launch" }));

    expect(screen.getByRole("heading", { name: "Choose a project to start working" })).toBeInTheDocument();
    expect(useWikiStore.getState().selectedPath).toBeNull();
    expect(useNavigationStore.getState().activeView).toBe("dashboard");
  });

  it("shows compact project paths in the topbar and full path in title", () => {
    render(<App />);

    const switcher = screen.getByRole("button", { name: "Switch project" });

    expect(within(switcher).getByText("D:/.../wiki/agent-llm")).toHaveAttribute(
      "title",
      "D:/Users/Aletta/Documents/wiki/agent-llm",
    );
  });

  it("marks missing recent projects without opening them", () => {
    useProjectStore.getState().setRecentProjects([
      {
        projectId: "missing-project",
        name: "Missing project",
        rootPath: "D:/Users/Aletta/Documents/wiki/missing-project",
        template: "general",
        openedAt: "2026-07-04T00:00:00Z",
        wikiPageCount: 0,
        sourceCount: 0,
        taskCount: 0,
        indexState: "missing",
        graphState: "missing",
        missing: true,
      },
    ]);
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Switch project" }));
    const missingRow = screen.getByRole("menuitem", { name: /Missing project/ });
    fireEvent.click(missingRow);

    expect(missingRow).toHaveAttribute("aria-disabled", "true");
    expect(useProjectStore.getState().currentProject.rootPath).toBe("D:/Users/Aletta/Documents/wiki/agent-llm");
  });

  it("opens the project menu with keyboard navigation and closes it with Escape", async () => {
    useProjectStore.getState().setRecentProjects([
      {
        projectId: "missing-project",
        name: "Missing project",
        rootPath: "D:/Users/Aletta/Documents/wiki/missing-project",
        template: "general",
        openedAt: "2026-07-03T00:00:00Z",
        wikiPageCount: 0,
        sourceCount: 0,
        taskCount: 0,
        indexState: "missing",
        graphState: "missing",
        missing: true,
      },
      {
        projectId: "enabled-project",
        name: "Enabled project",
        rootPath: "D:/Users/Aletta/Documents/wiki/enabled-project",
        template: "research",
        openedAt: "2026-07-04T00:00:00Z",
        wikiPageCount: 12,
        sourceCount: 3,
        taskCount: 0,
        indexState: "indexed",
        graphState: "cached",
        missing: false,
      },
    ]);
    render(<App />);

    const switcher = screen.getByRole("button", { name: "Switch project" });
    fireEvent.keyDown(switcher, { key: "ArrowDown" });
    const enabledRow = await screen.findByRole("menuitem", { name: /Enabled project/ });

    await waitFor(() => expect(enabledRow).toHaveFocus());
    fireEvent.keyDown(enabledRow, { key: "Escape" });

    expect(screen.queryByRole("menuitem", { name: /Enabled project/ })).not.toBeInTheDocument();
    expect(switcher).toHaveFocus();
  });

  it("supports Escape and arrow navigation after the project menu is opened by click", async () => {
    useProjectStore.getState().setRecentProjects([
      {
        projectId: "enabled-project",
        name: "Enabled project",
        rootPath: "D:/Users/Aletta/Documents/wiki/enabled-project",
        template: "research",
        openedAt: "2026-07-04T00:00:00Z",
        wikiPageCount: 12,
        sourceCount: 3,
        taskCount: 0,
        indexState: "indexed",
        graphState: "cached",
        missing: false,
      },
    ]);
    render(<App />);

    const switcher = screen.getByRole("button", { name: "Switch project" });
    fireEvent.click(switcher);
    switcher.focus();
    expect(await screen.findByRole("menuitem", { name: /Enabled project/ })).toBeInTheDocument();

    fireEvent.keyDown(switcher, { key: "Escape" });

    expect(screen.queryByRole("menuitem", { name: /Enabled project/ })).not.toBeInTheDocument();
    expect(switcher).toHaveFocus();

    fireEvent.click(switcher);
    switcher.focus();
    const enabledRow = await screen.findByRole("menuitem", { name: /Enabled project/ });
    fireEvent.keyDown(switcher, { key: "ArrowDown" });

    await waitFor(() => expect(enabledRow).toHaveFocus());
  });

  it("renders the desktop shell scaffold", () => {
    render(<App />);

    expect(screen.getByText("LLM Wiki Desktop")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
  });

  it("switches center workspace views from the left sidebar", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Graph" }));

    expect(screen.getByRole("button", { name: "Graph" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Graph" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Dashboard" })).not.toBeInTheDocument();

    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);

    expect(screen.getByRole("button", { name: "Import" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Import" })).toBeInTheDocument();
  });

  it("keeps context and status surfaces visible while navigation changes", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Chat" }));

    expect(screen.getByRole("complementary", { name: "Sources" })).toBeInTheDocument();
    expect(screen.getAllByText("D:/Users/Aletta/Documents/wiki/agent-llm").length).toBeGreaterThan(0);
    expect(screen.getByText("Wiki pages: 237")).toBeInTheDocument();
    expect(screen.getByText("Tasks: 1 running")).toBeInTheDocument();
  });

  it("passes focus-neighbor action from right panel inspector to graph store", async () => {
    useNavigationStore.getState().setActiveView("graph");
    useGraphStore.setState({
      status: "ready",
      data: {
        nodes: [
          { id: "a", path: "wiki/a.md", label: "Alpha", type: "concept", tags: [], starred: false, degree: 1 },
          { id: "b", path: "wiki/b.md", label: "Beta", type: "entity", tags: [], starred: false, degree: 1 },
        ],
        edges: [{ source: "a", target: "b", relation: "related", weight: 1 }],
        contentHash: "graph-hash",
        builtAt: "2026-07-04T00:00:00Z",
        layout: { positions: { a: [0, 0], b: [1, 1] }, communities: { a: 0, b: 0 } },
      },
      selectedNodeId: "a",
      load: async () => {},
    });

    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Focus neighbors" }));

    expect(useGraphStore.getState().focusedNodeId).toBe("a");
  });

  it("collapses and restores the right context panel", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "Collapse context panel" })).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(screen.getByRole("button", { name: "Collapse context panel" }));
    expect(screen.queryByRole("complementary", { name: "Project info" })).not.toBeInTheDocument();

    expect(screen.getByRole("button", { name: "Open context panel" })).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(screen.getByRole("button", { name: "Open context panel" }));
    expect(screen.getByRole("complementary", { name: "Project info" })).toBeInTheDocument();
  });

  it("keeps the sidebar splitter and topbar collapse control navigation reachable", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "Collapse sidebar" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Expand sidebar" })).not.toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Resize sidebar" })).toBeInTheDocument();

    useNavigationStore.getState().setPaneSize("sidebar", 56);

    expect(screen.getByRole("separator", { name: "Resize sidebar" })).toBeInTheDocument();
    for (const label of ["Dashboard", "Wiki", "Chat", "Graph", "Agent", "Import", "Lint", "Exports"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("exposes keyboard-resizable shell splitters", () => {
    render(<App />);

    const sidebarSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    const rightPanelSplitter = screen.getByRole("separator", { name: "Resize context panel" });

    fireEvent.keyDown(sidebarSplitter, { key: "ArrowRight" });

    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(
      PANE_WIDTH_LIMITS.sidebar.defaultValue + 12,
    );
    expect(rightPanelSplitter).toHaveAttribute("aria-valuenow", String(PANE_WIDTH_LIMITS.rightPanel.defaultValue));

    fireEvent.click(screen.getByRole("button", { name: "Collapse context panel" }));
    expect(screen.queryByRole("separator", { name: "Resize context panel" })).not.toBeInTheDocument();
  });

  it("starts the project context panel closed on narrow viewports", () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));

    render(<App />);

    expect(screen.queryByRole("complementary", { name: "Project info" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open context panel" })).toBeInTheDocument();
  });

  it("closes only the topmost modal when Escape is pressed", async () => {
    useProjectStore.getState().setPendingAction({
      id: "action-escape",
      actionType: "initialize_folder",
      title: "Initialize folder as project",
      message: "Create the project structure.",
      riskLevel: "medium",
      affectedPaths: ["purpose.md"],
      preview: null,
      expiresAt: null,
    });
    render(<App />);

    const dialog = screen.getByRole("dialog");
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
    expect(screen.getByRole("complementary", { name: "Project info" })).toBeInTheDocument();
  });

  it("omits unavailable placeholder metrics and actions from context panels", () => {
    render(<App />);

    expect(screen.queryByText("Last compile")).not.toBeInTheDocument();
    expect(screen.queryByText("Disk usage")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Chat" }));
    expect(screen.queryByText("Raw Sources")).not.toBeInTheDocument();
    expect(screen.queryByText("Time")).not.toBeInTheDocument();
    expect(screen.queryByText("Tokens")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Flag issue" })).not.toBeInTheDocument();
  });

  it("renders status from mutable project and task stores", () => {
    useProjectStore.getState().setCurrentProject(
      sampleProject({
        rootPath: "D:/tmp/research-wiki",
        wikiPageCount: 42,
        indexState: "stale",
        agentRoute: "byok",
      }),
    );
    useTaskStore.getState().setTasks([
      mockTask({ id: "task-import", title: "Parsing sources", status: "running" }),
      mockTask({ id: "task-lint", title: "Running local lint", status: "running" }),
      mockTask({ id: "task-export", title: "Export complete", status: "succeeded" }),
    ]);

    render(<App />);

    expect(screen.getAllByText("D:/tmp/research-wiki").length).toBeGreaterThan(0);
    expect(screen.getByText("Tasks: 2 running")).toBeInTheDocument();
    expect(screen.getByText("Wiki pages: 42")).toBeInTheDocument();
  });

  it("switches language from the top bar", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "中文" }));

    expect(await screen.findByRole("button", { name: "图谱" })).toBeInTheDocument();
    expect(screen.getByText("Wiki 页面: 237")).toBeInTheDocument();
  });

  it("opens settings from the top bar settings button", () => {
    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Settings" }).at(-1)!);

    expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("runs local keyword search from the top bar and opens a result in Wiki", async () => {
    const page: WikiPageContent = {
      meta: {
        path: "wiki/concepts/transformer.md",
        title: "Transformer",
        pageType: "concept",
        tags: ["nlp"],
        sources: [],
        aliases: [],
        created: null,
        updated: null,
        starred: false,
        bookmarked: false,
        wordCount: 5,
        fileSize: 20,
        modifiedTime: "2026-06-21T00:00:00Z",
        hash: "hash",
        wikilinks: [],
      },
      rawMarkdown: "# Transformer",
      bodyMarkdown: "# Transformer",
      frontmatterYaml: null,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "search_wiki") {
        return Promise.resolve({
          results: [{ path: page.meta.path, title: page.meta.title, pageType: "concept", starred: false, matchedFields: ["title"], score: 100 }],
          total: 1,
        });
      }
      if (command === "read_wiki_page") return Promise.resolve(page);
      return Promise.resolve([]);
    });
    render(<App />);

    const search = screen.getByRole("searchbox", { name: "Search current wiki" });
    fireEvent.change(search, { target: { value: "transformer" } });
    fireEvent.keyDown(search, { key: "Enter" });

    const result = await screen.findByRole("button", { name: /Transformer/ });
    expect(invokeMock).toHaveBeenCalledWith("search_wiki", {
      request: {
        projectId: "sample",
        projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
        query: "transformer",
        pageTypes: [],
        tags: [],
        source: null,
        limit: 20,
      },
    });
    fireEvent.click(result);

    await vi.waitFor(() => expect(useNavigationStore.getState().activeView).toBe("wiki"));
  });

  it("shows a visible error when local search IPC fails", async () => {
    invokeMock.mockRejectedValueOnce({ message: "index unavailable" });
    render(<App />);
    const search = screen.getByRole("searchbox", { name: "Search current wiki" });
    fireEvent.change(search, { target: { value: "broken" } });
    fireEvent.keyDown(search, { key: "Enter" });

    expect(await screen.findByRole("alert")).toHaveTextContent("Search failed");
  });

  it("shows and clears pending confirmation actions from the project store", () => {
    useProjectStore.getState().setPendingAction({
      id: "action-1",
      actionType: "initialize_folder",
      title: "Initialize folder as project",
      message: "Create the project structure and organize files.",
      riskLevel: "medium",
      affectedPaths: ["report.pdf"],
      preview: null,
      expiresAt: null,
    });

    render(<App />);

    expect(screen.getByRole("dialog", { name: "Initialize folder as project" })).toBeInTheDocument();
    expect(screen.getByText("report.pdf")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(useProjectStore.getState().pendingAction).toBeUndefined();
  });

  it("requests an import preview when files are selected in the import view", async () => {
    const preview = {
      files: [],
      conflicts: [],
      summary: {
        totalFiles: 0,
        archivedFiles: 0,
        duplicateFiles: 0,
        renamedFiles: 0,
        failedFiles: 0,
        conflictsCount: 0,
      },
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_import") {
        return Promise.resolve(mockTask({ id: "import-preview", projectId: "sample", status: "succeeded" }));
      }
      if (command === "get_import_preview") return Promise.resolve(preview);
      return Promise.resolve(null);
    });

    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    const sourcePathInput = screen.getByRole("textbox", { name: "Local file or folder paths" });
    fireEvent.change(sourcePathInput, {
      target: { value: "D:/tmp/sources/notes.md" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add to preview" }));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("preview_import", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          sourcePaths: ["D:/tmp/sources/notes.md"],
          allowDuplicates: false,
          linkDuplicates: false,
        },
      });
    });
  });

  it("stages pasted Markdown through the backend import preview", async () => {
    invokeMock.mockResolvedValueOnce({
      files: [],
      conflicts: [],
      summary: { totalFiles: 1, archivedFiles: 1, duplicateFiles: 0, renamedFiles: 0, failedFiles: 0, conflictsCount: 0 },
    });
    render(<App />);
    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    fireEvent.click(screen.getByRole("button", { name: "Clipboard" }));
    const input = screen.getByRole("textbox", { name: "Clipboard Markdown" });
    fireEvent.change(input, { target: { value: "# Pasted notes\n\nUseful text." } });
    fireEvent.click(within(input.closest(".import-paths")!).getByRole("button", { name: "Add to preview" }));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("preview_text_import", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          kind: "clipboard",
          sourceName: "clipboard-import",
          content: "# Pasted notes\n\nUseful text.",
          title: null,
          author: null,
        },
      }),
    );
  });

  it("fetches URL content in the backend then stages Readability Markdown", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "fetch_import_url") {
        return Promise.resolve({
          url: "https://example.com/article",
          html: "<!doctype html><html><head><title>URL note</title></head><body><article><h1>URL note</h1><p>Useful article body with enough words for extraction.</p></article></body></html>",
        });
      }
      if (command === "preview_text_import") {
        return Promise.resolve({ files: [], conflicts: [], summary: { totalFiles: 1, archivedFiles: 1, duplicateFiles: 0, renamedFiles: 0, failedFiles: 0, conflictsCount: 0 } });
      }
      return Promise.resolve(null);
    });
    render(<App />);
    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    fireEvent.click(screen.getByRole("button", { name: "URL / link" }));
    const input = await screen.findByRole("textbox", { name: "Link" });
    fireEvent.change(input, { target: { value: "https://example.com/article" } });
    fireEvent.click(screen.getByRole("button", { name: "Fetch & preview" }));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("fetch_import_url", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          url: "https://example.com/article",
        },
      });
      expect(invokeMock).toHaveBeenCalledWith("preview_text_import", {
        request: expect.objectContaining({
          kind: "url",
          title: "URL note",
          content: expect.stringContaining("source_url: \"https://example.com/article\""),
        }),
      });
    });
  });

  it("surfaces an import preview backend error instead of silently clearing the selection", async () => {
    invokeMock.mockRejectedValueOnce({ message: "source missing" });
    render(<App />);
    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    const sourcePathInput = screen.getByRole("textbox", { name: "Local file or folder paths" });
    fireEvent.change(sourcePathInput, { target: { value: "D:/missing" } });
    fireEvent.click(screen.getByRole("button", { name: "Add to preview" }));

    expect(await screen.findByRole("status")).toHaveTextContent("Could not preview sources");
  });

  it("confirms the current import preview and clears it after success", async () => {
    const preview = {
      files: [
        {
          originalName: "notes.md",
          sourcePath: "D:/tmp/sources/notes.md",
          archivedPath: "raw/sources/markdown/notes.md",
          fileType: "markdown",
          sizeBytes: 48,
          hash: "abc",
          extractionStatus: "extracted",
          extractionError: null,
          textPreview: null,
          pageCount: null,
          wordCount: 6820,
          metadata: null,
          extractedTextPath: null,
          extractedAssets: [],
          conflict: null,
          renamedFrom: null,
        },
      ],
      conflicts: [],
      summary: {
        totalFiles: 1,
        archivedFiles: 1,
        duplicateFiles: 0,
        renamedFiles: 0,
        failedFiles: 0,
        conflictsCount: 0,
      },
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "preview_import") {
        return Promise.resolve(mockTask({ id: "import-preview", projectId: "sample", status: "succeeded" }));
      }
      if (command === "get_import_preview") return Promise.resolve(preview);
      if (command === "confirm_import_preview") {
        return Promise.resolve({ preview, confirmedAt: new Date().toISOString() });
      }
      if (command === "scan_wiki") {
        return Promise.resolve({ root: { name: "", path: "", kind: "folder", children: [] }, pages: [], totalPages: 0 });
      }
      return Promise.resolve(null);
    });

    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    const sourcePathInput = screen.getByRole("textbox", { name: "Local file or folder paths" });
    fireEvent.change(sourcePathInput, { target: { value: "D:/tmp/sources/notes.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Add to preview" }));
    const confirmButton = await screen.findByRole("button", { name: "Confirm import & compile" });
    // Skip the compile step. Confirm still fires its payload; a wiki re-scan
    // now follows it so promoted wiki/sources/ pages appear without compiling.
    fireEvent.click(screen.getByRole("checkbox", { name: "Trigger Wiki compile after import" }));
    fireEvent.click(confirmButton);

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("confirm_import_preview", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          preview,
          createCheckpoint: true,
        },
      });
      expect(invokeMock).toHaveBeenCalledWith("scan_wiki", expect.anything());
      expect(screen.queryByRole("button", { name: "Confirm import & compile" })).not.toBeInTheDocument();
    });
  });

  it("loads indexed sources and requests backend confirmation before deleting one", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    const pendingAction = {
      id: "delete-source-1",
      actionType: "delete_source" as const,
      title: "Delete original source",
      message: "Checkpoint first.",
      riskLevel: "destructive" as const,
      affectedPaths: ["raw/sources/markdown/notes.md"],
      preview: null,
      expiresAt: null,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_imported_sources") {
        return Promise.resolve([
          {
            path: "raw/sources/markdown/notes.md",
            sizeBytes: 42,
            fileType: "markdown",
          },
        ]);
      }
      if (command === "request_delete_source") return Promise.resolve(pendingAction);
      if (command === "confirm_pending_action") {
        return Promise.resolve({
          action: pendingAction,
          status: "confirmed",
          checkpointExists: true,
          projectSummary: null,
        });
      }
      if (command === "start_wiki_compile") {
        return Promise.resolve(mockTask({ id: "source-follow-up-compile", projectId: "sample" }));
      }
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    fireEvent.click(await screen.findByText("Manage imported sources (1)"));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("request_delete_source", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          targetPath: "raw/sources/markdown/notes.md",
        },
      });
      expect(screen.getByRole("dialog", { name: "Delete original source" })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Confirm source deletion" }));
    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("start_wiki_compile", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          route: "auto",
          agent: null,
          provider: null,
        },
      });
    });
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });
});

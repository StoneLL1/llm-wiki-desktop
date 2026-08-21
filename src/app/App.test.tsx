import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../i18n";
import { useWikiStore } from "../features/wiki/wikiStore";
import { useGraphStore } from "../stores/graphStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";
import type { OpenedProject, ProjectSessionAuthority, ProjectSummary } from "../types/project";
import type { BackendTask } from "../types/task";
import type { WikiPageContent } from "../types/wiki";
import { App } from "./App";
import { PANE_WIDTH_LIMITS } from "../hooks/useResizablePane";

const invokeMock = vi.hoisted(() => vi.fn());
const openDialogMock = vi.hoisted(() => vi.fn());

function dispatchPointerEvent(
  target: Document | Element,
  type: string,
  clientX: number,
  pointerId = 1,
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clientX", { value: clientX });
  Object.defineProperty(event, "pointerId", { value: pointerId });
  fireEvent(target, event);
}

function installAnimationFrameHarness() {
  const callbacks = new Map<number, FrameRequestCallback>();
  let nextId = 1;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = nextId;
    nextId += 1;
    callbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    callbacks.delete(id);
  });

  return {
    callbacks,
    flush() {
      const pending = [...callbacks.values()];
      callbacks.clear();
      pending.forEach((callback) => callback(0));
    },
  };
}

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

const sampleAuthority = (project: ProjectSummary): ProjectSessionAuthority => ({
  projectId: project.projectId,
  canonicalRootPath: project.rootPath,
  canonicalIdentityKey: "identity-a",
  identityRevision: "revision-a",
  authorityRevision: "authority-a",
  format: "native_current",
  trust: "trusted",
  filesystemAccess: "writable",
  health: "healthy",
  layout: { markdownRoots: [{ path: "wiki", role: "wiki" }] },
  confidence: "high",
  capabilities: ["read_markdown", "project_write", "git_checkpoint"],
  warnings: [],
  layoutWarnings: [],
  git: { isRepository: true, branch: "main", head: "abc", hasChanges: false },
});

const sampleOpenedProject = (overrides: Partial<ProjectSummary> = {}): OpenedProject => {
  const summary = sampleProject(overrides);
  return { summary, authority: sampleAuthority(summary) };
};

beforeEach(() => {
  invokeMock.mockReset();
  openDialogMock.mockReset();
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  window.localStorage.clear();
  useToastStore.setState({ toasts: [] });
  useGraphStore.getState().reset();
  useNavigationStore.getState().setActiveView("dashboard");
  useNavigationStore.getState().setRightPanelOpen(true);
  useNavigationStore.getState().closeSettings();
  useNavigationStore.setState({ workspaceFocus: null, rightPanelOpenBeforeFocus: null });
  useNavigationStore.getState().setSidebarCollapsed(false);
  useNavigationStore.getState().resetPaneSize("sidebar");
  useNavigationStore.getState().resetPaneSize("rightPanel");
  useNavigationStore.getState().resetPaneSize("wikiTree");
  useNavigationStore.getState().resetPaneSize("exportsList");
  useNavigationStore.getState().resetPaneSize("lintDetails");
  useNavigationStore.getState().clearImportSuccessNotice();
  useNavigationStore.getState().clearPendingImportPath();
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
  it("does not offer an in-place ordinary-folder initialization action when no project is active", () => {
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

    expect(screen.getByRole("heading", { name: "Workspace" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /New knowledge base/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open existing knowledge base/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open folder as project/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /project path|open path|local file/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/Import materials into existing project/i)).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Primary" })).toBeInTheDocument();
  });

  it("opens existing projects through the native directory picker", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    openDialogMock.mockResolvedValue("D:\\知识库\\agent");
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "start_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a" });
      }
      if (command === "get_project_open_assessment") {
        return Promise.resolve({
          assessmentOperationId: "operation-a",
          status: "completed",
          assessment: {
            assessmentId: "assessment-a",
            canonicalRootPath: "D:/知识库/agent",
            canonicalIdentityKey: "identity-a",
            identityRevision: "revision-a",
            format: "native_current",
            trust: "trusted",
            filesystemAccess: "writable",
            health: "healthy",
            layout: { markdownRoots: [{ path: "wiki", role: "wiki" }] },
            confidence: "high",
            markers: [],
            capabilities: ["read_markdown", "project_write"],
            warnings: [],
            layoutWarnings: [],
            git: { isRepository: true, branch: "main", head: "abc", hasChanges: false },
          },
        });
      }
      if (command === "open_assessed_project") {
        return Promise.resolve(sampleOpenedProject({ rootPath: "D:/知识库/agent" }));
      }
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Open existing knowledge base/i }));

    await waitFor(() =>
      expect(openDialogMock).toHaveBeenCalledWith(
        expect.objectContaining({ directory: true, multiple: false }),
      ),
    );
    expect(invokeMock).toHaveBeenCalledWith("start_project_open_assessment", {
      request: { path: "D:\\知识库\\agent" },
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("open_assessed_project", {
        request: { assessmentId: "assessment-a" },
      }),
    );
  });

  it("shows launch picker errors instead of leaving an unhandled rejection", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    openDialogMock.mockRejectedValue(new Error("dialog unavailable"));
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Open existing knowledge base/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("dialog unavailable");
    expect(invokeMock).not.toHaveBeenCalledWith("open_project", expect.anything());
  });

  it("creates a project from a parent folder and project name", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    openDialogMock.mockResolvedValue("D:\\资料库");
    invokeMock.mockImplementation((command: string) => {
      if (command === "create_project") {
        return Promise.resolve(sampleOpenedProject({ rootPath: "D:/资料库/中文知识库", name: "中文知识库" }));
      }
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /New knowledge base/i }));
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

  it("uses the prepared Documents/LLM Wiki parent on the first new-project dialog", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
    invokeMock.mockImplementation((command: string) => {
      if (command === "prepare_default_project_parent") return Promise.resolve("D:\\Documents\\LLM Wiki");
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /New knowledge base/i }));

    expect(await screen.findByDisplayValue("D:\\Documents\\LLM Wiki")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("prepare_default_project_parent");
  });

  it("keeps a new-project creation failure visible inside the dialog", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", name: "", rootPath: "" }));
    openDialogMock.mockResolvedValue("D:\\Documents");
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "create_project") return Promise.reject(new Error("The selected directory is not empty."));
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /New knowledge base/i }));
    fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "Audit" } });
    fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("D:\\Documents\\Audit");
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    expect(await within(screen.getByRole("dialog")).findByRole("alert")).toHaveTextContent(
      "The selected directory is not empty.",
    );
  });

  it("keeps Settings available without showing Agent or provider setup on the workbench", () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", name: "", rootPath: "" }));

    render(<App />);

    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
    expect(screen.queryByText("Detected Agent CLIs")).not.toBeInTheDocument();
    expect(screen.queryByText("BYOK fallback")).not.toBeInTheDocument();
  });

  it("opens global language and theme preferences before a project is selected", async () => {
    useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", name: "", rootPath: "" }));
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_global_ui_preferences") return Promise.resolve({ language: "en", theme: "auto" });
      if (command === "save_global_ui_preferences") return Promise.resolve({ language: "en", theme: "dark" });
      return Promise.resolve([]);
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Low-glare shell for longer sessions." }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_global_ui_preferences", {
        preferences: { language: "en", theme: "dark" },
      }),
    );
  });

  it("returns to project selection when the project switcher is used", () => {
    useWikiStore.setState({ selectedPath: "wiki/old.md" });
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Switch project" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Back to workspace" })[0]);

    expect(screen.getByRole("heading", { name: "Workspace" })).toBeInTheDocument();
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
    const missingRow = screen.getByRole("button", { name: /^Missing project/ });
    fireEvent.click(missingRow);

    expect(missingRow).toHaveAttribute("aria-disabled", "true");
    expect(useProjectStore.getState().currentProject.rootPath).toBe("D:/Users/Aletta/Documents/wiki/agent-llm");
  });

  it("removes a missing recent project without touching the current project", async () => {
    const missing = {
      projectId: "missing-project",
      name: "Missing project",
      rootPath: "D:/Users/Aletta/Documents/wiki/missing-project",
      template: "general" as const,
      openedAt: "2026-07-04T00:00:00Z",
      wikiPageCount: 0,
      sourceCount: 0,
      taskCount: 0,
      indexState: "missing" as const,
      graphState: "missing" as const,
      missing: true,
    };
    useProjectStore.getState().setRecentProjects([missing]);
    invokeMock.mockResolvedValueOnce([]);
    render(<App />);

    const switcher = screen.getByRole("button", { name: "Switch project" });
    fireEvent.click(switcher);
    fireEvent.click(screen.getByRole("button", { name: "Remove Missing project from recent knowledge bases" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("remove_recent_project", {
      request: { projectId: missing.projectId, rootPath: missing.rootPath },
    }));
    expect(useProjectStore.getState().recentProjects).toEqual([]);
    expect(useProjectStore.getState().currentProject.rootPath).toBe("D:/Users/Aletta/Documents/wiki/agent-llm");
    expect(switcher).toHaveFocus();
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
    const locateMissing = await screen.findByRole("button", {
      name: "Locate the moved Missing project knowledge base",
    });

    await waitFor(() => expect(locateMissing).toHaveFocus());
    fireEvent.keyDown(locateMissing, { key: "Escape" });

    expect(screen.queryByRole("button", { name: "Locate the moved Missing project knowledge base" })).not.toBeInTheDocument();
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
    expect(await screen.findByRole("button", { name: /^Enabled project/ })).toBeInTheDocument();

    fireEvent.keyDown(switcher, { key: "Escape" });

    expect(screen.queryByRole("button", { name: /Enabled project/ })).not.toBeInTheDocument();
    expect(switcher).toHaveFocus();

    fireEvent.click(switcher);
    switcher.focus();
    const enabledRow = await screen.findByRole("button", { name: /^Enabled project/ });
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

  it("keeps context and status surfaces visible while navigation changes", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Chat" }));

    expect(await screen.findByRole(
      "complementary",
      { name: "Sources" },
      { timeout: 5_000 },
    )).toBeInTheDocument();
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
      projectKey: "sample\0D:/Users/Aletta/Documents/wiki/agent-llm",
      load: async () => {},
    });
    render(<App />);
    await screen.findByRole("heading", { name: "Graph" });

    fireEvent.click(await screen.findByRole(
      "button",
      { name: "Focus neighbors" },
      { timeout: 5_000 },
    ));

    expect(useGraphStore.getState().focusedNodeId).toBe("a");
  }, 15_000);

  it("collapses and restores the right context panel", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "Collapse context panel" })).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(screen.getByRole("button", { name: "Collapse context panel" }));
    expect(screen.queryByRole("complementary", { name: "Project info" })).not.toBeInTheDocument();

    expect(screen.getByRole("button", { name: "Open context panel" })).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(screen.getByRole("button", { name: "Open context panel" }));
    expect(screen.getByRole("complementary", { name: "Project info" })).toBeInTheDocument();
  });

  it("keeps the sidebar splitter reachable when the sidebar pane size reaches the collapse threshold", () => {
    // The topbar collapse button was intentionally removed; the sidebar now
    // collapses only by resizing the splitter below SIDEBAR_COLLAPSE_THRESHOLD
    // (derived in AppShell from paneSizes.sidebar). This pins that the manual
    // collapse/expand controls stay removed and the splitter remains the single
    // resize affordance. The pane size is set directly via the store rather than
    // via simulated pointer events; useResizablePane's drag/clamp path has its
    // own coverage in useResizablePane.test.ts.
    render(<App />);

    expect(screen.queryByRole("button", { name: "Collapse sidebar" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Expand sidebar" })).not.toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Resize sidebar" })).toBeInTheDocument();

    useNavigationStore.getState().setPaneSize("sidebar", PANE_WIDTH_LIMITS.sidebar.min);

    expect(useNavigationStore.getState().sidebarCollapsed).toBe(true);
    expect(screen.getByRole("separator", { name: "Resize sidebar" })).toBeInTheDocument();
    for (const label of ["Dashboard", "Wiki", "Chat", "Graph", "Workflows", "Import", "Lint", "Exports"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("does not emit jsdom canvas getContext noise when the graph view renders", async () => {
    // Sigma inits a WebGL renderer on a real <canvas>; in jsdom that used to
    // log "Not implemented: HTMLCanvasElement.prototype.getContext" three
    // times per render (once per webgl2/webgl/experimental-webgl probe).
    // test/setup.ts overrides getContext to return null cleanly so GraphView
    // falls back to its "canvas unavailable" placeholder without flooding the
    // output.
    //
    // jsdom routes "Not implemented:" through window._virtualConsole and out
    // to the test runner's stderr in a way that is not reliably capturable via
    // vi.spyOn (the routing differs across jsdom versions and Vitest's jsdom
    // env). So this regression guard uses three complementary checks:
    //   1. Pin the stub itself — setup.ts tags its override with
    //      `__silencedJSDOMCanvasNoise`. If someone removes the stub, this
    //      fails immediately and the noise returns across every graph test.
    //   2. Confirm the end-to-end fallback still renders, so we also catch a
    //      stub that exists but no longer returns null.
    //   3. Confirm GraphView's own `[graph] sigma renderer init failed:`
    //      console.warn still fires — proving the stub removes jsdom noise
    //      without suppressing GraphView's real diagnostics (Batch 1 goal:
    //      do not mask real problems via global console suppression).
    const getContext = HTMLCanvasElement.prototype.getContext as typeof HTMLCanvasElement.prototype.getContext & {
      __silencedJSDOMCanvasNoise?: boolean;
    };
    expect(getContext.__silencedJSDOMCanvasNoise).toBe(true);
    expect(document.createElement("canvas").getContext("webgl2")).toBeNull();

    const warnSpy = vi.spyOn(console, "warn");
    useNavigationStore.getState().setActiveView("graph");
    useGraphStore.setState({
      status: "ready",
      data: {
        nodes: [
          { id: "a", path: "wiki/a.md", label: "Alpha", type: "concept", tags: [], starred: false, degree: 1 },
          { id: "b", path: "wiki/b.md", label: "Beta", type: "entity", tags: [], starred: false, degree: 1 },
        ],
        edges: [{ source: "a", target: "b", relation: "related", weight: 1 }],
        contentHash: "regression-canvas-noise-hash",
        builtAt: "2026-07-04T00:00:00Z",
        layout: { positions: { a: [0, 0], b: [1, 1] }, communities: { a: 0, b: 0 } },
      },
      load: async () => {},
    });

    render(<App />);
    // GraphView is React.lazy behind the WorkspaceView Suspense boundary
    // (Batch 3 bundle split), so the placeholder only appears after the async
    // chunk resolves AND Sigma's init effect runs and catches. Under the full
    // suite the lazy resolution plus React re-render can exceed findByText's
    // 1000ms default. Native capability tests and full parallel CI can also
    // contend for CPU, so keep this assertion bounded without using the
    // single-test timing as the suite-wide budget.
    expect(
      await screen.findByText(
        "Graph canvas is unavailable in this environment.",
        {},
        { timeout: 15_000 },
      ),
    ).toBeInTheDocument();
    // GraphView's catch logs its own diagnostic; the stub must not suppress it.
    expect(warnSpy).toHaveBeenCalledWith("[graph] sigma renderer init failed:", expect.any(Error));
    warnSpy.mockRestore();
  }, 20_000);

  it("exposes keyboard-resizable shell splitters", () => {
    render(<App />);

    const sidebarSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    const rightPanelSplitter = screen.getByRole("separator", { name: "Resize context panel" });

    fireEvent.keyDown(sidebarSplitter, { key: "ArrowRight" });

    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(
      PANE_WIDTH_LIMITS.sidebar.defaultValue + 12,
    );
    expect(sidebarSplitter).toHaveAttribute(
      "aria-valuenow",
      String(PANE_WIDTH_LIMITS.sidebar.defaultValue + 12),
    );
    fireEvent.keyDown(sidebarSplitter, { key: "Home" });
    expect(sidebarSplitter).toHaveAttribute("aria-valuenow", String(PANE_WIDTH_LIMITS.sidebar.min));
    fireEvent.keyDown(sidebarSplitter, { key: "End" });
    expect(sidebarSplitter).toHaveAttribute("aria-valuenow", String(PANE_WIDTH_LIMITS.sidebar.max));
    fireEvent.keyDown(sidebarSplitter, { key: "Enter" });
    expect(sidebarSplitter).toHaveAttribute(
      "aria-valuenow",
      String(PANE_WIDTH_LIMITS.sidebar.defaultValue),
    );
    expect(rightPanelSplitter).toHaveAttribute("aria-valuenow", String(PANE_WIDTH_LIMITS.rightPanel.defaultValue));

    fireEvent.click(screen.getByRole("button", { name: "Collapse context panel" }));
    expect(screen.queryByRole("separator", { name: "Resize context panel" })).not.toBeInTheDocument();
  });

  it("previews a two-second pane drag outside the store and persists only the final commit", () => {
    const animationFrames = installAnimationFrameHarness();
    render(<App />);

    const shell = document.querySelector<HTMLElement>(".app-shell");
    const sidebarSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    const storageSpy = vi.spyOn(Storage.prototype, "setItem");

    dispatchPointerEvent(sidebarSplitter, "pointerdown", 240);
    for (let move = 1; move <= 120; move += 1) {
      dispatchPointerEvent(document, "pointermove", 240 + move);
    }

    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(240);
    expect(storageSpy).not.toHaveBeenCalled();
    expect(animationFrames.callbacks.size).toBe(1);

    animationFrames.flush();
    expect(shell?.style.getPropertyValue("--sidebar-w-current")).toBe("360px");
    expect(sidebarSplitter).toHaveAttribute("aria-valuenow", "360");
    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(240);

    dispatchPointerEvent(document, "pointerup", 360);

    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(360);
    expect(storageSpy).toHaveBeenCalledTimes(1);
    storageSpy.mockRestore();
  });

  it("changes sidebar collapse state only on commit and keeps the splitter usable", () => {
    const animationFrames = installAnimationFrameHarness();
    render(<App />);

    const shell = document.querySelector<HTMLElement>(".app-shell");
    const sidebarSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    dispatchPointerEvent(sidebarSplitter, "pointerdown", 240);
    dispatchPointerEvent(document, "pointermove", 80);
    animationFrames.flush();

    expect(useNavigationStore.getState().sidebarCollapsed).toBe(false);
    expect(shell).not.toHaveClass("is-sidebar-collapsed");
    expect(sidebarSplitter).toHaveAttribute("aria-valuenow", "80");

    dispatchPointerEvent(document, "pointerup", 80);
    expect(useNavigationStore.getState().sidebarCollapsed).toBe(true);
    expect(shell).toHaveClass("is-sidebar-collapsed");

    const collapsedSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    dispatchPointerEvent(collapsedSplitter, "pointerdown", 80, 2);
    dispatchPointerEvent(document, "pointerup", 160, 2);

    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(160);
    expect(useNavigationStore.getState().sidebarCollapsed).toBe(false);
    expect(shell).not.toHaveClass("is-sidebar-collapsed");
  });

  it("rolls back splitter DOM preview without persisting on pointercancel", () => {
    const animationFrames = installAnimationFrameHarness();
    render(<App />);

    const shell = document.querySelector<HTMLElement>(".app-shell");
    const sidebarSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    const storageSpy = vi.spyOn(Storage.prototype, "setItem");

    dispatchPointerEvent(sidebarSplitter, "pointerdown", 240);
    dispatchPointerEvent(document, "pointermove", 300);
    animationFrames.flush();
    expect(shell?.style.getPropertyValue("--sidebar-w-current")).toBe("300px");
    expect(sidebarSplitter).toHaveAttribute("aria-valuenow", "300");

    dispatchPointerEvent(document, "pointercancel", 300);

    expect(shell?.style.getPropertyValue("--sidebar-w-current")).toBe("240px");
    expect(sidebarSplitter).toHaveAttribute("aria-valuenow", "240");
    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(240);
    expect(storageSpy).not.toHaveBeenCalled();
    storageSpy.mockRestore();
  });

  it("persists a double-click reset once without no-op pointerup commits", () => {
    render(<App />);

    useNavigationStore.getState().setPaneSize("sidebar", 300);
    const sidebarSplitter = screen.getByRole("separator", { name: "Resize sidebar" });
    const storageSpy = vi.spyOn(Storage.prototype, "setItem");

    dispatchPointerEvent(sidebarSplitter, "pointerdown", 300);
    dispatchPointerEvent(document, "pointerup", 300);
    dispatchPointerEvent(sidebarSplitter, "pointerdown", 300, 2);
    dispatchPointerEvent(document, "pointerup", 300, 2);
    fireEvent.doubleClick(sidebarSplitter);

    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(
      PANE_WIDTH_LIMITS.sidebar.defaultValue,
    );
    expect(storageSpy).toHaveBeenCalledTimes(1);
    storageSpy.mockRestore();
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
      actionType: "overwrite_file",
      title: "Overwrite page",
      message: "Replace the generated page.",
      riskLevel: "high",
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
      actionType: "overwrite_file",
      title: "Overwrite page",
      message: "Replace the generated page.",
      riskLevel: "high",
      affectedPaths: ["report.pdf"],
      preview: null,
      expiresAt: null,
    });

    render(<App />);

    expect(screen.getByRole("dialog", { name: "Overwrite page" })).toBeInTheDocument();
    expect(screen.getByText("report.pdf")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(useProjectStore.getState().pendingAction).toBeUndefined();
  });

});

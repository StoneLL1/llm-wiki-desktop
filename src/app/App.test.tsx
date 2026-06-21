import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../i18n";
import { useWikiStore } from "../features/wiki/wikiStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";
import type { ProjectSummary } from "../types/project";
import type { BackendTask } from "../types/task";
import type { WikiPageContent } from "../types/wiki";
import { App } from "./App";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
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
  useToastStore.setState({ toasts: [] });
  useNavigationStore.getState().setActiveView("dashboard");
  useProjectStore.getState().setCurrentProject(
    sampleProject({
      rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
      agentRoute: "agent",
    }),
  );
  useProjectStore.getState().setPendingAction(undefined);
  useTaskStore.getState().setTasks([
    mockTask({ id: "task-graph-refresh", title: "Refreshing graph cache", status: "running" }),
  ]);
  void i18next.changeLanguage("en");
});

afterEach(() => {
  cleanup();
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
    expect(screen.getByRole("button", { name: "New project" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open folder" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Primary" })).not.toBeInTheDocument();
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
      return Promise.resolve(null);
    });

    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    const sourcePathInput = screen.getByRole("textbox", { name: "Local file or folder paths" });
    fireEvent.change(sourcePathInput, { target: { value: "D:/tmp/sources/notes.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Add to preview" }));
    const confirmButton = await screen.findByRole("button", { name: "Confirm import & compile" });
    // Skip the compile step so the last backend call stays the confirm itself.
    fireEvent.click(screen.getByRole("checkbox", { name: "Trigger Wiki compile after import" }));
    fireEvent.click(confirmButton);

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenLastCalledWith("confirm_import_preview", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          preview,
          createCheckpoint: true,
        },
      });
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

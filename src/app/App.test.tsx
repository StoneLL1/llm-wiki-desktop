import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../i18n";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import type { ProjectSummary } from "../types/project";
import type { BackendTask } from "../types/task";
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
  useNavigationStore.getState().setActiveView("dashboard");
  useProjectStore.getState().setCurrentProject(
    sampleProject({
      rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
      agentRoute: "agent",
    }),
  );
  useTaskStore.getState().setTasks([
    mockTask({ id: "task-graph-refresh", title: "Refreshing graph cache", status: "running" }),
  ]);
  void i18next.changeLanguage("en");
});

afterEach(() => {
  cleanup();
});

describe("App", () => {
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
    expect(screen.getByText("Route: Agent")).toBeInTheDocument();
    expect(screen.getByText("Tasks: 1 running")).toBeInTheDocument();
    expect(screen.getByText("Wiki pages: 237")).toBeInTheDocument();
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
    expect(screen.getByText("Route: BYOK")).toBeInTheDocument();
    expect(screen.getByText("Tasks: 2 running")).toBeInTheDocument();
    expect(screen.getByText("Wiki pages: 42")).toBeInTheDocument();
  });

  it("switches language from the top bar", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "中文" }));

    expect(await screen.findByRole("button", { name: "图谱" })).toBeInTheDocument();
    expect(screen.getByText("路线: Agent")).toBeInTheDocument();
  });

  it("opens settings from the top bar settings button", () => {
    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Settings" }).at(-1)!);

    expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
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
    invokeMock.mockResolvedValueOnce(preview);

    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(["# Notes"], "notes.md", { type: "text/markdown" });
    Object.defineProperty(file, "path", {
      value: "D:/tmp/sources/notes.md",
    });

    fireEvent.change(input, { target: { files: [file] } });

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

  it("confirms the current import preview and clears it after success", async () => {
    const preview = {
      files: [],
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
    invokeMock
      .mockResolvedValueOnce(preview)
      .mockResolvedValueOnce({
        preview,
        confirmedAt: new Date().toISOString(),
      });

    render(<App />);

    fireEvent.click(screen.getAllByRole("button", { name: "Import" })[0]);
    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(["# Notes"], "notes.md", { type: "text/markdown" });
    Object.defineProperty(file, "path", {
      value: "D:/tmp/sources/notes.md",
    });

    fireEvent.change(input, { target: { files: [file] } });
    const confirmButton = await screen.findByRole("button", { name: "Confirm & Compile" });
    fireEvent.click(confirmButton);

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenLastCalledWith("confirm_import_preview", {
        request: {
          projectId: "sample",
          projectRootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
          preview,
        },
      });
      expect(screen.queryByRole("button", { name: "Confirm & Compile" })).not.toBeInTheDocument();
    });
  });
});

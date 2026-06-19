import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { i18next } from "../i18n";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import type { ProjectSummary } from "../types/project";
import { App } from "./App";

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
  useNavigationStore.getState().setActiveView("dashboard");
  useProjectStore.getState().setCurrentProject(
    sampleProject({
      rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
      agentRoute: "agent",
    }),
  );
  useTaskStore.getState().setTasks([
    {
      id: "task-graph-refresh",
      title: "Refreshing graph cache",
      status: "running",
    },
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

    fireEvent.click(screen.getByRole("button", { name: "Import" }));

    expect(screen.getByRole("button", { name: "Import" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Import" })).toBeInTheDocument();
  });

  it("keeps context and status surfaces visible while navigation changes", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Chat" }));

    expect(screen.getByRole("complementary", { name: "Context" })).toBeInTheDocument();
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
      { id: "task-import", title: "Parsing sources", status: "running" },
      { id: "task-lint", title: "Running local lint", status: "running" },
      { id: "task-export", title: "Export complete", status: "succeeded" },
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
    expect(screen.getAllByRole("button", { name: "Settings" }).some((button) => button.getAttribute("aria-current") === "page")).toBe(true);
  });
});

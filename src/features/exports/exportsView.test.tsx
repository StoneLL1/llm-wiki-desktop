import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useExportStore } from "../../stores/exportStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import type { ProjectSummary } from "../../types/project";
import { ExportsView } from "./ExportsView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

const PROJECT = {
  ...defaultProject,
  projectId: "p",
  rootPath: "/x",
  name: "Test",
  agentRoute: "agent",
} satisfies ProjectSummary;

describe("ExportsView", () => {
  it("renders the empty state and new-export control before any export", () => {
    useExportStore.getState().reset();
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);
    expect(screen.getByRole("button", { name: /New export/i })).toBeInTheDocument();
    expect(screen.getByText(/Run an export/i)).toBeInTheDocument();
  });

  it("exposes a resizable export list splitter", () => {
    useExportStore.getState().reset();
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);
    expect(screen.getByRole("separator", { name: "Resize export list" })).toHaveAttribute("aria-valuemax", "480");
  });

  it("lists succeeded exports in the table with a success badge and preview action", () => {
    useExportStore.getState().reset();
    useExportStore.setState({
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);
    expect(screen.getByText("Agent")).toBeInTheDocument();
    expect(screen.getByText("agent-1.html")).toBeInTheDocument();
    expect(screen.getByTitle("exports/html/agent-1.html")).toBeInTheDocument();
    expect(screen.getByText("Succeeded")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Preview/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open in browser/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open folder/i })).toBeInTheDocument();
  });

  it("loads preview when a succeeded export row is clicked", () => {
    const loadPreview = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      loadPreview,
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    fireEvent.click(screen.getByRole("row", { name: /Agent/ }));

    expect(loadPreview).toHaveBeenCalledWith(
      { projectId: "p", projectRootPath: "/x", outputPath: "exports/html/agent-1.html" },
      "export-1",
    );
  });

  it("loads preview when a succeeded export row is activated from the keyboard", () => {
    const loadPreview = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      loadPreview,
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    fireEvent.keyDown(screen.getByRole("row", { name: /Agent/ }), { key: "Enter" });

    expect(loadPreview).toHaveBeenCalledWith(
      { projectId: "p", projectRootPath: "/x", outputPath: "exports/html/agent-1.html" },
      "export-1",
    );
  });

  it("does not preview failed exports from row click or keyboard activation", () => {
    const loadPreview = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      loadPreview,
      records: [
        {
          id: "export-failed",
          exportType: "project_report",
          title: "Project report",
          outputPath: "exports/html/project-report-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "agent",
          status: "failed",
          bookmarked: false,
          taskId: "task-failed",
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    const row = screen.getByRole("row", { name: /Project report/ });
    fireEvent.click(row);
    fireEvent.keyDown(row, { key: "Enter" });

    expect(loadPreview).not.toHaveBeenCalled();
  });

  it("opens a succeeded export in the browser without triggering row preview", () => {
    const loadPreview = vi.fn();
    const openInBrowser = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      loadPreview,
      openInBrowser,
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    fireEvent.click(screen.getByRole("button", { name: /Open in browser/i }));

    expect(openInBrowser).toHaveBeenCalledWith({
      projectId: "p",
      projectRootPath: "/x",
      outputPath: "exports/html/agent-1.html",
    });
    expect(loadPreview).not.toHaveBeenCalled();
  });

  it("keeps inline row action keyboard events from triggering row preview", () => {
    const loadPreview = vi.fn();
    const openFolder = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      loadPreview,
      openFolder,
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    const openFolderButton = screen.getByRole("button", { name: /Open folder/i });
    fireEvent.keyDown(openFolderButton, { key: "Enter" });
    fireEvent.click(openFolderButton);

    expect(openFolder).toHaveBeenCalledWith({
      projectId: "p",
      projectRootPath: "/x",
      outputPath: "exports/html/agent-1.html",
    });
    expect(loadPreview).not.toHaveBeenCalled();
  });

  it("switches the preview pane to source mode and can focus the preview workspace", () => {
    useExportStore.getState().reset();
    useExportStore.setState({
      previewHtml: "<!doctype html><html><body>Agent</body></html>",
      previewId: "export-1",
      previewMode: "inline",
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useNavigationStore.setState({ workspaceFocus: null, rightPanelOpenBeforeFocus: null });
    useProjectStore.setState({ currentProject: PROJECT });
    const { container } = render(<ExportsView />);

    const toolbar = screen.getByRole("toolbar", { name: "Preview tools" });
    fireEvent.click(within(toolbar).getByRole("button", { name: "HTML source" }));

    expect(useExportStore.getState().previewMode).toBe("source");
    expect(screen.getByText(/<!doctype html>/)).toBeInTheDocument();

    fireEvent.click(within(toolbar).getByRole("button", { name: "Focus preview" }));

    expect(useNavigationStore.getState().workspaceFocus).toBe("exportPreview");
    expect(container.querySelector(".exports-view-layout.is-preview-focused")).not.toBeNull();
  });

  it("clears workspace focus when the focused preview is cleared", () => {
    const clearPreview = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      clearPreview,
      previewHtml: "<!doctype html><html><body>Agent</body></html>",
      previewId: "export-1",
      previewMode: "inline",
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useNavigationStore.setState({ workspaceFocus: "exportPreview", rightPanelOpenBeforeFocus: true });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    fireEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(clearPreview).toHaveBeenCalled();
    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
    expect(useNavigationStore.getState().rightPanelOpenBeforeFocus).toBeNull();
  });

  it("clears export preview focus when the exports view unmounts", () => {
    useExportStore.getState().reset();
    useExportStore.setState({
      previewHtml: "<!doctype html><html><body>Agent</body></html>",
      previewId: "export-1",
      previewMode: "inline",
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useNavigationStore.setState({ workspaceFocus: "exportPreview", rightPanelOpenBeforeFocus: true });
    useProjectStore.setState({ currentProject: PROJECT });

    const { unmount } = render(<ExportsView />);
    unmount();

    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
    expect(useNavigationStore.getState().rightPanelOpenBeforeFocus).toBeNull();
  });

  it("toggles a succeeded export bookmark from the row action", () => {
    const toggleBookmark = vi.fn();
    useExportStore.getState().reset();
    useExportStore.setState({
      toggleBookmark,
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    fireEvent.click(screen.getByRole("button", { name: "Bookmark export" }));

    expect(toggleBookmark).toHaveBeenCalledWith("p", "/x", "export-1");
  });

  it("labels bookmarked rows as unbookmark actions", () => {
    useExportStore.getState().reset();
    useExportStore.setState({
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
          bookmarked: true,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);

    expect(screen.getByRole("button", { name: "Unbookmark export" })).toBeInTheDocument();
  });

  it("renders failed exports with a danger badge and retry control", () => {
    useExportStore.getState().reset();
    useExportStore.setState({
      records: [
        {
          id: "export-failed",
          exportType: "project_report",
          title: "Project report",
          outputPath: "exports/html/project-report-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "agent",
          status: "failed",
          bookmarked: false,
          taskId: "task-failed",
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT });
    render(<ExportsView />);
    expect(screen.getByText("Failed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Retry/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /View logs/i })).toBeInTheDocument();
    // success-only actions must not appear for failed rows
    expect(screen.queryByRole("button", { name: /Bookmark export/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Preview/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open folder/i })).not.toBeInTheDocument();
  });

  it("routes new export through structured Workflows preparation", () => {
    useExportStore.getState().reset();
    useProjectStore.setState({ currentProject: PROJECT });
    useNavigationStore.setState({ workflowLaunchIntent: null, activeView: "exports" });
    render(<ExportsView />);
    fireEvent.click(screen.getByRole("button", { name: /New export/i }));
    expect(useNavigationStore.getState().activeView).toBe("workflows");
    expect(useNavigationStore.getState().workflowLaunchIntent).toEqual({
      projectId: "p",
      projectRootPath: "/x",
      kind: "generate_content",
      origin: "exports",
      scopePreset: {
        kind: "generate_content",
        artifactType: "beautiful_read",
        pagePaths: [],
        outputPath: null,
      },
    });
    // beautiful_read (default) requires a source page → disabled.
    // project_report needs no source → enabled.
  });
});

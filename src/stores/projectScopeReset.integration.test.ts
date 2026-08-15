import { afterEach, describe, expect, it, vi } from "vitest";

import { useSourceStore } from "../features/wiki/sourceStore";
import { useWikiStore } from "../features/wiki/wikiStore";
import type { ProjectSummary } from "../types/project";
import type { BackendTask } from "../types/task";
import { useChatStore } from "./chatStore";
import { useExportStore } from "./exportStore";
import { useGraphStore } from "./graphStore";
import { useImportStore } from "./importStore";
import { useLintStore } from "./lintStore";
import { useNavigationStore } from "./navigationStore";
import { defaultProject, useProjectStore } from "./projectStore";
import { useSettingsStore } from "./settingsStore";
import { useTaskStore } from "./taskStore";
import { useWorkflowStore } from "./workflowStore";

function project(projectId: string, rootPath: string): ProjectSummary {
  return {
    ...defaultProject,
    projectId,
    name: projectId,
    rootPath,
    health: { ...defaultProject.health },
  };
}

function task(id: string): BackendTask {
  return {
    id,
    taskType: "import",
    projectId: "project-a",
    title: id,
    status: "running",
    progress: null,
    startedAt: "2026-08-16T00:00:00Z",
    updatedAt: "2026-08-16T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

const originalCancelPendingActions = useLintStore.getState().cancelPendingActions;

afterEach(() => {
  useLintStore.setState({ cancelPendingActions: originalCancelPendingActions });
  useProjectStore.getState().clearCurrentProject();
  useTaskStore.setState({
    taskFacts: {},
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
  });
});

describe("project-scope reset integration", () => {
  it("resets every loaded feature presentation while preserving global task facts", () => {
    useProjectStore.getState().setCurrentProject(project("project-a", "D:/wiki-a"));

    useWikiStore.setState({ selectedPath: "wiki/a.md", draft: "project A" });
    useSourceStore.setState({ error: "project A source" });
    useChatStore.setState({ error: "project A chat", streamingText: "partial" });
    useExportStore.setState({ previewId: "export-a", previewHtml: "<p>A</p>" });
    useGraphStore.setState({ projectKey: "project-a\0D:/wiki-a", selectedNodeId: "node-a", search: "A" });
    useImportStore.setState({ projectKey: "project-a\0D:/wiki-a", selectedItemId: "item-a", filter: "failed" });
    const cancelPendingActions = vi.fn(async () => undefined);
    useLintStore.setState({ selectedIssueId: "issue-a", cancelPendingActions });
    useSettingsStore.setState({ loadedProjectKey: "project-a:D:/wiki-a" });
    useWorkflowStore.setState({ projectKey: "project-a\0D:/wiki-a", selectedTaskId: "workflow-a" });
    useNavigationStore.setState({
      activeView: "graph",
      rightPanelMode: "wikiAssistant",
      wikiAssistantPagePath: "wiki/a.md",
    });
    const globalTask = task("task-a");
    useTaskStore.getState().setTasks([globalTask]);

    useProjectStore.getState().setCurrentProject(project("project-b", "D:/wiki-b"));

    expect(useWikiStore.getState()).toMatchObject({ selectedPath: null, draft: "" });
    expect(useSourceStore.getState().error).toBeNull();
    expect(useChatStore.getState()).toMatchObject({ error: null, streamingText: "" });
    expect(useExportStore.getState()).toMatchObject({ previewId: null, previewHtml: null });
    expect(useGraphStore.getState()).toMatchObject({ projectKey: null, selectedNodeId: null, search: "" });
    expect(useImportStore.getState()).toMatchObject({ projectKey: null, selectedItemId: null, filter: "all" });
    expect(cancelPendingActions).toHaveBeenCalledOnce();
    expect(useLintStore.getState().selectedIssueId).toBeNull();
    expect(useSettingsStore.getState().loadedProjectKey).toBeNull();
    expect(useWorkflowStore.getState()).toMatchObject({ projectKey: "", selectedTaskId: null });
    expect(useNavigationStore.getState()).toMatchObject({
      activeView: "dashboard",
      rightPanelMode: "default",
      wikiAssistantPagePath: null,
    });
    expect(useTaskStore.getState().tasks).toEqual([globalTask]);
    expect(useTaskStore.getState().taskFacts[globalTask.id]).toEqual(globalTask);
  });
});

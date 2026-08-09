import { act, render } from "@testing-library/react";
import { Profiler } from "react";
import { beforeEach, describe, expect, it } from "vitest";

import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { BackendTask } from "../../types/task";
import { RightContextPanel } from "./RightContextPanel";

const task: BackendTask = {
  id: "task-0",
  taskType: "import",
  projectId: "project-a",
  title: "Background task",
  status: "running",
  progress: null,
  startedAt: "2026-08-10T00:00:00Z",
  updatedAt: "2026-08-10T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
};

describe("RightContextPanel route-local subscriptions", () => {
  beforeEach(() => {
    useNavigationStore.setState({ activeView: "workflows", rightPanelMode: "default" });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-a",
        name: "Project A",
        rootPath: "D:/project-a",
      },
    });
    useWorkflowStore.getState().reset();
    useWorkflowStore.getState().activateProject("project-a\0D:/project-a");
    useTaskStore.setState({ tasks: [], logs: {}, activities: {}, taskOutputs: {} });
  });

  it("does not re-render the Workflows right panel for unrelated task-store updates", () => {
    let commits = 0;
    render(
      <Profiler id="workflows-right-panel" onRender={() => { commits += 1; }}>
        <RightContextPanel />
      </Profiler>,
    );
    const initialCommits = commits;

    for (let index = 0; index < 100; index += 1) {
      act(() => {
        useTaskStore.setState({
          tasks: [{ ...task, id: `task-${index}`, updatedAt: `2026-08-10T00:00:${String(index).padStart(2, "0")}Z` }],
        });
      });
    }

    expect(commits - initialCommits).toBe(0);
  });

  it("does not re-render the Wiki right panel for unrelated task-store updates", () => {
    useNavigationStore.setState({ activeView: "wiki" });
    let commits = 0;
    render(
      <Profiler id="wiki-right-panel" onRender={() => { commits += 1; }}>
        <RightContextPanel />
      </Profiler>,
    );
    const initialCommits = commits;

    for (let index = 0; index < 100; index += 1) {
      act(() => {
        useTaskStore.setState({
          tasks: [{ ...task, id: `wiki-unrelated-${index}` }],
        });
      });
    }

    expect(commits - initialCommits).toBe(0);
  });

  it("does not re-render the Wiki right panel for summary-only project updates", () => {
    useNavigationStore.setState({ activeView: "wiki" });
    let commits = 0;
    render(
      <Profiler id="wiki-project-updates" onRender={() => { commits += 1; }}>
        <RightContextPanel />
      </Profiler>,
    );
    const initialCommits = commits;

    for (let index = 0; index < 100; index += 1) {
      act(() => {
        useProjectStore.setState({ pendingAction: { id: `unrelated-${index}` } as never });
      });
    }

    expect(commits - initialCommits).toBe(0);
  });
});

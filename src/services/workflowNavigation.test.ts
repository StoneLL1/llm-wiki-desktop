import { beforeEach, describe, expect, it, vi } from "vitest";

const getWorkflowRunMock = vi.hoisted(() => vi.fn());

vi.mock("./workflowApi", () => ({ getWorkflowRun: getWorkflowRunMock }));

import { useWikiStore } from "../features/wiki/wikiStore";
import { useProjectStore, defaultProject } from "../stores/projectStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useWorkflowStore } from "../stores/workflowStore";
import type { WorkflowRun } from "../types/workflow";
import { hydrateAndSelectWorkflowRun, openWorkflowResult } from "./workflowNavigation";

const project = { projectId: "project-a", rootPath: "D:/知识库" };

function completedUpdate(): WorkflowRun {
  return {
    schemaVersion: 1,
    taskId: "run-a",
    projectId: project.projectId,
    canonicalIdentityKey: "identity-a",
    identityRevision: "revision-a",
    kind: "update_wiki",
    displayStatus: "completed",
    scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
    route: null,
    fingerprint: "fingerprint-a",
    baselineFingerprint: "baseline-a",
    stages: [],
    currentStageId: null,
    queuePosition: null,
    continuationRequired: false,
    retry: null,
    pendingAction: null,
    result: {
      kind: "update_wiki",
      created: 0,
      updated: 1,
      skipped: 0,
      deleted: 1,
      conflicted: 0,
      checkpointHash: "checkpoint-a",
      finalCommit: "final-a",
      affectedPaths: ["wiki/deleted.md", "wiki/existing.md"],
    },
    error: null,
    startedAt: "2026-08-02T00:00:00Z",
    updatedAt: "2026-08-02T00:01:00Z",
    completedAt: "2026-08-02T00:01:00Z",
  };
}

beforeEach(() => {
  getWorkflowRunMock.mockReset();
  useWikiStore.getState().reset();
  useWorkflowStore.getState().reset();
  useProjectStore.setState({
    currentProject: { ...defaultProject, ...project },
    authority: {
      projectId: project.projectId,
      canonicalRootPath: project.rootPath,
      canonicalIdentityKey: "identity-a",
      identityRevision: "revision-a",
    } as never,
  });
  useWorkflowStore.getState().activateProject(`${project.projectId}\0${project.rootPath}`);
  useWorkflowStore.getState().setOverviewSnapshot({
    schemaVersion: 1,
    projectAccess: {
      projectId: project.projectId,
      canonicalIdentityKey: "identity-a",
      identityRevision: "revision-a",
      trust: "trusted",
      filesystemAccess: "writable",
      persistence: "persistent",
      gitState: "clean",
    },
    rows: [],
  });
});

describe("workflow navigation", () => {
  it("does not inject a notification run after the user switches projects", async () => {
    let resolveRun!: (run: WorkflowRun) => void;
    getWorkflowRunMock.mockReturnValue(new Promise<WorkflowRun>((resolve) => { resolveRun = resolve; }));
    const opening = hydrateAndSelectWorkflowRun(project, "run-a");
    useProjectStore.setState({
      currentProject: { ...defaultProject, projectId: "project-b", rootPath: "D:/other" },
    });
    resolveRun(completedUpdate());
    await expect(opening).rejects.toThrow("WORKFLOW_PROJECT_CHANGED");
    expect(useWorkflowStore.getState().runs).toEqual([]);
  });

  it("does not select a run after same-root identity replacement", async () => {
    let resolveRun!: (run: WorkflowRun) => void;
    getWorkflowRunMock.mockReturnValue(new Promise<WorkflowRun>((resolve) => { resolveRun = resolve; }));
    const opening = hydrateAndSelectWorkflowRun(project, "run-a");
    useProjectStore.setState({
      authority: {
        ...useProjectStore.getState().authority!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    });
    useWorkflowStore.getState().setOverviewSnapshot({
      schemaVersion: 1,
      projectAccess: {
        projectId: project.projectId,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
        trust: "trusted",
        filesystemAccess: "writable",
        persistence: "persistent",
        gitState: "clean",
      },
      rows: [],
    });
    resolveRun(completedUpdate());

    await expect(opening).rejects.toThrow("WORKFLOW_PROJECT_CHANGED");
    expect(useWorkflowStore.getState().runs).toEqual([]);
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
  });

  it("opens an existing affected Wiki page instead of a deleted first path", async () => {
    const scan = vi.fn().mockImplementation(async () => {
      useWikiStore.setState({
        tree: { root: { name: "wiki", path: "wiki", kind: "directory", children: [] }, pages: [{ path: "wiki/existing.md" }] } as never,
      });
    });
    const openPage = vi.fn().mockResolvedValue(undefined);
    useWikiStore.setState({ scan, openPage });
    await openWorkflowResult(project, completedUpdate());
    expect(openPage).toHaveBeenCalledWith(
      project.projectId,
      project.rootPath,
      "wiki/existing.md",
      expect.any(Function),
    );
  });

  it("passes the identity guard into result-store commits", async () => {
    let releaseScan!: () => void;
    const scanGate = new Promise<void>((resolve) => { releaseScan = resolve; });
    const scan = vi.fn().mockImplementation(async (
      _projectId: string,
      _rootPath: string,
      commitGuard?: () => boolean,
    ) => {
      await scanGate;
      if (commitGuard?.()) {
        useWikiStore.setState({ tree: { pages: [{ path: "wiki/stale.md" }] } as never });
      }
    });
    useWikiStore.setState({ scan });
    const opening = openWorkflowResult(project, completedUpdate());
    useProjectStore.setState({
      authority: {
        ...useProjectStore.getState().authority!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    });
    releaseScan();

    await expect(opening).rejects.toThrow("WORKFLOW_PROJECT_CHANGED");
    expect(useWikiStore.getState().tree).toBeNull();
  });

  it("fails closed before direct result navigation when authority and workflow identity disagree", async () => {
    useProjectStore.setState({
      authority: {
        ...useProjectStore.getState().authority!,
        canonicalIdentityKey: "identity-b",
        identityRevision: "revision-b",
      },
    });
    useNavigationStore.setState({ activeView: "workflows" });
    const healthRun = {
      ...completedUpdate(),
      kind: "health_check",
      scope: { kind: "health_check", mode: "local_quick" },
      result: {
        kind: "health_check",
        reportId: null,
        persistent: false,
        errorCount: 0,
        warningCount: 0,
        infoCount: 0,
      },
    } as WorkflowRun;

    await expect(openWorkflowResult(project, healthRun)).rejects.toThrow("WORKFLOW_PROJECT_CHANGED");
    expect(useNavigationStore.getState().activeView).toBe("workflows");
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";

const getWorkflowRunMock = vi.hoisted(() => vi.fn());

vi.mock("./workflowApi", () => ({ getWorkflowRun: getWorkflowRunMock }));

import { useExportStore } from "../stores/exportStore";
import type { ExportRecord } from "../types/export";
import { useWikiStore } from "../features/wiki/wikiStore";
import { useProjectStore, defaultProject } from "../stores/projectStore";
import { useNavigationStore } from "../stores/navigationStore";
import { useWorkflowStore } from "../stores/workflowStore";
import { useTaskStore } from "../stores/taskStore";
import type { WorkflowRun } from "../types/workflow";
import { cancelWorkflowNavigation, hydrateAndSelectWorkflowRun, openWorkflowResult } from "./workflowNavigation";

const project = { projectId: "project-a", rootPath: "D:/知识库" };

function completedUpdate(): WorkflowRun {
  return {
    schemaVersion: 1,
    taskId: "run-a",
    projectId: project.projectId,
    canonicalIdentityKey: "identity-a",
    identityRevision: "revision-a",
    kind: "update_wiki",
    operation: { kind: "built_in" },
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
  useTaskStore.setState({ workflowById: {}, workflowSessionId: null, retiredWorkflowSessions: [] });
  useNavigationStore.setState({ activeView: "workflows" });
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
  it("previews the exact Workflow ExportRecord even when a newer unrelated artifact exists", async () => {
    const record: ExportRecord = { id: "record-a", exportType: "project_report", title: "项目报告", sourcePath: undefined,
      outputPath: "exports/中文报告.html", createdAt: "2026-09-07T00:00:00Z", route: "byok", status: "succeeded", bookmarked: false, taskId: "run-a" };
    const loadPreview = vi.fn(async (_request, id) => { useExportStore.setState({ previewId: id, previewHtml: "<h1>报告</h1>" }); });
    useExportStore.setState({ error: null, records: [{ ...record, id: "newest", taskId: "other-task", outputPath: "exports/other.html" }, record],
      loadExports: vi.fn().mockResolvedValue(undefined), loadPreview });
    const run: WorkflowRun = { ...completedUpdate(), kind: "generate_content",
      scope: { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: record.outputPath },
      result: { kind: "generate_content", artifactType: "project_report", recordId: record.id, outputPaths: [record.outputPath], artifactCount: 1, validationPassed: true } };
    await openWorkflowResult(project, run);
    expect(loadPreview).toHaveBeenCalledExactlyOnceWith({ projectId: project.projectId, projectRootPath: project.rootPath, outputPath: record.outputPath }, record.id, expect.any(Function));
    expect(useExportStore.getState().previewId).toBe(record.id);
    expect(useNavigationStore.getState().activeView).toBe("exports");
  });

  it.each(["missing", "foreign-task", "preview-failed"])("keeps the Workflow result visible when its export is %s", async (failure) => {
    const record: ExportRecord = { id: "record-a", exportType: "project_report", title: "项目报告", sourcePath: undefined,
      outputPath: "exports/中文报告.html", createdAt: "2026-09-07T00:00:00Z", route: "byok", status: "succeeded", bookmarked: false, taskId: failure === "foreign-task" ? "other-task" : "run-a" };
    const loadPreview = vi.fn(async () => { useExportStore.setState({ error: "PREVIEW_FAILED", previewId: null, previewHtml: null }); });
    useExportStore.setState({ error: null, records: failure === "missing" ? [] : [record], previewId: "older", previewHtml: "<h1>Old artifact</h1>",
      loadExports: vi.fn().mockResolvedValue(undefined), loadPreview });
    const run: WorkflowRun = { ...completedUpdate(), kind: "generate_content",
      scope: { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: record.outputPath },
      result: { kind: "generate_content", artifactType: "project_report", recordId: record.id, outputPaths: [record.outputPath], artifactCount: 1, validationPassed: true } };
    await expect(openWorkflowResult(project, run)).rejects.toThrow(failure === "preview-failed" ? "PREVIEW_FAILED" : "WORKFLOW_EXPORT_RESULT_UNAVAILABLE");
    expect(useNavigationStore.getState().activeView).toBe("workflows");
    if (failure !== "preview-failed") expect(loadPreview).not.toHaveBeenCalled();
  });

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

  it("keeps the newer same-project selection when the older detail arrives last", async () => {
    let releaseA!: (run: WorkflowRun) => void;
    getWorkflowRunMock.mockImplementation(({ taskId }) => taskId === "run-a"
      ? new Promise<WorkflowRun>((resolve) => { releaseA = resolve; })
      : Promise.resolve({ ...completedUpdate(), taskId }));
    const openingA = hydrateAndSelectWorkflowRun(project, "run-a");
    await hydrateAndSelectWorkflowRun(project, "run-b");
    releaseA(completedUpdate());

    await expect(openingA).rejects.toThrow("WORKFLOW_NAVIGATION_SUPERSEDED");
    expect(useWorkflowStore.getState().selectedTaskId).toBe("run-b");
  });

  it("abandons selection as soon as preparation begins, before its response changes the surface", async () => {
    let release!: (run: WorkflowRun) => void;
    getWorkflowRunMock.mockReturnValue(new Promise<WorkflowRun>((resolve) => { release = resolve; }));
    const opening = hydrateAndSelectWorkflowRun(project, "run-a");
    cancelWorkflowNavigation();
    release(completedUpdate());

    await expect(opening).rejects.toThrow("WORKFLOW_NAVIGATION_SUPERSEDED");
    expect(useWorkflowStore.getState()).toMatchObject({ surface: "overview", selectedTaskId: null });
  });

  it("abandons late selection when preparation is opened during hydration", async () => {
    let release!: (run: WorkflowRun) => void;
    getWorkflowRunMock.mockReturnValue(new Promise<WorkflowRun>((resolve) => { release = resolve; }));
    const opening = hydrateAndSelectWorkflowRun(project, "run-a");
    const preparation = { id: "new-preparation" } as never;
    useWorkflowStore.getState().setPreparation(preparation);
    release(completedUpdate());

    await expect(opening).rejects.toThrow("WORKFLOW_NAVIGATION_SUPERSEDED");
    expect(useWorkflowStore.getState()).toMatchObject({ surface: "preparation", preparation, selectedTaskId: null });
  });

  it("remembers a surface change even when the user returns before the detail arrives", async () => {
    let release!: (run: WorkflowRun) => void;
    getWorkflowRunMock.mockReturnValue(new Promise<WorkflowRun>((resolve) => { release = resolve; }));
    const opening = hydrateAndSelectWorkflowRun(project, "run-a");
    useWorkflowStore.getState().setSurface("history");
    useWorkflowStore.getState().setSurface("overview");
    release(completedUpdate());

    await expect(opening).rejects.toThrow("WORKFLOW_NAVIGATION_SUPERSEDED");
    expect(useWorkflowStore.getState().selectedTaskId).toBeNull();
  });

  it("does not commit a late result or reopen Wiki after the user leaves Workflows", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    useWikiStore.setState({ scan: vi.fn(async (_id, _root, commitGuard) => {
      await gate;
      if (commitGuard?.()) useWikiStore.setState({ tree: { pages: [] } as never });
    }) });
    const opening = openWorkflowResult(project, completedUpdate());
    useNavigationStore.getState().setActiveView("chat");
    release();

    await expect(opening).rejects.toThrow("WORKFLOW_NAVIGATION_SUPERSEDED");
    expect(useWikiStore.getState().tree).toBeNull();
    expect(useNavigationStore.getState().activeView).toBe("chat");
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

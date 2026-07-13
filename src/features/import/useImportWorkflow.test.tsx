import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../../stores/projectStore";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";

const api = vi.hoisted(() => ({
  getReadiness: vi.fn(),
  createSession: vi.fn(),
  getSession: vi.fn(),
  addPaths: vi.fn(),
  addUrl: vi.fn(),
  setSelection: vi.fn(),
  startItems: vi.fn(),
  confirmSession: vi.fn(),
}));

vi.mock("../../services/importV2Api", () => ({ importV2Api: api }));

import { useImportWorkflow } from "./useImportWorkflow";

const projectA = {
  ...defaultProject,
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
};
const projectB = {
  ...projectA,
  projectId: "project-b",
  name: "Project B",
  rootPath: "D:/知识库/project-b",
};

const readiness: ImportFrontendReadiness = {
  backendVersion: "2.0.0",
  active: true,
  migrationStatus: "applied",
  unfinishedSessionId: null,
  legacyHistoryAvailable: false,
};

function task(id: string, projectId = projectA.projectId, status: BackendTask["status"] = "queued"): BackendTask {
  return {
    id,
    taskType: "import",
    projectId,
    title: `Import ${id}`,
    status,
    progress: null,
    startedAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

function item(itemId: string, status: ImportItem["status"] = "queued"): ImportItem {
  return {
    itemId,
    input: {
      kind: itemId.startsWith("url") ? "url" : "file",
      displayName: itemId,
      locator: itemId.startsWith("url") ? `https://example.com/${itemId}` : `C:\\sources\\${itemId}`,
      normalizedLocator: null,
    },
    status,
    selected: status === "preview_ready",
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
  };
}

function session(projectId: string, items: ImportItem[] = []): ImportSession {
  return {
    schemaVersion: 2,
    sessionId: `session-${projectId}`,
    projectId,
    status: "draft",
    resourceMode: "balanced",
    createdAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    items,
  };
}

function launcher(): TaskLauncher {
  return {
    startCompile: vi.fn(),
    startDeepLint: vi.fn(),
    startExport: vi.fn(),
    cancel: vi.fn().mockResolvedValue(undefined),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  useImportStore.getState().reset();
  useTaskStore.setState({ tasks: [], logs: {}, drawerOpen: false, selectedTaskId: null, runningCount: 0 });
  api.getReadiness.mockResolvedValue(readiness);
  api.createSession.mockResolvedValue(session(projectA.projectId));
  api.getSession.mockResolvedValue(session(projectA.projectId));
  api.addPaths.mockResolvedValue(task("add-paths"));
  api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-1")]));
  api.setSelection.mockResolvedValue(session(projectA.projectId, [item("file.md", "preview_ready")]));
  api.startItems.mockResolvedValue([]);
  api.confirmSession.mockResolvedValue(task("confirm"));
});

describe("useImportWorkflow", () => {
  it("bootstraps readiness and creates a balanced draft when no unfinished session exists", async () => {
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    expect(api.getReadiness).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.createSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      resourceMode: "balanced",
    });
    expect(result.current.session?.sessionId).toBe("session-project-a");
  });

  it("recovers the unfinished session without creating a competing draft", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId, [item("recover.md")]), sessionId: "session-recover" });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.sessionId).toBe("session-recover"));
    expect(api.createSession).not.toHaveBeenCalled();
    expect(api.getSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-recover",
    });
  });

  it("blocks the import surface until migration activation is ready", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, active: false, migrationStatus: "awaiting_confirmation" });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("blocked"));
    expect(result.current.session).toBeNull();
    expect(api.createSession).not.toHaveBeenCalled();
  });

  it("ignores late project A readiness and session responses after switching to project B", async () => {
    let resolveA!: (value: ImportFrontendReadiness) => void;
    const readinessA = new Promise<ImportFrontendReadiness>((resolve) => { resolveA = resolve; });
    api.getReadiness.mockImplementation(({ projectId }: { projectId: string }) =>
      projectId === projectA.projectId ? readinessA : Promise.resolve(readiness));
    api.createSession.mockImplementation(({ projectId }: { projectId: string }) => Promise.resolve(session(projectId)));

    const { result, rerender } = renderHook(({ project }) => useImportWorkflow(project, "import", launcher()), {
      initialProps: { project: projectA },
    });
    rerender({ project: projectB });
    await waitFor(() => expect(result.current.session?.projectId).toBe(projectB.projectId));

    await act(async () => resolveA(readiness));
    expect(result.current.session?.projectId).toBe(projectB.projectId);
  });

  it("adds a URL, starts only newly queued items, and upserts returned task facts", async () => {
    const existing = item("existing.md", "preview_ready");
    api.createSession.mockResolvedValue(session(projectA.projectId, [existing]));
    api.addUrl.mockResolvedValue(session(projectA.projectId, [existing, item("url-1")]));
    const started = task("url-task");
    api.startItems.mockResolvedValue([started]);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.addUrl("https://example.com/article"));
    expect(api.addUrl).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      url: "https://example.com/article",
    });
    expect(api.startItems).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemIds: ["url-1"],
    });
    expect(useTaskStore.getState().tasks).toContainEqual(started);
  });

  it("routes selection, retry, cancellation, and confirmation through V2 contracts", async () => {
    const retryable = item("retry.md", "failed");
    retryable.taskId = "retry-task";
    api.createSession.mockResolvedValue(session(projectA.projectId, [retryable, item("ready.md", "preview_ready")]));
    const selectionSession = session(projectA.projectId, [item("retry.md", "failed"), item("ready.md", "preview_ready")]);
    selectionSession.items[0].taskId = "retry-task";
    api.setSelection.mockResolvedValue(selectionSession);
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(2));

    await act(async () => result.current.setItemSelected("ready.md", true));
    await act(async () => result.current.retryItem("retry.md"));
    await act(async () => result.current.cancelItem("retry.md"));
    await act(async () => result.current.confirm([{ itemId: "ready.md", conflictAction: null, expectedWikiHash: null }]));

    expect(api.setSelection).toHaveBeenCalled();
    expect(api.startItems).toHaveBeenCalledWith(expect.objectContaining({ itemIds: ["retry.md"] }));
    expect(taskLauncher.cancel).toHaveBeenCalledWith("retry-task");
    expect(api.confirmSession).toHaveBeenCalledWith(expect.objectContaining({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
    }));
  });
});

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { PendingAction } from "../../types/backend";
import type { ImportedSource, ImportPreview } from "../../types/import";
import type { BackendTask } from "../../types/task";
import { useWikiStore } from "../wiki/wikiStore";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  waitForTaskTerminal: vi.fn(),
  extractArticleFromHtml: vi.fn(),
  articleToMarkdown: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../../lib/waitForTaskTerminal", () => ({
  waitForTaskTerminal: mocks.waitForTaskTerminal,
}));
vi.mock("../../lib/readability", () => ({
  extractArticleFromHtml: mocks.extractArticleFromHtml,
  articleToMarkdown: mocks.articleToMarkdown,
}));

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

const preview: ImportPreview = {
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

const startedTask: BackendTask = {
  id: "preview-task",
  taskType: "import",
  projectId: projectA.projectId,
  title: "Preview import",
  status: "queued",
  progress: null,
  startedAt: "2026-07-10T00:00:00Z",
  updatedAt: "2026-07-10T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
};
const finishedTask: BackendTask = {
  ...startedTask,
  status: "succeeded",
  completedAt: "2026-07-10T00:00:01Z",
};

const pendingAction: PendingAction = {
  id: "action-1",
  actionType: "delete_source",
  title: "Delete source",
  message: "Delete source",
  riskLevel: "destructive",
  affectedPaths: ["raw/sources/a.pdf"],
  preview: null,
  expiresAt: null,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

let taskLauncher: TaskLauncher;
let scanMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.waitForTaskTerminal.mockReset();
  mocks.extractArticleFromHtml.mockReset();
  mocks.articleToMarkdown.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useProjectStore.setState({ currentProject: projectA, pendingAction: undefined });
  useImportStore.getState().reset();
  useImportStore.setState({ importedSources: [] });
  useTaskStore.setState({
    tasks: [],
    logs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
  });
  useToastStore.setState({ toasts: [] });
  scanMock = vi.fn().mockResolvedValue(undefined);
  useWikiStore.setState({ scan: scanMock });
  taskLauncher = {
    startCompile: vi.fn().mockResolvedValue(finishedTask),
    startDeepLint: vi.fn(),
    startExport: vi.fn(),
    cancel: vi.fn(),
  };
});

describe("useImportWorkflow", () => {
  it("resets staging on project change and ignores a late source list", async () => {
    const listA = deferred<ImportedSource[]>();
    const sourcesB: ImportedSource[] = [
      { path: "raw/sources/b.pdf", sizeBytes: 10, fileType: "pdf" },
    ];
    mocks.invoke
      .mockReturnValueOnce(listA.promise)
      .mockResolvedValueOnce(sourcesB);
    useImportStore.setState({ preview, importedSources: [{ ...sourcesB[0], path: "stale" }] });

    const { result, rerender } = renderHook(
      ({ project }) => useImportWorkflow(project, "import", taskLauncher),
      { initialProps: { project: projectA } },
    );
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(1));

    useProjectStore.getState().setCurrentProject(projectB);
    rerender({ project: projectB });
    await waitFor(() => expect(result.current.importedSources).toEqual(sourcesB));
    expect(useImportStore.getState().preview).toBeNull();

    await act(async () => {
      listA.resolve([{ path: "raw/sources/a.pdf", sizeBytes: 1, fileType: "pdf" }]);
      await listA.promise;
    });
    expect(result.current.importedSources).toEqual(sourcesB);
  });

  it("loads imported sources only while the Import view is active", async () => {
    mocks.invoke.mockResolvedValue([]);
    const { rerender } = renderHook(
      ({ activeView }) => useImportWorkflow(projectA, activeView, taskLauncher),
      { initialProps: { activeView: "dashboard" as const } },
    );
    expect(mocks.invoke).not.toHaveBeenCalled();

    rerender({ activeView: "import" });
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("list_imported_sources", {
        request: {
          projectId: projectA.projectId,
          projectRootPath: projectA.rootPath,
        },
      }),
    );
  });

  it("runs file preview as a tracked task and commits only the terminal result", async () => {
    mocks.invoke
      .mockResolvedValueOnce(startedTask)
      .mockResolvedValueOnce(preview);
    mocks.waitForTaskTerminal.mockResolvedValue(finishedTask);
    const { result } = renderHook(() =>
      useImportWorkflow(projectA, "dashboard", taskLauncher),
    );

    act(() => result.current.requestPreview(["  C:/资料/a.pdf  ", "  "]));

    await waitFor(() => expect(useImportStore.getState().preview).toEqual(preview));
    expect(mocks.invoke.mock.calls).toEqual([
      [
        "preview_import",
        {
          request: {
            projectId: projectA.projectId,
            projectRootPath: projectA.rootPath,
            sourcePaths: ["C:/资料/a.pdf"],
            allowDuplicates: false,
            linkDuplicates: false,
          },
        },
      ],
      [
        "get_import_preview",
        {
          request: {
            projectId: projectA.projectId,
            projectRootPath: projectA.rootPath,
            taskId: startedTask.id,
          },
        },
      ],
    ]);
    expect(useTaskStore.getState().tasks).toEqual([finishedTask]);
    expect(useTaskStore.getState().selectedTaskId).toBe(startedTask.id);

    act(() => result.current.requestPreview([" "]));
    expect(useImportStore.getState().preview).toBeNull();
  });

  it("reports a non-succeeded preview task without committing a preview", async () => {
    mocks.invoke.mockResolvedValue(startedTask);
    mocks.waitForTaskTerminal.mockResolvedValue({
      ...finishedTask,
      status: "failed",
      error: {
        code: "IMPORT_FAILED",
        message: "parser failed",
        details: null,
        recoverable: true,
        userActionRequired: false,
      },
    });
    const { result } = renderHook(() =>
      useImportWorkflow(projectA, "dashboard", taskLauncher),
    );

    act(() => result.current.requestPreview(["C:/资料/a.pdf"]));

    await waitFor(() =>
      expect(useToastStore.getState().toasts[0]?.message).toContain("parser failed"),
    );
    expect(useImportStore.getState().preview).toBeNull();
  });

  it("does not load Readability for clipboard previews but dynamically uses it for URLs", async () => {
    mocks.invoke
      .mockResolvedValueOnce(preview)
      .mockResolvedValueOnce({ url: "https://example.test/article", html: "<article>Hi</article>" })
      .mockResolvedValueOnce(preview);
    mocks.extractArticleFromHtml.mockReturnValue({
      title: "Article",
      byline: "Author",
      content: "<p>Hi</p>",
      textContent: "Hi",
      length: 2,
      excerpt: "Hi",
      siteName: "Example",
      dir: null,
      lang: null,
      publishedTime: null,
    });
    mocks.articleToMarkdown.mockReturnValue("# Article\n\nHi");
    const { result } = renderHook(() =>
      useImportWorkflow(projectA, "dashboard", taskLauncher),
    );

    await act(async () => result.current.requestClipboard("clipboard text"));
    expect(mocks.extractArticleFromHtml).not.toHaveBeenCalled();
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "preview_text_import", {
      request: expect.objectContaining({
        kind: "clipboard",
        content: "clipboard text",
      }),
    });

    await act(async () => result.current.requestUrl("https://example.test/article"));
    expect(mocks.extractArticleFromHtml).toHaveBeenCalledWith(
      "<article>Hi</article>",
      "https://example.test/article",
    );
    expect(mocks.articleToMarkdown).toHaveBeenCalled();
    expect(mocks.invoke).toHaveBeenNthCalledWith(3, "preview_text_import", {
      request: expect.objectContaining({
        kind: "url",
        sourceName: "Article",
        content: "# Article\n\nHi",
        title: "Article",
        author: "Author",
      }),
    });
  });

  it("confirms before scanning and optionally compiling, then clears confirming state", async () => {
    const order: string[] = [];
    mocks.invoke.mockImplementation((command: string) => {
      order.push(command);
      return Promise.resolve({ preview, confirmedAt: "2026-07-10T00:00:00Z" });
    });
    scanMock.mockImplementation(async () => {
      order.push("scan");
    });
    vi.mocked(taskLauncher.startCompile).mockImplementation(async () => {
      order.push("compile");
      return finishedTask;
    });
    const { result } = renderHook(() =>
      useImportWorkflow(projectA, "dashboard", taskLauncher),
    );
    act(() => useImportStore.setState({ preview }));

    act(() =>
      result.current.confirm({ createCheckpoint: true, compileAfterImport: true }),
    );
    expect(useImportStore.getState().isConfirming).toBe(true);

    await waitFor(() => expect(useImportStore.getState().isConfirming).toBe(false));
    expect(order).toEqual(["confirm_import_preview", "scan", "compile"]);
    expect(useImportStore.getState().preview).toBeNull();
  });

  it("keeps the preview after confirm failure and prepares typed source actions", async () => {
    mocks.invoke
      .mockRejectedValueOnce(new Error("checkpoint failed"))
      .mockResolvedValueOnce(pendingAction)
      .mockResolvedValueOnce({ ...pendingAction, actionType: "replace_source" });
    const { result } = renderHook(() =>
      useImportWorkflow(projectA, "dashboard", taskLauncher),
    );
    act(() => useImportStore.setState({ preview }));

    act(() =>
      result.current.confirm({ createCheckpoint: true, compileAfterImport: false }),
    );
    await waitFor(() => expect(useImportStore.getState().isConfirming).toBe(false));
    expect(useImportStore.getState().preview).toEqual(preview);

    await act(async () => result.current.requestDeleteSource("raw/sources/a.pdf"));
    expect(useProjectStore.getState().pendingAction).toEqual(pendingAction);
    await act(async () =>
      result.current.requestReplaceSource(
        "raw/sources/a.pdf",
        "C:/资料/replacement.pdf",
      ),
    );
    expect(mocks.invoke.mock.calls.slice(1)).toEqual([
      [
        "request_delete_source",
        {
          request: {
            projectId: projectA.projectId,
            projectRootPath: projectA.rootPath,
            targetPath: "raw/sources/a.pdf",
          },
        },
      ],
      [
        "request_replace_source",
        {
          request: {
            projectId: projectA.projectId,
            projectRootPath: projectA.rootPath,
            targetPath: "raw/sources/a.pdf",
            replacementPath: "C:/资料/replacement.pdf",
          },
        },
      ],
    ]);
  });
});

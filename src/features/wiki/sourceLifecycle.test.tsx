import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { AgentKind } from "../../types/agent";
import type { SourceDetail, SourceMutationResult } from "../../types/source";
import type { BackendTask } from "../../types/task";
import type { WikiPageContent, WikiPageMeta, WikiTree as WikiTreeData, WikiTreeNode } from "../../types/wiki";
import { SourceLifecycleDialogs } from "./SourceLifecycleDialogs";
import { SourceAiOrganizeDialog } from "./SourceAiOrganizeDialog";
import { SourceRightPanel } from "./SourceRightPanel";
import { useSourceStore } from "./sourceStore";
import { WikiPageFormDialog } from "./WikiPageFormDialog";
import { WikiTree } from "./WikiTree";
import {
  sameProjectRoot,
  selectSourceAiWorkbenchTask,
  WikiView,
} from "./WikiView";
import { useWikiStore } from "./wikiStore";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

const quality = {
  level: "pass" as const,
  metrics: [],
  warnings: [],
};

const emptyAiCapabilities = { agents: [], providers: [] };
const readySourceAiCapabilities = {
  agents: [
    {
      kind: "codex" as const,
      command: "codex",
      state: "installed" as const,
      version: "0.135.0",
      executablePath: "C:/tools/codex.exe",
      isDefault: true,
      installGuidance: "",
      error: null,
    },
  ],
  providers: [
    {
      config: {
        provider: "open_ai" as const,
        model: "gpt-source",
        baseUrl: "https://example.test",
        contextWindow: 32_000,
        enabled: true,
      },
      hasSecret: true,
      secretMask: "sk-…",
    },
  ],
};

function sourceAiTask(overrides: Partial<BackendTask> = {}): BackendTask {
  return {
    id: "source-ai-task",
    taskType: "source_ai_organize",
    projectId: "project-1",
    title: "AI organize Source",
    status: "queued",
    progress: null,
    startedAt: "2026-07-28T00:00:00Z",
    updatedAt: "2026-07-28T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: {
      summary: "queued",
      affectedPaths: [],
      reference: {
        type: "source_ai_organize",
        sourceId: "source-1",
        baseVersionId: "version-1",
        baseMarkdownHash: "a".repeat(64),
        candidateId: null,
        projectRootPath: "D:/知识库",
      },
    },
    error: null,
    ...overrides,
  };
}

function sourceDetail(): SourceDetail {
  return {
    sourceId: "source-1",
    versionId: "version-1",
    title: "访谈",
    sourceKind: "video",
    status: "current",
    currentPath: "wiki/sources/local/访谈.md",
    currentMarkdownHash: "a".repeat(64),
    primaryAction: "reprocess_asr",
    candidate: null,
    targetPath: "wiki/sources/local/访谈.md",
    evidenceRetention: "immutable_originals_retained",
    evidence: [
      {
        path: "raw/sources/source-1/version-1/subtitles/访谈.zh-CN.vtt",
        kind: "subtitle",
        sizeBytes: 120,
      },
    ],
    quality,
    originalDraft: "# 访谈\n\n原稿",
    originalDraftTruncated: false,
    versions: [
      {
        versionId: "version-1",
        createdAt: "2026-07-27T00:00:00Z",
        eventKind: "source_imported",
        quality,
        current: true,
        restorable: true,
        checkpoint: "checkpoint-1",
      },
    ],
    timeline: [
      {
        eventId: "event-1",
        kind: "source_imported",
        versionId: "version-1",
        createdAt: "2026-07-27T00:00:00Z",
        checkpoint: "checkpoint-1",
        restorable: true,
      },
    ],
    relatedWikiPaths: ["wiki/concepts/访谈摘要.md"],
    technicalDetails: {
      route: "media.asr",
      engine: "local-asr",
      engineVersion: "1",
      locator: "file:访谈.mp4",
      manifestPath: ".app/sources/source-1.json",
    },
    availableActions: ["asr", "subtitle"],
  };
}

function page(overrides: Partial<WikiPageMeta> = {}): WikiPageMeta {
  return {
    path: "wiki/sources/local/访谈.md",
    title: "访谈",
    pageType: "source",
    tags: [],
    sources: [],
    aliases: [],
    created: null,
    updated: null,
    starred: false,
    bookmarked: false,
    wordCount: 10,
    fileSize: 100,
    modifiedTime: "2026-07-27T00:00:00Z",
    hash: "a".repeat(64),
    wikilinks: [],
    ...overrides,
  };
}

function rootFor(meta: WikiPageMeta): WikiTreeNode {
  return {
    name: "wiki",
    kind: "folder",
    path: "wiki",
    starred: false,
    bookmarked: false,
    fileCount: 1,
    children: [{
      name: "访谈.md",
      kind: "file",
      path: meta.path,
      type: meta.pageType,
      starred: false,
      bookmarked: false,
      fileCount: 1,
      children: [],
    }],
  };
}

beforeEach(async () => {
  invokeMock.mockReset();
  useSourceStore.getState().reset();
  useWikiStore.getState().reset();
  useTaskStore.setState({
    taskById: {},
    taskIdsByProject: {},
    runningCountByProject: {},
    taskFacts: {},
    tasks: [],
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
  });
  await i18next.changeLanguage("en");
});

afterEach(() => {
  cleanup();
});

describe("Source reader and lifecycle boundaries", () => {
  it("matches Windows project roots case-insensitively without folding POSIX paths", () => {
    expect(sameProjectRoot("D:\\Knowledge\\Wiki\\", "d:/knowledge/wiki")).toBe(
      true,
    );
    expect(sameProjectRoot("/Users/Aletta/Wiki", "/users/aletta/wiki")).toBe(
      false,
    );
  });

  it("deduplicates StrictMode Source detail effect replay", async () => {
    invokeMock.mockResolvedValue(sourceDetail());

    render(
      <StrictMode>
        <SourceRightPanel
          projectId="project-1"
          rootPath="D:/knowledge"
          sourceId="source-1"
          onOpenPage={vi.fn()}
          onMutation={vi.fn()}
        />
      </StrictMode>,
    );

    await waitFor(() => expect(useSourceStore.getState().detail).not.toBeNull());
    const calls = invokeMock.mock.calls.filter(
      ([command]) => command === "get_source_detail",
    );
    expect(calls).toHaveLength(1);
  });

  it("starts a fresh detail snapshot when a completed candidate supersedes an active read", async () => {
    let resolveOld!: (detail: SourceDetail) => void;
    const oldSnapshot = new Promise<SourceDetail>((resolve) => {
      resolveOld = resolve;
    });
    const freshSnapshot = sourceDetail();
    freshSnapshot.candidate = {
      candidateId: "candidate-fresh",
      kind: "ai_organize",
      createdAt: "2026-07-30T01:00:00Z",
      baseVersionId: "version-1",
      baseMarkdownHash: "a".repeat(64),
      candidateMarkdownHash: "b".repeat(64),
      quality,
    };
    let detailReads = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command !== "get_source_detail") return Promise.resolve(null);
      detailReads += 1;
      return detailReads === 1
        ? oldSnapshot
        : Promise.resolve(freshSnapshot);
    });

    const oldLoad = useSourceStore
      .getState()
      .loadDetail("project-1", "D:/knowledge", "source-1");
    const freshLoad = useSourceStore
      .getState()
      .loadDetail(
        "project-1",
        "D:/knowledge",
        "source-1",
        "completed-task:candidate-fresh",
      );

    expect(detailReads).toBe(2);
    await freshLoad;
    resolveOld(sourceDetail());
    await oldLoad;
    expect(useSourceStore.getState().detail?.candidate?.candidateId).toBe(
      "candidate-fresh",
    );
  });

  it("reopens the candidate task after the Source baseline changed", () => {
    const completed = sourceAiTask({
      status: "succeeded",
      completedAt: "2026-07-28T00:02:00Z",
      result: {
        summary: "Candidate ready",
        affectedPaths: [],
        reference: {
          type: "source_ai_organize",
          sourceId: "source-1",
          baseVersionId: "version-1",
          baseMarkdownHash: "a".repeat(64),
          candidateId: "candidate-1",
          projectRootPath: "D:/知识库",
        },
      },
    });
    expect(
      selectSourceAiWorkbenchTask(
        [completed],
        "project-1",
        "D:/知识库",
        "source-1",
        "version-2",
        "b".repeat(64),
        "candidate-1",
      ),
    ).toBe(completed);
  });

  it("renders the authoritative eight-section right-panel order", async () => {
    invokeMock.mockResolvedValueOnce(sourceDetail());
    const { container } = render(
      <SourceRightPanel
        projectId="project-1"
        rootPath="D:/知识库"
        sourceId="source-1"
        onOpenPage={vi.fn()}
        onMutation={vi.fn()}
      />,
    );

    await screen.findByText(
      "Original evidence and historical versions remain in the project. An update creates a new version only after confirmation.",
    );
    const headings = Array.from(container.querySelectorAll("section > h3")).map(
      (heading) => heading.textContent,
    );
    expect(headings).toEqual([
      "1. Source and status",
      "2. Primary action",
      "3. Candidate preview",
      "4. Target path and evidence",
      "5. Quality and problems",
      "6. Original draft",
      "7. Version timeline",
      "8. Technical details and logs",
    ]);
    expect(
      within(container.querySelectorAll("section")[1]!).getAllByRole("button"),
    ).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "Show technical details" }));
    expect(screen.getByLabelText("Retained transcript")).toHaveValue(
      "raw/sources/source-1/version-1/subtitles/访谈.zh-CN.vtt",
    );
  });

  it("routes a registry-bound Source through dedicated move and delete actions", () => {
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
      sourceId: "source-1",
      versionId: "version-1",
      sourceStatus: "current",
    });
    const onSourceRename = vi.fn();
    const onSourceDelete = vi.fn();
    const onRename = vi.fn();
    const onDelete = vi.fn();
    render(
      <WikiTree
        root={rootFor(bound)}
        pages={[bound]}
        selectedPath={bound.path}
        onSelect={vi.fn()}
        onRefresh={vi.fn()}
        onCreate={vi.fn()}
        onRename={onRename}
        onDelete={onDelete}
        onSourceRename={onSourceRename}
        onSourceDelete={onSourceDelete}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Page actions: 访谈.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Move Source" }));
    expect(onSourceRename).toHaveBeenCalledWith("source-1", bound.path);
    expect(onRename).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Page actions: 访谈.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete Source" }));
    expect(onSourceDelete).toHaveBeenCalledWith("source-1");
    expect(onDelete).not.toHaveBeenCalled();
  });

  it("hides generic lifecycle actions for an unbound source-like page", () => {
    const unbound = page();
    render(
      <WikiTree
        root={rootFor(unbound)}
        pages={[unbound]}
        selectedPath={unbound.path}
        onSelect={vi.fn()}
        onRefresh={vi.fn()}
        onCreate={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "Page actions: 访谈.md" }),
    ).toBeNull();
  });

  it("does not offer Source creation through the generic Wiki dialog", () => {
    const onSubmit = vi.fn();
    render(
      <WikiPageFormDialog
        mode="create"
        initialPath="wiki/sources/"
        onCancel={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    expect(screen.queryByRole("option", { name: "Source" })).toBeNull();
    expect(screen.getByRole("button", { name: "Create page" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("button", { name: "Create page" }).closest("form")!);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("requires the specialized permanent-delete confirmation and shows inventory", async () => {
    useSourceStore.setState({
      detail: sourceDetail(),
      deletePreview: {
        sourceId: "source-1",
        title: "访谈",
        paths: [
          {
            path: "raw/assets/source-1/version-1/frame-01.png",
            kind: "image",
            sizeBytes: 2048,
          },
        ],
        versions: sourceDetail().versions,
        referencedBy: ["wiki/concepts/访谈摘要.md"],
        referenceCount: 1,
        expectedFreedBytes: 2048,
        guardToken: "guard",
      },
    });
    invokeMock.mockResolvedValueOnce({
      sourceId: "source-1",
      versionId: "version-1",
      wikiPath: "wiki/sources/local/访谈.md",
      checkpoint: "checkpoint-delete",
    });
    render(
      <SourceLifecycleDialogs
        projectId="project-1"
        rootPath="D:/知识库"
        onMoved={vi.fn()}
        onDeleted={vi.fn()}
      />,
    );

    expect(screen.getByText("raw/assets/source-1/version-1/frame-01.png")).toBeVisible();
    expect(screen.getByText("wiki/concepts/访谈摘要.md")).toBeVisible();
    expect(screen.getByText("version-1 · current")).toBeVisible();
    expect(screen.getAllByText("2.0 KB")).toHaveLength(2);
    fireEvent.click(
      screen.getByRole("button", { name: "Permanently delete this Source" }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_source", {
        request: {
          projectId: "project-1",
          projectRootPath: "D:/知识库",
          sourceId: "source-1",
          guardToken: "guard",
          confirmationText: "永久删除此来源",
        },
      }),
    );
  });

  it("requires an explicit merge draft for a three-way Source update", async () => {
    const detail = sourceDetail();
    detail.candidate = {
      candidateId: "candidate-1",
      kind: "asr",
      createdAt: "2026-07-27T01:00:00Z",
      baseVersionId: "version-1",
      baseMarkdownHash: "b".repeat(64),
      candidateMarkdownHash: "c".repeat(64),
      quality,
    };
    detail.status = "candidate_ready";
    detail.primaryAction = "review_candidate";
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_source_detail") return Promise.resolve(detail);
      if (command === "preview_source_update") {
        return Promise.resolve({
          sourceId: "source-1",
          candidateId: "candidate-1",
          mode: "three_way",
          baseMarkdown: "# 访谈\n\n基线",
          currentMarkdown: "# 访谈\n\n人工编辑",
          candidateMarkdown: "# 访谈\n\n新转录",
          diff: "- 人工编辑\n+ 新转录",
          currentMarkdownHash: "a".repeat(64),
          candidateMarkdownHash: "c".repeat(64),
          guardToken: "guard",
        });
      }
      return Promise.resolve(null);
    });
    render(
      <SourceRightPanel
        projectId="project-1"
        rootPath="D:/知识库"
        sourceId="source-1"
        onOpenPage={vi.fn()}
        onMutation={vi.fn()}
      />,
    );

    const reviewButtons = await screen.findAllByRole("button", {
      name: "Review candidate",
    });
    fireEvent.click(reviewButtons[0]);
    const draft = await screen.findByLabelText("Merged Markdown");
    expect(screen.getByText("Generation base")).toBeVisible();
    expect(screen.getByText("Current Source")).toBeVisible();
    expect(screen.getByText("AI candidate")).toBeVisible();
    expect(draft).toHaveValue("# 访谈\n\n人工编辑");
    expect(screen.getByRole("button", { name: "Apply update" })).toBeEnabled();
    fireEvent.change(draft, { target: { value: "# 访谈\n\n人工编辑与新转录" } });
    expect(screen.getByRole("button", { name: "Apply update" })).toBeEnabled();
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
    });
    useWikiStore.setState({
      page: {
        meta: bound,
        rawMarkdown: "# 访谈\n\n磁盘内容",
        bodyMarkdown: "# 访谈\n\n磁盘内容",
        frontmatterYaml: null,
      },
      mode: "edit",
      draft: "# 访谈\n\n尚未保存的编辑",
    });
    expect(
      await screen.findByText(
        "Save or discard the open editor draft before applying this candidate.",
      ),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Apply update" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Discard candidate" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("discard_source_candidate", {
        request: {
          projectId: "project-1",
          projectRootPath: "D:/知识库",
          sourceId: "source-1",
          candidateId: "candidate-1",
        },
      }),
    );
  });

  it("keeps a Source editor draft that changes while a candidate is applying", async () => {
    const detail = sourceDetail();
    detail.candidate = {
      candidateId: "candidate-1",
      kind: "ai_organize",
      createdAt: "2026-07-27T01:00:00Z",
      baseVersionId: "version-1",
      baseMarkdownHash: "a".repeat(64),
      candidateMarkdownHash: "c".repeat(64),
      quality,
    };
    detail.status = "candidate_ready";
    detail.primaryAction = "review_candidate";
    let detailReads = 0;
    let resolveApply!: (result: SourceMutationResult) => void;
    const applyPromise = new Promise<SourceMutationResult>((resolve) => {
      resolveApply = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_source_detail") {
        detailReads += 1;
        return Promise.resolve(detailReads === 1 ? detail : sourceDetail());
      }
      if (command === "preview_source_update") {
        return Promise.resolve({
          sourceId: "source-1",
          candidateId: "candidate-1",
          mode: "two_way",
          baseMarkdown: "# 访谈\n\n磁盘内容",
          currentMarkdown: "# 访谈\n\n磁盘内容",
          candidateMarkdown: "# 访谈\n\nAI 整理内容",
          diff: "- 磁盘内容\n+ AI 整理内容",
          currentMarkdownHash: "a".repeat(64),
          candidateMarkdownHash: "c".repeat(64),
          guardToken: "guard",
        });
      }
      if (command === "apply_source_candidate") return applyPromise;
      return Promise.resolve(null);
    });
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
    });
    useWikiStore.setState({
      page: {
        meta: bound,
        rawMarkdown: "# 访谈\n\n磁盘内容",
        bodyMarkdown: "# 访谈\n\n磁盘内容",
        frontmatterYaml: null,
      },
      mode: "edit",
      draft: "# 访谈\n\n磁盘内容",
    });
    const onMutation = vi.fn();
    render(
      <SourceRightPanel
        projectId="project-1"
        rootPath="D:/知识库"
        sourceId="source-1"
        onOpenPage={vi.fn()}
        onMutation={onMutation}
      />,
    );

    const reviewButtons = await screen.findAllByRole("button", {
      name: "Review candidate",
    });
    fireEvent.click(reviewButtons[0]);
    const applyButton = await screen.findByRole("button", { name: "Apply update" });
    fireEvent.click(applyButton);
    await waitFor(() => expect(useSourceStore.getState().mutating).toBe(true));

    useWikiStore.setState({ draft: "# 访谈\n\n点击后输入的草稿" });
    resolveApply({
      sourceId: "source-1",
      versionId: "version-2",
      wikiPath: "wiki/sources/local/访谈.md",
      checkpoint: "checkpoint-2",
    });

    expect(
      await screen.findByText(
        "The Source changed in the editor while the update was applying. Your draft was kept; save or discard it before reloading the applied version.",
      ),
    ).toBeVisible();
    expect(useWikiStore.getState().draft).toBe("# 访谈\n\n点击后输入的草稿");
    expect(onMutation).not.toHaveBeenCalled();
  });

  it("shows an ordinary recoverable AI failure with task details and explicit retry", async () => {
    const retry = vi.fn().mockResolvedValue(true);
    const close = vi.fn();
    const openTask = vi.fn();
    render(
      <SourceAiOrganizeDialog
        open
        sourceTitle="访谈"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[]}
        failedTask={sourceAiTask({
          status: "failed",
          completedAt: "2026-07-28T00:02:00Z",
          error: {
            code: "LLM_RESPONSE_INVALID",
            message: "Provider returned invalid JSON.",
            details: null,
            recoverable: true,
            userActionRequired: false,
          },
        })}
        error={null}
        onClose={close}
        onOpenTask={openTask}
        onStart={vi.fn().mockResolvedValue(false)}
        onRetry={retry}
      />,
    );

    expect(screen.getByText("The previous AI organization run failed")).toBeVisible();
    expect(screen.getByText("Provider returned invalid JSON.")).toBeVisible();
    expect(screen.getByText("Error code: LLM_RESPONSE_INVALID")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "View task details" }));
    expect(openTask).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Retry with saved settings" }));
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    expect(close).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", { name: "AI Organize Source" }),
    ).toBeVisible();
  });

  it("shows the validated final candidate first and keeps Diff optional", () => {
    const task = sourceAiTask({
      status: "succeeded",
      completedAt: "2026-07-28T00:02:00Z",
      result: {
        summary: "Candidate ready",
        affectedPaths: [],
        reference: {
          type: "source_ai_organize",
          sourceId: "source-1",
          baseVersionId: "version-1",
          baseMarkdownHash: "a".repeat(64),
          candidateId: "candidate-1",
          projectRootPath: "D:/知识库",
        },
      },
    });
    render(
      <SourceAiOrganizeDialog
        open
        sourceTitle="Interview"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[]}
        failedTask={null}
        task={task}
        candidateId="candidate-1"
        preview={{
          sourceId: "source-1",
          candidateId: "candidate-1",
          mode: "two_way",
          baseMarkdown: "# Original",
          currentMarkdown: "# Original",
          candidateMarkdown:
            "---\ntitle: Organized\n---\n# Organized final\n\nVerified text.",
          diff: "- # Original\n+ # Organized final",
          currentMarkdownHash: "a".repeat(64),
          candidateMarkdownHash: "b".repeat(64),
          guardToken: "guard",
        }}
        error={null}
        onClose={vi.fn()}
        onOpenTask={vi.fn()}
        onStart={vi.fn().mockResolvedValue(false)}
        onRetry={vi.fn().mockResolvedValue(false)}
        onApply={vi.fn().mockResolvedValue(true)}
        onDiscard={vi.fn().mockResolvedValue(true)}
      />,
    );

    expect(screen.getByRole("tab", { name: "Final" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getAllByRole("tabpanel", { hidden: true }),
    ).toHaveLength(3);
    expect(screen.getByRole("heading", { name: "Organized final" })).toBeVisible();
    expect(screen.queryByText("- # Original")).toBeNull();

    fireEvent.click(screen.getByRole("tab", { name: "Diff" }));
    expect(screen.getByText(/- # Original/)).toBeVisible();
  });

  it("loads a successful candidate once, exposes diagnostics, and retries explicitly", async () => {
    const task = sourceAiTask({
      status: "succeeded",
      completedAt: "2026-07-28T00:02:00Z",
      result: {
        summary: "Candidate ready",
        affectedPaths: [],
        reference: {
          type: "source_ai_organize",
          sourceId: "source-1",
          baseVersionId: "version-1",
          baseMarkdownHash: "a".repeat(64),
          candidateId: "candidate-1",
          projectRootPath: "D:/知识库",
        },
      },
    });
    const previewCandidate = vi.fn().mockResolvedValue(null);
    const openTask = vi.fn();
    const { rerender } = render(
      <SourceAiOrganizeDialog
        open
        sourceTitle="Interview"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[]}
        failedTask={null}
        task={task}
        candidateId="candidate-1"
        error={null}
        onClose={vi.fn()}
        onOpenTask={openTask}
        onStart={vi.fn().mockResolvedValue(null)}
        onRetry={vi.fn().mockResolvedValue(null)}
        onPreviewCandidate={previewCandidate}
      />,
    );

    await waitFor(() => expect(previewCandidate).toHaveBeenCalledTimes(1));
    const previewFailures = await screen.findAllByText(
      /candidate could not be loaded/i,
    );
    expect(
      previewFailures.find((entry) => !entry.closest("[hidden]")),
    ).toBeVisible();
    rerender(
      <SourceAiOrganizeDialog
        open
        sourceTitle="Interview"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[]}
        failedTask={null}
        task={task}
        candidateId="candidate-1"
        error={null}
        onClose={vi.fn()}
        onOpenTask={openTask}
        onStart={vi.fn().mockResolvedValue(null)}
        onRetry={vi.fn().mockResolvedValue(null)}
        onPreviewCandidate={previewCandidate}
      />,
    );
    await Promise.resolve();
    expect(previewCandidate).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("tab", { name: "Process" }));
    fireEvent.click(screen.getByRole("button", { name: "View task details" }));
    expect(openTask).toHaveBeenCalledWith(task.id);
    fireEvent.click(screen.getByRole("tab", { name: "Final" }));
    fireEvent.click(screen.getByRole("button", { name: "Retry loading" }));
    await waitFor(() => expect(previewCandidate).toHaveBeenCalledTimes(2));
  });

  it("keeps a minimized workbench minimized when the task succeeds", () => {
    const runningTask = sourceAiTask({ status: "running" });
    const succeededTask = sourceAiTask({
      status: "succeeded",
      completedAt: "2026-07-28T00:02:00Z",
      result: {
        summary: "Candidate ready",
        affectedPaths: [],
        reference: {
          type: "source_ai_organize",
          sourceId: "source-1",
          baseVersionId: "version-1",
          baseMarkdownHash: "a".repeat(64),
          candidateId: "candidate-1",
          projectRootPath: "D:/知识库",
        },
      },
    });
    const callbacks = {
      onClose: vi.fn(),
      onOpenTask: vi.fn(),
      onStart: vi.fn().mockResolvedValue(null),
      onRetry: vi.fn().mockResolvedValue(null),
    };
    const { rerender } = render(
      <SourceAiOrganizeDialog
        open
        sourceTitle="Interview"
        unsavedEdits={false}
        busy={false}
        running
        agents={[]}
        providers={[]}
        failedTask={null}
        task={runningTask}
        error={null}
        {...callbacks}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: "Minimize AI organization workbench",
      }),
    );
    rerender(
      <SourceAiOrganizeDialog
        open
        sourceTitle="Interview"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[]}
        failedTask={null}
        task={succeededTask}
        candidateId="candidate-1"
        error={null}
        {...callbacks}
      />,
    );
    expect(
      screen.getByRole("button", {
        name: "Restore AI organization workbench",
      }),
    ).toBeVisible();
    expect(screen.queryByRole("tab", { name: "Final" })).toBeNull();
  });

  it("moves cancellation focus to the safe choice and restores the trigger", () => {
    render(
      <SourceAiOrganizeDialog
        open
        sourceTitle="Interview"
        unsavedEdits={false}
        busy={false}
        running
        agents={[]}
        providers={[]}
        failedTask={null}
        task={sourceAiTask({ status: "running" })}
        error={null}
        onClose={vi.fn()}
        onOpenTask={vi.fn()}
        onStart={vi.fn().mockResolvedValue(null)}
        onRetry={vi.fn().mockResolvedValue(null)}
      />,
    );
    const cancel = screen.getByRole("button", { name: "Cancel task" });
    fireEvent.click(cancel);
    const keepRunning = screen.getByRole("button", { name: "Keep running" });
    expect(keepRunning).toHaveFocus();
    fireEvent.click(keepRunning);
    expect(screen.getByRole("button", { name: "Cancel task" })).toHaveFocus();
  });

  it.each(["claude", "codex"] as const)(
    "treats an installed default %s Agent as available for Source AI",
    async (kind: AgentKind) => {
      const start = vi.fn().mockResolvedValue(false);
      render(
        <SourceAiOrganizeDialog
          open
          sourceTitle="访谈"
          unsavedEdits={false}
          busy={false}
          running={false}
          agents={[
            {
              kind,
              command: kind,
              state: "installed",
              version: "1.0.0",
              executablePath: `C:/tools/${kind}.exe`,
              isDefault: true,
              installGuidance: "",
              error: null,
            },
          ]}
          providers={[]}
          failedTask={null}
          error={null}
          onClose={vi.fn()}
          onOpenTask={vi.fn()}
          onStart={start}
          onRetry={vi.fn().mockResolvedValue(false)}
        />,
      );

      fireEvent.click(screen.getByRole("radio", { name: "Agent" }));
      fireEvent.click(screen.getByRole("button", { name: "Generate candidate" }));
      await waitFor(() =>
        expect(start).toHaveBeenCalledWith({
          route: "agent",
          agent: kind,
          provider: null,
          customInstructions: null,
        }),
      );
    },
  );

  it("only treats BYOK providers with usable credentials as available", async () => {
    const start = vi.fn().mockResolvedValue(false);
    const { rerender } = render(
      <SourceAiOrganizeDialog
        open
        sourceTitle="访谈"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[
          {
            config: {
              provider: "open_ai",
              model: "gpt-source",
              baseUrl: "https://example.test",
              contextWindow: 32_000,
              enabled: true,
            },
            hasSecret: false,
            secretMask: null,
          },
        ]}
        failedTask={null}
        error={null}
        onClose={vi.fn()}
        onOpenTask={vi.fn()}
        onStart={start}
        onRetry={vi.fn().mockResolvedValue(false)}
      />,
    );

    expect(screen.getByRole("button", { name: "Generate candidate" })).toBeDisabled();
    expect(
      screen.getByText(
        "No installed default Agent or ready BYOK provider is available.",
      ),
    ).toBeVisible();

    rerender(
      <SourceAiOrganizeDialog
        open
        sourceTitle="访谈"
        unsavedEdits={false}
        busy={false}
        running={false}
        agents={[]}
        providers={[
          {
            config: {
              provider: "ollama",
              model: "qwen3",
              baseUrl: "http://127.0.0.1:11434",
              contextWindow: 32_000,
              enabled: true,
            },
            hasSecret: false,
            secretMask: null,
          },
        ]}
        failedTask={null}
        error={null}
        onClose={vi.fn()}
        onOpenTask={vi.fn()}
        onStart={start}
        onRetry={vi.fn().mockResolvedValue(false)}
      />,
    );

    fireEvent.click(screen.getByRole("radio", { name: "BYOK" }));
    fireEvent.click(screen.getByRole("button", { name: "Generate candidate" }));
    await waitFor(() =>
      expect(start).toHaveBeenCalledWith({
        route: "byok",
        agent: null,
        provider: "ollama",
        customInstructions: null,
      }),
    );
  });

  it("opens the bounded AI Organize dialog and starts a background candidate task without stealing focus", async () => {
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
      sourceId: "source-1",
      versionId: "version-1",
      sourceStatus: "current",
      quality,
    });
    const tree: WikiTreeData = {
      root: rootFor(bound),
      pages: [bound],
      totalPages: 1,
    };
    const content: WikiPageContent = {
      meta: bound,
      rawMarkdown: "# 访谈\n\n原稿",
      bodyMarkdown: "# 访谈\n\n原稿",
      frontmatterYaml: null,
    };
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-1",
        rootPath: "D:/知识库",
        name: "知识库",
      },
    });
    useTaskStore.setState({
      tasks: [
        {
          id: "foreign-project-task",
          taskType: "source_ai_organize",
          projectId: "project-copy",
          title: "AI organize copied Source",
          status: "running",
          progress: null,
          startedAt: "2026-07-28T00:00:00Z",
          updatedAt: "2026-07-28T00:00:00Z",
          completedAt: null,
          cancellable: true,
          logPath: null,
          result: {
            summary: "running",
            affectedPaths: [],
            reference: {
              type: "source_ai_organize",
              sourceId: "source-1",
              baseVersionId: "version-1",
              baseMarkdownHash: "a".repeat(64),
              candidateId: null,
            },
          },
          error: null,
        },
      ],
      drawerOpen: false,
      selectedTaskId: null,
      runningCount: 1,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(content);
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_source_detail") return Promise.resolve(sourceDetail());
      if (command === "start_source_ai_organize") {
        return Promise.resolve({
          id: "task-source-ai",
          taskType: "source_ai_organize",
          projectId: "project-1",
          title: "AI organize Source",
          status: "queued",
          progress: null,
          startedAt: "2026-07-28T00:00:00Z",
          updatedAt: "2026-07-28T00:00:00Z",
          completedAt: null,
          cancellable: true,
          logPath: null,
          result: {
            summary: "queued",
            affectedPaths: [],
            reference: {
              type: "source_ai_organize",
              sourceId: "source-1",
              baseVersionId: "version-1",
              baseMarkdownHash: "a".repeat(64),
              candidateId: null,
            },
          },
          error: null,
        });
      }
      return Promise.resolve(null);
    });

    render(<WikiView capabilities={readySourceAiCapabilities} />);

    const organize = await screen.findByRole("button", { name: "AI Organize" });
    expect(organize).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Ask AI" })).toBeNull();
    fireEvent.click(organize);
    expect(await screen.findByRole("dialog", { name: "AI Organize Source" })).toBeVisible();
    expect(screen.getByText("Current Agent: codex")).toBeVisible();
    expect(screen.getByText("Current BYOK: OpenAI / gpt-source")).toBeVisible();
    expect(
      screen.getByText(
        "No raw attachment bytes, other Wiki pages, or task logs; credentials are not included in the prompt",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Agent reuses the selected CLI login; BYOK keys stay in OS credential storage",
      ),
    ).toBeVisible();
    act(() => {
      const otherMeta = page({
        path: "wiki/sources/local/other.md",
        title: "Other Source",
        sourceBinding: {
          sourceId: "source-2",
          versionId: "version-2",
          status: "current",
          quality,
        },
        sourceId: "source-2",
        versionId: "version-2",
      });
      useWikiStore.setState({
        selectedPath: otherMeta.path,
        page: {
          meta: otherMeta,
          rawMarkdown: "# Other",
          bodyMarkdown: "# Other",
          frontmatterYaml: null,
        },
        draft: "# Other",
      });
      useSourceStore.setState({
        detail: {
          ...sourceDetail(),
          sourceId: "source-2",
          versionId: "version-2",
          title: "Other Source",
          currentMarkdownHash: "b".repeat(64),
        },
      });
    });
    fireEvent.change(screen.getByPlaceholderText(/Add organization preferences/), {
      target: { value: "Keep the current quotations." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Generate candidate" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("start_source_ai_organize", {
        request: {
          projectId: "project-1",
          projectRootPath: "D:/知识库",
          sourceId: "source-1",
          expectedVersionId: "version-1",
          expectedMarkdownHash: "a".repeat(64),
          route: "auto",
          agent: null,
          provider: null,
          customInstructions: "Keep the current quotations.",
        },
      }),
    );
    await waitFor(() => {
      expect(
        Boolean(useTaskStore.getState().taskById["task-source-ai"]),
      ).toBe(true);
      expect(useTaskStore.getState().drawerOpen).toBe(false);
      expect(useTaskStore.getState().selectedTaskId).toBeNull();
    });
    expect(
      screen.getByRole("dialog", { name: "AI Organize Source" }),
    ).toBeVisible();
  });

  it("shows only the latest failed attempt for the current Source version and hash", async () => {
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
    });
    const tree: WikiTreeData = {
      root: rootFor(bound),
      pages: [bound],
      totalPages: 1,
    };
    const content: WikiPageContent = {
      meta: bound,
      rawMarkdown: "# 访谈\n\n原稿",
      bodyMarkdown: "# 访谈\n\n原稿",
      frontmatterYaml: null,
    };
    const oldFailure = sourceAiTask({
      id: "old-failure",
      status: "failed",
      updatedAt: "2026-07-28T00:01:00Z",
      completedAt: "2026-07-28T00:01:00Z",
      error: {
        code: "OLD_FAILURE",
        message: "Older failure.",
        details: null,
        recoverable: true,
        userActionRequired: false,
      },
    });
    const currentFailure = sourceAiTask({
      id: "current-failure",
      status: "failed",
      updatedAt: "2026-07-28T00:02:00Z",
      completedAt: "2026-07-28T00:02:00Z",
      error: {
        code: "LLM_RESPONSE_INVALID",
        message: "Current failure.",
        details: null,
        recoverable: true,
        userActionRequired: false,
      },
    });
    const wrongHashFailure = sourceAiTask({
      id: "wrong-hash-failure",
      status: "failed",
      updatedAt: "2026-07-28T00:03:00Z",
      completedAt: "2026-07-28T00:03:00Z",
      result: {
        summary: "failed",
        affectedPaths: [],
        reference: {
          type: "source_ai_organize",
          sourceId: "source-1",
          baseVersionId: "version-1",
          baseMarkdownHash: "b".repeat(64),
          candidateId: null,
          projectRootPath: "D:/知识库",
        },
      },
      error: {
        code: "WRONG_HASH_FAILURE",
        message: "Failure for an obsolete hash.",
        details: null,
        recoverable: true,
        userActionRequired: false,
      },
    });
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-1",
        rootPath: "D:/知识库",
      },
    });
    useTaskStore.setState({
      tasks: [oldFailure, currentFailure, wrongHashFailure],
      drawerOpen: false,
      selectedTaskId: null,
      runningCount: 0,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(content);
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_source_detail") return Promise.resolve(sourceDetail());
      if (command === "retry_source_ai_organize") {
        return Promise.resolve(
          sourceAiTask({
            id: "retried-current-failure",
            updatedAt: "2026-07-28T00:04:00Z",
          }),
        );
      }
      return Promise.resolve(null);
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    fireEvent.click(await screen.findByRole("button", { name: "AI Organize" }));
    expect(await screen.findByText("Current failure.")).toBeVisible();
    expect(screen.queryByText("Older failure.")).toBeNull();
    expect(screen.queryByText("Failure for an obsolete hash.")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "View task details" }));
    expect(useTaskStore.getState().selectedTaskId).toBe("current-failure");
    expect(screen.queryByRole("dialog", { name: "AI Organize Source" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "AI Organize" }));
    expect(await screen.findByText("Current failure.")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Retry with saved settings" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("retry_source_ai_organize", {
        request: {
          projectId: "project-1",
          projectRootPath: "D:/知识库",
          taskId: "current-failure",
        },
      }),
    );
    await waitFor(() =>
      expect(
        useTaskStore.getState().taskById["retried-current-failure"] !== undefined,
      ).toBe(true),
    );
    expect(useTaskStore.getState().selectedTaskId).toBe("current-failure");
    expect(
      screen.getByRole("dialog", { name: "AI Organize Source" }),
    ).toBeVisible();
  });

  it("refreshes a successful candidate once per task identity", async () => {
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
    });
    const tree: WikiTreeData = {
      root: rootFor(bound),
      pages: [bound],
      totalPages: 1,
    };
    const content: WikiPageContent = {
      meta: bound,
      rawMarkdown: "# 访谈\n\n原稿",
      bodyMarkdown: "# 访谈\n\n原稿",
      frontmatterYaml: null,
    };
    let detailReads = 0;
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-1",
        rootPath: "D:/知识库",
      },
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(content);
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_source_detail") {
        detailReads += 1;
        return Promise.resolve(sourceDetail());
      }
      return Promise.resolve(null);
    });

    render(<WikiView capabilities={emptyAiCapabilities} />);
    await screen.findByRole("button", { name: "AI Organize" });
    act(() => {
      useTaskStore.setState({
        tasks: [
          sourceAiTask({
            id: "completed-source-ai",
            status: "succeeded",
            updatedAt: "2026-07-28T00:02:00Z",
            completedAt: "2026-07-28T00:02:00Z",
            result: {
              summary: "candidate ready",
              affectedPaths: [],
              reference: {
                type: "source_ai_organize",
                sourceId: "source-1",
                baseVersionId: "version-1",
                baseMarkdownHash: "a".repeat(64),
                candidateId: "candidate-1",
                projectRootPath: "D:/知识库",
              },
            },
          }),
        ],
        runningCount: 0,
      });
    });

    await waitFor(() => expect(detailReads).toBe(1));
    act(() => {
      useSourceStore.setState({ detail: sourceDetail() });
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(detailReads).toBe(1);
  });

  it("clears the pending AI start state after navigating to another Source", async () => {
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-1",
        rootPath: "D:/知识库",
      },
    });
    useSourceStore.setState({ detail: sourceDetail() });
    let resolveTask!: (task: BackendTask) => void;
    const pendingTask = new Promise<BackendTask>((resolve) => {
      resolveTask = resolve;
    });
    invokeMock.mockImplementation(
      (command: string) =>
        command === "start_source_ai_organize"
          ? pendingTask
          : Promise.resolve(null),
    );
    const start = useSourceStore.getState().startAiOrganize("project-1", "D:/知识库", {
      route: "auto",
      agent: null,
      provider: null,
      customInstructions: null,
    });
    expect(useSourceStore.getState().aiOrganizeStarting).toBe(true);
    useSourceStore.setState({
      detail: {
        ...sourceDetail(),
        sourceId: "source-2",
        versionId: "version-2",
      },
    });
    resolveTask({
      id: "task-after-navigation",
      taskType: "source_ai_organize",
      projectId: "project-1",
      title: "AI organize Source",
      status: "queued",
      progress: null,
      startedAt: "2026-07-28T00:00:00Z",
      updatedAt: "2026-07-28T00:00:00Z",
      completedAt: null,
      cancellable: true,
      logPath: null,
      result: null,
      error: null,
    });
    await start;
    expect(useSourceStore.getState().aiOrganizeStarting).toBe(false);
    expect(
      useTaskStore.getState().taskById["task-after-navigation"] !== undefined,
    ).toBe(true);
    expect(useTaskStore.getState().drawerOpen).toBe(false);
    expect(useTaskStore.getState().selectedTaskId).toBeNull();
  });

  it("drops a returned AI task after the active project changes", async () => {
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "project-1",
      rootPath: "D:/知识库",
    });
    useSourceStore.setState({ detail: sourceDetail() });
    let resolveTask!: (task: BackendTask) => void;
    const pendingTask = new Promise<BackendTask>((resolve) => {
      resolveTask = resolve;
    });
    invokeMock.mockImplementation(
      (command: string) =>
        command === "start_source_ai_organize"
          ? pendingTask
          : Promise.resolve(null),
    );

    const start = useSourceStore
      .getState()
      .startAiOrganize("project-1", "D:/知识库", {
        route: "auto",
        agent: null,
        provider: null,
        customInstructions: null,
      });
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "project-2",
      rootPath: "D:/另一个知识库",
    });
    resolveTask(sourceAiTask({ id: "old-project-task" }));

    await start;
    expect(
      useTaskStore
        .getState()
        .tasks.some((task) => task.id === "old-project-task"),
    ).toBe(false);
    expect(useTaskStore.getState().drawerOpen).toBe(false);
    expect(useTaskStore.getState().selectedTaskId).toBeNull();
  });

  it("blocks AI candidate generation while the Source editor has unsaved changes", async () => {
    await i18next.changeLanguage("zh-CN");
    const bound = page({
      sourceBinding: {
        sourceId: "source-1",
        versionId: "version-1",
        status: "current",
        quality,
      },
      sourceId: "source-1",
      versionId: "version-1",
      sourceStatus: "current",
      quality,
    });
    const tree: WikiTreeData = {
      root: rootFor(bound),
      pages: [bound],
      totalPages: 1,
    };
    const content: WikiPageContent = {
      meta: bound,
      rawMarkdown: "# 访谈\n\n原稿",
      bodyMarkdown: "# 访谈\n\n原稿",
      frontmatterYaml: null,
    };
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-1",
        rootPath: "D:/知识库",
        name: "知识库",
      },
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_wiki") return Promise.resolve(tree);
      if (command === "read_wiki_page") return Promise.resolve(content);
      if (command === "list_exports") return Promise.resolve([]);
      if (command === "get_source_detail") return Promise.resolve(sourceDetail());
      return Promise.resolve(null);
    });
    render(<WikiView capabilities={emptyAiCapabilities} />);
    await screen.findByRole("button", { name: "AI 整理" });
    useWikiStore.setState({
      mode: "edit",
      draft: "# 访谈\n\n未保存的人工修改",
    });
    fireEvent.click(screen.getByRole("button", { name: "AI 整理" }));
    expect(await screen.findByText(/生成候选稿前，请先保存或放弃/)).toBeVisible();
    expect(screen.getByRole("button", { name: "生成候选稿" })).toBeDisabled();
    const dialog = screen.getByRole("dialog", { name: "AI 整理 Source" });
    expect(dialog.contains(document.activeElement)).toBe(false);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(dialog).toBeVisible();
  });
});

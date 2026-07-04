import { type CSSProperties, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Book, Edit2, FileOutput, LoaderCircle, Star } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import { useExportStore } from "../../stores/exportStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { ConfirmationDialog } from "../../components/app/ConfirmationDialog";
import type { PendingAction } from "../../types/backend";
import type { ExportRecord, ExportType } from "../../types/export";
import { isTerminalStatus, type BackendTask } from "../../types/task";
import type { CreateWikiPageInput, WikiPageContent, WikiPageMeta } from "../../types/wiki";
import { MarkdownReader } from "./MarkdownReader";
import { ConflictDiffDialog } from "./ConflictDiffDialog";
import { GenerateHtmlDialog } from "./GenerateHtmlDialog";
import { HtmlPreviewPane } from "./HtmlPreviewPane";
import { WikiEditor } from "./WikiEditor";
import { WikiPageFormDialog } from "./WikiPageFormDialog";
import { WikiTree } from "./WikiTree";
import { useWikiStore } from "./wikiStore";

export function selectWikiPreviewRecord(
  records: ExportRecord[],
  previewId: string | null,
  selectedPath: string | null,
): ExportRecord | null {
  const belongsToPage = (record: ExportRecord) =>
    record.exportType === "project_report" || record.sourcePath === selectedPath;
  const selected = records.find((record) => record.id === previewId);
  if (selected && belongsToPage(selected)) return selected;
  return (
    records
      .filter(belongsToPage)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0] ?? null
  );
}

export function WikiView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const paneSizes = useNavigationStore((state) => state.paneSizes);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);
  const [pageForm, setPageForm] = useState<
    { mode: "create" | "rename"; path: string } | null
  >(null);
  const [pendingLifecycle, setPendingLifecycle] = useState<
    | { kind: "rename"; action: PendingAction; oldPath: string; newPath: string }
    | { kind: "delete"; action: PendingAction }
    | null
  >(null);
  const [htmlDialogOpen, setHtmlDialogOpen] = useState(false);
  const [htmlTemplate, setHtmlTemplate] = useState<ExportType>("beautiful_read");
  const [conflictDialogOpen, setConflictDialogOpen] = useState(true);

  const tree = useWikiStore((state) => state.tree);
  const loadingTree = useWikiStore((state) => state.loadingTree);
  const selectedPath = useWikiStore((state) => state.selectedPath);
  const page = useWikiStore((state) => state.page);
  const mode = useWikiStore((state) => state.mode);
  const draft = useWikiStore((state) => state.draft);
  const saveState = useWikiStore((state) => state.saveState);
  const conflict = useWikiStore((state) => state.conflict);
  const loadingPage = useWikiStore((state) => state.loadingPage);
  const wikiError = useWikiStore((state) => state.error);
  const scan = useWikiStore((state) => state.scan);
  const openPage = useWikiStore((state) => state.openPage);
  const startEdit = useWikiStore((state) => state.startEdit);
  const setMode = useWikiStore((state) => state.setMode);
  const setDraft = useWikiStore((state) => state.setDraft);
  const save = useWikiStore((state) => state.save);
  const resolveConflict = useWikiStore((state) => state.resolveConflict);
  const cancelEdit = useWikiStore((state) => state.cancelEdit);
  const reload = useWikiStore((state) => state.reload);
  const toggleBookmark = useWikiStore((state) => state.toggleBookmark);
  const createPage = useWikiStore((state) => state.createPage);
  const renamePage = useWikiStore((state) => state.renamePage);
  const requestDeletePage = useWikiStore((state) => state.requestDeletePage);
  const confirmDeletePage = useWikiStore((state) => state.confirmDeletePage);
  const cancelPendingAction = useWikiStore((state) => state.cancelPendingAction);
  const requestedExportType = useWikiStore((state) => state.requestedExportType);
  const consumeExportRequest = useWikiStore((state) => state.consumeExportRequest);

  const exportRecords = useExportStore((state) => state.records);
  const runningExportTaskId = useExportStore((state) => state.runningTaskId);
  const previewHtml = useExportStore((state) => state.previewHtml);
  const previewId = useExportStore((state) => state.previewId);
  const loadExports = useExportStore((state) => state.loadExports);
  const startExport = useExportStore((state) => state.startExport);
  const regenerateExport = useExportStore((state) => state.regenerateExport);
  const clearRunningTask = useExportStore((state) => state.clearRunningTask);
  const loadPreview = useExportStore((state) => state.loadPreview);
  const openFolder = useExportStore((state) => state.openFolder);
  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const { projectId, rootPath } = currentProject;
  const layoutStyle = {
    "--wiki-tree-w-current": `${paneSizes.wikiTree}px`,
  } as CSSProperties;

  useEffect(() => {
    void scan(projectId, rootPath);
  }, [projectId, rootPath, scan]);

  useEffect(() => {
    void loadExports(projectId, rootPath);
  }, [projectId, rootPath, loadExports]);

  useEffect(() => {
    if (!requestedExportType) return;
    setHtmlTemplate(requestedExportType);
    setHtmlDialogOpen(true);
    consumeExportRequest();
  }, [requestedExportType, consumeExportRequest]);

  useEffect(() => {
    if (conflict) setConflictDialogOpen(true);
  }, [conflict?.currentHash]);

  const runningExportTask = runningExportTaskId
    ? tasks.find((task) => task.id === runningExportTaskId) ?? null
    : null;

  useEffect(() => {
    if (!runningExportTask || !isTerminalStatus(runningExportTask.status)) return;
    void loadExports(projectId, rootPath).then(() => {
      clearRunningTask();
      if (runningExportTask.status !== "succeeded") return;
      const latest = useExportStore
        .getState()
        .records.filter((record) => record.exportType === htmlTemplate)
        .filter((record) =>
          htmlTemplate === "project_report"
            ? true
            : record.sourcePath === useWikiStore.getState().selectedPath,
        )
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
      if (!latest) return;
      void loadPreview(
        { projectId, projectRootPath: rootPath, outputPath: latest.outputPath },
        latest.id,
      );
    });
  }, [
    runningExportTask,
    projectId,
    rootPath,
    htmlTemplate,
    loadExports,
    clearRunningTask,
    loadPreview,
  ]);

  const handleOpen = (path: string) => {
    void openPage(projectId, rootPath, path);
  };

  const breadcrumbs = selectedPath ? selectedPath.split("/") : [];
  const previewRecord = selectWikiPreviewRecord(exportRecords, previewId, selectedPath);
  const pagePreviewHtml = previewRecord?.id === previewId ? previewHtml : null;

  const showExportTask = (taskId: string) => {
    void invoke<BackendTask>("get_task", { request: { taskId } }).then((task) => {
      if (task) upsertTask(task);
    });
    openTaskDrawer(taskId);
  };

  const handleGenerateHtml = (type: ExportType) => {
    if (!page) return;
    setHtmlTemplate(type);
    setHtmlDialogOpen(false);
    setMode("preview");
    const sourcePath = type === "project_report" ? "" : page.meta.path;
    void startExport(projectId, rootPath, type, sourcePath).then((taskId) => {
      if (taskId) showExportTask(taskId);
    });
  };

  const handleRegenerateHtml = () => {
    if (!previewRecord) {
      setHtmlDialogOpen(true);
      return;
    }
    setHtmlTemplate(previewRecord.exportType);
    void regenerateExport(projectId, rootPath, previewRecord).then((taskId) => {
      if (taskId) showExportTask(taskId);
    });
  };

  const handlePageFormSubmit = (input: CreateWikiPageInput) => {
    if (pageForm?.mode === "create") {
      setPageForm(null);
      void createPage(projectId, rootPath, input);
      return;
    }
    if (pageForm?.mode !== "rename") return;
    const oldPath = pageForm.path;
    const action: PendingAction = {
      id: `rename-${oldPath}`,
      actionType: "batch_rewrite",
      title: t("wiki.rename.confirmTitle"),
      message: t("wiki.rename.confirmMessage", {
        oldPath,
        newPath: input.relativePath,
      }),
      riskLevel: "high",
      affectedPaths: [oldPath, input.relativePath],
      preview: {
        summary: t("wiki.rename.confirmSummary"),
        before: oldPath,
        after: input.relativePath,
        diff: null,
      },
      expiresAt: null,
      checkpointHash: null,
    };
    setPageForm(null);
    setPendingLifecycle({
      kind: "rename",
      action,
      oldPath,
      newPath: input.relativePath,
    });
  };

  const handleDeleteRequest = (path: string) => {
    void requestDeletePage(projectId, rootPath, path).then((action) => {
      if (action) setPendingLifecycle({ kind: "delete", action });
    });
  };

  const handleLifecycleCancel = () => {
    const pending = pendingLifecycle;
    setPendingLifecycle(null);
    if (pending?.kind === "delete") {
      void cancelPendingAction(pending.action);
    }
  };

  const handleLifecycleConfirm = () => {
    const pending = pendingLifecycle;
    if (!pending) return;
    setPendingLifecycle(null);
    if (pending.kind === "delete") {
      void confirmDeletePage(projectId, rootPath, pending.action);
    } else {
      void renamePage(projectId, rootPath, pending.oldPath, pending.newPath);
    }
  };

  return (
    <div className="wiki-view-layout" style={layoutStyle}>
      {tree ? (
        <WikiTree
          root={tree.root}
          pages={tree.pages}
          selectedPath={selectedPath}
          onSelect={handleOpen}
          onRefresh={() => void reload(projectId, rootPath)}
          onCreate={() => setPageForm({ mode: "create", path: "wiki/" })}
          onRename={(path) => setPageForm({ mode: "rename", path })}
          onDelete={handleDeleteRequest}
        />
      ) : (
        <div className="wiki-tree items-center justify-center text-[12px] text-[var(--text-muted)]">
          {loadingTree ? (
            <LoaderCircle size={16} className="animate-spin" />
          ) : (
            t("wiki.tree.empty")
          )}
        </div>
      )}

      <ResizableSplitter
        paneId="wikiTree"
        label={t("shell.splitter.wikiTree")}
        min={PANE_WIDTH_LIMITS.wikiTree.min}
        max={PANE_WIDTH_LIMITS.wikiTree.max}
        value={paneSizes.wikiTree}
        onChange={(value) => setPaneSize("wikiTree", value)}
        onReset={() => resetPaneSize("wikiTree")}
      />

      <div className="flex min-w-0 flex-1 flex-col bg-[var(--background)]">
        <div className="view-toolbar border-b border-[var(--border)] px-5">
          <div className="flex min-w-0 items-center gap-1.5 font-mono text-[12px] text-[var(--text-muted)]">
            {breadcrumbs.length === 0 ? (
              <span>{t("wiki.content.noSelection")}</span>
            ) : (
              breadcrumbs.map((segment, index) => (
                <span key={`${segment}-${index}`} className="flex items-center gap-1.5">
                  <span
                    className={
                      index === breadcrumbs.length - 1
                        ? "font-medium text-[var(--text-primary)]"
                        : ""
                    }
                  >
                    {segment}
                  </span>
                  {index < breadcrumbs.length - 1 ? (
                    <span className="text-[var(--text-disabled)]">/</span>
                  ) : null}
                </span>
              ))
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <span
              className={`hidden items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium sm:inline-flex ${
                saveState === "saved"
                  ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
                  : saveState === "conflict" || saveState === "error"
                    ? "bg-[var(--warning-soft)] text-[var(--warning)]"
                    : "bg-[var(--surface-muted)] text-[var(--text-muted)]"
              }`}
            >
              <span
                className={`inline-block h-[6px] w-[6px] rounded-full ${
                  saveState === "saved"
                    ? "bg-[var(--accent)]"
                    : saveState === "conflict" || saveState === "error"
                      ? "bg-[var(--warning)]"
                      : "bg-[var(--text-muted)]"
                }`}
              />
              {t(`wiki.editor.saveState.${saveState}`)}
            </span>
            <div className="flex overflow-hidden rounded-[var(--radius-sm)] border border-[var(--border)]">
              <ModeButton
                active={mode === "read"}
                onClick={() => setMode("read")}
                icon={<Book size={13} />}
                label={t("wiki.mode.read")}
              />
              <ModeButton
                active={mode === "edit"}
                onClick={() => startEdit()}
                icon={<Edit2 size={13} />}
                label={t("wiki.mode.edit")}
              />
              <ModeButton
                active={mode === "preview"}
                onClick={() => {
                  if (pagePreviewHtml) {
                    setMode("preview");
                  } else if (previewRecord) {
                    void loadPreview(
                      {
                        projectId,
                        projectRootPath: rootPath,
                        outputPath: previewRecord.outputPath,
                      },
                      previewRecord.id,
                    );
                    setMode("preview");
                  } else {
                    setHtmlDialogOpen(true);
                  }
                }}
                icon={<FileOutput size={13} />}
                label={t("wiki.mode.preview")}
              />
            </div>
            <button
              type="button"
              disabled={!page || Boolean(runningExportTaskId)}
              onClick={() => setHtmlDialogOpen(true)}
              className="inline-flex h-[28px] items-center gap-1.5 rounded-[var(--radius-sm)] bg-[var(--foreground)] px-3 text-[11.5px] font-medium text-[var(--text-inverse)] disabled:opacity-40"
            >
              <FileOutput size={13} />
              {t("wiki.html.generate")}
            </button>
            <button
              type="button"
              disabled={!page}
              onClick={() => {
                if (page) void toggleBookmark(projectId, rootPath);
              }}
              title={t("wiki.content.star")}
              aria-label={t("wiki.content.star")}
              aria-pressed={page?.meta.bookmarked ?? false}
              className="grid h-[28px] w-[28px] place-items-center rounded-[var(--radius-sm)] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)] disabled:opacity-40"
            >
              <Star
                size={14}
                className={
                  page?.meta.bookmarked
                    ? "fill-[var(--accent)] text-[var(--accent)]"
                    : ""
                }
              />
            </button>
          </div>
        </div>

        {wikiError ? (
          <div role="alert" className="shrink-0 border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {wikiError}
          </div>
        ) : null}

        <div className="min-h-0 flex-1 overflow-y-auto">
          {!page ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {loadingPage ? (
                <LoaderCircle size={16} className="animate-spin" />
              ) : (
                t("wiki.content.noSelection")
              )}
            </div>
          ) : mode === "edit" ? (
            <div className="mx-auto flex h-full max-w-[760px] flex-col px-8 py-6">
              <WikiEditor
                key={selectedPath ?? undefined}
                draft={draft}
                saveState={saveState}
                onDraftChange={setDraft}
                onSave={() => void save(projectId, rootPath)}
                onCancel={cancelEdit}
                onReload={() => void reload(projectId, rootPath)}
                onReviewConflict={() => setConflictDialogOpen(true)}
              />
            </div>
          ) : mode === "preview" ? (
            <HtmlPreviewPane
              html={pagePreviewHtml}
              outputPath={previewRecord?.outputPath ?? null}
              templateLabel={t(`wiki.html.template.${previewRecord?.exportType ?? htmlTemplate}.title`)}
              busy={Boolean(runningExportTaskId)}
              onBack={() => setMode("read")}
              onRegenerate={handleRegenerateHtml}
              onOpenFolder={() => {
                if (!previewRecord) return;
                void openFolder({
                  projectId,
                  projectRootPath: rootPath,
                  outputPath: previewRecord.outputPath,
                });
              }}
              onCopyPath={() => {
                if (previewRecord) void navigator.clipboard.writeText(previewRecord.outputPath);
              }}
            />
          ) : (
            <ReadingPane
              page={page}
              pages={tree?.pages ?? []}
              onOpenPage={handleOpen}
            />
          )}
        </div>
      </div>
      {pageForm ? (
        <WikiPageFormDialog
          mode={pageForm.mode}
          initialPath={pageForm.path}
          onCancel={() => setPageForm(null)}
          onSubmit={handlePageFormSubmit}
        />
      ) : null}
      {pendingLifecycle ? (
        <ConfirmationDialog
          action={pendingLifecycle.action}
          checkpointExists={pendingLifecycle.action.checkpointHash != null}
          onCancel={handleLifecycleCancel}
          onConfirm={handleLifecycleConfirm}
        />
      ) : null}
      {conflict && conflictDialogOpen ? (
        <ConflictDiffDialog
          conflict={conflict}
          onCancel={() => setConflictDialogOpen(false)}
          onKeepCurrent={() => void resolveConflict(projectId, rootPath, "keep_current")}
          onUseIncoming={() => void resolveConflict(projectId, rootPath, "use_incoming")}
          onManualMerge={(content) =>
            void resolveConflict(projectId, rootPath, "manual_merge", content)
          }
        />
      ) : null}
      {htmlDialogOpen && page ? (
        <GenerateHtmlDialog
          pagePath={page.meta.path}
          initialType={htmlTemplate}
          onCancel={() => setHtmlDialogOpen(false)}
          onGenerate={handleGenerateHtml}
        />
      ) : null}
    </div>
  );
}

function ReadingPane({
  page,
  pages,
  onOpenPage,
}: {
  page: WikiPageContent;
  pages: WikiPageMeta[];
  onOpenPage: (path: string) => void;
}) {
  return (
    <div className="flex justify-center px-8 py-6">
      <div className="w-full max-w-[760px]">
        <MarkdownReader
          bodyMarkdown={page.bodyMarkdown}
          frontmatterYaml={page.frontmatterYaml}
          pages={pages}
          onOpenPage={onOpenPage}
        />
      </div>
    </div>
  );
}

function ModeButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={`inline-flex h-[26px] items-center gap-1 px-2 text-[11.5px] font-medium transition-colors ${
        active
          ? "bg-[var(--surface-muted)] text-[var(--text-primary)]"
          : "bg-transparent text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
      }`}
    >
      {icon}
      {label}
    </button>
  );
}

import { type CSSProperties, useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  Code2,
  ExternalLink,
  Eye,
  FileOutput,
  FolderOpen,
  List,
  Maximize2,
  Minimize2,
  Plus,
  Star,
  type LucideIcon,
} from "lucide-react";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import { pathBasename } from "../../lib/pathDisplay";
import { useExportStore } from "../../stores/exportStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { cancelTaskRequest, useTaskStore } from "../../stores/taskStore";
import { isTerminalStatus } from "../../types/task";
import { type ExportRecord, type ExportType } from "../../types/export";
import { HtmlPreviewPane } from "./HtmlPreviewPane";

const TYPE_ICON: Record<ExportType, LucideIcon> = {
  beautiful_read: FileOutput,
  knowledge_card: FileOutput,
  concept_map: FileOutput,
  project_report: FileOutput,
};

function formatTimestamp(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString();
}

export function ExportsView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const exportsListWidth = useNavigationStore((state) => state.paneSizes.exportsList);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);
  const requestWorkflowLaunch = useNavigationStore((state) => state.requestWorkflowLaunch);

  const records = useExportStore((state) => state.records);
  const loading = useExportStore((state) => state.loading);
  const runningTaskId = useExportStore((state) => state.runningTaskId);
  const previewHtml = useExportStore((state) => state.previewHtml);
  const previewId = useExportStore((state) => state.previewId);
  const previewMode = useExportStore((state) => state.previewMode);
  const error = useExportStore((state) => state.error);

  const loadExports = useExportStore((state) => state.loadExports);
  const clearRunningTask = useExportStore((state) => state.clearRunningTask);
  const loadPreview = useExportStore((state) => state.loadPreview);
  const clearPreview = useExportStore((state) => state.clearPreview);
  const setPreviewMode = useExportStore((state) => state.setPreviewMode);
  const toggleBookmark = useExportStore((state) => state.toggleBookmark);
  const openFolder = useExportStore((state) => state.openFolder);
  const openInBrowser = useExportStore((state) => state.openInBrowser);
  const workspaceFocus = useNavigationStore((state) => state.workspaceFocus);
  const focusWorkspace = useNavigationStore((state) => state.focusWorkspace);
  const clearWorkspaceFocus = useNavigationStore((state) => state.clearWorkspaceFocus);

  const tasks = useTaskStore((state) => state.tasks);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const { projectId, rootPath } = currentProject;
  const layoutRef = useRef<HTMLDivElement>(null);
  const layoutStyle = {
    "--exports-list-w-current": `${exportsListWidth}px`,
  } as CSSProperties;
  // Guards the terminal handler against re-running for the same task if the
  // task event stream emits two rapid updates before the running-task id clears.
  const processedTerminalRef = useRef<string | null>(null);

  // Load the export history when the view mounts or the project changes.
  useEffect(() => {
    void loadExports(projectId, rootPath);
  }, [projectId, rootPath, loadExports]);

  useEffect(() => {
    return () => {
      if (useNavigationStore.getState().workspaceFocus === "exportPreview") {
        useNavigationStore.getState().clearWorkspaceFocus();
      }
    };
  }, [projectId, rootPath]);

  const runningTask = runningTaskId
    ? tasks.find((task) => task.id === runningTaskId) ?? null
    : null;

  // When the background export task lands, refresh the list + clear the running
  // id. If the user asked to open the preview, load the just-written record.
  useEffect(() => {
    if (!runningTask || !isTerminalStatus(runningTask.status)) return;
    const finishedId = runningTask.id;
    if (processedTerminalRef.current === finishedId) return;
    processedTerminalRef.current = finishedId;
    const succeeded = runningTask.status === "succeeded";
    void loadExports(projectId, rootPath).then(() => {
      // Only auto-preview the exact record this task produced — never fall back
      // to the newest row, which could belong to a different concurrent export.
      if (succeeded) {
        const target = useExportStore
          .getState()
          .records.find((record) => record.taskId === finishedId);
        if (target) {
          void loadPreview(
            { projectId, projectRootPath: rootPath, outputPath: target.outputPath },
            target.id,
          );
        }
      }
    });
    clearRunningTask();
  }, [
    runningTask,
    projectId,
    rootPath,
    loadExports,
    clearRunningTask,
    loadPreview,
  ]);

  const handleCancel = () => {
    if (!runningTaskId) return;
    void cancelTaskRequest(runningTaskId);
  };

  const handlePreview = (record: ExportRecord) => {
    void loadPreview(
      { projectId, projectRootPath: rootPath, outputPath: record.outputPath },
      record.id,
    );
  };

  const handleRegenerate = (record: ExportRecord) => {
    requestWorkflowLaunch({
      projectId,
      projectRootPath: rootPath,
      kind: "generate_content",
      origin: "exports",
      scopePreset: {
        kind: "generate_content",
        artifactType: record.exportType,
        pagePaths: record.sourcePath ? [record.sourcePath] : [],
        outputPath: record.outputPath,
      },
    });
  };

  const handleViewLogs = (record: ExportRecord) => {
    if (record.taskId) openTaskDrawer(record.taskId);
  };

  const handleOpenFolder = (record: ExportRecord) => {
    void openFolder({
      projectId,
      projectRootPath: rootPath,
      outputPath: record.outputPath,
    });
  };

  const handleOpenInBrowser = (record: ExportRecord) => {
    void openInBrowser({
      projectId,
      projectRootPath: rootPath,
      outputPath: record.outputPath,
    });
  };

  const handleToggleFocus = () => {
    if (workspaceFocus === "exportPreview") {
      clearWorkspaceFocus();
      return;
    }
    focusWorkspace("exportPreview");
  };

  const handleClearPreview = () => {
    clearPreview();
    if (workspaceFocus === "exportPreview") {
      clearWorkspaceFocus();
    }
  };

  const handleToggleBookmark = (record: ExportRecord) => {
    void toggleBookmark(projectId, rootPath, record.id);
  };

  const sortedRecords = useMemo(
    () =>
      [...records].sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
    [records],
  );
  const previewRecord = sortedRecords.find((record) => record.id === previewId) ?? null;

  return (
    <div
      ref={layoutRef}
      className={`exports-view-layout ${workspaceFocus === "exportPreview" ? "is-preview-focused" : ""}`.trim()}
      style={layoutStyle}
    >
      <div className="exports-view__list-pane">
        <div className="view-toolbar border-b border-[var(--border)] px-4">
          <div className="ml-auto flex items-center gap-2">
            {runningTaskId ? (
              <button
                type="button"
                onClick={handleCancel}
                className="btn btn--sm"
              >
                {t("exports.actions.cancel")}
              </button>
            ) : (
              <button
                type="button"
                onClick={() =>
                  requestWorkflowLaunch({
                    projectId,
                    projectRootPath: rootPath,
                    kind: "generate_content",
                    origin: "exports",
                    scopePreset: {
                      kind: "generate_content",
                      artifactType: "beautiful_read",
                      pagePaths: [],
                      outputPath: null,
                    },
                  })
                }
                className="btn btn--primary btn--sm"
              >
                <Plus size={12} strokeWidth={2} aria-hidden />
                {t("exports.actions.newExport")}
              </button>
            )}
          </div>
        </div>
        {error ? (
          <div className="border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {error}
          </div>
        ) : null}
        <div className="min-h-0 flex-1 overflow-auto">
          {sortedRecords.length === 0 ? (
            <div className="px-4 py-8 text-[12px] text-[var(--text-muted)]">
              {t("exports.list.empty")}
            </div>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th className="col-icon"></th>
                  <th className="col-file">{t("exports.table.file")}</th>
                  <th>{t("exports.table.type")}</th>
                  <th>{t("exports.table.source")}</th>
                  <th>{t("exports.table.time")}</th>
                  <th>{t("exports.table.route")}</th>
                  <th>{t("exports.table.status")}</th>
                  <th className="col-export-actions"></th>
                </tr>
              </thead>
              <tbody>
                {sortedRecords.map((record) => {
                  const Icon = TYPE_ICON[record.exportType];
                  const failed = record.status === "failed";
                  const isPreviewing = previewId === record.id;
                  return (
                    <tr
                      key={record.id}
                      aria-selected={isPreviewing}
                      className={`${isPreviewing ? "is-selected" : ""} ${failed ? "" : "is-clickable"}`.trim()}
                      tabIndex={failed ? undefined : 0}
                      onClick={failed ? undefined : () => handlePreview(record)}
                      onKeyDown={
                        failed
                          ? undefined
                          : (event) => {
                              if (event.key === "Enter" || event.key === " ") {
                                event.preventDefault();
                                handlePreview(record);
                              }
                            }
                      }
                    >
                      <td className="col-icon">
                        <Icon
                          size={14}
                          strokeWidth={1.5}
                          className={
                            failed
                              ? "text-[var(--warning)]"
                              : "text-[var(--accent)]"
                          }
                        />
                      </td>
                      <td className="col-file" title={record.outputPath}>
                        <div className="min-w-0">
                          <div className="primary truncate">{record.title}</div>
                          <div className="secondary font-mono truncate">{pathBasename(record.outputPath)}</div>
                        </div>
                      </td>
                      <td>
                        <span className="badge">
                          {t(`exports.type.${record.exportType}`)}
                        </span>
                      </td>
                      <td className="col-path">{record.sourcePath ?? "—"}</td>
                      <td className="col-path">{formatTimestamp(record.createdAt)}</td>
                      <td>
                        <span className="badge badge--accent">
                          {t(`exports.route.${record.route}`)}
                        </span>
                      </td>
                      <td>
                        {failed ? (
                          <span className="badge badge--danger">
                            <span className="dot"></span>
                            {t("exports.status.failed")}
                          </span>
                        ) : (
                          <span className="badge badge--success">
                            <span className="dot"></span>
                            {t("exports.status.succeeded")}
                          </span>
                        )}
                      </td>
                      <td className="col-export-actions">
                        {!failed ? (
                          <div className="export-row-actions">
                            <IconButton
                              label={t(record.bookmarked ? "exports.actions.unbookmark" : "exports.actions.bookmark")}
                              onClick={() => handleToggleBookmark(record)}
                              active={record.bookmarked}
                            >
                              <Star
                                size={14}
                                strokeWidth={1.5}
                                fill={record.bookmarked ? "currentColor" : "none"}
                              />
                            </IconButton>
                            <IconButton
                              label={t("exports.actions.preview")}
                              onClick={() => handlePreview(record)}
                              active={isPreviewing}
                            >
                              <Eye size={14} strokeWidth={1.5} />
                            </IconButton>
                            <IconButton
                              label={t("exports.actions.openInBrowser", { defaultValue: "Open in browser" })}
                              onClick={() => handleOpenInBrowser(record)}
                            >
                              <ExternalLink size={14} strokeWidth={1.5} />
                            </IconButton>
                            <IconButton
                              label={t("exports.actions.openFolder")}
                              onClick={() => handleOpenFolder(record)}
                            >
                              <FolderOpen size={14} strokeWidth={1.5} />
                            </IconButton>
                          </div>
                        ) : (
                          <div className="flex items-center justify-end gap-1">
                            <IconButton
                              label={t("exports.actions.viewLogs")}
                              onClick={() => handleViewLogs(record)}
                            >
                              <List size={14} strokeWidth={1.5} />
                            </IconButton>
                            <button
                              type="button"
                              className="btn btn--sm"
                              onClick={() => handleRegenerate(record)}
                            >
                              {t("exports.actions.retry")}
                            </button>
                          </div>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
          {loading ? (
            <div className="px-4 py-2 text-[11px] text-[var(--text-muted)]">
              {t("exports.list.loading")}
            </div>
          ) : null}
        </div>
      </div>

      <ResizableSplitter
        paneId="exportsList"
        label={t("shell.splitter.exportsList")}
        min={PANE_WIDTH_LIMITS.exportsList.min}
        max={PANE_WIDTH_LIMITS.exportsList.max}
        value={exportsListWidth}
        previewTargetRef={layoutRef}
        previewCssVariable="--exports-list-w-current"
        onCommit={(value) => setPaneSize("exportsList", value)}
        onReset={() => resetPaneSize("exportsList")}
      />

      <aside className="exports-view__preview-pane">
        <div
          className="view-toolbar border-b border-[var(--border)] px-4"
          role="toolbar"
          aria-label={t("exports.preview.tools", { defaultValue: "Preview tools" })}
        >
          <span className="text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("exports.preview.title")}
          </span>
          {previewHtml ? (
            <div className="ml-auto flex items-center gap-1.5">
              <div className="segmented-control" role="group" aria-label={t("exports.preview.mode", { defaultValue: "Preview mode" })}>
                <button
                  type="button"
                  aria-pressed={previewMode === "inline"}
                  onClick={() => setPreviewMode("inline")}
                >
                  <Eye size={13} strokeWidth={1.5} aria-hidden />
                  {t("exports.preview.mode.inline", { defaultValue: "Preview" })}
                </button>
                <button
                  type="button"
                  aria-pressed={previewMode === "source"}
                  onClick={() => setPreviewMode("source")}
                >
                  <Code2 size={13} strokeWidth={1.5} aria-hidden />
                  {t("exports.preview.mode.source", { defaultValue: "HTML source" })}
                </button>
              </div>
              {previewRecord ? (
                <IconButton
                  label={t("exports.actions.openInBrowser", { defaultValue: "Open in browser" })}
                  onClick={() => handleOpenInBrowser(previewRecord)}
                >
                  <ExternalLink size={14} strokeWidth={1.5} />
                </IconButton>
              ) : null}
              <IconButton
                label={
                  workspaceFocus === "exportPreview"
                    ? t("exports.preview.exitFocus", { defaultValue: "Exit focus" })
                    : t("exports.preview.focus", { defaultValue: "Focus preview" })
                }
                onClick={handleToggleFocus}
                active={workspaceFocus === "exportPreview"}
              >
                {workspaceFocus === "exportPreview" ? (
                  <Minimize2 size={14} strokeWidth={1.5} />
                ) : (
                  <Maximize2 size={14} strokeWidth={1.5} />
                )}
              </IconButton>
              <button
                type="button"
                onClick={handleClearPreview}
                className="text-[11px] text-[var(--text-muted)] hover:text-[var(--text-primary)]"
              >
                {t("exports.actions.clearPreview")}
              </button>
            </div>
          ) : null}
        </div>
        <div className="min-h-0 flex-1 overflow-hidden">
          <HtmlPreviewPane html={previewHtml} mode={previewMode} />
        </div>
      </aside>

    </div>
  );
}

interface IconButtonProps {
  label: string;
  onClick: () => void;
  active?: boolean;
  children: React.ReactNode;
}

function IconButton({ label, onClick, active, children }: IconButtonProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.stopPropagation();
        }
      }}
      className={`flex h-[26px] w-[26px] items-center justify-center rounded-[var(--radius-md)] text-[var(--text-muted)] hover:bg-[var(--surface-muted)] hover:text-[var(--text-primary)] ${
        active ? "bg-[var(--surface-muted)] text-[var(--text-primary)]" : ""
      }`}
    >
      {children}
    </button>
  );
}

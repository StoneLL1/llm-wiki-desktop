import { type CSSProperties, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  Eye,
  FileOutput,
  FolderOpen,
  List,
  Plus,
  type LucideIcon,
} from "lucide-react";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import { useExportStore } from "../../stores/exportStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { isTerminalStatus } from "../../types/task";
import { type ExportRecord, type ExportType } from "../../types/export";
import { ExportDialog, type ExportDialogResult } from "./ExportDialog";
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
  const paneSizes = useNavigationStore((state) => state.paneSizes);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);

  const records = useExportStore((state) => state.records);
  const loading = useExportStore((state) => state.loading);
  const runningTaskId = useExportStore((state) => state.runningTaskId);
  const previewHtml = useExportStore((state) => state.previewHtml);
  const previewId = useExportStore((state) => state.previewId);
  const error = useExportStore((state) => state.error);

  const loadExports = useExportStore((state) => state.loadExports);
  const startExport = useExportStore((state) => state.startExport);
  const regenerateExport = useExportStore((state) => state.regenerateExport);
  const clearRunningTask = useExportStore((state) => state.clearRunningTask);
  const loadPreview = useExportStore((state) => state.loadPreview);
  const clearPreview = useExportStore((state) => state.clearPreview);
  const openFolder = useExportStore((state) => state.openFolder);

  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const { projectId, rootPath } = currentProject;
  const layoutStyle = {
    "--exports-list-w-current": `${paneSizes.exportsList}px`,
  } as CSSProperties;
  const [dialogOpen, setDialogOpen] = useState(false);
  const [pendingPreviewTaskId, setPendingPreviewTaskId] = useState<string | null>(null);
  // Guards the terminal handler against re-running for the same task if the
  // task event stream emits two rapid updates before the running-task id clears.
  const processedTerminalRef = useRef<string | null>(null);

  // Load the export history when the view mounts or the project changes.
  useEffect(() => {
    void loadExports(projectId, rootPath);
  }, [projectId, rootPath, loadExports]);

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
    const wantPreview = pendingPreviewTaskId === finishedId;
    void loadExports(projectId, rootPath).then(() => {
      // Only auto-preview the exact record this task produced — never fall back
      // to the newest row, which could belong to a different concurrent export.
      if (wantPreview && succeeded) {
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
    if (wantPreview) setPendingPreviewTaskId(null);
  }, [
    runningTask,
    projectId,
    rootPath,
    loadExports,
    clearRunningTask,
    pendingPreviewTaskId,
    loadPreview,
  ]);

  const handleDialogGenerate = (result: ExportDialogResult) => {
    setDialogOpen(false);
    void startExport(projectId, rootPath, result.type, result.sourcePath, {
      route: result.route,
      template: result.template,
      options: result.options,
    }).then((taskId) => {
      if (!taskId) return;
      if (result.openPreview) setPendingPreviewTaskId(taskId);
      void invoke("list_tasks", { request: { statusFilter: null } }).then((list) => {
        const found = (list as { id: string }[]).find((task) => task.id === taskId);
        if (found) {
          void invoke("get_task", { request: { taskId } }).then((task) => {
            if (task) upsertTask(task as never);
          });
        }
      });
      openTaskDrawer(taskId);
    });
  };

  const handleCancel = () => {
    if (!runningTaskId) return;
    void invoke("cancel_task", { request: { taskId: runningTaskId } }).then((task) => {
      if (task) upsertTask(task as never);
    });
  };

  const handlePreview = (record: ExportRecord) => {
    void loadPreview(
      { projectId, projectRootPath: rootPath, outputPath: record.outputPath },
      record.id,
    );
  };

  const handleRegenerate = (record: ExportRecord) => {
    void regenerateExport(projectId, rootPath, record).then((taskId) => {
      if (!taskId) return;
      void invoke("get_task", { request: { taskId } }).then((task) => {
        if (task) upsertTask(task as never);
      });
      openTaskDrawer(taskId);
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

  const sortedRecords = useMemo(
    () =>
      [...records].sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
    [records],
  );

  return (
    <div className="exports-view-layout" style={layoutStyle}>
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
                onClick={() => setDialogOpen(true)}
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
                  <th>{t("exports.table.file")}</th>
                  <th>{t("exports.table.type")}</th>
                  <th>{t("exports.table.source")}</th>
                  <th>{t("exports.table.time")}</th>
                  <th>{t("exports.table.route")}</th>
                  <th>{t("exports.table.status")}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {sortedRecords.map((record) => {
                  const Icon = TYPE_ICON[record.exportType];
                  const failed = record.status === "failed";
                  const isPreviewing = previewId === record.id;
                  return (
                    <tr key={record.id} className={isPreviewing ? "is-selected" : ""}>
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
                      <td>
                        <div className="primary truncate">{record.title}</div>
                        <div className="secondary font-mono">{record.outputPath}</div>
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
                      <td>
                        {failed ? (
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
                        ) : (
                          <div className="flex items-center justify-end gap-1">
                            <IconButton
                              label={t("exports.actions.preview")}
                              onClick={() => handlePreview(record)}
                              active={isPreviewing}
                            >
                              <Eye size={14} strokeWidth={1.5} />
                            </IconButton>
                            <IconButton
                              label={t("exports.actions.openFolder")}
                              onClick={() => handleOpenFolder(record)}
                            >
                              <FolderOpen size={14} strokeWidth={1.5} />
                            </IconButton>
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
        value={paneSizes.exportsList}
        onChange={(value) => setPaneSize("exportsList", value)}
        onReset={() => resetPaneSize("exportsList")}
      />

      <aside className="exports-view__preview-pane">
          <div className="view-toolbar border-b border-[var(--border)] px-4">
          <span className="text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("exports.preview.title")}
          </span>
          {previewHtml ? (
            <button
              type="button"
              onClick={clearPreview}
              className="ml-auto text-[11px] text-[var(--text-muted)] hover:text-[var(--text-primary)]"
            >
              {t("exports.actions.clearPreview")}
            </button>
          ) : null}
        </div>
        <div className="min-h-0 flex-1 overflow-hidden">
          <HtmlPreviewPane html={previewHtml} />
        </div>
      </aside>

      <ExportDialog
        open={dialogOpen}
        projectId={projectId}
        rootPath={rootPath}
        onClose={() => setDialogOpen(false)}
        onGenerate={handleDialogGenerate}
      />
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
      onClick={onClick}
      className={`flex h-[26px] w-[26px] items-center justify-center rounded-[var(--radius-md)] text-[var(--text-muted)] hover:bg-[var(--surface-muted)] hover:text-[var(--text-primary)] ${
        active ? "bg-[var(--surface-muted)] text-[var(--text-primary)]" : ""
      }`}
    >
      {children}
    </button>
  );
}

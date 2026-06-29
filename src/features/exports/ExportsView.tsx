import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  FileOutput,
  FolderOpen,
  RefreshCw,
  Eye,
  Plus,
  type LucideIcon,
} from "lucide-react";

import { useExportStore } from "../../stores/exportStore";
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
  const [dialogOpen, setDialogOpen] = useState(false);
  const [pendingPreviewTaskId, setPendingPreviewTaskId] = useState<string | null>(null);

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
    const succeeded = runningTask.status === "succeeded";
    const wantPreview = pendingPreviewTaskId === finishedId;
    void loadExports(projectId, rootPath).then(() => {
      if (wantPreview && succeeded) {
        const fresh = useExportStore.getState().records;
        const target = fresh.find((record) => record.taskId === finishedId) ?? fresh[0];
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
    <div className="exports-view-layout">
      <div className="flex min-w-0 flex-col border-r border-[var(--border)]">
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
            <ul className="divide-y divide-[var(--border-subtle)]">
              {sortedRecords.map((record) => {
                const Icon = TYPE_ICON[record.exportType];
                const isPreviewing = previewId === record.id;
                return (
                  <li
                    key={record.id}
                    className={`flex items-start gap-3 px-4 py-2.5 ${
                      isPreviewing ? "bg-[var(--surface-muted)]" : ""
                    }`}
                  >
                    <Icon
                      size={14}
                      strokeWidth={1.5}
                      className="mt-1 shrink-0 text-[var(--text-muted)]"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-[13px] font-medium text-[var(--text-primary)]">
                          {record.title}
                        </span>
                        <span className="rounded-full border border-[var(--border-subtle)] px-1.5 py-px text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                          {t(`exports.type.${record.exportType}`)}
                        </span>
                        <span className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                          {record.route}
                        </span>
                      </div>
                      <div className="mt-0.5 truncate font-mono text-[11px] text-[var(--text-muted)]">
                        {record.outputPath}
                      </div>
                      <div className="text-[11px] text-[var(--text-muted)]">
                        {formatTimestamp(record.createdAt)}
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                      <IconButton
                        label={t("exports.actions.preview")}
                        onClick={() => handlePreview(record)}
                        active={isPreviewing}
                      >
                        <Eye size={14} strokeWidth={1.5} />
                      </IconButton>
                      <IconButton
                        label={t("exports.actions.regenerate")}
                        onClick={() => handleRegenerate(record)}
                      >
                        <RefreshCw size={14} strokeWidth={1.5} />
                      </IconButton>
                      <IconButton
                        label={t("exports.actions.openFolder")}
                        onClick={() => handleOpenFolder(record)}
                      >
                        <FolderOpen size={14} strokeWidth={1.5} />
                      </IconButton>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          {loading ? (
            <div className="px-4 py-2 text-[11px] text-[var(--text-muted)]">
              {t("exports.list.loading")}
            </div>
          ) : null}
        </div>
      </div>

      <aside className="flex min-h-0 flex-col bg-[var(--surface-raised)]">
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

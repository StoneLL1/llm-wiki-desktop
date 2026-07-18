import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  X,
  Square,
  LoaderCircle,
  CircleCheck,
  CircleAlert,
  CircleX,
  Clock,
  HelpCircle,
  Copy,
  Trash2,
  Maximize2,
  ChevronDown,
} from "lucide-react";
import { useTaskStore } from "../../stores/taskStore";
import { fetchTaskActivities, fetchTaskLogs, cancelTaskRequest } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { BackendTask, LogLine, TaskStatus } from "../../types/task";
import { isTerminalStatus } from "../../types/task";
import { AgentActivityTimeline } from "../agent/AgentActivityTimeline";
import { IMPORT_PROGRESS_LABEL_KEYS, isMeasuredImportProgress } from "../../features/import/importStatusPresentation";
import {
  DEFAULT_TASK_SORT_MODE,
  readTaskSortModePreference,
  sortTasks,
  type TaskSortMode,
  writeTaskSortModePreference,
} from "./taskSort";

const LEVEL_BADGE: Record<string, string> = {
  info: "INFO",
  warn: "WARN",
  error: "ERR",
  debug: "DBG",
};

const LEVEL_BADGE_CLASS: Record<string, string> = {
  info: "lvl-info",
  warn: "lvl-warn",
  error: "lvl-err",
  debug: "lvl-info",
};

function StatusIcon({ status }: { status: TaskStatus }) {
  const { t } = useTranslation();
  const cls = "h-3.5 w-3.5 shrink-0";
  const iconProps = { "aria-hidden": true } as const;
  let icon: ReactNode;
  switch (status) {
    case "running":
    case "cancelling":
      icon = <LoaderCircle {...iconProps} className={`${cls} animate-spin text-[var(--accent)]`} />;
      break;
    case "succeeded":
      icon = <CircleCheck {...iconProps} className={`${cls} text-[var(--accent)]`} />;
      break;
    case "failed":
      icon = <CircleAlert {...iconProps} className={`${cls} text-[var(--danger)]`} />;
      break;
    case "cancelled":
      icon = <CircleX {...iconProps} className={`${cls} text-[var(--text-muted)]`} />;
      break;
    case "queued":
      icon = <Clock {...iconProps} className={`${cls} text-[var(--text-muted)]`} />;
      break;
    case "waiting_for_confirmation":
      icon = <HelpCircle {...iconProps} className={`${cls} text-[var(--warning)]`} />;
      break;
  }
  return (
    <span className="inline-flex shrink-0" role="img" aria-label={t(`task.status.${status}`)}>
      {icon}
    </span>
  );
}

function LogLineView({ line }: { line: LogLine }) {
  const badge = LEVEL_BADGE[line.level] ?? "INFO";
  const badgeClass = LEVEL_BADGE_CLASS[line.level] ?? "lvl-info";
  return (
    <div className="terminal__line flex items-start gap-2 py-0.5 font-mono text-[11px] leading-[1.45]">
      <span className="ts mt-px shrink-0 text-[10px] opacity-60">{line.timestamp.slice(11, 19)}</span>
      <span className={`${badgeClass} mt-px shrink-0 text-[10px] font-semibold`}>[{badge}]</span>
      <span className="break-all">{line.message}</span>
    </div>
  );
}

function ProgressBar({ task }: { task: BackendTask }) {
  const progress = task.progress;
  if (!progress && task.status !== "running" && task.status !== "cancelling") return null;

  const current = progress?.current ?? 0;
  const total = progress?.total;
  // Import tasks currently report pipeline stages (0/4…4/4), not a measured
  // fraction of source work. Showing that as a percentage makes long fetches
  // look stalled or falsely precise, so keep those tasks indeterminate.
  const canMeasure = task.taskType !== "import" || isMeasuredImportProgress(progress);
  const pct = canMeasure && total && total > 0 ? Math.round((current / total) * 100) : null;

  return (
    <div
      className="flex items-center gap-2 mt-1"
      role="progressbar"
      {...(pct !== null ? { "aria-valuenow": pct, "aria-valuemin": 0, "aria-valuemax": 100 } : {})}
      aria-label={task.title}
    >
      <div className="h-1 flex-1 rounded-full bg-[var(--surface-muted)] overflow-hidden">
        <div
          className={`h-full rounded-full bg-[var(--accent)] ${pct === null ? "w-full animate-pulse opacity-60" : "transition-all duration-300"}`}
          style={pct === null ? undefined : { width: `${pct}%` }}
        />
      </div>
      {pct !== null && (
        <span className="text-[10px] font-mono text-[var(--text-muted)] shrink-0">{pct}%</span>
      )}
    </div>
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Unknown error";
}

function computeDurationLabel(startedAt: string, completedAt: string | null): string | null {
  const start = Date.parse(startedAt);
  if (!start || Number.isNaN(start)) return null;
  const end = completedAt ? Date.parse(completedAt) : Date.now();
  if (!end || Number.isNaN(end)) return null;
  const seconds = Math.max(0, Math.round((end - start) / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rem = seconds % 60;
  return rem === 0 ? `${minutes}m` : `${minutes}m ${rem}s`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

type ImportBatchLog = { taskTitle: string; line: LogLine };

interface ImportBatchView {
  id: string;
  title: string;
  tasks: BackendTask[];
  processed: number;
  active: number;
  failed: number;
  cancelled: number;
  waitingForConfirmation: number;
  status: TaskStatus;
  logs: ImportBatchLog[];
}

export function TaskLogDrawer() {
  const { t } = useTranslation();
  const tasks = useTaskStore((s) => s.tasks);
  const logs = useTaskStore((s) => s.logs);
  const activities = useTaskStore((s) => s.activities);
  const taskOutputs = useTaskStore((s) => s.taskOutputs);
  const drawerOpen = useTaskStore((s) => s.drawerOpen);
  const selectedTaskId = useTaskStore((s) => s.selectedTaskId);
  const closeDrawer = useTaskStore((s) => s.closeDrawer);
  const selectTask = useTaskStore((s) => s.selectTask);
  const setLogs = useTaskStore((s) => s.setLogs);
  const pushToast = useToastStore((s) => s.pushToast);
  const translationRef = useRef(t);
  translationRef.current = t;
  const [expanded, setExpanded] = useState(false);
  const [sortMode, setSortMode] = useState<TaskSortMode>(() => {
    if (typeof window === "undefined") return DEFAULT_TASK_SORT_MODE;
    return readTaskSortModePreference();
  });
  const [cancellingTaskIds, setCancellingTaskIds] = useState<ReadonlySet<string>>(new Set());
  const cancellingTaskIdsRef = useRef(new Set<string>());
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const drawerRef = useRef<HTMLDivElement | null>(null);
  const logScrollRef = useRef<HTMLDivElement | null>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const wasDrawerOpenRef = useRef(false);
  const [logsPinned, setLogsPinned] = useState(true);

  const sorted = useMemo(() => sortTasks(tasks, sortMode), [tasks, sortMode]);
  // Batched imports already expose their child tasks in the batch section;
  // repeating them in the main list makes child actions ambiguous and harms
  // keyboard navigation.
  const visibleSorted = useMemo(() => sorted.filter((task) => !task.batchId), [sorted]);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;
  const selectedLogs = selectedTaskId ? logs[selectedTaskId] ?? [] : [];
  const selectedActivities = selectedTaskId ? activities[selectedTaskId] ?? [] : [];
  const selectedOutput = selectedTaskId ? taskOutputs[selectedTaskId] ?? "" : "";
  const selectedProgressLabel = selectedTask?.progress?.label
    ? selectedTask.taskType === "import"
      ? t(IMPORT_PROGRESS_LABEL_KEYS[selectedTask.progress.label] ?? "importV2.progress.working")
      : selectedTask.progress.label
    : null;
  const importSummary = useMemo(() => {
    const importTasks = tasks.filter((task) => task.taskType === "import");
    if (importTasks.length < 2) return null;

    return {
      total: importTasks.length,
      processed: importTasks.filter((task) => isTerminalStatus(task.status) || task.status === "waiting_for_confirmation").length,
      active: importTasks.filter((task) => !isTerminalStatus(task.status) && task.status !== "waiting_for_confirmation").length,
      failed: importTasks.filter((task) => task.status === "failed").length,
      cancelled: importTasks.filter((task) => task.status === "cancelled").length,
      waitingForConfirmation: importTasks.filter((task) => task.status === "waiting_for_confirmation").length,
    };
  }, [tasks]);
  const importBatches = useMemo<ImportBatchView[]>(() => {
    const grouped = new Map<string, BackendTask[]>();
    tasks
      .filter((task) => task.taskType === "import" && task.batchId)
      .forEach((task) => {
        const batchId = task.batchId!;
        grouped.set(batchId, [...(grouped.get(batchId) ?? []), task]);
      });

    return [...grouped.entries()]
      .sort(([, first], [, second]) => (second[0]?.startedAt ?? "").localeCompare(first[0]?.startedAt ?? ""))
      .map(([id, batchTasks]) => {
        const waitingForConfirmation = batchTasks.filter((task) => task.status === "waiting_for_confirmation").length;
        const processed = batchTasks.filter((task) => isTerminalStatus(task.status) || task.status === "waiting_for_confirmation").length;
        const active = batchTasks.length - processed;
        const failed = batchTasks.filter((task) => task.status === "failed").length;
        const cancelled = batchTasks.filter((task) => task.status === "cancelled").length;
        const batchLogs = batchTasks
          .flatMap((task) => (logs[task.id] ?? []).map((line) => ({ taskTitle: task.title, line })))
          .sort((first, second) => first.line.timestamp.localeCompare(second.line.timestamp))
          .slice(-24);
        const status: TaskStatus = active > 0
          ? "running"
          : failed > 0
            ? "failed"
            : cancelled === batchTasks.length
              ? "cancelled"
              : waitingForConfirmation > 0
                ? "waiting_for_confirmation"
              : "succeeded";
        return { id, title: batchTasks[0]?.title ?? id.slice(0, 8), tasks: batchTasks, processed, active, failed, cancelled, waitingForConfirmation, status, logs: batchLogs };
      });
  }, [logs, tasks]);

  const selectSortMode = (mode: TaskSortMode) => {
    setSortMode(mode);
    writeTaskSortModePreference(mode);
  };

  const loadLogs = useCallback((taskId: string) => {
    void fetchTaskLogs(taskId).catch((error) => {
      pushToast("error", translationRef.current("task.logsError", { message: errorMessage(error) }));
    });
  }, [pushToast]);

  const loadActivities = useCallback((taskId: string) => {
    void fetchTaskActivities(taskId).catch((error) => {
      pushToast("error", translationRef.current("task.activitiesError", { message: errorMessage(error) }));
    });
  }, [pushToast]);

  useEffect(() => {
    const terminalIds = tasks.filter((task) => isTerminalStatus(task.status)).map((task) => task.id);
    if (terminalIds.length === 0) return;
    const next = new Set(cancellingTaskIdsRef.current);
    terminalIds.forEach((taskId) => next.delete(taskId));
    if (next.size === cancellingTaskIdsRef.current.size) return;
    cancellingTaskIdsRef.current = next;
    setCancellingTaskIds(next);
  }, [tasks]);

  useEffect(() => {
    if (selectedTaskId && !isTerminalStatus(selectedTask?.status ?? "queued")) {
      const interval = setInterval(() => {
        loadLogs(selectedTaskId);
        loadActivities(selectedTaskId);
      }, 2000);
      return () => clearInterval(interval);
    }
  }, [loadActivities, loadLogs, selectedTaskId, selectedTask?.status]);

  useEffect(() => {
    if (selectedTaskId) {
      loadLogs(selectedTaskId);
      loadActivities(selectedTaskId);
    }
  }, [loadActivities, loadLogs, selectedTaskId]);

  useEffect(() => {
    if (drawerOpen && !wasDrawerOpenRef.current) {
      returnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      requestAnimationFrame(() => closeButtonRef.current?.focus());
    } else if (!drawerOpen && wasDrawerOpenRef.current) {
      const opener = returnFocusRef.current;
      if (opener?.isConnected) opener.focus();
      returnFocusRef.current = null;
    }
    wasDrawerOpenRef.current = drawerOpen;
  }, [drawerOpen]);

  useEffect(() => {
    if (!drawerOpen) return;
    const handleDrawerKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeDrawer();
        return;
      }
      if (event.key !== "Tab") return;
      const drawer = drawerRef.current;
      if (!drawer) return;
      const focusable = Array.from(
        drawer.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleDrawerKeyDown);
    return () => document.removeEventListener("keydown", handleDrawerKeyDown);
  }, [closeDrawer, drawerOpen]);

  const handleCancel = async (taskId: string) => {
    if (cancellingTaskIdsRef.current.has(taskId)) return;
    cancellingTaskIdsRef.current.add(taskId);
    setCancellingTaskIds(new Set(cancellingTaskIdsRef.current));
    try {
      await cancelTaskRequest(taskId);
    } catch (error) {
      pushToast("error", t("task.cancelError", { message: errorMessage(error) }));
    } finally {
      const task = useTaskStore.getState().tasks.find((candidate) => candidate.id === taskId);
      if (task?.status !== "cancelling") {
        cancellingTaskIdsRef.current.delete(taskId);
        setCancellingTaskIds(new Set(cancellingTaskIdsRef.current));
      }
    }
  };

  const handleCopyLogs = async () => {
    if (!selectedTaskId || selectedLogs.length === 0) return;
    const text = selectedLogs
      .map((line) => `${line.timestamp.slice(11, 19)} [${LEVEL_BADGE[line.level] ?? "INFO"}] ${line.message}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      pushToast("info", t("task.logsCopied"));
    } catch {
      pushToast("error", t("task.logsCopyError"));
    }
  };

  const handleClearLogs = () => {
    if (!selectedTaskId) return;
    setLogs(selectedTaskId, []);
  };

  const footerDuration = selectedTask
    ? computeDurationLabel(selectedTask.startedAt, selectedTask.completedAt)
    : null;
  const footerBytes = new Blob(
    selectedLogs.map((line) => `${line.message}\n`),
  ).size;

  const handleLogScroll = () => {
    const element = logScrollRef.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    setLogsPinned(distance < 72);
  };

  useEffect(() => {
    setLogsPinned(true);
    const element = logScrollRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [selectedTaskId]);

  useEffect(() => {
    if (!logsPinned) return;
    const element = logScrollRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [logsPinned, selectedLogs.length, selectedActivities.length, selectedOutput.length]);

  if (!drawerOpen) return null;

  return (
      <div
        ref={drawerRef}
        className={`task-drawer ${expanded ? "is-expanded" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-label={t("task.drawer.title")}
      >
      {/* Header */}
      <div className="flex items-center justify-between px-4 h-[44px] border-b border-[var(--border)] shrink-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-[12px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("task.drawer.title")}
          </span>
          <div className="seg task-drawer__sort" role="group" aria-label={t("task.sort.label")}>
            {(["execution_time", "updated_time", "status"] as TaskSortMode[]).map((mode) => (
              <button
                key={mode}
                type="button"
                aria-pressed={sortMode === mode}
                className={sortMode === mode ? "is-active" : ""}
                onClick={() => selectSortMode(mode)}
              >
                {t(`task.sort.${mode}`)}
              </button>
            ))}
          </div>
        </div>
        <button
          ref={closeButtonRef}
          onClick={closeDrawer}
          className="icon-button"
          aria-label={t("task.drawer.close")}
          title={t("task.drawer.close")}
          type="button"
        >
          <X size={14} />
        </button>
      </div>

      {importSummary && (
        <div className="border-b border-[var(--border-subtle)] px-4 py-2 text-[11px] text-[var(--text-muted)]" role="status" aria-live="polite">
          <div className="flex items-center justify-between gap-3">
            <span className="font-medium text-[var(--text-secondary)]">
              {t("task.importSummary.title")}
            </span>
            <span className="font-mono">
              {t("task.importSummary.progress", {
                processed: importSummary.processed,
                total: importSummary.total,
              })}
            </span>
          </div>
          <div className="mt-1 truncate">
            {t("task.importSummary.summary", {
              active: importSummary.active,
              waitingForConfirmation: importSummary.waitingForConfirmation,
              failed: importSummary.failed,
              cancelled: importSummary.cancelled,
            })}
          </div>
        </div>
      )}

      {importBatches.length > 0 && (
        <div className="border-b border-[var(--border-subtle)] px-4 py-2" role="region" aria-label={t("task.importBatches.title")}>
          <div className="mb-1 text-[11px] font-medium text-[var(--text-secondary)]">{t("task.importBatches.title")}</div>
          <div className="space-y-1">
            {importBatches.map((batch, index) => (
              <details key={batch.id} className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)]">
                <summary className="flex cursor-pointer list-none items-center gap-2 px-2 py-1.5 text-[11px] text-[var(--text-secondary)]" onClick={() => batch.tasks.forEach((task) => loadLogs(task.id))}>
                  <StatusIcon status={batch.status} />
                  <span className="min-w-0 flex-1 truncate">{t("task.importBatches.batch", { index: index + 1, title: batch.title })}</span>
                  <span className="shrink-0 font-mono text-[10px] text-[var(--text-muted)]">{t("task.importBatches.progress", { processed: batch.processed, total: batch.tasks.length })}</span>
                </summary>
                <div className="border-t border-[var(--border-subtle)] px-2 py-2">
                  <div className="text-[10.5px] text-[var(--text-muted)]">
                    {t("task.importBatches.summary", { active: batch.active, waitingForConfirmation: batch.waitingForConfirmation, failed: batch.failed, cancelled: batch.cancelled })}
                  </div>
                  {batch.logs.length > 0 ? (
                    <div className="mt-2 max-h-[132px] overflow-y-auto rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1" aria-label={t("task.importBatches.logs")}>
                      {batch.logs.map(({ taskTitle, line }, logIndex) => (
                        <LogLineView key={`${line.timestamp}-${taskTitle}-${logIndex}`} line={{ ...line, message: `${taskTitle}: ${line.message}` }} />
                      ))}
                    </div>
                  ) : null}
                  <div className="mt-2 flex flex-wrap gap-1">
                    {batch.tasks.map((task) => (
                      <button key={task.id} type="button" className="btn btn--sm" onClick={() => selectTask(task.id)}>
                        <StatusIcon status={task.status} />
                        <span className="ml-1 max-w-[180px] truncate">{task.title}</span>
                      </button>
                    ))}
                  </div>
                </div>
              </details>
            ))}
          </div>
        </div>
      )}

      <div className="task-drawer__body flex flex-1 min-h-0">
        {/* Task list */}
        <div className="task-drawer__list w-[180px] shrink-0 border-r border-[var(--border)] overflow-y-auto">
          {visibleSorted.length === 0 ? (
            <div className="p-3 text-[12px] text-[var(--text-muted)]">
              {t("task.drawer.empty")}
            </div>
          ) : (
            visibleSorted.map((task) => (
              <div
                key={task.id}
                className={`w-full flex items-center text-[13px] hover:bg-[var(--surface-muted)] transition-colors border-l-2 ${
                  selectedTaskId === task.id
                    ? "border-l-[var(--accent)] bg-[var(--accent-soft)]"
                    : "border-l-transparent"
                }`}
              >
                <button
                  onClick={() => selectTask(task.id)}
                  className="min-w-0 flex flex-1 items-center gap-2 px-3 py-2 text-left"
                  aria-current={selectedTaskId === task.id ? "true" : undefined}
                  type="button"
                >
                  <StatusIcon status={task.status} />
                  <span className="truncate flex-1">{task.title}</span>
                </button>
                {((!isTerminalStatus(task.status) && task.status !== "waiting_for_confirmation" && task.cancellable) || cancellingTaskIds.has(task.id)) && (
                  <button
                    onClick={() => handleCancel(task.id)}
                    disabled={cancellingTaskIds.has(task.id) && !isTerminalStatus(task.status)}
                    aria-busy={cancellingTaskIds.has(task.id)}
                    className="icon-button mr-2 shrink-0"
                    aria-label={t("task.action.cancel")}
                    title={t("task.action.cancel")}
                    type="button"
                  >
                    <Square size={10} />
                  </button>
                )}
              </div>
            ))
          )}
        </div>

        {/* Log panel */}
        <div className="flex-1 flex flex-col min-w-0 overflow-y-auto">
          {!selectedTask ? (
            <div className="flex-1 flex items-center justify-center text-[12px] text-[var(--text-muted)] p-4 text-center">
              {t("task.drawer.selectHint")}
            </div>
          ) : (
            <div className="flex flex-col flex-1">
              {/* Task details header */}
              <div className="px-3 py-2 border-b border-[var(--border-subtle)]">
                <div className="flex items-center gap-2">
                  <StatusIcon status={selectedTask.status} />
                  <span className="text-[13px] font-medium truncate">{selectedTask.title}</span>
                </div>
                <div className="flex items-center gap-3 mt-1 text-[11px] text-[var(--text-muted)]">
                  <span>{t(`task.status.${selectedTask.status}`)}</span>
                  {selectedProgressLabel && (
                    <span className="truncate">{selectedProgressLabel}</span>
                  )}
                </div>
                <ProgressBar task={selectedTask} />

                {/* Error details */}
                {selectedTask.error && (
                  <div className="mt-2 p-2 rounded bg-[var(--danger)]/10 border border-[var(--danger)]/20 text-[11px] text-[var(--danger)]">
                    <span className="font-medium">{selectedTask.error.code}</span>:{" "}
                    {selectedTask.error.message}
                  </div>
                )}

                {/* Result */}
                {selectedTask.result && (
                  <div className="mt-2 text-[12px] text-[var(--text-secondary)]">
                    <p>{selectedTask.result.summary}</p>
                    {selectedTask.result.affectedPaths.length > 0 && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {selectedTask.result.affectedPaths.map((p, i) => (
                          <span
                            key={i}
                            className="font-mono text-[10px] bg-[var(--surface-muted)] px-1.5 py-0.5 rounded"
                          >
                            {p}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Cancel button for active tasks */}
                {!isTerminalStatus(selectedTask.status) && selectedTask.status !== "waiting_for_confirmation" && selectedTask.cancellable && (
                  <button
                    onClick={() => handleCancel(selectedTask.id)}
                    disabled={cancellingTaskIds.has(selectedTask.id)}
                    aria-busy={cancellingTaskIds.has(selectedTask.id)}
                    className="mt-2 text-[12px] px-3 py-1 rounded border border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--danger)]/10 transition-colors"
                    type="button"
                  >
                    {t("task.action.cancel")}
                  </button>
                )}
              </div>

              {selectedActivities.length > 0 ? (
                <div className="border-b border-[var(--border-subtle)] px-3 py-2">
                  <div className="mb-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
                    {t("task.activities.title")}
                  </div>
                  <AgentActivityTimeline activities={selectedActivities} taskStatus={selectedTask.status} compact />
                </div>
              ) : null}

              {selectedOutput ? (
                <div className="border-b border-[var(--border-subtle)] bg-[var(--surface-muted)] px-3 py-2" aria-label={t("task.activities.output")}>
                  <div className="mb-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
                    {t("task.activities.output")}
                  </div>
                  <pre className="agent-task-output">{selectedOutput}</pre>
                </div>
              ) : null}

              {/* Log lines */}
              <div className="terminal-wrap relative flex-1 min-h-0 flex flex-col">
                <div className="terminal__overlay">
                  <button
                    type="button"
                    onClick={handleCopyLogs}
                    disabled={selectedLogs.length === 0}
                    aria-label={t("task.logsCopy")}
                    title={t("task.logsCopy")}
                  >
                    <Copy size={12} aria-hidden />
                  </button>
                  <button
                    type="button"
                    onClick={handleClearLogs}
                    disabled={selectedLogs.length === 0}
                    aria-label={t("task.logsClear")}
                    title={t("task.logsClear")}
                  >
                    <Trash2 size={12} aria-hidden />
                  </button>
                  <button
                    type="button"
                    onClick={() => setExpanded((value) => !value)}
                    aria-label={t(expanded ? "task.logsCollapse" : "task.logsExpand")}
                    title={t(expanded ? "task.logsCollapse" : "task.logsExpand")}
                  >
                    <Maximize2 size={12} aria-hidden />
                  </button>
                </div>
                <div
                  ref={logScrollRef}
                  onScroll={handleLogScroll}
                  className="flex-1 overflow-y-auto p-2 bg-[var(--surface)]"
                  role="log"
                  aria-live="polite"
                  aria-label={t("task.logs.title")}
                >
                  {selectedLogs.length === 0 ? (
                    <div className="text-[11px] text-[var(--text-muted)] p-2">
                      {t("task.drawer.noLogs")}
                    </div>
                  ) : (
                    selectedLogs.map((line, i) => <LogLineView key={i} line={line} />)
                  )}
                </div>
                {!logsPinned ? (
                  <button
                    type="button"
                    className="task-log-back-latest"
                    onClick={() => {
                      const element = logScrollRef.current;
                      if (!element) return;
                      element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
                      setLogsPinned(true);
                    }}
                  >
                    <ChevronDown size={14} aria-hidden="true" />
                    {t("task.logsBackToLatest")}
                  </button>
                ) : null}
                <div className="terminal__foot border-t border-[var(--border-subtle)] px-2 py-1.5">
                  <span className="dotstatus dotstatus--busy" aria-hidden />
                  <span>{t(`task.status.${selectedTask.status}`)}</span>
                  {footerDuration ? (
                    <>
                      <span>·</span>
                      <span>{footerDuration}</span>
                    </>
                  ) : null}
                  <span>·</span>
                  <span>{formatBytes(footerBytes)}</span>
                  <span className="terminal__foot-spacer">{t("task.logsBackgroundHint")}</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

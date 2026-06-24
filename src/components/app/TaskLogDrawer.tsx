import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
} from "lucide-react";
import { useTaskStore } from "../../stores/taskStore";
import { fetchTaskLogs, cancelTaskRequest } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { BackendTask, LogLine, TaskStatus } from "../../types/task";
import { isTerminalStatus, TASK_STATUS_ORDER } from "../../types/task";

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
  const cls = "h-3.5 w-3.5 shrink-0";
  switch (status) {
    case "running":
    case "cancelling":
      return <LoaderCircle className={`${cls} animate-spin text-[var(--accent)]`} />;
    case "succeeded":
      return <CircleCheck className={`${cls} text-[var(--accent)]`} />;
    case "failed":
      return <CircleAlert className={`${cls} text-[var(--danger)]`} />;
    case "cancelled":
      return <CircleX className={`${cls} text-[var(--text-muted)]`} />;
    case "queued":
      return <Clock className={`${cls} text-[var(--text-muted)]`} />;
    case "waiting_for_confirmation":
      return <HelpCircle className={`${cls} text-[var(--warning)]`} />;
  }
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
  const pct = total && total > 0 ? Math.round((current / total) * 100) : null;
  const fillPct = pct ?? (task.status === "running" ? 99 : 100);
  const nowAria = pct ?? 0;

  return (
    <div
      className="flex items-center gap-2 mt-1"
      role="progressbar"
      aria-valuenow={nowAria}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={task.title}
    >
      <div className="h-1 flex-1 rounded-full bg-[var(--surface-muted)] overflow-hidden">
        <div
          className="h-full rounded-full bg-[var(--accent)] transition-all duration-300"
          style={{ width: `${fillPct}%` }}
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

export function TaskLogDrawer() {
  const { t } = useTranslation();
  const tasks = useTaskStore((s) => s.tasks);
  const logs = useTaskStore((s) => s.logs);
  const drawerOpen = useTaskStore((s) => s.drawerOpen);
  const selectedTaskId = useTaskStore((s) => s.selectedTaskId);
  const closeDrawer = useTaskStore((s) => s.closeDrawer);
  const selectTask = useTaskStore((s) => s.selectTask);
  const setLogs = useTaskStore((s) => s.setLogs);
  const pushToast = useToastStore((s) => s.pushToast);
  const translationRef = useRef(t);
  translationRef.current = t;
  const [expanded, setExpanded] = useState(false);

  const sorted = useMemo(() => {
    return [...tasks].sort(
      (a, b) => (TASK_STATUS_ORDER[a.status] ?? 99) - (TASK_STATUS_ORDER[b.status] ?? 99)
    );
  }, [tasks]);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;
  const selectedLogs = selectedTaskId ? logs[selectedTaskId] ?? [] : [];

  const loadLogs = useCallback((taskId: string) => {
    void fetchTaskLogs(taskId).catch((error) => {
      pushToast("error", translationRef.current("task.logsError", { message: errorMessage(error) }));
    });
  }, [pushToast]);

  useEffect(() => {
    if (selectedTaskId && !isTerminalStatus(selectedTask?.status ?? "queued")) {
      const interval = setInterval(() => {
        loadLogs(selectedTaskId);
      }, 2000);
      return () => clearInterval(interval);
    }
  }, [loadLogs, selectedTaskId, selectedTask?.status]);

  useEffect(() => {
    if (selectedTaskId) {
      loadLogs(selectedTaskId);
    }
  }, [loadLogs, selectedTaskId]);

  const handleCancel = async (taskId: string) => {
    try {
      await cancelTaskRequest(taskId);
    } catch (error) {
      pushToast("error", t("task.cancelError", { message: errorMessage(error) }));
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

  if (!drawerOpen) return null;

  return (
    <div className={`task-drawer ${expanded ? "is-expanded" : ""}`}>
      {/* Header */}
      <div className="flex items-center justify-between px-4 h-[44px] border-b border-[var(--border)] shrink-0">
        <span className="text-[12px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("task.drawer.title")}
        </span>
        <button
          onClick={closeDrawer}
          className="icon-button"
          aria-label={t("task.drawer.close")}
          title={t("task.drawer.close")}
          type="button"
        >
          <X size={14} />
        </button>
      </div>

      <div className="task-drawer__body flex flex-1 min-h-0">
        {/* Task list */}
        <div className="task-drawer__list w-[180px] shrink-0 border-r border-[var(--border)] overflow-y-auto">
          {sorted.length === 0 ? (
            <div className="p-3 text-[12px] text-[var(--text-muted)]">
              {t("task.drawer.empty")}
            </div>
          ) : (
            sorted.map((task) => (
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
                  type="button"
                >
                  <StatusIcon status={task.status} />
                  <span className="truncate flex-1">{task.title}</span>
                </button>
                {!isTerminalStatus(task.status) && task.cancellable && (
                  <button
                    onClick={() => handleCancel(task.id)}
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
                  {selectedTask.progress?.label && (
                    <span className="truncate">{selectedTask.progress.label}</span>
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
                {!isTerminalStatus(selectedTask.status) && selectedTask.cancellable && (
                  <button
                    onClick={() => handleCancel(selectedTask.id)}
                    className="mt-2 text-[12px] px-3 py-1 rounded border border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--danger)]/10 transition-colors"
                    type="button"
                  >
                    {t("task.action.cancel")}
                  </button>
                )}
              </div>

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
                <div className="flex-1 overflow-y-auto p-2 bg-[var(--surface)]">
                  {selectedLogs.length === 0 ? (
                    <div className="text-[11px] text-[var(--text-muted)] p-2">
                      {t("task.drawer.noLogs")}
                    </div>
                  ) : (
                    selectedLogs.map((line, i) => <LogLineView key={i} line={line} />)
                  )}
                </div>
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

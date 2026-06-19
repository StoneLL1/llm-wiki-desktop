import { useEffect, useMemo } from "react";
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
} from "lucide-react";
import { useTaskStore } from "../../stores/taskStore";
import { fetchTaskLogs, cancelTaskRequest } from "../../stores/taskStore";
import type { BackendTask, LogLine, TaskStatus } from "../../types/task";
import { isTerminalStatus, TASK_STATUS_ORDER } from "../../types/task";

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
  const colorMap: Record<string, string> = {
    info: "var(--text-secondary)",
    warn: "var(--warning)",
    error: "var(--danger)",
    debug: "var(--text-muted)",
  };
  return (
    <div
      className="flex items-start gap-2 py-0.5 font-mono text-[11px] leading-[1.45]"
      style={{ color: colorMap[line.level] || "var(--text-secondary)" }}
    >
      <span className="mt-px shrink-0 text-[10px] opacity-60">{line.timestamp.slice(11, 19)}</span>
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

  return (
    <div className="flex items-center gap-2 mt-1">
      <div className="h-1 flex-1 rounded-full bg-[var(--surface-muted)] overflow-hidden">
        <div
          className="h-full rounded-full bg-[var(--accent)] transition-all duration-300"
          style={{ width: `${pct ?? (task.status === "running" ? 99 : 100)}%` }}
        />
      </div>
      {pct !== null && (
        <span className="text-[10px] font-mono text-[var(--text-muted)] shrink-0">{pct}%</span>
      )}
    </div>
  );
}

export function TaskLogDrawer() {
  const { t } = useTranslation();
  const tasks = useTaskStore((s) => s.tasks);
  const logs = useTaskStore((s) => s.logs);
  const drawerOpen = useTaskStore((s) => s.drawerOpen);
  const selectedTaskId = useTaskStore((s) => s.selectedTaskId);
  const closeDrawer = useTaskStore((s) => s.closeDrawer);
  const selectTask = useTaskStore((s) => s.selectTask);

  const sorted = useMemo(() => {
    return [...tasks].sort(
      (a, b) => (TASK_STATUS_ORDER[a.status] ?? 99) - (TASK_STATUS_ORDER[b.status] ?? 99)
    );
  }, [tasks]);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;
  const selectedLogs = selectedTaskId ? logs[selectedTaskId] ?? [] : [];

  useEffect(() => {
    if (selectedTaskId && !isTerminalStatus(selectedTask?.status ?? "queued")) {
      const interval = setInterval(() => {
        fetchTaskLogs(selectedTaskId);
      }, 2000);
      return () => clearInterval(interval);
    }
  }, [selectedTaskId, selectedTask?.status]);

  useEffect(() => {
    if (selectedTaskId) {
      fetchTaskLogs(selectedTaskId);
    }
  }, [selectedTaskId]);

  const handleCancel = async (taskId: string) => {
    try {
      await cancelTaskRequest(taskId);
    } catch {
      // handled by store
    }
  };

  if (!drawerOpen) return null;

  return (
    <div className="fixed inset-y-0 right-0 z-40 w-[420px] border-l border-[var(--border)] bg-[var(--background)] shadow-lg flex flex-col">
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

      <div className="flex flex-1 min-h-0">
        {/* Task list */}
        <div className="w-[180px] shrink-0 border-r border-[var(--border)] overflow-y-auto">
          {sorted.length === 0 ? (
            <div className="p-3 text-[12px] text-[var(--text-muted)]">
              {t("task.drawer.empty")}
            </div>
          ) : (
            sorted.map((task) => (
              <button
                key={task.id}
                onClick={() => selectTask(task.id)}
                className={`w-full text-left px-3 py-2 flex items-center gap-2 text-[13px] hover:bg-[var(--surface-muted)] transition-colors border-l-2 ${
                  selectedTaskId === task.id
                    ? "border-l-[var(--accent)] bg-[var(--accent-soft)]"
                    : "border-l-transparent"
                }`}
                type="button"
              >
                <StatusIcon status={task.status} />
                <span className="truncate flex-1">{task.title}</span>
                {!isTerminalStatus(task.status) && task.cancellable && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleCancel(task.id);
                    }}
                    className="icon-button shrink-0 ml-1"
                    aria-label={t("task.action.cancel")}
                    title={t("task.action.cancel")}
                    type="button"
                  >
                    <Square size={10} />
                  </button>
                )}
              </button>
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
              <div className="flex-1 overflow-y-auto p-2 bg-[var(--surface)]">
                {selectedLogs.length === 0 ? (
                  <div className="text-[11px] text-[var(--text-muted)] p-2">
                    {t("task.drawer.noLogs")}
                  </div>
                ) : (
                  selectedLogs.map((line, i) => <LogLineView key={i} line={line} />)
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

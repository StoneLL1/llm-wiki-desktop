export type TaskType =
  | "import"
  | "wiki_compile"
  | "agent_run"
  | "llm_request"
  | "graph_build"
  | "deep_lint"
  | "auto_fix"
  | "export";

export type TaskStatus =
  | "queued"
  | "running"
  | "waiting_for_confirmation"
  | "cancelling"
  | "cancelled"
  | "succeeded"
  | "failed";

export type BackendEventType =
  | "task_updated"
  | "task_log"
  | "task_completed"
  | "task_failed"
  | "task_cancelled"
  | "task_stream_output"
  | "confirmation_requested"
  | "project_refreshed"
  | "wiki_changed"
  | "graph_updated"
  | "agent_output";

export type LogLevel = "info" | "warn" | "error" | "debug";

export interface TaskProgress {
  current: number;
  total: number | null;
  label: string | null;
}

export interface TaskResult {
  summary: string;
  affectedPaths: string[];
  pendingAction?: import("./backend").PendingAction;
}

export interface BackendError {
  code: string;
  message: string;
  details: unknown;
  recoverable: boolean;
  userActionRequired: boolean;
}

export interface BackendTask {
  id: string;
  taskType: TaskType;
  projectId: string | null;
  title: string;
  status: TaskStatus;
  progress: TaskProgress | null;
  startedAt: string;
  updatedAt: string;
  completedAt: string | null;
  cancellable: boolean;
  logPath: string | null;
  result: TaskResult | null;
  error: BackendError | null;
}

export interface LogLine {
  timestamp: string;
  level: LogLevel;
  message: string;
}

export interface BackendEvent<T = unknown> {
  eventId: string;
  eventType: BackendEventType;
  projectId: string | null;
  taskId: string | null;
  timestamp: string;
  payload: T;
}

export interface CreateTaskRequest {
  taskType: TaskType;
  projectId: string | null;
  title: string;
  cancellable: boolean;
}

export const TASK_TYPE_LABELS: Record<TaskType, string> = {
  import: "task.type.import",
  wiki_compile: "task.type.wikiCompile",
  agent_run: "task.type.agentRun",
  llm_request: "task.type.llmRequest",
  graph_build: "task.type.graphBuild",
  deep_lint: "task.type.deepLint",
  auto_fix: "task.type.autoFix",
  export: "task.type.export",
};

export const TASK_STATUS_ORDER: Record<TaskStatus, number> = {
  running: 0,
  cancelling: 1,
  queued: 2,
  waiting_for_confirmation: 3,
  succeeded: 4,
  failed: 5,
  cancelled: 6,
};

export const isTerminalStatus = (status: TaskStatus): boolean =>
  status === "succeeded" || status === "failed" || status === "cancelled";

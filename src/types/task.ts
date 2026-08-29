export type TaskType =
  | "import"
  | "wiki_compile"
  | "agent_run"
  | "llm_request"
  | "graph_build"
  | "deep_lint"
  | "auto_fix"
  | "export"
  | "source_ai_organize"
  | "project_inventory"
  | "workflow";

export type TaskStatus =
  | "queued"
  | "running"
  | "waiting_for_confirmation"
  | "cancelling"
  | "cancelled"
  | "succeeded"
  | "failed"
  | "interrupted";

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
  | "agent_output"
  | "task_activity"
  | "workflow_updated"
  | "import_session_patch";

export type LogLevel = "info" | "warn" | "error" | "debug";

export interface TaskProgress {
  current: number;
  total: number | null;
  label: string | null;
}

export type TaskOperation = {
  kind: "import_batch";
  sessionId: string;
  itemCount: number;
  sourceLabel?: string | null;
} | {
  kind: "capability_install";
  sessionId: string;
  itemId: string;
  capabilityId: string;
  requirementRevision: string;
} | {
  kind: "import_commit";
  sessionId: string;
} | {
  kind: "import_recovery";
  sessionId: string;
} | {
  kind: "import_history_index_rebuild";
};

export interface TaskResult {
  summary: string;
  affectedPaths: string[];
  reference?: TaskResultReference;
  pendingAction?: import("./backend").PendingAction;
}

export type TaskResultReference = {
  type: "import_preview";
  sessionId: string;
  itemId: string;
} | {
  type: "import_operation";
  sessionId: string;
  taskId: string;
  itemCount: number;
} | {
  type: "import_v2_session_preview";
  sessionId: string;
  batchId?: string | null;
  completion?: import("./importV2").ImportCompletion | null;
} | {
  type: "compile";
  result: import("./compile").CompileResult;
} | {
  type: "source_ai_organize";
  sourceId: string;
  baseVersionId: string;
  baseMarkdownHash: string;
  candidateId?: string | null;
  route?: "auto" | "agent" | "byok" | null;
  agent?: import("./agent").AgentKind | null;
  provider?: import("./llm").LlmProviderKind | null;
  customInstructions?: string | null;
  projectRootPath?: string | null;
  resolvedEngine?: string | null;
  resolvedModel?: string | null;
};

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
  /** Stable identity shared by tasks created from one import operation. */
  batchId?: string | null;
  /** Typed semantics for a user-visible operation; absent on legacy tasks. */
  operation?: TaskOperation | null;
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

export type TaskProjectPersistenceReason =
  | "no_project"
  | "project_untrusted"
  | "project_read_only"
  | "task_state_root_unavailable";

export interface SetActiveProjectResult {
  tasks: BackendTask[];
  persistence: import("./workflow").WorkflowPersistenceMode;
  persistenceReason?: TaskProjectPersistenceReason;
}

export interface LogLine {
  timestamp: string;
  level: LogLevel;
  message: string;
}

export interface StreamDelta {
  delta: string;
  route?: string | null;
}

export interface BackendEvent<T = unknown> {
  eventId: string;
  eventType: BackendEventType;
  projectId: string | null;
  taskId: string | null;
  timestamp: string;
  payload: T;
}

export type TaskActivityStatus = "started" | "completed" | "failed";

/** Safe structured activity emitted by an Agent/LLM task. It intentionally
 * excludes hidden reasoning, raw tool arguments, file contents, and command
 * output. */
export type TaskActivity =
  | {
      kind: "phase";
      name: string;
      status: TaskActivityStatus;
      label?: string;
    }
  | {
      kind: "thinking";
      status: TaskActivityStatus;
      summary?: string;
      durationMs?: number;
    }
  | {
      kind: "tool_call";
      callId: string;
      name: string;
      detail?: string;
    }
  | {
      kind: "tool_result";
      callId: string;
      success: boolean;
      summary?: string;
    };

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
  source_ai_organize: "task.type.sourceAiOrganize",
  project_inventory: "task.type.projectInventory",
  workflow: "task.type.workflow",
};

export const TASK_STATUS_ORDER: Record<TaskStatus, number> = {
  running: 0,
  cancelling: 1,
  queued: 2,
  waiting_for_confirmation: 3,
  succeeded: 4,
  failed: 5,
  cancelled: 6,
  interrupted: 7,
};

export const isTerminalStatus = (status: TaskStatus): boolean =>
  status === "succeeded" ||
  status === "failed" ||
  status === "cancelled" ||
  status === "interrupted";

const LEGACY_IMPORT_OPERATION_PREFIX = "import-v2-operation:";

/** Recognize current typed operations and persisted pre-cutover tasks. */
export const isImportBatchOperationTask = (task: BackendTask): boolean =>
  task.taskType === "import"
  && (task.operation?.kind === "import_batch"
    || task.batchId?.startsWith(LEGACY_IMPORT_OPERATION_PREFIX) === true);

export const importBatchOperationSessionId = (task: BackendTask): string | null => {
  if (task.operation?.kind === "import_batch") return task.operation.sessionId;
  return task.batchId?.startsWith(LEGACY_IMPORT_OPERATION_PREFIX)
    ? task.batchId.slice(LEGACY_IMPORT_OPERATION_PREFIX.length)
    : null;
};

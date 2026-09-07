import { compareWorkflowRevision } from "./workflowTaskSnapshot";
import type { WorkflowRunSummary } from "../types/workflow";
import type { BackendEvent, StreamDelta } from "../types/task";
import { StreamDeltaBatcher, type StreamDeltaScheduler } from "./streamDeltaBatcher";
import { TaskSnapshotBatcher } from "./taskSnapshotBatcher";

export type TaskEventListener = (event: BackendEvent) => void;

interface TaskEventDispatcherOptions {
  scheduler?: StreamDeltaScheduler;
  flushIntervalMs?: number;
  frameFallbackMs?: number;
  taskFlushIntervalMs?: number;
}

function isTerminalEvent(event: BackendEvent): boolean {
  return event.eventType === "task_completed"
    || event.eventType === "task_failed"
    || event.eventType === "task_cancelled";
}

export class TaskEventDispatcher {
  private readonly listeners = new Set<TaskEventListener>();
  private readonly streamBatcher: StreamDeltaBatcher;
  private readonly taskSnapshotBatcher: TaskSnapshotBatcher;
  private readonly workflowPending = new Map<string, BackendEvent<WorkflowRunSummary>>();
  private workflowTimer: number | null = null;
  private readonly workflowScheduler: StreamDeltaScheduler;
  private ownerListener: TaskEventListener | null = null;

  constructor(options: TaskEventDispatcherOptions = {}) {
    this.workflowScheduler = options.scheduler ?? {
      setTimeout: (callback, delay) => window.setTimeout(callback, delay),
      clearTimeout: (id) => window.clearTimeout(id),
      requestAnimationFrame: (callback) => window.requestAnimationFrame(callback),
      cancelAnimationFrame: (id) => window.cancelAnimationFrame(id),
    };
    this.streamBatcher = new StreamDeltaBatcher(
      (event) => this.emit(event),
      options,
    );
    this.taskSnapshotBatcher = new TaskSnapshotBatcher(
      (event) => this.emit(event),
      {
        scheduler: options.scheduler,
        flushIntervalMs: options.taskFlushIntervalMs,
      },
    );
  }

  /** Feature listeners consume delivered events; they never own canonical task facts. */
  register(listener: TaskEventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  registerOwner(listener: TaskEventListener): () => void {
    this.ownerListener = listener;
    return () => {
      if (this.ownerListener === listener) this.ownerListener = null;
    };
  }

  dispatch(event: BackendEvent): void {
    if (event.eventType === "workflow_updated" && event.taskId) {
      const summaryEvent = event as BackendEvent<WorkflowRunSummary>;
      // Revisions are local to one backend session and one canonical project owner.
      // Keep other owners' facts available for the canonical store to accept or reject.
      const key = JSON.stringify([
        event.taskId, event.projectId, summaryEvent.payload.projectId,
        summaryEvent.payload.sessionId ?? null,
        summaryEvent.payload.canonicalIdentityKey, summaryEvent.payload.identityRevision,
      ]);
      const pending = this.workflowPending.get(key);
      if (pending && compareWorkflowRevision(summaryEvent.payload, pending.payload) < 0) return;
      if (summaryEvent.payload.displayStatus === "running") {
        this.workflowPending.set(key, summaryEvent);
        if (this.workflowTimer === null) this.workflowTimer = this.workflowScheduler.setTimeout(() => this.flushWorkflows(), 100);
        return;
      }
      this.workflowPending.delete(key);
      this.emit(event);
      return;
    }
    if (event.eventType === "task_stream_output") {
      this.streamBatcher.enqueue(event as BackendEvent<StreamDelta>);
      return;
    }
    if (isTerminalEvent(event) && event.taskId) {
      this.streamBatcher.flushTask(event.projectId, event.taskId);
    }
    if (this.taskSnapshotBatcher.enqueue(event)) return;
    this.emit(event);
  }

  retainProject(projectId: string | null): void {
    this.flushWorkflows();
    this.streamBatcher.retainProject(projectId);
    this.taskSnapshotBatcher.retainProject(projectId);
  }

  clearPending(shouldFlush?: (event: BackendEvent) => boolean): void {
    this.flushWorkflows();
    this.streamBatcher.dispose(shouldFlush ? (event) => shouldFlush(event) : undefined);
    this.taskSnapshotBatcher.dispose(shouldFlush ? (event) => shouldFlush(event) : undefined);
  }

  private flushWorkflows(): void {
    if (this.workflowTimer !== null) this.workflowScheduler.clearTimeout(this.workflowTimer);
    this.workflowTimer = null;
    const events = [...this.workflowPending.values()];
    this.workflowPending.clear();
    for (const event of events) this.emit(event);
  }

  private emit(event: BackendEvent): void {
    this.ownerListener?.(event);
    for (const listener of this.listeners) listener(event);
  }
}

const taskEventDispatcher = new TaskEventDispatcher();

export function registerTaskEventListener(listener: TaskEventListener): () => void {
  return taskEventDispatcher.register(listener);
}

export function registerTaskEventOwner(listener: TaskEventListener): () => void {
  return taskEventDispatcher.registerOwner(listener);
}

export function dispatchTaskEvent(event: BackendEvent): void {
  taskEventDispatcher.dispatch(event);
}

export function retainTaskEventProject(projectId: string | null): void {
  taskEventDispatcher.retainProject(projectId);
}

export function clearPendingTaskEvents(
  shouldFlush?: (event: BackendEvent) => boolean,
): void {
  taskEventDispatcher.clearPending(shouldFlush);
}

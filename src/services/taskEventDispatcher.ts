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
  private ownerListener: TaskEventListener | null = null;

  constructor(options: TaskEventDispatcherOptions = {}) {
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
    this.streamBatcher.retainProject(projectId);
    this.taskSnapshotBatcher.retainProject(projectId);
  }

  clearPending(shouldFlush?: (event: BackendEvent) => boolean): void {
    this.streamBatcher.dispose(shouldFlush ? (event) => shouldFlush(event) : undefined);
    this.taskSnapshotBatcher.dispose(shouldFlush ? (event) => shouldFlush(event) : undefined);
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

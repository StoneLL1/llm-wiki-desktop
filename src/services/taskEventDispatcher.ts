import type { BackendEvent, StreamDelta } from "../types/task";
import { StreamDeltaBatcher, type StreamDeltaScheduler } from "./streamDeltaBatcher";

export type TaskEventListener = (event: BackendEvent) => void;

interface TaskEventDispatcherOptions {
  scheduler?: StreamDeltaScheduler;
  flushIntervalMs?: number;
  frameFallbackMs?: number;
}

function isTerminalEvent(event: BackendEvent): boolean {
  return event.eventType === "task_completed"
    || event.eventType === "task_failed"
    || event.eventType === "task_cancelled";
}

export class TaskEventDispatcher {
  private readonly listeners = new Set<TaskEventListener>();
  private readonly streamBatcher: StreamDeltaBatcher;
  private ownerListener: TaskEventListener | null = null;

  constructor(options: TaskEventDispatcherOptions = {}) {
    this.streamBatcher = new StreamDeltaBatcher(
      (event) => this.emit(event),
      options,
    );
  }

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
    this.emit(event);
  }

  retainProject(projectId: string | null): void {
    this.streamBatcher.retainProject(projectId);
  }

  clearPending(shouldFlush?: (event: BackendEvent<StreamDelta>) => boolean): void {
    this.streamBatcher.dispose(shouldFlush);
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
  shouldFlush?: (event: BackendEvent<StreamDelta>) => boolean,
): void {
  taskEventDispatcher.clearPending(shouldFlush);
}

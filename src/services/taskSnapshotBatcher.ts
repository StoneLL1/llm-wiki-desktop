import { isTerminalStatus, type BackendEvent, type BackendTask } from "../types/task";
import {
  isBackendTaskSnapshot,
  isProgressOnlyTaskSnapshot,
  taskSnapshotsEqual,
} from "./taskSnapshotSemantics";

const DEFAULT_FLUSH_INTERVAL_MS = 250;

export interface TaskSnapshotScheduler {
  setTimeout(callback: () => void, delayMs: number): number;
  clearTimeout(id: number): void;
}

interface TaskSnapshotBatcherOptions {
  flushIntervalMs?: number;
  scheduler?: TaskSnapshotScheduler;
}

interface PendingSnapshot {
  event: BackendEvent<BackendTask>;
  timerId: number;
}

function defaultScheduler(): TaskSnapshotScheduler {
  return {
    setTimeout: (callback, delayMs) => window.setTimeout(callback, delayMs),
    clearTimeout: (id) => window.clearTimeout(id),
  };
}

function taskKey(projectId: string | null, taskId: string): string {
  return JSON.stringify([projectId, taskId]);
}

function isImmediateBoundary(event: BackendEvent<BackendTask>): boolean {
  return event.eventType === "task_completed"
    || event.eventType === "task_failed"
    || event.eventType === "task_cancelled"
    || event.eventType === "confirmation_requested"
    || event.payload.status === "waiting_for_confirmation"
    || isTerminalStatus(event.payload.status);
}

function isStaleSnapshot(previous: BackendTask, incoming: BackendTask): boolean {
  const previousUpdated = Date.parse(previous.updatedAt);
  const incomingUpdated = Date.parse(incoming.updatedAt);
  if (!Number.isNaN(previousUpdated) && !Number.isNaN(incomingUpdated) && previousUpdated > incomingUpdated) {
    return true;
  }
  if (isTerminalStatus(previous.status) && !isTerminalStatus(incoming.status)) return true;
  return previous.status === "waiting_for_confirmation" && incoming.status === "queued";
}

/**
 * Coalesces observational task progress by project/task while preserving every
 * status, confirmation, error, result, cancellation, and terminal boundary.
 * Feature listeners receive only delivered events; the registered event owner
 * remains the sole writer of canonical task facts.
 */
export class TaskSnapshotBatcher {
  private readonly committed = new Map<string, BackendTask>();
  private readonly pending = new Map<string, PendingSnapshot>();
  private readonly scheduler: TaskSnapshotScheduler;
  private readonly flushIntervalMs: number;

  constructor(
    private readonly deliver: (event: BackendEvent<BackendTask>) => void,
    options: TaskSnapshotBatcherOptions = {},
  ) {
    this.scheduler = options.scheduler ?? defaultScheduler();
    this.flushIntervalMs = options.flushIntervalMs ?? DEFAULT_FLUSH_INTERVAL_MS;
  }

  enqueue(rawEvent: BackendEvent): boolean {
    if (!rawEvent.taskId || !isBackendTaskSnapshot(rawEvent.payload)) return false;
    const event = rawEvent as BackendEvent<BackendTask>;
    const key = taskKey(event.projectId, rawEvent.taskId);
    const pending = this.pending.get(key);
    const previous = pending?.event.payload ?? this.committed.get(key);

    if (previous && isStaleSnapshot(previous, event.payload)) return true;
    if (previous && taskSnapshotsEqual(previous, event.payload)) return true;
    if (isImmediateBoundary(event)) {
      this.drop(key);
      this.commit(key, event);
      return true;
    }
    if (!previous || !isProgressOnlyTaskSnapshot(previous, event.payload)) {
      this.drop(key);
      this.commit(key, event);
      return true;
    }

    if (pending) {
      pending.event = event;
      return true;
    }
    const timerId = this.scheduler.setTimeout(() => this.flush(key), this.flushIntervalMs);
    this.pending.set(key, { event, timerId });
    return true;
  }

  flushTask(projectId: string | null, taskId: string): boolean {
    return this.flush(taskKey(projectId, taskId));
  }

  /** Flush old-project facts after a scope switch; presentation guards decide visibility. */
  retainProject(projectId: string | null): void {
    for (const [key, pending] of [...this.pending]) {
      if (pending.event.projectId !== projectId) this.flush(key);
    }
    for (const [key, task] of [...this.committed]) {
      if (task.projectId !== projectId) this.committed.delete(key);
    }
  }

  dispose(shouldFlush: (event: BackendEvent<BackendTask>) => boolean = () => false): void {
    for (const [key, pending] of [...this.pending]) {
      if (shouldFlush(pending.event)) this.flush(key);
      else this.drop(key);
    }
    this.committed.clear();
  }

  private flush(key: string): boolean {
    const pending = this.pending.get(key);
    if (!pending) return false;
    this.scheduler.clearTimeout(pending.timerId);
    this.pending.delete(key);
    this.commit(key, pending.event);
    return true;
  }

  private drop(key: string): void {
    const pending = this.pending.get(key);
    if (!pending) return;
    this.scheduler.clearTimeout(pending.timerId);
    this.pending.delete(key);
  }

  private commit(key: string, event: BackendEvent<BackendTask>): void {
    this.committed.set(key, event.payload);
    this.deliver(event);
  }
}

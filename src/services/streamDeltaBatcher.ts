import type { BackendEvent, StreamDelta } from "../types/task";

const DEFAULT_FLUSH_INTERVAL_MS = 40;
const DEFAULT_FRAME_FALLBACK_MS = 10;

export interface StreamDeltaScheduler {
  setTimeout(callback: () => void, delayMs: number): number;
  clearTimeout(id: number): void;
  requestAnimationFrame(callback: FrameRequestCallback): number | null;
  cancelAnimationFrame(id: number): void;
}

interface BufferedStream {
  projectId: string | null;
  taskId: string;
  chunks: string[];
  lastEvent: BackendEvent<StreamDelta>;
  route: StreamDelta["route"];
  delayTimer: number | null;
  frameId: number | null;
  fallbackTimer: number | null;
}

interface StreamDeltaBatcherOptions {
  flushIntervalMs?: number;
  frameFallbackMs?: number;
  scheduler?: StreamDeltaScheduler;
}

function defaultScheduler(): StreamDeltaScheduler {
  return {
    setTimeout: (callback, delayMs) => window.setTimeout(callback, delayMs),
    clearTimeout: (id) => window.clearTimeout(id),
    requestAnimationFrame: (callback) =>
      typeof window.requestAnimationFrame === "function"
        ? window.requestAnimationFrame(callback)
        : null,
    cancelAnimationFrame: (id) => {
      if (typeof window.cancelAnimationFrame === "function") {
        window.cancelAnimationFrame(id);
      }
    },
  };
}

function streamKey(projectId: string | null, taskId: string): string {
  return JSON.stringify([projectId, taskId]);
}

/**
 * Buffers high-frequency stream events by project and task. A normal flush is
 * delayed by 40 ms and aligned to the next animation frame, with a short
 * timeout fallback for hidden documents where RAF may be suspended.
 */
export class StreamDeltaBatcher {
  private readonly buffers = new Map<string, BufferedStream>();
  private readonly scheduler: StreamDeltaScheduler;
  private readonly flushIntervalMs: number;
  private readonly frameFallbackMs: number;

  constructor(
    private readonly deliver: (event: BackendEvent<StreamDelta>) => void,
    options: StreamDeltaBatcherOptions = {},
  ) {
    this.scheduler = options.scheduler ?? defaultScheduler();
    this.flushIntervalMs = options.flushIntervalMs ?? DEFAULT_FLUSH_INTERVAL_MS;
    this.frameFallbackMs = options.frameFallbackMs ?? DEFAULT_FRAME_FALLBACK_MS;
  }

  enqueue(event: BackendEvent<StreamDelta>): void {
    const taskId = event.taskId;
    const payload = event.payload;
    if (!taskId || typeof payload?.delta !== "string") return;
    const key = streamKey(event.projectId, taskId);
    const existing = this.buffers.get(key);
    if (existing) {
      existing.chunks.push(payload.delta);
      existing.lastEvent = event;
      if (payload.route != null) existing.route = payload.route;
      return;
    }

    const buffer: BufferedStream = {
      projectId: event.projectId,
      taskId,
      chunks: [payload.delta],
      lastEvent: event,
      route: payload.route,
      delayTimer: null,
      frameId: null,
      fallbackTimer: null,
    };
    this.buffers.set(key, buffer);
    buffer.delayTimer = this.scheduler.setTimeout(() => {
      buffer.delayTimer = null;
      this.alignFlush(key, buffer);
    }, this.flushIntervalMs);
  }

  flushTask(projectId: string | null, taskId: string): boolean {
    return this.flush(streamKey(projectId, taskId));
  }

  retainProject(projectId: string | null): void {
    for (const [key, buffer] of this.buffers) {
      if (buffer.projectId !== projectId) this.drop(key, buffer);
    }
  }

  /** Flush selected valid buffers and drop the rest, then clear all handles. */
  dispose(shouldFlush: (event: BackendEvent<StreamDelta>) => boolean = () => false): void {
    for (const [key, buffer] of [...this.buffers]) {
      if (shouldFlush(buffer.lastEvent)) this.flush(key);
      else this.drop(key, buffer);
    }
  }

  private alignFlush(key: string, buffer: BufferedStream): void {
    if (this.buffers.get(key) !== buffer) return;
    buffer.frameId = this.scheduler.requestAnimationFrame(() => {
      buffer.frameId = null;
      this.flush(key);
    });
    buffer.fallbackTimer = this.scheduler.setTimeout(() => {
      buffer.fallbackTimer = null;
      this.flush(key);
    }, this.frameFallbackMs);
  }

  private flush(key: string): boolean {
    const buffer = this.buffers.get(key);
    if (!buffer) return false;
    this.cancelSchedule(buffer);
    this.buffers.delete(key);
    const payload = buffer.lastEvent.payload;
    this.deliver({
      ...buffer.lastEvent,
      payload: {
        ...payload,
        delta: buffer.chunks.join(""),
        route: buffer.route,
      },
    });
    return true;
  }

  private drop(key: string, buffer: BufferedStream): void {
    this.cancelSchedule(buffer);
    this.buffers.delete(key);
  }

  private cancelSchedule(buffer: BufferedStream): void {
    if (buffer.delayTimer !== null) this.scheduler.clearTimeout(buffer.delayTimer);
    if (buffer.frameId !== null) this.scheduler.cancelAnimationFrame(buffer.frameId);
    if (buffer.fallbackTimer !== null) this.scheduler.clearTimeout(buffer.fallbackTimer);
    buffer.delayTimer = null;
    buffer.frameId = null;
    buffer.fallbackTimer = null;
  }
}

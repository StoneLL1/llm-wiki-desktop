import { describe, expect, it, vi } from "vitest";

import type { BackendEvent, StreamDelta } from "../types/task";
import {
  StreamDeltaBatcher,
  type StreamDeltaScheduler,
} from "./streamDeltaBatcher";

function streamEvent(
  taskId: string,
  delta: string,
  projectId = "project-a",
  route: StreamDelta["route"] = "chat-byok",
): BackendEvent<StreamDelta> {
  return {
    eventId: `${taskId}-${delta}`,
    eventType: "task_stream_output",
    projectId,
    taskId,
    timestamp: "2026-08-16T00:00:00Z",
    payload: { delta, route },
  };
}

function createScheduler() {
  let nextId = 1;
  let now = 0;
  const timeouts = new Map<number, { callback: () => void; dueAt: number }>();
  const frames = new Map<number, { callback: FrameRequestCallback; dueAt: number }>();
  const scheduler: StreamDeltaScheduler = {
    setTimeout: vi.fn((callback, delayMs) => {
      const id = nextId++;
      timeouts.set(id, { callback, dueAt: now + delayMs });
      return id;
    }),
    clearTimeout: vi.fn((id) => {
      timeouts.delete(id);
    }),
    requestAnimationFrame: vi.fn((callback) => {
      const id = nextId++;
      frames.set(id, {
        callback,
        dueAt: (Math.floor(now / 16) + 1) * 16,
      });
      return id;
    }),
    cancelAnimationFrame: vi.fn((id) => {
      frames.delete(id);
    }),
  };
  return {
    scheduler,
    runNextTimeout() {
      const entry = [...timeouts.entries()].sort(([, left], [, right]) => left.dueAt - right.dueAt)[0];
      if (!entry) return false;
      timeouts.delete(entry[0]);
      now = entry[1].dueAt;
      entry[1].callback();
      return true;
    },
    runAllFrames() {
      const pending = [...frames.entries()];
      frames.clear();
      pending.forEach(([, frame]) => {
        now = Math.max(now, frame.dueAt);
        frame.callback(now);
      });
    },
    advanceBy(durationMs: number) {
      const target = now + durationMs;
      while (true) {
        const timeout = [...timeouts.entries()]
          .map(([id, scheduled]) => ({ id, kind: "timeout" as const, ...scheduled }))
          .filter((scheduled) => scheduled.dueAt <= target);
        const frame = [...frames.entries()]
          .map(([id, scheduled]) => ({ id, kind: "frame" as const, ...scheduled }))
          .filter((scheduled) => scheduled.dueAt <= target);
        const next = [...timeout, ...frame].sort((left, right) => left.dueAt - right.dueAt)[0];
        if (!next) break;
        now = next.dueAt;
        if (next.kind === "timeout") {
          timeouts.delete(next.id);
          next.callback();
        } else {
          frames.delete(next.id);
          next.callback(now);
        }
      }
      now = target;
    },
    pendingTimeouts: () => timeouts.size,
    pendingFrames: () => frames.size,
  };
}

describe("StreamDeltaBatcher", () => {
  it("joins 10,000 synchronous deltas into one byte-identical publication", () => {
    const fake = createScheduler();
    const delivered = vi.fn<(event: BackendEvent<StreamDelta>) => void>();
    const batcher = new StreamDeltaBatcher(delivered, { scheduler: fake.scheduler });
    const deltas = Array.from({ length: 10_000 }, (_, index) => `${index % 10}`);

    deltas.forEach((delta) => batcher.enqueue(streamEvent("task-a", delta)));
    expect(delivered).not.toHaveBeenCalled();

    fake.runNextTimeout();
    fake.runAllFrames();

    expect(delivered).toHaveBeenCalledTimes(1);
    expect(delivered.mock.calls[0]?.[0].payload.delta).toBe(deltas.join(""));
  });

  it("keeps interleaved tasks isolated and ordered", () => {
    const fake = createScheduler();
    const delivered: BackendEvent<StreamDelta>[] = [];
    const batcher = new StreamDeltaBatcher((event) => delivered.push(event), {
      scheduler: fake.scheduler,
    });

    batcher.enqueue(streamEvent("task-a", "A1"));
    batcher.enqueue(streamEvent("task-b", "B1"));
    batcher.enqueue(streamEvent("task-a", "A2"));
    batcher.enqueue(streamEvent("task-b", "B2"));
    batcher.flushTask("project-a", "task-a");
    batcher.flushTask("project-a", "task-b");

    expect(delivered.map((event) => [event.taskId, event.payload.delta])).toEqual([
      ["task-a", "A1A2"],
      ["task-b", "B1B2"],
    ]);
  });

  it("stays within 25 Hz over a sustained two-second stream plus terminal flush", () => {
    const fake = createScheduler();
    const delivered: BackendEvent<StreamDelta>[] = [];
    const batcher = new StreamDeltaBatcher((event) => delivered.push(event), {
      scheduler: fake.scheduler,
    });

    for (let elapsedMs = 0; elapsedMs < 2_000; elapsedMs += 1) {
      batcher.enqueue(streamEvent("task-a", "."));
      fake.advanceBy(1);
    }
    batcher.enqueue(streamEvent("task-a", "tail"));
    batcher.flushTask("project-a", "task-a");

    expect(delivered.length).toBeLessThanOrEqual(51);
    expect(delivered.map((event) => event.payload.delta).join("")).toBe(`${".".repeat(2_000)}tail`);
  });

  it("uses the timeout fallback when animation frames do not run", () => {
    const fake = createScheduler();
    const delivered = vi.fn<(event: BackendEvent<StreamDelta>) => void>();
    const batcher = new StreamDeltaBatcher(delivered, { scheduler: fake.scheduler });

    batcher.enqueue(streamEvent("task-a", "visible while hidden"));
    expect(fake.runNextTimeout()).toBe(true);
    expect(fake.pendingFrames()).toBe(1);
    expect(delivered).not.toHaveBeenCalled();
    expect(fake.runNextTimeout()).toBe(true);

    expect(delivered).toHaveBeenCalledTimes(1);
  });

  it("drops stale-project buffers and clears every scheduled callback on dispose", () => {
    const fake = createScheduler();
    const delivered = vi.fn<(event: BackendEvent<StreamDelta>) => void>();
    const batcher = new StreamDeltaBatcher(delivered, { scheduler: fake.scheduler });

    batcher.enqueue(streamEvent("task-a", "old", "project-a"));
    batcher.enqueue(streamEvent("task-b", "new", "project-b"));
    batcher.retainProject("project-b");
    batcher.dispose((event) => event.projectId === "project-b");

    expect(delivered).toHaveBeenCalledTimes(1);
    expect(delivered.mock.calls[0]?.[0].payload.delta).toBe("new");
    expect(fake.pendingTimeouts()).toBe(0);
    expect(fake.pendingFrames()).toBe(0);
  });
});

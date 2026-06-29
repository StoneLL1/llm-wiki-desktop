import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendEvent, BackendTask } from "../types/task";
import { waitForTaskTerminal } from "./waitForTaskTerminal";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());
const unlistenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  unlistenMock.mockClear();
});

afterEach(() => {
  vi.clearAllMocks();
});

interface CapturedHandler {
  channel: string;
  handler: (evt: { payload: BackendEvent }) => void;
}

function captureHandlers(): CapturedHandler[] {
  const handlers: CapturedHandler[] = [];
  listenMock.mockImplementation((channel: string, handler: (evt: { payload: BackendEvent }) => void) => {
    handlers.push({ channel, handler });
    return Promise.resolve(unlistenMock);
  });
  return handlers;
}

function terminalEvent(taskId: string, task: BackendTask): { payload: BackendEvent } {
  return {
    payload: {
      eventId: "e1",
      eventType: "task_completed",
      projectId: null,
      taskId,
      timestamp: "",
      payload: task,
    },
  };
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

const running = (id: string): BackendTask => ({ id, status: "running" } as BackendTask);

describe("waitForTaskTerminal", () => {
  it("resolves immediately without the event bus when the task is already terminal", async () => {
    const task = { id: "t1", status: "succeeded" } as BackendTask;

    await expect(waitForTaskTerminal(task)).resolves.toEqual(task);

    expect(listenMock).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("resolves when a terminal event for the task arrives and unregisters listeners", async () => {
    const handlers = captureHandlers();
    invokeMock.mockResolvedValue(running("t1"));

    const promise = waitForTaskTerminal(running("t1"));
    await flush();

    const done: BackendTask = { id: "t1", status: "succeeded" } as BackendTask;
    handlers
      .find((h) => h.channel === "task://completed")!
      .handler(terminalEvent("t1", done));

    await expect(promise).resolves.toEqual(done);
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });

  it("resolves from get_task if the task terminates before listeners attach", async () => {
    captureHandlers();
    const terminal: BackendTask = { id: "t1", status: "failed" } as BackendTask;
    invokeMock.mockResolvedValue(terminal);

    const promise = waitForTaskTerminal(running("t1"));
    await expect(promise).resolves.toEqual(terminal);
    await flush();
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });

  it("ignores terminal events for other task ids", async () => {
    const handlers = captureHandlers();
    invokeMock.mockResolvedValue(running("t1"));

    const promise = waitForTaskTerminal(running("t1"));
    await flush();

    const other: BackendTask = { id: "other", status: "succeeded" } as BackendTask;
    handlers
      .find((h) => h.channel === "task://completed")!
      .handler(terminalEvent("other", other));

    const mine: BackendTask = { id: "t1", status: "cancelled" } as BackendTask;
    handlers
      .find((h) => h.channel === "task://cancelled")!
      .handler(terminalEvent("t1", mine));

    await expect(promise).resolves.toEqual(mine);
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });

  it("does not resolve while the task is still running and no terminal event fires", async () => {
    captureHandlers();
    invokeMock.mockResolvedValue(running("t1"));

    const promise = waitForTaskTerminal(running("t1"));
    await flush();

    const result = await Promise.race([
      promise.then((t) => t.status),
      flush().then(() => "pending" as const),
    ]);
    expect(result).toBe("pending");
  });
});

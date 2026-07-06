import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackendEvent, BackendTask } from "../types/task";
import { waitForTaskTerminal, WaitForTaskTerminalTimeoutError } from "./waitForTaskTerminal";

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
const succeeded = (id: string): BackendTask => ({ id, status: "succeeded" } as BackendTask);

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

    const done: BackendTask = succeeded("t1");
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

    const other: BackendTask = succeeded("other");
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
});

// The polling/timeout paths use fake timers (scoped to setTimeout/clearTimeout)
// so the tests can advance the poll interval and the deadline deterministically
// without waiting real time. Event-driven behavior stays on real timers above.
describe("waitForTaskTerminal polling and timeout", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("recovers a missed terminal event by polling get_task", async () => {
    captureHandlers();
    let calls = 0;
    invokeMock.mockImplementation(() => {
      calls += 1;
      return Promise.resolve(calls === 1 ? running("t1") : succeeded("t1"));
    });

    const promise = waitForTaskTerminal(running("t1"), { pollMs: 1000, timeoutMs: 30000 });

    await vi.advanceTimersByTimeAsync(1000);
    await expect(promise).resolves.toEqual(succeeded("t1"));
    expect(calls).toBe(2);
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });

  it("keeps polling across a transient get_task failure", async () => {
    captureHandlers();
    let calls = 0;
    invokeMock.mockImplementation(() => {
      calls += 1;
      if (calls <= 2) return Promise.reject(new Error("transient"));
      return Promise.resolve(succeeded("t1"));
    });

    const promise = waitForTaskTerminal(running("t1"), { pollMs: 1000, timeoutMs: 30000 });

    await vi.advanceTimersByTimeAsync(2000);
    await expect(promise).resolves.toEqual(succeeded("t1"));
    expect(calls).toBe(3);
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });

  it("does not resolve when get_task returns a terminal task with a different id", async () => {
    captureHandlers();
    invokeMock.mockResolvedValue(succeeded("other"));

    const promise = waitForTaskTerminal(running("t1"), { pollMs: 1000, timeoutMs: 2500 });
    const caught = promise.catch((err: unknown) => err);

    await vi.advanceTimersByTimeAsync(2500);
    const err = await caught;
    expect(err).toBeInstanceOf(WaitForTaskTerminalTimeoutError);
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });

  it("rejects with a typed timeout error and cleans up when no signal arrives", async () => {
    captureHandlers();
    invokeMock.mockResolvedValue(running("t1"));

    const promise = waitForTaskTerminal(running("t1"), { pollMs: 1000, timeoutMs: 2500 });
    // Attach the rejection handler before advancing time so the deadline's
    // reject never sits unhandled for a tick (which would surface as an
    // unhandled-rejection warning and could mask real failures).
    const caught = promise.catch((err: unknown) => err);

    await vi.advanceTimersByTimeAsync(2500);
    const err = await caught;
    expect(err).toBeInstanceOf(WaitForTaskTerminalTimeoutError);
    expect(err).toMatchObject({ code: "TASK_WAIT_TIMEOUT", taskId: "t1", timeoutMs: 2500 });
    expect(unlistenMock).toHaveBeenCalledTimes(3);

    const callsAfterTimeout = invokeMock.mock.calls.length;
    await vi.advanceTimersByTimeAsync(10000);
    expect(invokeMock.mock.calls.length).toBe(callsAfterTimeout);
  });

  it("clears the polling and deadline timers when an event resolves the wait", async () => {
    const handlers = captureHandlers();
    invokeMock.mockResolvedValue(running("t1"));

    const promise = waitForTaskTerminal(running("t1"), { pollMs: 1000, timeoutMs: 2500 });
    await vi.advanceTimersByTimeAsync(0);
    const done = succeeded("t1");
    handlers
      .find((h) => h.channel === "task://completed")!
      .handler(terminalEvent("t1", done));
    await expect(promise).resolves.toEqual(done);

    const callsAfterResolve = invokeMock.mock.calls.length;
    await vi.advanceTimersByTimeAsync(60000);
    expect(invokeMock.mock.calls.length).toBe(callsAfterResolve);
    expect(unlistenMock).toHaveBeenCalledTimes(3);
  });
});

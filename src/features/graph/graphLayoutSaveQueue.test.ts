import { describe, expect, it, vi } from "vitest";

import { createLatestLayoutSaveQueue } from "./graphLayoutSaveQueue";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("createLatestLayoutSaveQueue", () => {
  it("serializes writes and coalesces pending work to the newest layout", async () => {
    const queue = createLatestLayoutSaveQueue();
    const first = deferred();
    const order: string[] = [];

    queue.request(async () => {
      order.push("first:start");
      await first.promise;
      order.push("first:end");
    });
    queue.request(async () => {
      order.push("superseded");
    });
    queue.request(async () => {
      order.push("latest");
    });

    expect(order).toEqual(["first:start"]);
    first.resolve();
    await queue.whenIdle();

    expect(order).toEqual(["first:start", "first:end", "latest"]);
  });

  it("continues with the newest pending write after the active write resolves", async () => {
    const queue = createLatestLayoutSaveQueue();
    const save = vi.fn(async () => {});

    queue.request(save);
    await queue.whenIdle();
    queue.request(save);
    await queue.whenIdle();

    expect(save).toHaveBeenCalledTimes(2);
  });
});

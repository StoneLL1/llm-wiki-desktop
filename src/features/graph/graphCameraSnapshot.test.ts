import { describe, expect, it, vi } from "vitest";

import {
  captureGraphCameraSnapshot,
  createGraphCameraSnapshotGate,
  restoreGraphCameraSnapshot,
} from "./graphCameraSnapshot";

describe("graph camera snapshot", () => {
  it("captures and restores a finite camera for the same topology", () => {
    const setState = vi.fn();
    const snapshot = captureGraphCameraSnapshot("hash-a", {
      getState: () => ({ x: 0.25, y: 0.75, ratio: 1.4, angle: 0.2 }),
    });

    expect(restoreGraphCameraSnapshot("hash-a", { setState }, snapshot)).toBe(true);
    expect(setState).toHaveBeenCalledWith({ x: 0.25, y: 0.75, ratio: 1.4, angle: 0.2 });
  });

  it("does not apply a camera to a different topology", () => {
    const setState = vi.fn();
    const restored = restoreGraphCameraSnapshot("hash-b", { setState }, {
      contentHash: "hash-a",
      x: 0.25,
      y: 0.75,
      ratio: 1.4,
      angle: 0.2,
    });

    expect(restored).toBe(false);
    expect(setState).not.toHaveBeenCalled();
  });

  it("rejects invalid ratios and non-finite values", () => {
    expect(captureGraphCameraSnapshot("hash-a", {
      getState: () => ({ x: Number.NaN, y: 0.5, ratio: 1, angle: 0 }),
    })).toBeNull();
    expect(captureGraphCameraSnapshot("hash-a", {
      getState: () => ({ x: 0.5, y: 0.5, ratio: 0, angle: 0 }),
    })).toBeNull();
  });

  it("keeps fit animation updates suppressed until a later user camera gesture", () => {
    const gate = createGraphCameraSnapshotGate();

    gate.invalidate();
    // animatedReset emits several updated events; none may recreate the
    // explicitly-cleared snapshot.
    expect(gate.cameraUpdated()).toBe(false);
    expect(gate.cameraUpdated()).toBe(false);
    expect(gate.canCapture()).toBe(false);

    gate.noteUserIntent();
    expect(gate.cameraUpdated()).toBe(true);
    expect(gate.canCapture()).toBe(true);
  });

  it("does not let pre-existing pointer intent survive an explicit reset", () => {
    const gate = createGraphCameraSnapshotGate();
    gate.noteUserIntent();
    gate.invalidate();
    expect(gate.cameraUpdated()).toBe(false);
  });
});

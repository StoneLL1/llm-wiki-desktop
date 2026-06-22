import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { describe, expect, it, vi } from "vitest";

import { reduceDragDrop, subscribeToDragDrop } from "./dragDrop";

const position = new PhysicalPosition(12, 24);

describe("reduceDragDrop", () => {
  it("activates the drop target when files enter", () => {
    expect(
      reduceDragDrop({
        type: "enter",
        paths: ["D:\\资料\\论文.pdf"],
        position,
      }),
    ).toEqual({ active: true, paths: null });
  });

  it("preserves absolute Windows and CJK paths on drop", () => {
    const paths = ["D:\\资料\\论文.pdf", "C:\\Notes\\研究.docx"];

    expect(reduceDragDrop({ type: "drop", paths, position })).toEqual({
      active: false,
      paths,
    });
  });

  it("does not request a preview for an empty drop", () => {
    expect(reduceDragDrop({ type: "drop", paths: [], position })).toEqual({
      active: false,
      paths: null,
    });
  });

  it("clears the highlight when a drag leaves", () => {
    expect(reduceDragDrop({ type: "leave" })).toEqual({
      active: false,
      paths: null,
    });
  });
});

describe("subscribeToDragDrop", () => {
  it("unwraps the Tauri event payload before forwarding dropped paths", async () => {
    const registration: {
      handler?: (event: { payload: Parameters<typeof reduceDragDrop>[0] }) => void;
    } = {};
    const onPaths = vi.fn();
    const onActive = vi.fn();
    await subscribeToDragDrop({
      listen: async (next) => {
        registration.handler = next;
        return () => undefined;
      },
      isCancelled: () => false,
      onActive,
      onPaths,
    });

    registration.handler?.({
      payload: { type: "drop", paths: ["D:\\资料\\论文.pdf"], position },
    });

    expect(onActive).toHaveBeenCalledWith(false);
    expect(onPaths).toHaveBeenCalledWith(["D:\\资料\\论文.pdf"]);
  });

  it("unlistens when registration resolves after the component was cancelled", async () => {
    const registration: { finish?: (unlisten: () => void) => void } = {};
    const unlisten: () => void = vi.fn();
    let cancelled = false;
    const subscription = subscribeToDragDrop({
      listen: () =>
        new Promise((resolve) => {
          registration.finish = resolve;
        }),
      isCancelled: () => cancelled,
      onActive: vi.fn(),
      onPaths: vi.fn(),
    });

    cancelled = true;
    registration.finish?.(unlisten);
    await subscription;

    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});

import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { describe, expect, it } from "vitest";

import { reduceDragDrop } from "./dragDrop";

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

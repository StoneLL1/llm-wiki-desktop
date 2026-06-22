import { describe, expect, it, vi } from "vitest";

import { normalizeSelectedPaths, selectImportFiles } from "./nativeFilePicker";

describe("normalizeSelectedPaths", () => {
  it("keeps a single absolute CJK path", () => {
    expect(normalizeSelectedPaths("D:\\资料\\论文.pdf")).toEqual([
      "D:\\资料\\论文.pdf",
    ]);
  });

  it("keeps all paths returned by a multi-file selection", () => {
    expect(
      normalizeSelectedPaths(["D:\\资料\\论文.pdf", "C:\\Notes\\研究.docx"]),
    ).toEqual(["D:\\资料\\论文.pdf", "C:\\Notes\\研究.docx"]);
  });

  it("returns no paths when the dialog is cancelled", () => {
    expect(normalizeSelectedPaths(null)).toEqual([]);
  });
});

describe("selectImportFiles", () => {
  it("opens the native dialog in multi-file mode", async () => {
    const open = vi.fn().mockResolvedValue(["D:\\资料\\论文.pdf"]);

    await expect(selectImportFiles(open)).resolves.toEqual(["D:\\资料\\论文.pdf"]);
    expect(open).toHaveBeenCalledWith({ directory: false, multiple: true });
  });
});

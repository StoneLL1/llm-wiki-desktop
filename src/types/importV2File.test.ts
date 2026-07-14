import { describe, expect, it } from "vitest";

import { FILE_FORMATS, FILE_SKIP_REASONS } from "./importV2File";

describe("Import V2 file contracts", () => {
  it("keeps stable format and skip reason wire names", () => {
    expect(FILE_FORMATS).toEqual([
      "markdown",
      "doc",
      "docx",
      "xls",
      "xlsx",
      "ppt",
      "pptx",
      "pdf",
    ]);
    expect(FILE_SKIP_REASONS).toContain("symlink_or_reparse_point");
  });
});

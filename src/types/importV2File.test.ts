import { describe, expect, it } from "vitest";

import { FILE_FORMATS, FILE_SKIP_REASONS } from "./importV2File";

describe("Import V2 file contracts", () => {
  it("keeps stable format and skip reason wire names", () => {
    expect(FILE_FORMATS).toEqual([
      "markdown",
      "text",
      "html",
      "csv",
      "doc",
      "docx",
      "xls",
      "xlsx",
      "ppt",
      "pptx",
      "pdf",
      "png",
      "jpeg",
      "webp",
      "bmp",
      "tiff",
      "heic",
      "heif",
      "animated_gif",
      "mp3",
      "wav",
      "m4a",
      "aac",
      "flac",
      "ogg",
      "opus",
      "wma",
      "mp4",
      "mov",
      "mkv",
      "webm",
      "avi",
      "m4v",
      "wmv",
      "srt",
      "vtt",
      "ass",
      "lrc",
    ]);
    expect(FILE_SKIP_REASONS).toContain("symlink_or_reparse_point");
  });
});

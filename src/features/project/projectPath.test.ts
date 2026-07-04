import { describe, expect, it } from "vitest";

import { buildProjectRootPath, sanitizeProjectFolderName } from "./projectPath";

describe("sanitizeProjectFolderName", () => {
  it("keeps CJK names and trims whitespace", () => {
    expect(sanitizeProjectFolderName("  知识库 项目  ")).toBe("知识库 项目");
  });

  it("removes path separators and Windows-invalid filename characters", () => {
    expect(sanitizeProjectFolderName("agent/wiki:2026?")).toBe("agentwiki2026");
  });

  it("returns an empty string when no valid folder characters remain", () => {
    expect(sanitizeProjectFolderName("///:::")).toBe("");
  });
});

describe("buildProjectRootPath", () => {
  it("joins Windows parent paths with backslashes", () => {
    expect(buildProjectRootPath("D:\\资料", "知识库")).toBe("D:\\资料\\知识库");
  });

  it("joins POSIX parent paths with slashes", () => {
    expect(buildProjectRootPath("/Users/aletta/wiki", "agent")).toBe(
      "/Users/aletta/wiki/agent",
    );
  });

  it("does not duplicate trailing separators", () => {
    expect(buildProjectRootPath("D:\\资料\\", "知识库")).toBe("D:\\资料\\知识库");
    expect(buildProjectRootPath("/tmp/wiki/", "agent")).toBe("/tmp/wiki/agent");
  });
});

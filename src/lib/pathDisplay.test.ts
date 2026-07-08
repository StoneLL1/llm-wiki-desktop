import { describe, expect, it } from "vitest";
import { compactPath, pathBasename } from "./pathDisplay";

describe("pathBasename", () => {
  it("returns the last segment of a posix path", () => {
    expect(pathBasename("exports/html/agent-1.html")).toBe("agent-1.html");
  });

  it("handles windows backslash paths", () => {
    expect(pathBasename("D:\\Users\\Aletta\\agent.md")).toBe("agent.md");
  });

  it("handles mixed separators", () => {
    expect(pathBasename("exports\\html/agent-1.html")).toBe("agent-1.html");
  });

  it("returns the whole string when there is no separator", () => {
    expect(pathBasename("agent.md")).toBe("agent.md");
  });

  it("strips a trailing slash and returns the last non-empty segment", () => {
    expect(pathBasename("exports/html/")).toBe("html");
  });

  it("ignores a leading slash", () => {
    expect(pathBasename("/agent.md")).toBe("agent.md");
  });

  it("preserves CJK filenames", () => {
    expect(pathBasename("导出/html/智能体.html")).toBe("智能体.html");
  });

  it("returns empty string for empty input", () => {
    expect(pathBasename("")).toBe("");
  });
});

describe("compactPath", () => {
  it("keeps short paths unchanged", () => {
    expect(compactPath("D:/wiki")).toBe("D:/wiki");
  });

  it("compacts Windows drive paths", () => {
    expect(compactPath("D:/Users/Aletta/Documents/wiki/agent-llm")).toBe("D:/.../wiki/agent-llm");
  });

  it("compacts UNC paths without losing server and share", () => {
    expect(compactPath("//server/share/team/wiki/project")).toBe("//server/share/.../wiki/project");
  });

  it("compacts POSIX paths", () => {
    expect(compactPath("/Users/aletta/Documents/wiki/agent-llm")).toBe("/.../wiki/agent-llm");
  });

  it("preserves CJK leaf names", () => {
    expect(compactPath("D:/知识库/研究/智能体项目")).toBe("D:/知识库/研究/智能体项目");
  });
});

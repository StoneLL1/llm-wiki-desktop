import { describe, expect, it } from "vitest";
import { compactPath } from "./pathDisplay";

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

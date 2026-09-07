import { describe, expect, it } from "vitest";
import { workflowScopeEqual } from "./workflowDraft";

describe("workflow draft identity", () => {
  it("treats a changed concept-map center as a scope change", () => {
    const scope = { kind: "generate_content", artifactType: "concept_map", pagePaths: ["wiki/中心.md", "wiki/关联.md", "wiki/其他.md"], outputPath: null } as const;
    const original = { ...scope, pagePaths: [...scope.pagePaths] };
    expect(workflowScopeEqual(original, { ...original, pagePaths: ["wiki/关联.md", "wiki/中心.md", "wiki/其他.md"] })).toBe(false);
    expect(workflowScopeEqual(original, { ...original, pagePaths: ["wiki/中心.md", "wiki/其他.md", "wiki/关联.md"] })).toBe(true);
  });
});

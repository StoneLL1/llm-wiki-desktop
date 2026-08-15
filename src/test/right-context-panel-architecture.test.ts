import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const source = readFileSync(
  resolve(process.cwd(), "src/components/app/RightContextPanel.tsx"),
  "utf8",
);

describe("RightContextPanel lazy architecture", () => {
  it("keeps feature panels and stores out of the shell-level module", () => {
    for (const forbidden of [
      "features/graph",
      "features/import",
      "features/workflows",
      "features/wiki",
      "stores/chatStore",
      "stores/graphStore",
      "stores/importStore",
      "stores/taskStore",
    ]) {
      expect(source).not.toContain(forbidden);
    }
  });

  it("loads each route panel through its own lazy boundary", () => {
    for (const host of [
      "ProjectSummaryRightPanel",
      "WikiRightPanelHost",
      "ChatRightPanelHost",
      "GraphRightPanelHost",
      "ImportRightPanelHost",
      "WorkflowsRightPanelHost",
    ]) {
      expect(source).toContain(`right-panels/${host}`);
    }
    expect(source).toContain("<ViewErrorBoundary");
    expect(source).toContain("<Suspense");
  });
});

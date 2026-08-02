import { describe, expect, it } from "vitest";

import contract from "../../test-fixtures/project-layout-contract.json";
import type {
  ProjectContextDocument,
  ProjectLayout,
  ProjectLayoutConfidence,
  ProjectLayoutWarningCode,
  ProjectMarkdownRootRole,
} from "./project";

describe("Project layout shared Rust/TypeScript contract", () => {
  it("keeps the native layout fixture assignable to the frontend mirror", () => {
    const nativeLayout: ProjectLayout = {
      ...contract,
      markdownRoots: contract.markdownRoots.map((root) => ({
        ...root,
        role: root.role as ProjectMarkdownRootRole,
      })),
    };

    expect(nativeLayout.markdownRoots.map((root) => root.role)).toEqual([
      "wiki",
      "source",
      "source",
    ]);
    expect(nativeLayout.sourceWriteRoot).toBe("wiki/sources");
    expect(nativeLayout.taskStateRoot).toBe(".app/tasks");
    expect(Object.keys(nativeLayout).sort()).toEqual([
      "activityLogPath",
      "agentConfigPath",
      "appStateRoot",
      "bookmarksPath",
      "chatStateRoot",
      "compileStateRoot",
      "evidenceRoot",
      "exportRecordPath",
      "exportRoot",
      "graphCachePath",
      "importStateRoot",
      "lintIgnorePath",
      "lintReportRoot",
      "markdownRoots",
      "purposeContext",
      "queriesWriteRoot",
      "schemaContext",
      "settingsPath",
      "skillsRoot",
      "sourceStateRoot",
      "sourceWriteRoot",
      "taskStateRoot",
      "wikiIndexPath",
      "wikiOverviewPath",
      "wikiWriteRoot",
      "workflowStateRoot",
    ].sort());
  });

  it("freezes role, confidence, and warning unions", () => {
    const roles = ["source", "wiki", "mixed"] as const satisfies readonly ProjectMarkdownRootRole[];
    const confidence = ["high", "medium", "low"] as const satisfies readonly ProjectLayoutConfidence[];
    const warningCodes = [
      "LOW_CONFIDENCE",
      "DISCOVERY_LIMIT_REACHED",
      "UNSAFE_ENTRY_SKIPPED",
    ] as const satisfies readonly ProjectLayoutWarningCode[];

    expect(roles).toEqual(["source", "wiki", "mixed"]);
    expect(confidence).toEqual(["high", "medium", "low"]);
    expect(warningCodes).toHaveLength(3);

    const everyRole: Record<ProjectMarkdownRootRole, true> = {
      source: true,
      wiki: true,
      mixed: true,
    };
    const everyConfidence: Record<ProjectLayoutConfidence, true> = {
      high: true,
      medium: true,
      low: true,
    };
    const everyWarning: Record<ProjectLayoutWarningCode, true> = {
      LOW_CONFIDENCE: true,
      DISCOVERY_LIMIT_REACHED: true,
      UNSAFE_ENTRY_SKIPPED: true,
    };
    expect(Object.keys(everyRole)).toEqual(roles);
    expect(Object.keys(everyConfidence)).toEqual(confidence);
    expect(Object.keys(everyWarning)).toEqual(warningCodes);
  });

  it("allows compatible read context while keeping write paths absent", () => {
    const inferredContext = {
      readPath: "purpose.md",
      inferred: true,
    } satisfies ProjectContextDocument;
    const compatible = {
      markdownRoots: [{ path: ".", role: "mixed", exclude: [".obsidian"] }],
      wikiIndexPath: "index.md",
      purposeContext: inferredContext,
    } satisfies ProjectLayout;

    expect(compatible).not.toHaveProperty("appStateRoot");
    expect(compatible).not.toHaveProperty("wikiWriteRoot");
    expect(compatible.purposeContext).not.toHaveProperty("writePath");
  });

  it("keeps markdownRoots as the only required layout field", () => {
    const minimum = { markdownRoots: [] } satisfies ProjectLayout;
    expect(minimum.markdownRoots).toEqual([]);
  });
});

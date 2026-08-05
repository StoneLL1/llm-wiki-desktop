import { describe, expect, it } from "vitest";

import contract from "../../test-fixtures/project-layout-contract.json";
import type {
  ProjectContextDocument,
  ProjectLayout,
  ProjectLayoutConfidence,
  ProjectLayoutWarningCode,
  ProjectMarkdownRootRole,
  ProjectCapability,
  ProjectFilesystemAccess,
  ProjectFormat,
  ProjectHealth,
  ProjectRepairOperation,
  ProjectRepairOperationType,
  ProjectTrustState,
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

  it("keeps compatible app state distinct from writable content roots", () => {
    const enabledCompatible = {
      appStateRoot: ".app",
      markdownRoots: [{ path: ".", role: "mixed" }],
      purposeContext: { readPath: ".app/compat/purpose.md", writePath: ".app/compat/purpose.md", inferred: false },
    } satisfies ProjectLayout;
    expect(enabledCompatible).toHaveProperty("appStateRoot", ".app");
    expect(enabledCompatible).not.toHaveProperty("taskStateRoot");
    expect(enabledCompatible).not.toHaveProperty("sourceWriteRoot");
  });

  it("freezes every typed assessment dimension independently", () => {
    const formats = [
      "native_current", "native_legacy", "nashsu_llm_wiki", "obsidian_vault",
      "markdown_vault", "ambiguous_markdown", "ordinary_materials", "unknown",
    ] as const satisfies readonly ProjectFormat[];
    const trust = ["trusted", "untrusted"] as const satisfies readonly ProjectTrustState[];
    const access = ["writable", "read_only"] as const satisfies readonly ProjectFilesystemAccess[];
    const health = ["healthy", "repairable", "recovery", "unreadable"] as const satisfies readonly ProjectHealth[];
    const capabilities = [
      "read_markdown", "local_search", "in_memory_graph", "local_health_check",
      "external_ai", "project_write", "git_checkpoint", "enable_compatible_features",
    ] as const satisfies readonly ProjectCapability[];

    expect(formats).toHaveLength(8);
    expect(trust).toEqual(["trusted", "untrusted"]);
    expect(access).toEqual(["writable", "read_only"]);
    expect(health).toHaveLength(4);
    expect(capabilities).toHaveLength(8);
  });

  it("models directory repairs without fake cache backup or hash fields", () => {
    const operationTypes = ["regenerate_graph_cache", "create_directory"] as const satisfies readonly ProjectRepairOperationType[];
    const directoryRepair = {
      operationType: "create_directory",
      targetPath: ".app/tasks",
      allowlistDescriptor: "native-layout-v1",
    } satisfies ProjectRepairOperation;

    expect(operationTypes).toHaveLength(2);
    expect(directoryRepair).not.toHaveProperty("backupPath");
    expect(directoryRepair).not.toHaveProperty("expectedHash");
  });
});

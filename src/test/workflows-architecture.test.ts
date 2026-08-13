import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, join, relative } from "node:path";
import { describe, expect, it } from "vitest";

interface SourceFile {
  path: string;
  source: string;
}

const root = process.cwd();

const walkProductionSources = (directory: string): string[] => {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return walkProductionSources(path);
    if (!/\.(ts|tsx)$/.test(entry.name) || entry.name.includes(".test.")) return [];
    return [path];
  });
};

const currentWorkflowSources = (): SourceFile[] =>
  walkProductionSources(join(root, "src"))
    .filter((path) => {
      const normalized = relative(root, path).replaceAll("\\", "/");
      return (
        normalized.includes("/features/workflows/") ||
        /^workflow/i.test(basename(path))
      );
    })
    .map((path) => ({
      path: relative(root, path).replaceAll("\\", "/"),
      source: readFileSync(path, "utf8"),
    }));

const workflowApiPath = "src/services/workflowApi.ts";
const workflowCommands = [
  "get_workflows_overview",
  "prepare_workflow",
  "start_workflow",
  "list_workflow_runs",
  "get_workflow_run",
  "get_workflow_file_diff",
  "cancel_workflow_run",
  "undo_cancel_queued_workflow",
  "reorder_queued_workflow",
  "continue_queued_workflows",
  "retry_workflow",
  "confirm_workflow_action",
  "discard_workflow_result",
  "rollback_agent_lint_repair",
] as const;

const workflowApiExports = [
  "getWorkflowsOverview",
  "prepareWorkflow",
  "startWorkflow",
  "listWorkflowRuns",
  "getWorkflowRun",
  "getWorkflowFileDiff",
  "cancelWorkflowRun",
  "undoCancelQueuedWorkflow",
  "reorderQueuedWorkflow",
  "continueQueuedWorkflows",
  "retryWorkflow",
  "confirmWorkflowAction",
  "discardWorkflowResult",
  "rollbackAgentLintRepair",
] as const;

const rustAuthoritySources = (): SourceFile[] => [
  join(root, "src-tauri", "src", "commands", "task_commands.rs"),
  join(root, "src-tauri", "src", "commands", "workflow_commands.rs"),
  join(root, "src-tauri", "src", "tasks", "task_service.rs"),
  ...readdirSync(join(root, "src-tauri", "src", "services", "workflow_service"), {
    recursive: true,
    withFileTypes: true,
  })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .map((entry) => join(entry.parentPath, entry.name)),
].map((path) => ({
  path: relative(root, path).replaceAll("\\", "/"),
  source: readFileSync(path, "utf8"),
}));

const rustWorkflowCommandSource = (): string =>
  readFileSync(
    join(root, "src-tauri", "src", "commands", "workflow_commands.rs"),
    "utf8",
  );

const commandBody = (source: string, name: string, nextName: string): string => {
  const start = source.indexOf(`pub fn ${name}(`);
  const end = source.indexOf(`pub fn ${nextName}(`, start + 1);
  return start >= 0 && end > start ? source.slice(start, end) : "";
};

const workflowAuthorityViolations = (files: SourceFile[]): string[] => {
  const violations: string[] = [];
  const forbiddenAuthorityCalls = /\b(?:grant_compatible_project_trust|revoke_project_trust|register_trusted_native|register_trusted_compatible(?:_with_identity)?|revoke_trust|initialize_git_repository|initialize_repository|start_project_open_assessment|assess_project_folder)\s*\(/g;
  const forbiddenAuthorityDerivation = /\b(?:resolve_authority|filesystem_access|has_writable_task_state_root)\s*\(|\b(?:ProjectTrustAuthority|ProjectFilesystemAccess|ProjectTrustState)::|permissions\(\)\.readonly\(\)/g;
  const forbiddenGitDerivation = /\brepository_status(?:_for_assessment)?\s*\(/g;
  const checkpointRevalidationCallCounts: Record<string, number> = {
    "src-tauri/src/services/workflow_service/runners/update_wiki.rs": 3,
    "src-tauri/src/services/workflow_service/runners/agent_lint_repair.rs": 3,
  };
  for (const file of files) {
    const { path, source } = file;
    const productionSource = source.split(/\r?\n#\[cfg\(test\)\]\r?\n/, 1)[0];
    if (forbiddenAuthorityCalls.test(productionSource)) {
      violations.push(`${path}: derives or mutates project authority`);
    }
    forbiddenAuthorityCalls.lastIndex = 0;
    const gitDerivationCount = [...productionSource.matchAll(forbiddenGitDerivation)].length;
    const allowedGitDerivationCount = checkpointRevalidationCallCounts[path] ?? 0;
    if (
      forbiddenAuthorityDerivation.test(productionSource) ||
      gitDerivationCount !== allowedGitDerivationCount
    ) {
      violations.push(`${path}: derives trust, writability, or Git state`);
    }
    forbiddenAuthorityDerivation.lastIndex = 0;
    if (
      path.endsWith("task_commands.rs")
      && /(?:app_dir\.join\(\s*"tasks"\s*\)|join\(\s*"\.app"\s*\)\.join\(\s*"tasks"\s*\))/.test(source)
    ) {
      violations.push(`${path}: hand-built task persistence root`);
    }
    if (
      path.endsWith("coordinator.rs")
      && /task_state_root:\s*tasks\.workflow_persistence_dir\(task_id\)/.test(source)
    ) {
      violations.push(`${path}: retry reuses prior task persistence root`);
    }
  }
  return violations;
};

const workflowApiViolations = (source: string): string[] => {
  const violations: string[] = [];
  const invokeCalls = [...source.matchAll(/\binvoke(?:<[^>]+>)?\s*\(/g)];
  const literalCommands = [
    ...source.matchAll(/\binvoke(?:<[^>]+>)?\s*\(\s*["']([^"']+)["']/g),
  ].map((match) => match[1]);
  const exportedValues = [
    ...source.matchAll(
      /\bexport\s+(?:async\s+)?(?:function|const|let|var|class)\s+([A-Za-z0-9_]+)/g,
    ),
  ].map((match) => match[1]);

  if (invokeCalls.length !== workflowCommands.length) {
    violations.push(`${workflowApiPath}: non-literal or extra invoke call`);
  }
  if ([...literalCommands].sort().join("\n") !== [...workflowCommands].sort().join("\n")) {
    violations.push(`${workflowApiPath}: command literal set drift`);
  }
  if ([...exportedValues].sort().join("\n") !== [...workflowApiExports].sort().join("\n")) {
    violations.push(`${workflowApiPath}: exported API set drift`);
  }
  if (/\bexport\s*(?:\{|\*|default\b)/.test(source)) {
    violations.push(`${workflowApiPath}: re-export or default escape hatch`);
  }
  return violations;
};

const architectureViolations = (files: SourceFile[]): string[] => {
  const violations: string[] = [];
  for (const file of files) {
    const { path, source } = file;
    if (/from\s+["'](?:node:fs(?:\/promises)?|fs(?:\/promises)?|@tauri-apps\/plugin-fs)["']/.test(source)) {
      violations.push(`${path}: filesystem import`);
    }
    if (/\b(?:prompt|promptText|customPrompt|instructions|customInstructions)\??\s*:\s*string\b/.test(source)) {
      violations.push(`${path}: arbitrary prompt or instructions`);
    }
    if (/\b(?:command|shellCommand|commandLine|rawCommand)\??\s*(?::\s*string\b|=)/.test(source)) {
      violations.push(`${path}: arbitrary command string`);
    }
    if (/\b(?:ProviderStatus|hasSecret|secretMask|apiKey|apiToken|password|credential)\b/i.test(source)) {
      violations.push(`${path}: Provider secret surface`);
    }
    if (
      path !== "src/services/workflowApi.ts" &&
      (source.includes("@tauri-apps/api/core") || /\binvoke\s*\(/.test(source))
    ) {
      violations.push(`${path}: direct IPC outside workflowApi`);
    }
    if (
      /(?:node:child_process|@tauri-apps\/plugin-shell)/.test(source) ||
      /\b(?:Command\.create|spawn)\s*\(/.test(source)
    ) {
      violations.push(`${path}: process execution`);
    }
    if (path === workflowApiPath) violations.push(...workflowApiViolations(source));
  }
  return violations;
};

describe("Workflows architecture", () => {
  it("keeps Workflows contracts away from filesystem, arbitrary input, secrets, and direct IPC", () => {
    expect(architectureViolations(currentWorkflowSources())).toEqual([]);
  });

  it("rejects arbitrary prompt and shell-shaped contract fields", () => {
    expect(
      architectureViolations([
        {
          path: "synthetic-workflow.ts",
          source: "interface UnsafeWorkflow { prompt: string; shellCommand: string }",
        },
      ]),
    ).toEqual([
      "synthetic-workflow.ts: arbitrary prompt or instructions",
      "synthetic-workflow.ts: arbitrary command string",
    ]);
  });

  it("rejects a generic IPC escape hatch beside the frozen wrappers", () => {
    const api = currentWorkflowSources().find(({ path }) => path === workflowApiPath);
    expect(api).toBeDefined();
    const unsafe = `${api?.source ?? ""}\nexport function raw(name: string, payload: unknown) { return invoke(name, payload); }`;
    expect(workflowApiViolations(unsafe)).toEqual([
      `${workflowApiPath}: non-literal or extra invoke call`,
      `${workflowApiPath}: exported API set drift`,
    ]);
  });

  it("rejects invoke aliases exported through an export list", () => {
    const api = currentWorkflowSources().find(({ path }) => path === workflowApiPath);
    expect(api).toBeDefined();
    const unsafe = `${api?.source ?? ""}\nexport { invoke as rawInvoke };`;
    expect(workflowApiViolations(unsafe)).toEqual([
      `${workflowApiPath}: re-export or default escape hatch`,
    ]);
  });

  it("keeps project trust, writability, Git, assessment, and persistence derivation outside Workflows", () => {
    expect(workflowAuthorityViolations(rustAuthoritySources())).toEqual([]);
  });

  it("keeps authority-sensitive start and confirmation inside the project transition lock", () => {
    const source = rustWorkflowCommandSource();
    expect(commandBody(source, "start_workflow", "list_workflow_runs")).toContain(
      "with_workflow_access",
    );
    expect(commandBody(source, "confirm_workflow_action", "discard_workflow_result")).toContain(
      "with_workflow_access",
    );
  });

  it("rejects authority mutation and cached persistence reuse in synthetic Rust sources", () => {
    expect(workflowAuthorityViolations([
      {
        path: "src-tauri/src/services/workflow_service/unsafe.rs",
        source: "grant_compatible_project_trust();",
      },
      {
        path: "src-tauri/src/services/workflow_service/derived.rs",
        source: "filesystem_access(); repository_status();",
      },
      {
        path: "src-tauri/src/tasks/task_service.rs",
        source: "ProjectTrustAuthority::TrustedNative;",
      },
      {
        path: "src-tauri/src/commands/task_commands.rs",
        source: 'let root = context.app_dir.join("tasks"); resolve_authority();',
      },
      {
        path: "src-tauri/src/commands/workflow_commands.rs",
        source: 'ProjectTrustAuthority::TrustedNative; permissions().readonly(); repository_status();',
      },
      {
        path: "src-tauri/src/services/workflow_service/coordinator.rs",
        source: "task_state_root: tasks.workflow_persistence_dir(task_id),",
      },
    ])).toEqual([
      "src-tauri/src/services/workflow_service/unsafe.rs: derives or mutates project authority",
      "src-tauri/src/services/workflow_service/derived.rs: derives trust, writability, or Git state",
      "src-tauri/src/tasks/task_service.rs: derives trust, writability, or Git state",
      "src-tauri/src/commands/task_commands.rs: derives trust, writability, or Git state",
      "src-tauri/src/commands/task_commands.rs: hand-built task persistence root",
      "src-tauri/src/commands/workflow_commands.rs: derives trust, writability, or Git state",
      "src-tauri/src/services/workflow_service/coordinator.rs: retry reuses prior task persistence root",
    ]);
  });

  it("keeps Workflows as the only user-facing workflow route", () => {
    const navigation = readFileSync(
      join(root, "src", "stores", "navigationStore.ts"),
      "utf8",
    );
    const router = readFileSync(
      join(root, "src", "components", "app", "WorkspaceRouter.tsx"),
      "utf8",
    );
    const controller = readFileSync(
      join(root, "src", "components", "app", "WorkspaceController.tsx"),
      "utf8",
    );
    const rightPanel = readFileSync(
      join(root, "src", "components", "app", "RightContextPanel.tsx"),
      "utf8",
    );
    const launcher = readFileSync(
      join(root, "src", "hooks", "useTaskLauncher.ts"),
      "utf8",
    );
    const lintView = readFileSync(
      join(root, "src", "features", "lint", "LintView.tsx"),
      "utf8",
    );

    expect(navigation).not.toContain('| "agent"');
    expect(navigation).toContain("workflowLaunchIntent");
    expect(navigation).toContain("requestWorkflowLaunch");
    expect(navigation).toContain('| "workflows"');
    expect(router).not.toContain('case "agent"');
    expect(router).not.toContain("features/agent/AgentView");
    expect(router).toContain('case "workflows"');
    expect(router).toContain("features/workflows/WorkflowsView");
    expect(controller).not.toContain("RunAgentDialog");
    expect(controller).not.toContain("useAgentWorkflow");
    expect(rightPanel).not.toContain("AgentRightPanel");
    expect(existsSync(join(root, "src", "features", "agent", "AgentView.tsx"))).toBe(false);
    expect(existsSync(join(root, "src", "features", "agent", "RunAgentDialog.tsx"))).toBe(false);
    expect(launcher).not.toContain('"start_wiki_compile"');
    expect(launcher).not.toContain('"start_deep_lint"');
    expect(launcher).not.toContain('"start_export"');
    expect(lintView).not.toContain("startCompile");
    expect(lintView).toContain('kind: "update_wiki"');
    expect(lintView).toContain("requestWorkflowLaunch");
  });

  it("rejects extra Git derivation even inside checkpoint-revalidating runners", () => {
    expect(workflowAuthorityViolations([{
      path: "src-tauri/src/services/workflow_service/runners/agent_lint_repair.rs",
      source: "repository_status();\nrepository_status();\nrepository_status();\nrepository_status();",
    }])).toEqual([
      "src-tauri/src/services/workflow_service/runners/agent_lint_repair.rs: derives trust, writability, or Git state",
    ]);
  });

  it("keeps Wiki quick exports local while Exports and Workflows retain preparation", () => {
    const wikiView = readFileSync(
      join(root, "src", "features", "wiki", "WikiView.tsx"),
      "utf8",
    );
    const rightPanelSource = readFileSync(
      join(root, "src", "components", "app", "RightContextPanel.tsx"),
      "utf8",
    );
    const exportsView = readFileSync(
      join(root, "src", "features", "exports", "ExportsView.tsx"),
      "utf8",
    );
    const workflowsOverview = readFileSync(
      join(root, "src", "features", "workflows", "WorkflowsOverview.tsx"),
      "utf8",
    );
    const exportTypes = readFileSync(
      join(root, "src", "types", "export.ts"),
      "utf8",
    );
    const singlePageTypes =
      exportTypes.match(
        /export const SINGLE_PAGE_EXPORT_TYPES: ExportType\[] = \[([\s\S]*?)\n\];/,
      )?.[1] ?? "";

    const wikiQuickExportSection =
      wikiView.match(
        /const handleGenerateHtml =([\s\S]*?)const handleDialogGenerate/,
      )?.[0] ?? "";
    expect(wikiQuickExportSection).not.toContain("requestWorkflowLaunch");
    expect(wikiView).toContain("consumeExportRequest");
    expect(rightPanelSource).not.toContain("requestWorkflowLaunch");
    expect(rightPanelSource).toContain("requestExport");
    expect(exportsView).toContain("requestWorkflowLaunch");
    expect(exportsView).toContain('origin: "exports"');
    expect(workflowsOverview).toContain("onPrepare={() => onPrepare(kind)}");
    expect(exportTypes).toContain("export type ExportType");
    expect(singlePageTypes).not.toContain('"project_report"');
  });
});

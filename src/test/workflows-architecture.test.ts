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
  "cancel_workflow_run",
  "undo_cancel_queued_workflow",
  "reorder_queued_workflow",
  "continue_queued_workflows",
  "retry_workflow",
  "confirm_workflow_action",
  "discard_workflow_result",
] as const;

const workflowApiExports = [
  "getWorkflowsOverview",
  "prepareWorkflow",
  "startWorkflow",
  "listWorkflowRuns",
  "getWorkflowRun",
  "cancelWorkflowRun",
  "undoCancelQueuedWorkflow",
  "reorderQueuedWorkflow",
  "continueQueuedWorkflows",
  "retryWorkflow",
  "confirmWorkflowAction",
  "discardWorkflowResult",
] as const;

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

  it("adds the Workflows route while preserving the legacy Agent compatibility surface", () => {
    const navigation = readFileSync(
      join(root, "src", "stores", "navigationStore.ts"),
      "utf8",
    );
    const router = readFileSync(
      join(root, "src", "components", "app", "WorkspaceRouter.tsx"),
      "utf8",
    );
    const dialog = readFileSync(
      join(root, "src", "features", "agent", "RunAgentDialog.tsx"),
      "utf8",
    );
    const launcher = readFileSync(
      join(root, "src", "hooks", "useTaskLauncher.ts"),
      "utf8",
    );

    expect(navigation).toContain('| "agent"');
    expect(navigation).toContain("workflowLaunchIntent");
    expect(navigation).toContain("requestWorkflowLaunch");
    expect(navigation).toContain('| "workflows"');
    expect(router).toContain('case "agent"');
    expect(router).toContain("features/agent/AgentView");
    expect(router).toContain('case "workflows"');
    expect(router).toContain("features/workflows/WorkflowsView");
    expect(dialog).toContain('"wiki-ingest"');
    expect(dialog).toContain('"html-project-report"');
    expect(launcher).toContain('"start_wiki_compile"');
    expect(launcher).toContain('"start_deep_lint"');
    expect(launcher).toContain('"start_export"');
  });
});

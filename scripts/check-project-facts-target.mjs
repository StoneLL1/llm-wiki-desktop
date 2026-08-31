import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

import {
  assertProjectFactsP0Target,
  inspectCommandExecution,
} from "./tauri-command-execution.mjs";

const repositoryRoot = path.join(import.meta.dirname, "..");
let commandTargetPassed = true;
try {
  assertProjectFactsP0Target(await inspectCommandExecution(repositoryRoot));
} catch (error) {
  commandTargetPassed = false;
  process.stderr.write(`Project Facts command target is red: ${error.message}\n`);
}

const frontend = spawnSync(
  process.execPath,
  [
    path.join(repositoryRoot, "node_modules", "vitest", "vitest.mjs"),
    "run",
    "src/hooks/useProjectStatus.test.tsx",
    "src/hooks/useAiCapabilities.test.tsx",
    "src/stores/projectFactsStore.test.ts",
    "src/components/app/appShellActions.test.tsx",
    "src/stores/projectStore.test.ts",
    "--reporter=verbose",
  ],
  {
    cwd: repositoryRoot,
    env: { ...process.env, LLM_WIKI_PROJECT_FACTS_TARGET: "1" },
    stdio: "inherit",
    windowsHide: true,
  },
);

if (frontend.error) {
  process.stderr.write(`Project Facts frontend target could not run: ${frontend.error.message}\n`);
}
if (!commandTargetPassed || frontend.status !== 0) process.exitCode = 1;

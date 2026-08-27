import path from "node:path";

import { assertInventoryComplete, inspectCommandExecution } from "./tauri-command-execution.mjs";

const repositoryRoot = path.join(import.meta.dirname, "..");
const result = await inspectCommandExecution(repositoryRoot);
assertInventoryComplete(result);

const baseline = result.inventory.baseline;
for (const [key, value] of Object.entries(result.counts)) {
  if (baseline[key] !== value) {
    throw new Error(`Command execution baseline drift for ${key}: inventory=${baseline[key]} actual=${value}`);
  }
}

process.stdout.write(`${JSON.stringify({ status: baseline.status, ...result.counts })}\n`);

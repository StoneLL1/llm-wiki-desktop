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

if (baseline.status !== "batch1_import_green") {
  throw new Error(`Expected the Batch 1 Import execution gate, received ${baseline.status}.`);
}
if (result.counts.importSync !== 0) {
  throw new Error(`Synchronous registered Import commands remain: ${result.counts.importSync}`);
}
if (!Number.isInteger(result.inventory.legacyBlockingSyncCeiling)) {
  throw new Error("legacyBlockingSyncCeiling must be an explicit integer.");
}
if (result.counts.blockingSync > result.inventory.legacyBlockingSyncCeiling) {
  throw new Error(
    `Blocking-sync debt increased: ceiling=${result.inventory.legacyBlockingSyncCeiling} actual=${result.counts.blockingSync}`,
  );
}

process.stdout.write(`${JSON.stringify({ status: baseline.status, ...result.counts })}\n`);

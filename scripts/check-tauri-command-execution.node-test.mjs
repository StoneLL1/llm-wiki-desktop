import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import { assertInventoryComplete, inspectCommandExecution } from "./tauri-command-execution.mjs";

const repositoryRoot = path.join(import.meta.dirname, "..");

test("every registered Tauri command has one explicit execution classification", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  assert.doesNotThrow(() => assertInventoryComplete(result));
  assert.equal(result.registered.length, result.inventory.commands.length);
});

test("Batch 0 records the current synchronous red baseline without weakening the future gate", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  assert.deepEqual(result.counts, result.inventory.baseline.counts ?? {
    total: result.inventory.baseline.total,
    sync: result.inventory.baseline.sync,
    async: result.inventory.baseline.async,
    importTotal: result.inventory.baseline.importTotal,
    importSync: result.inventory.baseline.importSync,
    blockingSync: result.inventory.baseline.blockingSync,
  });
  assert.equal(result.inventory.baseline.status, "red");
  assert.ok(result.counts.blockingSync > 0, "Batch 1 owns making the blocking-sync gate green");
});

test("the named P0 Import commands are classified as blocking work and remain visible in the red ledger", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  const byName = new Map(result.inventory.commands.map((entry) => [entry.command, entry]));
  for (const command of result.inventory.p0Commands) {
    const entry = byName.get(command);
    assert.ok(entry, `missing P0 command ${command}`);
    assert.notEqual(entry.classification, "PureMemory");
    assert.equal(entry.currentExecution, "sync");
  }
});

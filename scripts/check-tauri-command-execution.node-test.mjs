import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import { assertInventoryComplete, inspectCommandExecution } from "./tauri-command-execution.mjs";

const repositoryRoot = path.join(import.meta.dirname, "..");

test("every registered Tauri command has one explicit execution classification", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  assert.doesNotThrow(() => assertInventoryComplete(result));
  assert.equal(result.registered.length, result.inventory.commands.length);
});

test("Batch 1 keeps the explicit execution ledger synchronized with the registered commands", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  assert.deepEqual(result.counts, result.inventory.baseline.counts ?? {
    total: result.inventory.baseline.total,
    sync: result.inventory.baseline.sync,
    async: result.inventory.baseline.async,
    importTotal: result.inventory.baseline.importTotal,
    importSync: result.inventory.baseline.importSync,
    blockingSync: result.inventory.baseline.blockingSync,
  });
  assert.equal(result.inventory.baseline.status, "batch1_import_green");
  assert.equal(result.inventory.legacyBlockingSyncCeiling, 133);
  assert.ok(result.counts.blockingSync <= result.inventory.legacyBlockingSyncCeiling);
});

test("the named P0 Import commands are classified as blocking work and execute asynchronously", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  const byName = new Map(result.inventory.commands.map((entry) => [entry.command, entry]));
  const signatures = new Map(result.registered.map((entry) => [entry.command, entry]));
  for (const command of result.inventory.p0Commands) {
    const entry = byName.get(command);
    assert.ok(entry, `missing P0 command ${command}`);
    assert.notEqual(entry.classification, "PureMemory");
    assert.equal(signatures.has(command), true);
    assert.equal(entry.currentExecution, "async");
  }
});

test("every registered Import command enters through an async boundary", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  assert.equal(result.counts.importTotal, 60);
  assert.equal(result.counts.importSync, 0);
});

test("blocking Import commands enter the shared bounded runtime while network commands stay async", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  const source = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "import_v2_async_commands.rs"),
    "utf8",
  );
  const byName = new Map(result.inventory.commands.map((entry) => [entry.command, entry]));
  for (const command of result.registered.filter((entry) => entry.module === "import_v2_async_commands")) {
    const escaped = command.command.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const body = source.match(new RegExp(`pub async fn ${escaped}\\b[\\s\\S]*?\\n}`))?.[0];
    assert.ok(body, `missing async wrapper body for ${command.command}`);
    if (byName.get(command.command)?.classification === "AsyncNetwork") {
      assert.doesNotMatch(body, /blocking!\(/, `${command.command} must remain native async network work`);
    } else {
      assert.match(body, /blocking!\(/, `${command.command} bypasses the bounded blocking runtime`);
    }
  }

  const networkSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "import_v2_web_commands.rs"),
    "utf8",
  );
  for (const command of ["discover_import_collection_v2", "authorize_import_private_target_v2"]) {
    const escaped = command.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const body = networkSource.match(new RegExp(`pub async fn ${escaped}\\b[\\s\\S]*?\\n}`))?.[0];
    assert.ok(body, `missing native async implementation for ${command}`);
    assert.ok(
      (body.match(/BlockingWorkClass::HeavyIo/g) ?? []).length >= 2,
      `${command} must use blocking preflight and finalize phases around native network await`,
    );
  }
});

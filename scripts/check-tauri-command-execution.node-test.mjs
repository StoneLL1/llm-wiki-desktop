import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import {
  assertInventoryComplete,
  assertProjectFactsP0Target,
  inspectCommandExecution,
  refreshInventorySnapshotCounts,
} from "./tauri-command-execution.mjs";

const repositoryRoot = path.join(import.meta.dirname, "..");

test("inventory regeneration preserves the reviewed current-snapshot binding", () => {
  const currentSnapshot = {
    status: "project_facts_batch4_green",
    measuredAt: "2026-08-29",
    sourceCommit: "CONTAINING_COMMIT",
    total: 1,
    sync: 1,
  };
  assert.deepEqual(
    refreshInventorySnapshotCounts(currentSnapshot, { total: 205, sync: 130 }),
    {
      ...currentSnapshot,
      total: 205,
      sync: 130,
    },
  );
});

test("every registered Tauri command has one explicit execution classification", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  assert.doesNotThrow(() => assertInventoryComplete(result));
  assert.equal(result.registered.length, result.inventory.commands.length);
});

test("Batch 1 keeps the explicit execution ledger synchronized with the registered commands", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  const gateSource = await readFile(
    path.join(repositoryRoot, "scripts", "check-tauri-command-execution.mjs"),
    "utf8",
  );
  assert.deepEqual(result.counts, {
    total: result.inventory.currentSnapshot.total,
    sync: result.inventory.currentSnapshot.sync,
    async: result.inventory.currentSnapshot.async,
    importTotal: result.inventory.currentSnapshot.importTotal,
    importSync: result.inventory.currentSnapshot.importSync,
    blockingSync: result.inventory.currentSnapshot.blockingSync,
  });
  assert.equal(result.inventory.baseline.status, "project_facts_batch1_green");
  assert.equal(result.inventory.currentSnapshot.status, "project_facts_batch4_green");
  assert.equal(result.inventory.currentSnapshot.sourceCommit, "CONTAINING_COMMIT");
  assert.match(gateSource, /currentSnapshot\.status !== "project_facts_batch4_green"/);
  assert.equal(result.inventory.legacyBlockingSyncCeiling, 130);
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

test("Batch 1 closes the Project Facts P0 async execution target", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  const target = result.inventory.projectFactsBatch0Target;
  assert.deepEqual(result.inventory.projectFactsP0Commands, [
    "git_status",
    "detect_agents",
    "list_llm_providers",
  ]);
  assert.deepEqual(target, {
    status: "green",
    requiredExecution: "async",
    reviewedCommandTotal: 205,
    targetBlockingSyncCeiling: 130,
  });
  assert.equal(result.counts.total, target.reviewedCommandTotal);
  assert.ok(result.counts.blockingSync <= target.targetBlockingSyncCeiling);

  const byName = new Map(result.inventory.commands.map((entry) => [entry.command, entry]));
  for (const command of result.inventory.projectFactsP0Commands) {
    const entry = byName.get(command);
    assert.ok(entry, `missing Project Facts P0 command ${command}`);
    assert.notEqual(entry.classification, "PureMemory");
    assert.equal(entry.currentExecution, "async");
  }
  assert.doesNotThrow(() => assertProjectFactsP0Target(result));
});

test("Project Facts commands use the named bounded worker lanes", async () => {
  const runtimeSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "runtime.rs"),
    "utf8",
  );
  assert.match(runtimeSource, /pub async fn run_blocking_named\b/);
  assert.match(runtimeSource, /coordinator\s*\.run_named\(class, operation_label,/);
  const coordinatorSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "services", "blocking_work.rs"),
    "utf8",
  );
  assert.match(
    coordinatorSource,
    /run_project_facts_agent[\s\S]*?BlockingWorkClass::ProcessProbe[\s\S]*?ProjectFactsAgentDetection/,
  );
  assert.match(
    coordinatorSource,
    /run_project_facts_provider[\s\S]*?BlockingWorkClass::HeavyIo[\s\S]*?ProjectFactsProviderStatus/,
  );
  assert.match(
    coordinatorSource,
    /run_project_facts_git[\s\S]*?BlockingWorkClass::MetadataIo[\s\S]*?ProjectFactsGitStatus[\s\S]*?run_project_git_named/,
  );

  const gitSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "git_commands.rs"),
    "utf8",
  );
  const gitBody = gitSource.match(/pub async fn git_status\b[\s\S]*?\n}/)?.[0];
  assert.ok(gitBody, "git_status must remain an async command");
  assert.match(gitBody, /run_project_facts_git/);
  assert.match(gitBody, /project_identity/);
  assert.match(gitBody, /repository_status/);

  const agentSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "agent_commands.rs"),
    "utf8",
  );
  const agentBody = agentSource.match(/pub async fn detect_agents\b[\s\S]*?\n}/)?.[0];
  assert.ok(agentBody, "detect_agents must remain an async command");
  assert.match(agentBody, /run_project_facts_agent/);
  const invalidationIndex = agentBody.indexOf("invalidate_workflow_route_cache");
  const detectionIndex = agentBody.indexOf("agent_service.detect_agents");
  assert.ok(invalidationIndex >= 0, "force refresh invalidation must remain present");
  assert.ok(detectionIndex >= 0, "Agent probing must remain present");
  assert.ok(invalidationIndex < detectionIndex, "force refresh must invalidate before probing");

  const providerSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "llm_commands.rs"),
    "utf8",
  );
  const providerBody = providerSource.match(
    /pub async fn list_llm_providers\b[\s\S]*?\n}/,
  )?.[0];
  assert.ok(providerBody, "list_llm_providers must remain an async command");
  assert.match(providerBody, /run_project_facts_provider/);
  assert.match(providerBody, /require_external_ai_access/);
  assert.match(providerBody, /status_with_secret/);
});

test("Batch 4 offloads the evidenced project-open residual commands", async () => {
  const result = await inspectCommandExecution(repositoryRoot);
  const byName = new Map(result.inventory.commands.map((entry) => [entry.command, entry]));
  for (const command of ["open_project", "set_active_project", "list_exports"]) {
    assert.equal(byName.get(command)?.currentExecution, "async");
  }

  const taskSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "task_commands.rs"),
    "utf8",
  );
  assert.match(taskSource, /pub async fn set_active_project[\s\S]*?run_project_activation/);

  const projectSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "project_commands.rs"),
    "utf8",
  );
  assert.match(projectSource, /pub async fn open_project[\s\S]*?BlockingWorkClass::HeavyIo[\s\S]*?BlockingWorkOperation::OpenProject/);

  const exportSource = await readFile(
    path.join(repositoryRoot, "src-tauri", "src", "commands", "export_commands.rs"),
    "utf8",
  );
  assert.match(exportSource, /pub async fn list_exports[\s\S]*?BlockingWorkClass::MetadataIo[\s\S]*?BlockingWorkOperation::ListExports/);
  assert.deepEqual(result.inventory.projectFactsBatch4Target, {
    status: "green",
    residualCommands: ["open_project", "set_active_project", "list_exports"],
    targetBlockingSyncCeiling: 127,
  });
  assert.equal(result.counts.blockingSync, 127);
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

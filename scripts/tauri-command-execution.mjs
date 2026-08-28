import { readFile } from "node:fs/promises";
import path from "node:path";

export const COMMAND_CLASSES = [
  "PureMemory",
  "BlockingRead",
  "BlockingMutation",
  "AsyncNetwork",
  "LongTaskStart",
  "ProcessOrMedia",
];

const commandPattern = /commands::(?<module>[A-Za-z0-9_]+)::(?<name>[A-Za-z0-9_]+)/g;

export async function registeredCommands(repositoryRoot) {
  const libPath = path.join(repositoryRoot, "src-tauri", "src", "lib.rs");
  const source = await readFile(libPath, "utf8");
  const handler = source.match(/tauri::generate_handler!\[(?<body>[\s\S]*?)\]\)\s*\.run/);
  if (!handler?.groups?.body) throw new Error("Could not locate the Tauri generate_handler! registry.");
  return [...handler.groups.body.matchAll(commandPattern)].map((match) => ({
    module: match.groups.module,
    command: match.groups.name,
  }));
}

export async function commandSignature(repositoryRoot, entry) {
  const sourcePath = path.join(repositoryRoot, "src-tauri", "src", "commands", `${entry.module}.rs`);
  const source = await readFile(sourcePath, "utf8");
  const escaped = entry.command.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const signature = source.match(new RegExp(`^pub\\s+(?<async>async\\s+)?fn\\s+${escaped}\\s*[<(]`, "m"));
  if (!signature) throw new Error(`Registered command ${entry.module}::${entry.command} has no public function signature.`);
  return signature.groups?.async ? "async" : "sync";
}

export async function loadCommandInventory(repositoryRoot) {
  const inventoryPath = path.join(repositoryRoot, "docs", "qa", "tauri-command-execution-inventory.json");
  return JSON.parse(await readFile(inventoryPath, "utf8"));
}

export async function inspectCommandExecution(repositoryRoot) {
  const registered = await registeredCommands(repositoryRoot);
  const inventory = await loadCommandInventory(repositoryRoot);
  const byName = new Map(inventory.commands.map((entry) => [entry.command, entry]));
  const registeredNames = new Set(registered.map((entry) => entry.command));
  const missing = registered.filter((entry) => !byName.has(entry.command));
  const stale = inventory.commands.filter((entry) => !registeredNames.has(entry.command));
  const duplicates = inventory.commands
    .map((entry) => entry.command)
    .filter((command, index, commands) => commands.indexOf(command) !== index);
  const invalidClasses = inventory.commands.filter((entry) => !COMMAND_CLASSES.includes(entry.classification));
  const missingRationales = inventory.commands.filter(
    (entry) => typeof entry.rationale !== "string" || entry.rationale.trim().length === 0,
  );
  const signatures = [];
  for (const entry of registered) {
    signatures.push({ ...entry, execution: await commandSignature(repositoryRoot, entry) });
  }
  const mismatchedModules = registered.filter((entry) => byName.get(entry.command)?.module !== entry.module);
  const currentExecutionMismatches = signatures.filter(
    (entry) => byName.get(entry.command)?.currentExecution !== entry.execution,
  );
  const sync = signatures.filter((entry) => entry.execution === "sync");
  const importCommands = signatures.filter((entry) => entry.module.startsWith("import_v2"));
  const synchronousImportCommands = importCommands.filter((entry) => entry.execution === "sync");
  const blockingSync = sync.filter((entry) => byName.get(entry.command)?.classification !== "PureMemory");
  return {
    inventory,
    registered,
    missing,
    stale,
    duplicates,
    invalidClasses,
    missingRationales,
    mismatchedModules,
    currentExecutionMismatches,
    counts: {
      total: signatures.length,
      sync: sync.length,
      async: signatures.length - sync.length,
      importTotal: importCommands.length,
      importSync: synchronousImportCommands.length,
      blockingSync: blockingSync.length,
    },
  };
}

export function assertInventoryComplete(result) {
  const failures = [
    ["unclassified registered commands", result.missing],
    ["stale inventory commands", result.stale],
    ["duplicate inventory commands", result.duplicates],
    ["invalid command classes", result.invalidClasses],
    ["commands without an explicit rationale", result.missingRationales],
    ["module mismatches", result.mismatchedModules],
    ["signature mismatches", result.currentExecutionMismatches],
  ].filter(([, entries]) => entries.length > 0);
  if (failures.length === 0) return;
  throw new Error(failures.map(([label, entries]) => `${label}: ${JSON.stringify(entries)}`).join("\n"));
}

export function assertProjectFactsP0Target(result) {
  const target = result.inventory.projectFactsBatch0Target;
  const commands = result.inventory.projectFactsP0Commands;
  if (!target || !Array.isArray(commands)) {
    throw new Error("Project Facts P0 target metadata is missing from the command inventory.");
  }
  if (result.counts.total !== target.reviewedCommandTotal) {
    throw new Error(
      `Project Facts reviewed command total drifted: reviewed=${target.reviewedCommandTotal} actual=${result.counts.total}`,
    );
  }

  const byName = new Map(result.inventory.commands.map((entry) => [entry.command, entry]));
  const signatures = new Map(result.registered.map((entry) => [entry.command, entry]));
  for (const command of commands) {
    const entry = byName.get(command);
    if (!entry) throw new Error(`Project Facts P0 command is missing from inventory: ${command}`);
    if (entry.classification === "PureMemory") {
      throw new Error(`Project Facts P0 command was misclassified as PureMemory: ${command}`);
    }
    if (!signatures.has(command)) {
      throw new Error(`Project Facts P0 command is not registered: ${command}`);
    }
    if (entry.currentExecution !== target.requiredExecution) {
      throw new Error(
        `Project Facts P0 command must execute ${target.requiredExecution}: ${command} is ${entry.currentExecution}`,
      );
    }
  }
  if (result.counts.blockingSync > target.targetBlockingSyncCeiling) {
    throw new Error(
      `Project Facts blocking-sync target not met: ceiling=${target.targetBlockingSyncCeiling} actual=${result.counts.blockingSync}`,
    );
  }
}

import { writeFile } from "node:fs/promises";
import path from "node:path";

import {
  commandSignature,
  loadCommandInventory,
  refreshInventorySnapshotCounts,
  registeredCommands,
} from "./tauri-command-execution.mjs";

const repositoryRoot = path.join(import.meta.dirname, "..");
const inventoryPath = path.join(
  repositoryRoot,
  "docs",
  "qa",
  "tauri-command-execution-inventory.json",
);
const inventory = await loadCommandInventory(repositoryRoot);
const registered = await registeredCommands(repositoryRoot);
const byName = new Map(inventory.commands.map((entry) => [entry.command, entry]));

const commands = [];
for (const registeredCommand of registered) {
  const prior = byName.get(registeredCommand.command);
  if (!prior) {
    throw new Error(`Refusing to invent a classification for ${registeredCommand.command}.`);
  }
  commands.push({
    ...prior,
    module: registeredCommand.module,
    currentExecution: await commandSignature(repositoryRoot, registeredCommand),
  });
}

const sync = commands.filter((entry) => entry.currentExecution === "sync");
const importCommands = commands.filter((entry) => entry.module.startsWith("import_v2"));
const counts = {
  total: commands.length,
  sync: sync.length,
  async: commands.length - sync.length,
  importTotal: importCommands.length,
  importSync: importCommands.filter((entry) => entry.currentExecution === "sync").length,
  blockingSync: sync.filter((entry) => entry.classification !== "PureMemory").length,
};

inventory.commands = commands;
inventory.currentSnapshot = refreshInventorySnapshotCounts(inventory.currentSnapshot, counts);

await writeFile(inventoryPath, `${JSON.stringify(inventory, null, 2)}\n`, "utf8");
process.stdout.write(`${JSON.stringify(inventory.currentSnapshot)}\n`);

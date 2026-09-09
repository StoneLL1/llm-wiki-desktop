import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";

const arguments_ = process.argv.slice(2);
const repositoryRoot = path.join(import.meta.dirname, "..");
const environment = { ...process.env };
if (arguments_[0] === "dev") {
  environment.LLM_WIKI_DEV_CAPABILITIES ??= path.join(repositoryRoot, ".dev-capabilities");
}

const cli = path.join(import.meta.dirname, "..", "node_modules", "@tauri-apps", "cli", "tauri.js");
const child = spawn(process.execPath, [cli, ...arguments_], {
  cwd: repositoryRoot,
  env: environment,
  shell: false,
  stdio: "inherit",
  windowsHide: true,
});
child.once("error", (error) => {
  process.stderr.write(`tauri launcher: ${error.message}\n`);
  process.exitCode = 1;
});
child.once("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});

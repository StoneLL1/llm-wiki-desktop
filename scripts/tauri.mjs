import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";

const arguments_ = process.argv.slice(2);
if (arguments_[0] === "dev" && process.env.LLM_WIKI_SKIP_DEV_CAPABILITY !== "1") {
  const { prepareSenseVoiceDevelopmentCapability } = await import("./prepare-sensevoice-dev.mjs");
  await prepareSenseVoiceDevelopmentCapability();
}

const cli = path.join(import.meta.dirname, "..", "node_modules", "@tauri-apps", "cli", "tauri.js");
const child = spawn(process.execPath, [cli, ...arguments_], {
  cwd: path.join(import.meta.dirname, ".."),
  env: process.env,
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

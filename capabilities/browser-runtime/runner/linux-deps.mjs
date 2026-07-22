import { spawnSync } from "node:child_process";
import process from "node:process";

export function assertLinuxBrowserDependencies(
  executable,
  run = spawnSync,
  platform = process.platform,
) {
  if (platform !== "linux") return;
  const result = run("ldd", [executable], { encoding: "utf8", timeout: 15_000 });
  const output = `${result.stdout || ""}\n${result.stderr || ""}`;
  const missing = output
    .split(/\r?\n/)
    .filter((line) => /=>\s+not found\s*$/i.test(line.trim()))
    .map((line) => line.trim().split(/\s+/)[0]);
  if (result.error || result.status !== 0 || missing.length) {
    const details = missing.length ? ` Missing: ${missing.join(", ")}.` : "";
    throw new Error(
      `IMPORT_WEB_BROWSER_DEPENDENCY_MISSING: Chromium host dependencies are unavailable.${details}`,
    );
  }
}

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

const readRootFile = (path: string) =>
  readFileSync(rootPath(path), "utf8");

const rootPath = (relativePath: string) => path.join(process.cwd(), relativePath);

const workflowRunCommands = (workflow: string) =>
  workflow
    .split(/\r?\n/)
    .map((line) => line.match(/^\s+run:\s*(.+)$/)?.[1]?.trim())
    .filter((command): command is string => Boolean(command));

const workflowMatrixPlatforms = (workflow: string) => {
  const lines = workflow.split(/\r?\n/);
  const osLineIndex = lines.findIndex((line) => /^\s+os:\s*$/.test(line));

  if (osLineIndex < 0) {
    return [];
  }

  const platforms: string[] = [];

  for (const line of lines.slice(osLineIndex + 1)) {
    const platform = line.match(/^\s+-\s+([\w-]+)\s*$/)?.[1];

    if (!platform) {
      break;
    }

    platforms.push(platform);
  }

  return platforms;
};

const expectCommandsInOrder = (commands: string[], requiredCommands: string[]) => {
  let currentIndex = 0;

  for (const requiredCommand of requiredCommands) {
    const foundIndex = commands.findIndex(
      (command, index) => index >= currentIndex && command === requiredCommand,
    );

    expect(foundIndex, `missing run command: ${requiredCommand}`).toBeGreaterThanOrEqual(
      currentIndex,
    );
    currentIndex = foundIndex + 1;
  }
};

describe("CI validation contract", () => {
  it("exposes one local check command for all required gates", () => {
    const packageJson = JSON.parse(readRootFile("package.json")) as {
      scripts: Record<string, string>;
    };

    expect(packageJson.scripts.check).toBe(
      "npm run test && npm run lint && npm run build && npm run check:console && npm run check:rust:gui && npm run test:rust",
    );
    expect(packageJson.scripts["check:console"]).toBe(
      "node scripts/check-console-log.mjs",
    );
    expect(packageJson.scripts["check:rust:gui"]).toBe(
      "cargo check --manifest-path src-tauri/Cargo.toml",
    );
    expect(packageJson.scripts["test:rust"]).toBe(
      "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features",
    );
  });

  it("runs the required GitHub Actions checks on every supported desktop platform", () => {
    const workflowPath = rootPath(".github/workflows/ci.yml");

    expect(existsSync(workflowPath)).toBe(true);

    const workflow = readFileSync(workflowPath, "utf8");
    const runCommands = workflowRunCommands(workflow);
    const matrixPlatforms = workflowMatrixPlatforms(workflow);

    expect(workflow).toContain("pull_request:");
    expect(workflow).toContain("push:");
    expect(matrixPlatforms).toEqual([
      "windows-latest",
      "macos-latest",
      "ubuntu-latest",
    ]);
    expect(workflow).toContain("libwebkit2gtk-4.1-dev");
    expectCommandsInOrder(runCommands, [
      "npm ci",
      "npm run test",
      "npm run lint",
      "npm run build",
      "npm run check:console",
      "npm run check:rust:gui",
      "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features",
    ]);
  });
});

import { existsSync, readFileSync, readdirSync } from "node:fs";
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

const workflowRunBlocks = (workflow: string) => {
  const lines = workflow.split(/\r?\n/);
  const blocks: string[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^(\s*)run:\s*(.*)$/);
    if (!match) continue;
    const indent = match[1].length;
    const block = [match[2]];
    for (const line of lines.slice(index + 1)) {
      if (line.trim() && (line.match(/^\s*/)?.[0].length ?? 0) <= indent) break;
      block.push(line);
    }
    blocks.push(block.join("\n"));
  }
  return blocks;
};

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
  it("keeps the Tauri desktop binary as Cargo's default run target", () => {
    const cargoManifest = readRootFile("src-tauri/Cargo.toml");

    expect(cargoManifest).toMatch(
      /^\[package\][\s\S]*?^default-run\s*=\s*"llm-wiki-desktop"\s*$/m,
    );
  });

  it("exposes one local check command for all required gates", () => {
    const packageJson = JSON.parse(readRootFile("package.json")) as {
      scripts: Record<string, string>;
    };
    const checkOrchestrator = readRootFile("scripts/run-checks.mjs");

    expect(packageJson.scripts.check).toBe("node scripts/run-checks.mjs");
    expect(checkOrchestrator).toContain(
      'scripts: ["check:import-source-media", "test", "test:capability-tools", "lint", "build", "check:console"]',
    );
    expect(checkOrchestrator).toContain(
      'scripts: ["check:rust:gui", "test:rust"]',
    );
    expect(checkOrchestrator).toContain(
      'scripts: ["lint", "build", "check:console"]',
    );
    expect(checkOrchestrator).toContain(
      'scripts: ["check:rust:core"]',
    );
    expect(packageJson.scripts["check:console"]).toBe(
      "node scripts/check-console-log.mjs",
    );
    expect(packageJson.scripts["check:import-source-media"]).toBe(
      "node scripts/check-import-source-media-flow.mjs",
    );
    expect(packageJson.scripts["check:rust:gui"]).toBe(
      "cargo check --manifest-path src-tauri/Cargo.toml",
    );
    expect(packageJson.scripts["test:rust"]).toBe(
      "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features",
    );
    expect(packageJson.scripts["check:quick"]).toBe(
      "node scripts/run-checks.mjs quick",
    );
    expect(packageJson.scripts["check:rust:core"]).toBe(
      "cargo check --manifest-path src-tauri/Cargo.toml --no-default-features",
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
      "npm run check:import-source-media",
      "npm run test",
      "npm run test:capability-tools",
      "npm run lint",
      "npm run build",
      "npm run check:console",
      "npm run check:rust:gui",
      "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features",
    ]);
  });

  it("keeps capability release inputs out of executable scripts", () => {
    const workflow = readRootFile(".github/workflows/capability-release.yml");
    const runBlocks = workflowRunBlocks(workflow);

    expect(runBlocks.every((block) => !block.includes("${{ inputs."))).toBe(true);
    expect(workflow).toContain("environment: capability-release");
    expect(workflow).toContain("$PSNativeCommandUseErrorActionPreference = $true");
    expect(workflow).toContain("invalid catalog matrix");
    expect(workflow).toContain('const packs=["browser-runtime","browser-runtime-lite","media-metadata","asr-sensevoice-small","ocr-cjk-accurate"]');
    expect(workflow).toContain("sensevoice-capabilities-${{ matrix.target }}");
    expect(workflow).toContain("ocr-capabilities-${{ matrix.target }}");
    expect(workflow).toContain("fetch-rapidocr-sources.mjs");
    expect(workflow).toContain("runner/qualification.mjs");
    expect(workflow).not.toContain("--clobber");
    expect(workflow).not.toMatch(/uses:\s+[^\s#]+@(v\d+|stable)\b/);
  });

  it("keeps every Import icon-only dialog button named and titled", () => {
    const importRoot = rootPath("src/features/import");
    const dialogFiles = readdirSync(importRoot)
      .filter((name) => /^Import.*Dialog\.tsx$/.test(name));

    for (const name of dialogFiles) {
      const source = readFileSync(path.join(importRoot, name), "utf8");
      const buttons = source.match(/<button\b[\s\S]*?>/g) ?? [];
      for (const button of buttons.filter((tag) => tag.includes('className="icon-button"'))) {
        expect(button, `${name}: ${button}`).toContain("aria-label=");
        expect(button, `${name}: ${button}`).toContain("title=");
      }
    }
  });
});

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
      'scripts: ["check:release-config", "test:final-four-redlines", "check:command-execution", "check:import-source-media", "test", "test:capability-tools", "lint", "build", "check:bundle", "check:console"]',
    );
    expect(checkOrchestrator).toContain(
      'scripts: ["check:rust:gui", "test:rust"]',
    );
    expect(checkOrchestrator).toContain(
      'scripts: ["check:release-config", "test:final-four-redlines", "check:command-execution", "lint", "build", "check:bundle", "check:console"]',
    );
    expect(checkOrchestrator).toContain(
      'scripts: ["check:rust:core"]',
    );
    expect(packageJson.scripts["check:console"]).toBe(
      "node scripts/check-console-log.mjs",
    );
    expect(packageJson.scripts["check:bundle"]).toBe(
      "node --test --experimental-test-isolation=none scripts/check-initial-bundle.node-test.mjs && node scripts/check-initial-bundle.mjs",
    );
    expect(packageJson.scripts["check:import-source-media"]).toBe(
      "node scripts/check-import-source-media-flow.mjs",
    );
    expect(packageJson.scripts["check:release-config"]).toBe(
      "node --test --experimental-test-isolation=none scripts/check-release-config.node-test.mjs scripts/release-assets.node-test.mjs scripts/verify-product-capabilities.node-test.mjs scripts/verify-capability-catalog.node-test.mjs scripts/verify-embedded-capability-catalog.node-test.mjs && npm run test:updater-signature && node scripts/check-release-version.mjs && node scripts/verify-product-capabilities.mjs && node scripts/verify-capability-catalog.mjs --catalog capabilities/install-catalog.json --trusted-keys capabilities/trusted-keys.json --mode source",
    );
    expect(packageJson.scripts["test:final-four-redlines"]).toBe(
      "node --test --experimental-test-isolation=none scripts/check-final-four-redlines.node-test.mjs",
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
    const packageJson = JSON.parse(readRootFile("package.json")) as {
      packageManager?: string;
    };

    expect(existsSync(workflowPath)).toBe(true);

    const workflow = readFileSync(workflowPath, "utf8");
    const runCommands = workflowRunCommands(workflow);
    const matrixPlatforms = workflowMatrixPlatforms(workflow);

    expect(workflow).toContain("pull_request:");
    expect(workflow).toContain("push:");
    expect(workflow).toContain("node-version: 22.23.1");
    expect(packageJson.packageManager).toBe("npm@10.9.8");
    expect(matrixPlatforms).toEqual([
      "windows-latest",
      "macos-latest",
      "ubuntu-latest",
    ]);
    expect(workflow).toContain("libwebkit2gtk-4.1-dev");
    expectCommandsInOrder(runCommands, [
      "npm ci",
      "npm run check:release-config",
      "npm run test:final-four-redlines",
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

  it("keeps build tooling compatible while capability distribution is quarantined", () => {
    const capabilityWorkflow = readRootFile(".github/workflows/capability-release.yml");
    const desktopWorkflow = readRootFile(".github/workflows/desktop-release.yml");
    const releaseSources = JSON.parse(
      readRootFile("capabilities/release-sources.json"),
    ) as {
      node: {
        version: string;
        source: string;
        distributions: Record<string, { file: string; root: string }>;
      };
    };

    expect(desktopWorkflow).toContain("NODE_VERSION: 22.23.1");
    expect(capabilityWorkflow.match(/node-version: 22\.23\.1/g)).toHaveLength(1);
    expect(capabilityWorkflow).not.toContain("--node-version 22.17.0");
    expect(capabilityWorkflow).not.toMatch(
      /& \$(?:browserNode|liteNode|mediaNode|node) --test --(?:experimental-)?test-isolation=none/,
    );
    expect(releaseSources.node.version).toBe("22.17.0");
    expect(releaseSources.node.source).toBe("https://nodejs.org/dist/v22.17.0/");
    expect(releaseSources.node.distributions).toMatchObject({
      "x86_64-pc-windows-msvc": {
        file: "node-v22.17.0-win-x64.zip",
        root: "node-v22.17.0-win-x64",
      },
      "aarch64-apple-darwin": {
        file: "node-v22.17.0-darwin-arm64.tar.xz",
        root: "node-v22.17.0-darwin-arm64",
      },
      "x86_64-apple-darwin": {
        file: "node-v22.17.0-darwin-x64.tar.xz",
        root: "node-v22.17.0-darwin-x64",
      },
      "x86_64-unknown-linux-gnu": {
        file: "node-v22.17.0-linux-x64.tar.xz",
        root: "node-v22.17.0-linux-x64",
      },
    });
  });

  it("keeps capability release inputs out of executable scripts", () => {
    const workflow = readRootFile(".github/workflows/capability-release.yml");
    const runBlocks = workflowRunBlocks(workflow);

    expect(runBlocks.every((block) => !block.includes("${{ inputs."))).toBe(true);
    expect(workflow).toContain("environment: capability-release");
    expect(workflow).toContain("verify-product-capabilities.mjs --print-matrix");
    expect(workflow).toContain("verify-product-capabilities.mjs --require-release-ready");
    expect(workflow).toContain("Capability publication remains quarantined");
    expect(workflow).toContain("exit 1");
    expect(workflow).not.toContain("matrix.target");
    expect(workflow).not.toContain("merge-catalog");
    expect(workflow).not.toMatch(/gh release (?:create|upload)/i);
    expect(workflow).toMatch(/^ {2}workflow_call:\s*$/m);
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

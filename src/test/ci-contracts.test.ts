import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

const readRootFile = (path: string) =>
  readFileSync(rootPath(path), "utf8");

const rootPath = (relativePath: string) => path.join(process.cwd(), relativePath);

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

describe("CI validation contract", () => {
  it("keeps the Tauri desktop binary as Cargo's default run target", () => {
    const cargoManifest = readRootFile("src-tauri/Cargo.toml");

    expect(cargoManifest).toMatch(
      /^\[package\][\s\S]*?^default-run\s*=\s*"llm-wiki-desktop"\s*$/m,
    );
  });

  // Workflow syntax is checked with actionlint. Do not duplicate the YAML
  // step order or the check runner's script arrays as string snapshots here.

  it("keeps desktop build tooling pinned while capability runtimes ship their own sources", () => {
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
    expect(desktopWorkflow).toContain("RUST_VERSION: 1.92.0");
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

  it("keeps release workflow inputs out of executable run blocks", () => {
    const workflow = readRootFile(".github/workflows/desktop-release.yml");
    const runBlocks = workflowRunBlocks(workflow);

    expect(runBlocks.every((block) => !block.includes("${{ inputs."))).toBe(true);
    expect(workflow).toContain("environment: desktop-release");
    expect(workflow).not.toMatch(/gh release (?:create|upload)/i);
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

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

  it("keeps build tooling compatible while capability distribution uses the formal matrix", () => {
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
    expect(capabilityWorkflow.match(/node-version: 22\.23\.1/g)).toHaveLength(3);
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

  it("keeps capability release inputs out of executable scripts and closes the formal matrix", () => {
    const workflow = readRootFile(".github/workflows/capability-release.yml");
    const runBlocks = workflowRunBlocks(workflow);

    expect(runBlocks.every((block) => !block.includes("${{ inputs."))).toBe(true);
    expect(workflow).toContain("environment: capability-release");
    expect(workflow).toContain("verify-product-capabilities.mjs --print-matrix");
    expect(workflow).toContain("verify-product-capabilities.mjs --require-release-ready");
    expect(workflow).toContain("capability-release-plan.mjs");
    expect(workflow).toContain("prepare-release-capability.mjs");
    expect(workflow).toContain("qualify-release-corpus.mjs");
    expect(workflow).toContain("matrix.targetTriple");
    expect(workflow).toContain("merge-catalog --input capability-dist");
    expect(workflow).toContain("name: capability-install-catalog");
    expect(workflow).not.toContain("Capability publication remains quarantined");
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

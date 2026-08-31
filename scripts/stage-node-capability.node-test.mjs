import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { stageNodeCapability } from "./stage-node-capability.mjs";

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-stage-node-"));
  const repositoryRoot = path.join(root, "repository");
  const source = path.join(repositoryRoot, "capabilities", "browser-runtime-lite");
  const nodeRoot = path.join(root, "node");
  await fs.mkdir(path.join(source, "runner"), { recursive: true });
  await fs.mkdir(process.platform === "win32" ? nodeRoot : path.join(nodeRoot, "bin"), { recursive: true });
  await fs.writeFile(path.join(source, "runner", "index.mjs"), "process.exit(0);\n");
  await fs.writeFile(path.join(source, "package.json"), "{}\n");
  await fs.writeFile(path.join(nodeRoot, "LICENSE"), "Node license\n");
  await fs.writeFile(
    process.platform === "win32" ? path.join(nodeRoot, "node.exe") : path.join(nodeRoot, "bin", "node"),
    "runtime",
  );
  return { root, repositoryRoot, nodeRoot, output: path.join(root, "payload") };
}

test("stages a self-contained Node entrypoint without a preinstalled system runtime", async (context) => {
  const value = await fixture();
  context.after(() => fs.rm(value.root, { recursive: true, force: true }));
  const result = await stageNodeCapability({
    pack: "browser-runtime-lite",
    target: "x86_64-pc-windows-msvc",
    nodeVersion: "22.17.0",
    nodeRoot: value.nodeRoot,
    output: value.output,
    browserRoot: null,
    repositoryRoot: value.repositoryRoot,
  });
  const nodeName = process.platform === "win32" ? "node.exe" : "node";
  assert.deepEqual(result, { entrypoint: `runtime/${nodeName}`, entrypointArgs: ["runner/index.mjs"] });
  assert.equal((await fs.stat(path.join(value.output, "runtime", nodeName))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "runtime", "NODE-LICENSE"))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "SBOM.spdx.json"))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "NOTICE.md"))).isFile(), true);
  const sbom = JSON.parse(await fs.readFile(path.join(value.output, "SBOM.spdx.json"), "utf8"));
  assert.equal(sbom.spdxVersion, "SPDX-2.3");
  assert.equal(sbom.packages.some((item) => item.name === "Node.js"), true);
});

test("refuses to stage browser-runtime without a pinned Chromium payload", async (context) => {
  const value = await fixture();
  context.after(() => fs.rm(value.root, { recursive: true, force: true }));
  await fs.cp(
    path.join(value.repositoryRoot, "capabilities", "browser-runtime-lite"),
    path.join(value.repositoryRoot, "capabilities", "browser-runtime"),
    { recursive: true },
  );
  await assert.rejects(
    stageNodeCapability({
      pack: "browser-runtime",
      target: "x86_64-pc-windows-msvc",
      nodeVersion: "22.17.0",
      nodeRoot: value.nodeRoot,
      output: value.output,
      browserRoot: null,
      repositoryRoot: value.repositoryRoot,
    }),
    /browser-root/,
  );
});

test("release sources pin an official Node archive for every supported target", async () => {
  const config = JSON.parse(await fs.readFile(
    path.join(import.meta.dirname, "..", "capabilities", "release-sources.json"),
    "utf8",
  ));
  assert.equal(config.schemaVersion, 1);
  assert.match(config.node.source, /^https:\/\/nodejs\.org\/dist\/v\d+\.\d+\.\d+\/$/);
  assert.deepEqual(Object.keys(config.node.distributions).sort(), [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
  ]);
  for (const declaration of Object.values(config.node.distributions)) {
    assert.match(declaration.sha256, /^[0-9a-f]{64}$/);
    assert.doesNotMatch(declaration.sha256, /^0+$/);
    assert.match(declaration.file, /^node-v\d+\.\d+\.\d+-.+\.(?:zip|tar\.xz)$/);
    assert.match(declaration.root, /^node-v\d+\.\d+\.\d+-/);
  }
});

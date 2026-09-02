import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";

import { stagePreparedCapability } from "./stage-prepared-capability.mjs";
import { copyTreePreservingExecutables, findDirectoryContaining } from "./prepare-release-capability.mjs";

test("stages a manifest-derived signed payload contract without links", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-prepared-stage-"));
  try {
    const prepared = path.join(root, "prepared");
    const output = path.join(root, "output");
    await fs.mkdir(path.join(prepared, "runtime"), { recursive: true });
    await Promise.all([
      fs.writeFile(path.join(prepared, "runtime", "node.exe"), "runtime"),
      fs.writeFile(path.join(prepared, "NOTICE.md"), "notice"),
      fs.writeFile(path.join(prepared, "SBOM.spdx.json"), "{}"),
      fs.writeFile(path.join(prepared, "BUILD-PROVENANCE.json"), JSON.stringify({
        packId: "media-metadata", target: "x86_64-pc-windows-msvc", runtimeNetwork: false,
      })),
    ]);
    const result = await stagePreparedCapability({
      pack: "media-metadata", target: "x86_64-pc-windows-msvc",
      preparedRoot: prepared, output, entrypoint: "runtime/node.exe", entrypointArgs: ["runner/index.mjs"],
    });
    assert.deepEqual(result.contract.routes, ["web.bilibili.metadata"]);
    assert.deepEqual(result.contract.formats.extensions, []);
    assert.equal(result.contract.targetTriple, "x86_64-pc-windows-msvc");
    assert.equal(JSON.parse(await fs.readFile(path.join(output, "CAPABILITY-CONTRACT.json"), "utf8")).capabilityId, "media-metadata");
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});

test("finds a LibreOffice MSI application root by its required executable", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-office-layout-"));
  try {
    await fs.mkdir(path.join(root, "program"), { recursive: true });
    await fs.writeFile(path.join(root, "program", "soffice.exe"), "runtime");
    assert.equal(await findDirectoryContaining(root, path.join("program", "soffice.exe")), root);
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});

test("materializes runtime links and preserves executable files", async (context) => {
  if (process.platform === "win32") {
    context.skip("Windows test sessions cannot reliably create symbolic links");
    return;
  }
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-runtime-copy-"));
  try {
    const source = path.join(root, "source");
    const destination = path.join(root, "destination");
    await fs.mkdir(source);
    await fs.writeFile(path.join(source, "runtime.1"), "runtime");
    await fs.chmod(path.join(source, "runtime.1"), 0o755);
    await fs.symlink("runtime.1", path.join(source, "runtime"));
    await copyTreePreservingExecutables(source, destination);
    assert.equal((await fs.lstat(path.join(destination, "runtime"))).isSymbolicLink(), false);
    assert.notEqual((await fs.stat(path.join(destination, "runtime"))).mode & 0o111, 0);
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});

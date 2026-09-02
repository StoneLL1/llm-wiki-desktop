import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";

import { stagePreparedCapability } from "./stage-prepared-capability.mjs";
import {
  copyTreePreservingExecutables,
  findDirectoryContaining,
  linuxCpuRequirements,
  materializeDoclingModels,
} from "./prepare-release-capability.mjs";

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

test("filters CUDA-only requirements and pins the Linux CPU PyTorch wheels", () => {
  const lock = [
    "ordinary==1 \\",
    "    --hash=sha256:" + "1".repeat(64),
    "nvidia-cublas==13 ; sys_platform == 'linux' \\",
    "    --hash=sha256:" + "2".repeat(64),
    "torch==2.13.0 \\",
    "    --hash=sha256:" + "3".repeat(64),
    "torchvision==0.28.0 \\",
    "    --hash=sha256:" + "4".repeat(64),
    "triton==3.7.1 ; sys_platform == 'linux' \\",
    "    --hash=sha256:" + "5".repeat(64),
  ].join("\n");
  const filtered = linuxCpuRequirements(lock);
  assert.match(filtered, /ordinary==1/u);
  assert.match(filtered, /torch-2\.13\.0%2Bcpu-cp312-cp312-manylinux_2_28_x86_64\.whl/u);
  assert.match(filtered, /torchvision-0\.28\.0%2Bcpu-cp312-cp312-manylinux_2_28_x86_64\.whl/u);
  assert.doesNotMatch(filtered, /(?:nvidia-cublas|triton)==/u);
});

test("materializes the pinned Docling model repository in both expected model roots", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-docling-models-"));
  try {
    const downloaded = path.join(root, "downloaded");
    const output = path.join(root, "models");
    await fs.mkdir(path.join(downloaded, "model_artifacts", "layout"), { recursive: true });
    await fs.writeFile(path.join(downloaded, "model_artifacts", "layout", "model.safetensors"), "layout");
    await fs.writeFile(path.join(downloaded, "model_artifacts", "tableformer.bin"), "table");
    await materializeDoclingModels(downloaded, output);
    assert.equal(await fs.readFile(path.join(output, "ds4sd--docling-layout-old", "model.safetensors"), "utf8"), "layout");
    assert.equal(await fs.readFile(path.join(output, "ds4sd--docling-models", "model_artifacts", "tableformer.bin"), "utf8"), "table");
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

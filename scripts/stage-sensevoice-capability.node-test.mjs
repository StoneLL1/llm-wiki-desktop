import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { stageSenseVoiceCapability } from "./stage-sensevoice-capability.mjs";
import {
  hasCompleteInstalledPayload,
  hasCompleteReusablePayload,
  refreshDevelopmentRunner,
  runnerSourceFingerprint,
  validPrepared,
} from "./prepare-sensevoice-dev.mjs";

async function writeFile(filePath, value = "fixture") {
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, value);
}

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-stage-sensevoice-"));
  const repositoryRoot = path.join(root, "repository");
  const source = path.join(repositoryRoot, "capabilities", "asr-sensevoice-small");
  const sourcesRoot = path.join(root, "sources");
  const nodeRoot = path.join(root, "node");
  const executable = process.platform === "win32" ? ".exe" : "";
  await Promise.all([
    writeFile(path.join(source, "runner", "index.mjs"), "process.exit(0);\n"),
    writeFile(path.join(source, "licenses", "APACHE-2.0.txt"), "license\n"),
    writeFile(process.platform === "win32" ? path.join(nodeRoot, "node.exe") : path.join(nodeRoot, "bin", "node")),
    writeFile(path.join(nodeRoot, "LICENSE"), "Node license\n"),
    writeFile(path.join(sourcesRoot, "sherpa", "bin", `sherpa-onnx-offline${executable}`)),
    writeFile(path.join(sourcesRoot, "sherpa", "bin", `sherpa-onnx-version${executable}`)),
    writeFile(path.join(sourcesRoot, "model", "model.int8.onnx"), "model"),
    writeFile(path.join(sourcesRoot, "model", "tokens.txt"), "tokens"),
    writeFile(path.join(sourcesRoot, "model", "README.md"), "model readme"),
    writeFile(path.join(sourcesRoot, "model", "test_wavs", "zh.wav"), "wave"),
    writeFile(path.join(sourcesRoot, "ffmpeg", "bin", `ffmpeg${executable}`)),
    writeFile(path.join(sourcesRoot, "ffmpeg", "bin", `ffprobe${executable}`)),
    writeFile(path.join(sourcesRoot, "ffmpeg", "LICENSE.txt"), "LGPL"),
    writeFile(path.join(sourcesRoot, "SOURCE-PROVENANCE.json"), `${JSON.stringify({
      schemaVersion: 1,
      target: "x86_64-pc-windows-msvc",
      sherpa: { version: "1.13.4", accelerator: "cuda", cpuFallback: true },
      model: { version: "2024-07-17-int8" },
      ffmpeg: { version: "8.1.2", kind: "prebuilt" },
    })}\n`),
  ]);
  return { root, repositoryRoot, sourcesRoot, nodeRoot, output: path.join(root, "payload") };
}

test("stages a self-contained signed-inventory-ready SenseVoice payload", async (context) => {
  const value = await fixture();
  context.after(() => fs.rm(value.root, { recursive: true, force: true }));
  const result = await stageSenseVoiceCapability({
    target: "x86_64-pc-windows-msvc",
    nodeVersion: "22.17.0",
    nodeRoot: value.nodeRoot,
    sourcesRoot: value.sourcesRoot,
    output: value.output,
    repositoryRoot: value.repositoryRoot,
  });
  const nodeName = process.platform === "win32" ? "node.exe" : "node";
  assert.deepEqual(result, { entrypoint: `runtime/${nodeName}`, entrypointArgs: ["runner/index.mjs"], modelBytes: 5 });
  assert.equal((await fs.stat(path.join(value.output, "models", "model.int8.onnx"))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "SBOM.spdx.json"))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "NOTICE.md"))).isFile(), true);
  const executable = process.platform === "win32" ? ".exe" : "";
  await assert.rejects(fs.stat(path.join(value.output, "runtime", "sherpa", "bin", `sherpa-onnx-version${executable}`)));
  await assert.rejects(fs.stat(path.join(value.output, "runtime", "ffmpeg", "bin", `ffprobe${executable}`)));
  const sbom = JSON.parse(await fs.readFile(path.join(value.output, "SBOM.spdx.json"), "utf8"));
  assert.equal(sbom.packages.some((item) => item.name === "SenseVoiceSmall int8 model"), true);
});

test("rejects source provenance for another target or without CPU fallback", async (context) => {
  const value = await fixture();
  context.after(() => fs.rm(value.root, { recursive: true, force: true }));
  const provenancePath = path.join(value.sourcesRoot, "SOURCE-PROVENANCE.json");
  const provenance = JSON.parse(await fs.readFile(provenancePath, "utf8"));
  provenance.sherpa.cpuFallback = false;
  await fs.writeFile(provenancePath, JSON.stringify(provenance));
  await assert.rejects(stageSenseVoiceCapability({
    target: "x86_64-pc-windows-msvc",
    nodeVersion: "22.17.0",
    nodeRoot: value.nodeRoot,
    sourcesRoot: value.sourcesRoot,
    output: value.output,
    repositoryRoot: value.repositoryRoot,
  }), /provenance/);
});

test("pins complete four-target SenseVoice, model, and FFmpeg release sources", async () => {
  const sources = JSON.parse(await fs.readFile(
    path.join(import.meta.dirname, "..", "capabilities", "release-sources.json"),
    "utf8",
  ));
  const targets = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
  ];
  assert.deepEqual(Object.keys(sources.senseVoice.distributions).sort(), targets);
  assert.deepEqual(Object.keys(sources.ffmpeg.distributions).sort(), targets);
  for (const declaration of [
    ...Object.values(sources.senseVoice.distributions),
    sources.senseVoice.model,
    ...Object.values(sources.ffmpeg.distributions),
  ]) {
    assert.match(declaration.sha256, /^[0-9a-f]{64}$/);
    assert.doesNotMatch(declaration.sha256, /^0+$/);
    assert.equal(Number.isSafeInteger(declaration.bytes) && declaration.bytes > 0, true);
    assert.match(declaration.file, /^[A-Za-z0-9][A-Za-z0-9._+-]+$/);
    assert.match(declaration.root, /^[A-Za-z0-9][A-Za-z0-9._+-]+$/);
  }
  assert.equal(sources.senseVoice.distributions["x86_64-pc-windows-msvc"].accelerator, "cuda");
  assert.equal(sources.senseVoice.distributions["x86_64-unknown-linux-gnu"].accelerator, "cuda");
  assert.equal(sources.senseVoice.distributions["aarch64-apple-darwin"].accelerator, "coreml");
  assert.equal(sources.senseVoice.model.sha256, "7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e");
});

test("development preparation refreshes changed runner sources instead of reusing stale code", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-refresh-sensevoice-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const sourceRunner = path.join(root, "source-runner");
  const installedRunner = path.join(root, "pack", "runner");
  await writeFile(path.join(sourceRunner, "index.mjs"), "export const revision = 2;\n");
  await writeFile(path.join(installedRunner, "index.mjs"), "export const revision = 1;\n");
  const runnerSha256 = await runnerSourceFingerprint(sourceRunner);
  await refreshDevelopmentRunner(sourceRunner, installedRunner);
  assert.equal(await fs.readFile(path.join(installedRunner, "index.mjs"), "utf8"), "export const revision = 2;\n");

  const packRoot = path.join(root, "pack");
  const publicKeyPath = path.join(root, "public-key.hex");
  const statePath = path.join(root, "prepared-state.json");
  const executable = process.platform === "win32" ? ".exe" : "";
  const requiredFiles = [
    process.platform === "win32" ? "runtime/node.exe" : "runtime/node",
    `runtime/ffmpeg/bin/ffmpeg${executable}`,
    `runtime/sherpa/bin/sherpa-onnx-offline${executable}`,
    "models/model.int8.onnx",
    "models/tokens.txt",
    "qualification/zh.wav",
    "runner/index.mjs",
    "runner/qualification.mjs",
  ];
  await Promise.all(requiredFiles.map((relativePath) =>
    writeFile(path.join(packRoot, ...relativePath.split("/")), `payload:${relativePath}`)));
  const files = await Promise.all(requiredFiles.map(async (relativePath) => ({
    path: relativePath,
    sha256: "c".repeat(64),
    bytes: (await fs.stat(path.join(packRoot, ...relativePath.split("/")))).size,
  })));
  await writeFile(path.join(packRoot, "manifest.json"), JSON.stringify({
    packId: "asr-sensevoice-small",
    version: "1.13.4+2024.07.17",
    targetTriples: ["x86_64-pc-windows-msvc"],
    files,
  }));
  await writeFile(publicKeyPath, "a".repeat(64));
  await writeFile(statePath, JSON.stringify({
    revision: 5,
    version: "1.13.4+2024.07.17",
    target: "x86_64-pc-windows-msvc",
    runnerSha256,
  }));
  assert.equal(await validPrepared(
    packRoot,
    publicKeyPath,
    statePath,
    "x86_64-pc-windows-msvc",
    runnerSha256,
  ), true);
  assert.equal(await validPrepared(
    packRoot,
    publicKeyPath,
    statePath,
    "x86_64-pc-windows-msvc",
    "b".repeat(64),
  ), false);
  const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
  assert.equal(await hasCompleteInstalledPayload(packRoot, manifest), true);
  await fs.rm(path.join(packRoot, "runner", "index.mjs"));
  assert.equal(await hasCompleteInstalledPayload(packRoot, manifest), false);
  assert.equal(await hasCompleteReusablePayload(packRoot, manifest), true);
  await fs.rm(path.join(packRoot, "models", "tokens.txt"));
  assert.equal(await hasCompleteReusablePayload(packRoot, manifest), false);
  assert.equal(await hasCompleteInstalledPayload(packRoot, manifest), false);
  assert.equal(await validPrepared(
    packRoot,
    publicKeyPath,
    statePath,
    "x86_64-pc-windows-msvc",
    runnerSha256,
  ), false);
  const preparationSource = await fs.readFile(path.join(import.meta.dirname, "prepare-sensevoice-dev.mjs"), "utf8");
  assert.match(
    preparationSource,
    /if \(!stagedPayloadReady\) \{\s+if \(!\(await fs\.stat\(cachedNode\)/u,
    "an installed native/model payload must refresh the runner without redownloading the large source archive",
  );
});

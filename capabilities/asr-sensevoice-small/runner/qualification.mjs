/* global process */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { MODEL_ID } from "./core.mjs";

// This file is executed only by release CI against the fully staged payload.
const packRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function sha256(filePath) {
  const hash = createHash("sha256");
  const stream = createReadStream(filePath);
  for await (const chunk of stream) hash.update(chunk);
  return hash.digest("hex");
}

async function declaration(relativePath) {
  const status = await fs.stat(path.join(packRoot, relativePath));
  return { path: relativePath, sha256: await sha256(path.join(packRoot, relativePath)), bytes: status.size };
}

function runNode(nodePath, request) {
  return new Promise((resolve, reject) => {
    const child = spawn(nodePath, [path.join(packRoot, "runner", "index.mjs")], {
      cwd: packRoot,
      shell: false,
      windowsHide: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? resolve(stdout) : reject(new Error(`runner exited ${code}: ${stderr.slice(0, 500)}`)));
    child.stdin.end(JSON.stringify(request));
  });
}

function runTool(program, arguments_) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, arguments_, {
      cwd: packRoot,
      shell: false,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("exit", (code) => code === 0
      ? resolve()
      : reject(new Error(`qualification tool exited ${code}: ${stderr.slice(0, 500)}`)));
  });
}

const manifestPath = path.join(packRoot, "manifest.json");
const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-sensevoice-qualification-"));
let createdQualificationManifest = false;
try {
  const platform = process.platform;
  const required = [
    platform === "win32" ? "runtime/ffmpeg/bin/ffmpeg.exe" : "runtime/ffmpeg/bin/ffmpeg",
    platform === "win32" ? "runtime/sherpa/bin/sherpa-onnx-offline.exe" : "runtime/sherpa/bin/sherpa-onnx-offline",
    "models/model.int8.onnx",
    "models/tokens.txt",
  ];
  const existingManifest = await fs.stat(manifestPath).catch(() => null);
  if (!existingManifest?.isFile()) {
    await fs.writeFile(manifestPath, JSON.stringify({
      schemaVersion: 2,
      packId: "asr-sensevoice-small",
      version: "1.13.4+2024.07.17",
      protocolVersion: "2",
      files: await Promise.all(required.map(declaration)),
    }), { encoding: "utf8", flag: "wx" });
    createdQualificationManifest = true;
  }
  const shallowStaging = path.join(root, "staging");
  const representativeShard = path.join(
    shallowStaging,
    "asr-shards",
    `${"a".repeat(64)}-${MODEL_ID}`,
    "decoded-0000.wav",
  );
  const staging = process.platform === "win32"
    ? path.join(
      shallowStaging,
      "p".repeat(Math.max(1, 280 - representativeShard.length - 1)),
    )
    : shallowStaging;
  await fs.mkdir(staging, { recursive: true });
  await fs.copyFile(path.join(packRoot, "qualification", "zh.wav"), path.join(staging, "zh.wav"));
  const ffmpegPath = path.join(packRoot, required[0]);
  await runTool(ffmpegPath, [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-i", path.join(staging, "zh.wav"), "-c:a", "aac", "-b:a", "64k",
    path.join(staging, "zh.m4a"),
  ]);
  await runTool(ffmpegPath, [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-stream_loop", "4", "-i", path.join(staging, "zh.wav"), "-t", "27",
    "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le",
    path.join(staging, "zh-long.wav"),
  ]);
  const nodePath = platform === "win32" ? path.join(packRoot, "runtime", "node.exe") : path.join(packRoot, "runtime", "node");
  let qualifiedProvider;
  const fixtures = [
    [1, "zh.wav", "wav"],
    [2, "zh.m4a", "aac-in-m4a"],
    [3, "zh-long.wav", "long-wav-multi-chunk"],
  ];
  for (const [id, fixture] of fixtures) {
    const stdout = await runNode(nodePath, {
      jsonrpc: "2.0",
      id,
      params: {
        operation: "extract",
        projectRoot: root,
        stagingRoot: staging,
        input: { kind: "file", locator: fixture },
        chainedInput: fixture,
        localAsrAuthorized: true,
      },
    });
    const messages = stdout.trim().split(/\r?\n/u).map((line) => JSON.parse(line));
    const progress = messages
      .filter((message) => message?.method === "import.progress")
      .map((message) => message.params);
    const response = messages.find((message) => message?.id === id);
    assert.ok(response, "the runner must emit a terminal response after progress notifications");
    assert.ok(progress.length >= 3, "the runner must expose multiple observable recognition stages");
    assert.ok(progress.every((entry) => entry.total === 100 && entry.current >= 0 && entry.current < 100));
    assert.ok(progress.every((entry, index) => index === 0 || entry.current >= progress[index - 1].current));
    assert.equal(response.error, null);
    const metadata = JSON.parse(await fs.readFile(path.join(staging, response.result.metadataPath), "utf8"));
    const markdown = await fs.readFile(path.join(staging, response.result.markdownPath), "utf8");
    const expectedText = fixture.endsWith(".wav")
      ? /开放时间早上\s*[九9]\s*点至下午\s*[五5]\s*点/u
      : /开[放饭]时间早上\s*9\s*点至下午\s*5\s*点/u;
    assert.match(markdown, expectedText);
    assert.match(metadata.provider, /^(cpu|cuda|coreml)$/);
    assert.equal(metadata.provenance, "authorized-local-asr");
    assert.match(metadata.modelSha256, /^[0-9a-f]{64}$/);
    assert.match(metadata.tokensSha256, /^[0-9a-f]{64}$/);
    if (fixture === "zh-long.wav") {
      assert.ok(progress.some((entry) => entry.label === "asr.recognizing" && entry.current > 22));
      assert.ok(metadata.segments.length >= 2, "the long fixture must exercise multiple ASR chunks");
      assert.ok(metadata.segments[1].startMs >= 20_000, "later chunk timestamps must stay on the media timeline");
    }
    qualifiedProvider = metadata.provider;
  }
  process.stdout.write(`${JSON.stringify({
    qualified: true,
    provider: qualifiedProvider,
    fixtures: fixtures.map(([, , label]) => label),
  })}\n`);
} finally {
  if (createdQualificationManifest) await fs.rm(manifestPath, { force: true });
  await fs.rm(root, { recursive: true, force: true });
}

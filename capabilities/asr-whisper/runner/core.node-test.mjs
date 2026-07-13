import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  FIXED_ARGUMENTS,
  buildArguments,
  classifyExecutionError,
  parseTimedText,
  parseWhisperJson,
  renderTranscript,
  resolveStagingMedia,
  verifyArtifact,
} from "./core.mjs";

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "asr-runner-test-"));
  const staging = path.join(root, "staging");
  await fs.mkdir(staging);
  return { root, staging, cleanup: () => fs.rm(root, { recursive: true, force: true }) };
}

test("resolves only regular supported media contained by staging", async (t) => {
  const value = await fixture();
  t.after(value.cleanup);
  await fs.writeFile(path.join(value.staging, "clip.mp4"), "media");
  const resolved = await resolveStagingMedia(value.root, "staging", "clip.mp4");
  assert.equal(resolved.mediaPath, path.join(value.staging, "clip.mp4"));
  await assert.rejects(resolveStagingMedia(value.root, "staging", "../outside.mp4"), /IMPORT_ASR_POLICY_BLOCKED/);
  await fs.writeFile(path.join(value.staging, "flags.txt"), "--model evil");
  await assert.rejects(resolveStagingMedia(value.root, "staging", "flags.txt"), /IMPORT_ASR_UNSUPPORTED_MEDIA/);
});

test("fixed argv does not accept caller flags", () => {
  const args = buildArguments("C:/pack/model.bin", "C:/stage/--threads=999.mp4", "C:/stage/out");
  assert.deepEqual(args, [
    "--model", "C:/pack/model.bin", "--file", "C:/stage/--threads=999.mp4", "--output-file", "C:/stage/out",
    ...FIXED_ARGUMENTS,
  ]);
  assert.equal(args.filter((arg) => arg === "--model").length, 1);
});

test("classifies enforced process timeout without exposing child diagnostics", () => {
  assert.equal(classifyExecutionError({ killed: true, signal: "SIGKILL" }), "IMPORT_ASR_TIMEOUT");
  assert.equal(classifyExecutionError({ code: "ETIMEDOUT" }), "IMPORT_ASR_TIMEOUT");
  assert.equal(classifyExecutionError({ code: 2, stderr: "secret-bearing diagnostics" }), "IMPORT_ASR_ENGINE_FAILED");
});

test("artifact verification accepts an exact digest and rejects mismatch or traversal", async (t) => {
  const value = await fixture();
  t.after(value.cleanup);
  const binary = path.join(value.root, "whisper-cli");
  const bytes = Buffer.from("fixture-binary");
  await fs.writeFile(binary, bytes);
  const digest = createHash("sha256").update(bytes).digest("hex");
  assert.equal(await verifyArtifact(value.root, { file: "whisper-cli", sha256: digest }, "whisper-cli"), binary);
  await assert.rejects(verifyArtifact(value.root, { file: "whisper-cli", sha256: "f".repeat(64) }, "whisper-cli"), /IMPORT_ASR_ENGINE_INTEGRITY_FAILED/);
  await assert.rejects(verifyArtifact(value.root, { file: "../whisper-cli", sha256: digest }, "whisper-cli"), /IMPORT_ASR_ENGINE_UNAVAILABLE/);
  await assert.rejects(verifyArtifact(value.root, null, "whisper-cli"), /IMPORT_ASR_ENGINE_UNAVAILABLE/);
});

test("parses whisper.cpp JSON variants and renders timestamped provenance", () => {
  const parsed = parseWhisperJson({
    result: { language: "zh", language_probability: 0.925 },
    transcription: [{ offsets: { from: 1250, to: 3500 }, text: " hello " }],
  });
  const markdown = renderTranscript(parsed, "clip.mp4");
  assert.match(markdown, /engine: whisper\.cpp-1\.8\.3/);
  assert.match(markdown, /model: ggml-small/);
  assert.match(markdown, /provenance: authorized-local-asr/);
  assert.match(markdown, /\[00:00:01\.250 --> 00:00:03\.500\] hello/);

  assert.deepEqual(parseWhisperJson({ segments: [{ start: 1, end: 2.5, text: "world" }] }).segments[0], {
    startMs: 1000, endMs: 2500, text: "world",
  });
  assert.match(renderTranscript({ segments: [{ startMs: 0, endMs: 1, text: "<script>" }], language: "zh: injected", languageConfidence: null }, "clip.mp4"), /language: "zh: injected"[\s\S]*&lt;script&gt;/);
});

test("parses SRT and VTT and rejects empty output", () => {
  const srt = "1\n00:00:01,000 --> 00:00:02,250\nhello\n\n2\n00:00:03,000 --> 00:00:04,000\nworld\n";
  assert.equal(parseTimedText(srt).segments.length, 2);
  const vtt = "WEBVTT\n\n00:00:05.000 --> 00:00:06.500 align:start\ncaption\n";
  assert.deepEqual(parseTimedText(vtt).segments[0], { startMs: 5000, endMs: 6500, text: "caption" });
  assert.throws(() => parseTimedText("WEBVTT\n"), /IMPORT_ASR_OUTPUT_INVALID/);
});

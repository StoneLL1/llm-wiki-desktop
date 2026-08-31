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
  buildEmbeddedSubtitleArguments,
  buildVideoOcrFrameArguments,
  buildVideoTextProbeArguments,
  classifyExecutionError,
  ffmpegRelativePath,
  isNoAudioExecutionError,
  nativeToolPath,
  parseTimedText,
  parseWhisperJson,
  renderTranscript,
  resolveStagingMedia,
  selectStableTextFrameIndexes,
  verifyArtifact,
} from "./core.mjs";

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "asr-runner-test-"));
  const staging = path.join(root, "staging");
  await fs.mkdir(staging);
  return { root, staging, cleanup: () => fs.rm(root, { recursive: true, force: true }) };
}

test("native child processes never use deep staging directories as cwd", async () => {
  const source = await fs.readFile(path.join(import.meta.dirname, "index.mjs"), "utf8");
  assert.match(source, /cwd:\s*packRoot/u);
  assert.doesNotMatch(source, /cwd:\s*(?:probeRoot|ocrRoot|temporaryRoot)/u);
});

test("resolves only regular supported media contained by staging", async (t) => {
  const value = await fixture();
  t.after(value.cleanup);
  await fs.writeFile(path.join(value.staging, "clip.mp4"), "media");
  const resolved = await resolveStagingMedia(value.root, "staging", "clip.mp4");
  // Resolve through symlinks before comparing: macOS temp directories live
  // under /var/folders, which realpath canonicalizes to /private/var/folders.
  assert.equal(resolved.mediaPath, await fs.realpath(path.join(value.staging, "clip.mp4")));
  await assert.rejects(resolveStagingMedia(value.root, "staging", "../outside.mp4"), /IMPORT_ASR_POLICY_BLOCKED/);
  await fs.writeFile(path.join(value.staging, "flags.txt"), "--model evil");
  await assert.rejects(resolveStagingMedia(value.root, "staging", "flags.txt"), /IMPORT_ASR_UNSUPPORTED_MEDIA/);
});

test("fixed argv does not accept caller flags", () => {
  const args = buildArguments("pack/model.bin", "stage/--threads=999.mp4", "stage/out");
  assert.deepEqual(args, [
    "--model", "pack/model.bin", "--file", "stage/--threads=999.mp4", "--output-file", "stage/out",
    "--language", "auto",
    ...FIXED_ARGUMENTS,
  ]);
  assert.equal(args.filter((arg) => arg === "--model").length, 1);
  assert.ok(buildArguments("model", "audio.wav", "out", "ZH_cn").includes("zh-cn"));
  assert.throws(() => buildArguments("model", "audio.wav", "out", "--inject"), /IMPORT_ASR_INVALID_REQUEST/u);
});

test("uses Windows extended paths for every absolute native-tool argument", () => {
  const drivePath = String.raw`C:\deep\input.wav`;
  const uncPath = String.raw`\\server\share\input.wav`;
  assert.equal(nativeToolPath(drivePath, "win32"), String.raw`\\?\C:\deep\input.wav`);
  assert.equal(nativeToolPath(uncPath, "win32"), String.raw`\\?\UNC\server\share\input.wav`);
  assert.equal(nativeToolPath("//server/share/input.wav", "win32"), String.raw`\\?\UNC\server\share\input.wav`);
  assert.equal(
    nativeToolPath(String.raw`\\server\share\..\other\input.wav`, "win32"),
    String.raw`\\?\UNC\server\share\other\input.wav`,
  );
  assert.equal(nativeToolPath(String.raw`\\?\C:\deep\input.wav`, "win32"), String.raw`\\?\C:\deep\input.wav`);
  assert.equal(nativeToolPath(String.raw`\\.\pipe\runner`, "win32"), String.raw`\\.\pipe\runner`);
  assert.equal(nativeToolPath(String.raw`\root-relative.wav`, "win32"), String.raw`\root-relative.wav`);
  assert.equal(nativeToolPath(String.raw`C:drive-relative.wav`, "win32"), String.raw`C:drive-relative.wav`);
  assert.equal(nativeToolPath(drivePath, "linux"), drivePath);
  assert.equal(nativeToolPath("relative.wav", "win32"), "relative.wav");
});

test("embedded subtitle probing is bounded and no-audio video errors are distinguishable", () => {
  assert.deepEqual(buildEmbeddedSubtitleArguments("input.mp4", "embedded.srt"), [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe", "-i", "input.mp4",
    "-map", "0:s:0", "-vn", "-an", "-dn", "-c:s", "srt", "-f", "srt", "embedded.srt",
  ]);
  assert.equal(isNoAudioExecutionError({ stderr: "Input does not contain any audio stream" }), true);
  assert.equal(isNoAudioExecutionError({ stderr: "invalid packet" }), false);
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
  // verifyArtifact returns the realpath-resolved absolute path; compare
  // against the canonicalized fixture so the assertion holds on macOS.
  assert.equal(await verifyArtifact(value.root, { file: "whisper-cli", sha256: digest }, "whisper-cli"), await fs.realpath(binary));
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
  assert.match(markdown, /## \[00:00:01\.250\]\n\nhello/u);
  assert.match(
    renderTranscript(parsed, "clip.mp4", "local-embedded-subtitle"),
    /provenance: local-embedded-subtitle/u,
  );

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

test("renders sparse anchors and exposes only bounded local video probe arguments", () => {
  const markdown = renderTranscript({
    segments: [
      { startMs: 0, endMs: 1_000, text: "first" },
      { startMs: 30_000, endMs: 31_000, text: "middle" },
      { startMs: 60_000, endMs: 61_000, text: "later" },
    ],
    language: "en",
    languageConfidence: 1,
  }, "clip.mp4");
  assert.match(markdown, /## \[00:00:00\.000\]/u);
  assert.match(markdown, /## \[00:01:00\.000\]/u);
  assert.doesNotMatch(markdown, /## (?:Summary|Key points|Topics|摘要|要点)/iu);

  const width = 16;
  const height = 16;
  const pixels = Buffer.alloc(width * height, 240);
  for (let y = 0; y < height; y += 1) pixels[y * width + 4] = 20;
  const pgm = Buffer.concat([Buffer.from(`P5\n${width} ${height}\n255\n`, "ascii"), pixels]);
  assert.deepEqual(selectStableTextFrameIndexes([pgm, Buffer.from(pgm)]), [1]);
  assert.equal(buildVideoTextProbeArguments("clip.mp4", "probe-%03d.pgm").at(-2), "180");
  assert.equal(buildVideoOcrFrameArguments("clip.mp4", 20, "frame.png").at(-1), "frame.png");
  assert.equal(ffmpegRelativePath("win32"), "bin/ffmpeg.exe");
  assert.equal(ffmpegRelativePath("linux"), "bin/ffmpeg");
});

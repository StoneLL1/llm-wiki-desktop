import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";

import {
  verifySignedFile,
  completeDecodeMarker,
  canReuseDecodedChunks,
  decodedChunks,
  assertProviderWasUsed,
  buildChunkedFfmpegArguments,
  buildEmbeddedSubtitleArguments,
  buildFfmpegArguments,
  buildSenseVoiceBatchArguments,
  buildSenseVoiceArguments,
  buildVideoOcrFrameArguments,
  buildVideoTextProbeArguments,
  executeWithProviderFallback,
  isNoAudioExecutionError,
  isVideoMedia,
  mergeSenseVoiceTranscripts,
  nativeToolPath,
  parseSenseVoiceBatchStdout,
  parseEmbeddedSubtitle,
  parseSenseVoiceStdout,
  preferredProviders,
  renderEmbeddedTranscript,
  renderTranscript,
  resolveStagingMedia,
  selectStableTextFrameIndexes,
} from "./core.mjs";

test("production ASR wrapper has no network client", async () => {
  const source = await fs.readFile(path.join(import.meta.dirname, "index.mjs"), "utf8");
  assert.doesNotMatch(source, /node:(?:net|http|https|http2|dns|tls|dgram)|\bfetch\s*\(|\bWebSocket\b/u);
  assert.match(source, /method:\s*"import\.progress"/u);
  assert.match(source, /"asr\.recognizing"/u);
  assert.match(source, /cwd:\s*packRoot/u);
  assert.doesNotMatch(source, /cwd:\s*(?:probeRoot|ocrRoot|temporaryRoot|shardRoot)/u);
});

test("accepts the manifest-declared WMA and WMV inputs", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-sensevoice-windows-media-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const staging = path.join(root, "staging");
  await fs.mkdir(staging);
  for (const name of ["audio.wma", "video.wmv"]) {
    await fs.writeFile(path.join(staging, name), "media");
    assert.equal((await resolveStagingMedia(root, "staging", name)).mediaPath, await fs.realpath(path.join(staging, name)));
  }
  assert.equal(isVideoMedia("audio.wma"), false);
  assert.equal(isVideoMedia("video.wmv"), true);
});

test("accepts the staging-relative chained media handoff and rejects escaping inputs", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-sensevoice-policy-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const staging = path.join(root, "staging");
  const relativeMedia = ".asr-input-fixture/输入.m4a";
  const media = path.join(staging, relativeMedia);
  await fs.mkdir(path.dirname(media), { recursive: true });
  await fs.writeFile(media, "media");
  const source = await fs.readFile(path.join(import.meta.dirname, "index.mjs"), "utf8");
  assert.match(source, /const mediaLocator = params\.chainedInput \|\| params\.input\.locator;/u);
  assert.equal((await resolveStagingMedia(root, "staging", relativeMedia)).mediaPath, await fs.realpath(media));
  if (process.platform === "win32") {
    await assert.rejects(
      resolveStagingMedia(root, "staging", path.toNamespacedPath(media)),
      /IMPORT_ASR_POLICY_BLOCKED/,
    );
  }
  await assert.rejects(resolveStagingMedia(root, "staging", "../outside.m4a"), /IMPORT_ASR_POLICY_BLOCKED/);
  await fs.writeFile(path.join(staging, "input.txt"), "text");
  await assert.rejects(resolveStagingMedia(root, "staging", "input.txt"), /IMPORT_ASR_UNSUPPORTED_MEDIA/);
});

test("builds fixed local-only decode and SenseVoice commands", () => {
  assert.deepEqual(buildEmbeddedSubtitleArguments("input.mp4", "embedded.srt"), [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe", "-i", "input.mp4",
    "-map", "0:s:0", "-vn", "-an", "-dn", "-c:s", "srt", "-f", "srt", "embedded.srt",
  ]);
  assert.deepEqual(buildFfmpegArguments("input.m4a", "decoded.wav"), [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe", "-i", "input.m4a",
    "-map", "0:a:0", "-vn", "-sn", "-dn",
    "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "decoded.wav",
  ]);
  assert.deepEqual(buildChunkedFfmpegArguments("input.m4a", "decoded-%04d.wav"), [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe", "-i", "input.m4a",
    "-map", "0:a:0", "-vn", "-sn", "-dn",
    "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le",
    "-f", "segment", "-segment_time", "20", "-reset_timestamps", "1",
    "decoded-%04d.wav",
  ]);
  assert.deepEqual(buildSenseVoiceArguments("model", "tokens", "audio", "cpu", 99), [
    "--tokens=tokens", "--sense-voice-model=model", "--sense-voice-language=auto",
    "--sense-voice-use-itn=true", "--provider=cpu", "--num-threads=8",
    "--debug=false", "--print-args=false", "audio",
  ]);
  assert.deepEqual(buildSenseVoiceBatchArguments("model", "tokens", ["a.wav", "b.wav"], "cpu", 2).slice(-2), [
    "a.wav", "b.wav",
  ]);
  assert.ok(buildSenseVoiceBatchArguments("model", "tokens", ["a.wav"], "cpu", 2, "zh")
    .includes("--sense-voice-language=zh"));
  assert.throws(
    () => buildSenseVoiceBatchArguments("model", "tokens", ["a.wav"], "cpu", 2, "--inject"),
    /IMPORT_ASR_INVALID_REQUEST/u,
  );
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

test("parses and renders embedded subtitle cues before ASR fallback", () => {
  const transcript = parseEmbeddedSubtitle([
    "1",
    "00:00:00,720 --> 00:00:02,000",
    "<i>第一句</i>",
    "",
    "2",
    "00:00:02,100 --> 00:00:03,000",
    "第二句",
  ].join("\n"));
  assert.equal(transcript.text, "第一句 第二句");
  assert.deepEqual(transcript.segments, [
    { startMs: 720, text: "第一句" },
    { startMs: 2100, text: "第二句" },
  ]);
  const markdown = renderEmbeddedTranscript(transcript, "视频.mp4");
  assert.match(markdown, /provenance: local-embedded-subtitle/u);
  assert.match(markdown, /## \[00:00:00\.720\]\n\n第一句/u);
  assert.doesNotMatch(markdown, /SenseVoice/u);
});

test("recognizes ffmpeg no-audio failures without treating ordinary decode errors as no speech", () => {
  assert.equal(isNoAudioExecutionError({
    cause: { stderr: "Stream map '0:a:0' matches no streams." },
  }), true);
  assert.equal(isNoAudioExecutionError({
    stderr: "Invalid data found when processing input",
  }), false);
});

test("parses the single structured sherpa result without inventing end timestamps", () => {
  const result = parseSenseVoiceStdout([
    "Creating recognizer ...",
    '{"lang":"<|zh|>","emotion":"<|NEUTRAL|>","event":"<|Speech|>","text":"开放时间早上9点至下午5点。","timestamps":[0.72,0.96],"tokens":["开","放"]}',
    "Elapsed seconds: 0.5",
  ].join("\n"));
  assert.equal(result.language, "zh");
  assert.equal(result.emotion, "neutral");
  assert.deepEqual(result.tokenTimings, [{ startMs: 720, token: "开" }, { startMs: 960, token: "放" }]);
  const markdown = renderTranscript(result, "中文 输入.wav", "cpu");
  assert.match(markdown, /timing: token_start/);
  assert.match(markdown, /## \[00:00:00\.720\]\n\n开放时间/u);
  assert.doesNotMatch(markdown, /-->/);
});

test("rejects ambiguous, non-monotonic, or oversized structured output", () => {
  assert.throws(() => parseSenseVoiceStdout('{"text":"one"}\n{"text":"two"}'), /IMPORT_ASR_OUTPUT_INVALID/);
  assert.throws(() => parseSenseVoiceStdout('{"text":"x","timestamps":[1,0],"tokens":["x","y"]}'), /IMPORT_ASR_OUTPUT_INVALID/);
});

test("merges bounded SenseVoice batches onto the original media timeline", () => {
  const transcripts = parseSenseVoiceBatchStdout([
    '{"lang":"<|zh|>","emotion":"<|NEUTRAL|>","event":"<|Speech|>","text":"第一段","timestamps":[0.72],"tokens":["第"]}',
    '{"lang":"","emotion":"","event":"","text":"","timestamps":[],"tokens":[]}',
    '{"lang":"<|zh|>","emotion":"<|NEUTRAL|>","event":"<|Speech|>","text":"第三段","timestamps":[0.5],"tokens":["第"]}',
  ].join("\n"), [0, 20_000, 40_000]);
  const merged = mergeSenseVoiceTranscripts(transcripts);
  assert.equal(merged.text, "第一段 第三段");
  assert.deepEqual(merged.segments, [
    { startMs: 720, text: "第一段" },
    { startMs: 40_500, text: "第三段" },
  ]);
  assert.deepEqual(merged.tokenTimings, [
    { startMs: 720, token: "第" },
    { startMs: 40_500, token: "第" },
  ]);
  assert.match(renderTranscript(merged, "long.mp4", "cpu"), /第一段\n\n第三段/u);
  assert.throws(
    () => parseSenseVoiceBatchStdout('{"text":"only one"}', [0, 20_000]),
    /IMPORT_ASR_OUTPUT_INVALID/u,
  );
});

test("renders sparse 30-60 second anchors without invented AI sections", () => {
  const markdown = renderTranscript({
    language: "zh",
    emotion: "neutral",
    event: "speech",
    text: "first middle later",
    tokenTimings: [],
    segments: [
      { startMs: 0, text: "first" },
      { startMs: 30_000, text: "middle" },
      { startMs: 60_000, text: "later" },
    ],
  }, "mixed-language.mp4", "cpu");
  assert.match(markdown, /## \[00:00:00\.000\]/u);
  assert.match(markdown, /## \[00:01:00\.000\]/u);
  assert.doesNotMatch(markdown, /## (?:Summary|Key points|Topics|摘要|要点)/iu);
});

test("video fallback performs a lightweight stable-frame probe before explicit OCR frames", () => {
  const width = 16;
  const height = 16;
  const pixels = Buffer.alloc(width * height, 240);
  for (let y = 0; y < height; y += 1) pixels[y * width + 4] = 20;
  const pgm = Buffer.concat([Buffer.from(`P5\n${width} ${height}\n255\n`, "ascii"), pixels]);
  assert.deepEqual(selectStableTextFrameIndexes([pgm, Buffer.from(pgm)]), [1]);
  assert.deepEqual(buildVideoTextProbeArguments("clip.mp4", "probe-%03d.pgm").slice(-2), [
    "180", "probe-%03d.pgm",
  ]);
  assert.deepEqual(buildVideoOcrFrameArguments("clip.mp4", 10, "frame.png").slice(-3), [
    "-vf", "scale='min(1920,iw)':-2:flags=lanczos", "frame.png",
  ]);
});

test("tries the platform accelerator first and falls back to CPU", async () => {
  assert.deepEqual(preferredProviders("win32"), ["cuda", "cpu"]);
  assert.deepEqual(preferredProviders("darwin"), ["coreml", "cpu"]);
  const calls = [];
  const result = await executeWithProviderFallback(["cuda", "cpu"], async (provider) => {
    calls.push(provider);
    if (provider === "cuda") throw new Error("unavailable");
    return "ok";
  });
  assert.deepEqual(calls, ["cuda", "cpu"]);
  assert.equal(result.provider, "cpu");
  assert.deepEqual(result.attemptedProviders, ["cuda", "cpu"]);
  await assert.rejects(
    executeWithProviderFallback(["cuda", "cpu"], async (provider) => {
      throw new Error(provider === "cpu" ? "IMPORT_ASR_OUTPUT_INVALID" : "IMPORT_ASR_ACCELERATOR_UNAVAILABLE");
    }),
    /IMPORT_ASR_OUTPUT_INVALID/u,
  );
  assert.throws(
    () => assertProviderWasUsed("cuda", "Available providers: CPUExecutionProvider. Fallback to cpu!"),
    /IMPORT_ASR_ACCELERATOR_UNAVAILABLE/,
  );
  assert.doesNotThrow(() => assertProviderWasUsed("cuda", "Available providers: CUDAExecutionProvider, CPUExecutionProvider"));
});


test("installed models are checked for availability without repeating download hashes", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "sensevoice-模型-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.mkdir(path.join(root, "models"));
  const model = path.join(root, "models", "model.int8.onnx");
  await fs.writeFile(model, "model-data");
  const manifest = { files: [{ path: "models/model.int8.onnx", bytes: 10, sha256: "a".repeat(64) }] };
  assert.equal(await verifySignedFile(root, manifest, "models/model.int8.onnx"), await fs.realpath(model));
  await fs.writeFile(model, "short");
  await assert.rejects(verifySignedFile(root, manifest, "models/model.int8.onnx"), /INTEGRITY_FAILED/u);
});


test("decode caches reject legacy, interrupted, missing and changed coverage", () => {
  const chunks = [{ startMs: 0, durationMs: 20032, bytes: 641102 }, { startMs: 20032, durationMs: 968, bytes: 31054 }];
  const marker = completeDecodeMarker("input-hash", chunks);
  assert.equal(canReuseDecodedChunks(marker, "input-hash", chunks), true);
  assert.equal(marker.durationMs, 21000);
  assert.equal(canReuseDecodedChunks({ schemaVersion: 1, complete: true, mediaSha256: "input-hash", chunkSeconds: 20, chunks: 360 }, "input-hash", chunks), false);
  assert.equal(canReuseDecodedChunks({ ...marker, complete: false }, "input-hash", chunks), false);
  assert.equal(canReuseDecodedChunks(marker, "other-input", chunks), false);
  assert.equal(canReuseDecodedChunks(marker, "input-hash", chunks.slice(0, 1)), false);
  assert.equal(canReuseDecodedChunks(marker, "input-hash", [{ ...chunks[0], bytes: 641104 }, chunks[1]]), false);
});

test("decoded PCM coverage uses real sample lengths and rejects partial WAVs", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "sensevoice-coverage-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const wav = (samples) => {
    const result = Buffer.alloc(44 + samples * 2);
    result.write("RIFF"); result.writeUInt32LE(result.length - 8, 4); result.write("WAVEfmt ", 8);
    result.writeUInt32LE(16, 16); result.writeUInt16LE(1, 20); result.writeUInt16LE(1, 22);
    result.writeUInt32LE(16000, 24); result.writeUInt32LE(32000, 28); result.writeUInt16LE(2, 32); result.writeUInt16LE(16, 34);
    result.write("data", 36); result.writeUInt32LE(samples * 2, 40); return result;
  };
  await fs.writeFile(path.join(root, "decoded-0000.wav"), wav(320512));
  await fs.writeFile(path.join(root, "decoded-0001.wav"), wav(15488));
  const chunks = await decodedChunks(root);
  assert.equal(chunks[1].startMs, 20032);
  assert.equal(chunks[0].hasSignal, false);
  const signal = wav(15488); signal[44] = 1;
  await fs.writeFile(path.join(root, "decoded-0001.wav"), signal);
  assert.equal((await decodedChunks(root))[1].hasSignal, true);
  assert.equal(completeDecodeMarker("hash", chunks).durationMs, 21000);
  await fs.writeFile(path.join(root, "decoded-0001.wav"), wav(15488).subarray(0, 500));
  await assert.rejects(decodedChunks(root), /IMPORT_ASR_DECODE_FAILED/);
});

test("real FFmpeg decodes beyond two hours to EOF and verifies reusable full coverage", { skip: !process.env.LLM_WIKI_TEST_FFMPEG }, async (t) => {
  const { execFileSync } = await import("node:child_process");
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "sensevoice-long-decode-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const source = path.join(root, "input.flac");
  execFileSync(process.env.LLM_WIKI_TEST_FFMPEG, ["-v", "error", "-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-t", "7221", "-c:a", "flac", source]);
  execFileSync(process.env.LLM_WIKI_TEST_FFMPEG, buildChunkedFfmpegArguments(source, path.join(root, "decoded-%04d.wav")));
  const chunks = await decodedChunks(root);
  assert.equal(chunks.length, 362);
  const marker = completeDecodeMarker("hash", chunks);
  assert.equal(marker.durationMs, 7221000);
  assert.ok(chunks.at(-1).startMs > 7200000);
  assert.equal(canReuseDecodedChunks(marker, "hash", chunks), true);
  await fs.rm(chunks.at(-1).path);
  assert.equal(canReuseDecodedChunks(marker, "hash", await decodedChunks(root)), false);
});


test("silent punctuation-only shards are not speech or placeholder Sources", () => {
  const silent = '{"text":"。","timestamps":[19.86],"tokens":["。"]}';
  assert.deepEqual(parseSenseVoiceBatchStdout(silent, [0]), []);
  assert.throws(() => parseSenseVoiceStdout(silent), /IMPORT_ASR_OUTPUT_INVALID/);
  const oldSilentCache = { text: "。", segments: [{ startMs: 19860, text: "。" }], tokenTimings: [{ startMs: 19860, token: "。" }] };
  assert.throws(() => mergeSenseVoiceTranscripts([oldSilentCache]), /IMPORT_ASR_OUTPUT_INVALID/);
  const speech = parseSenseVoiceBatchStdout('{"text":"2026 中文尾段","timestamps":[5],"tokens":["2026 中文尾段"]}', [7200000]);
  assert.equal(mergeSenseVoiceTranscripts([oldSilentCache, ...speech]).text, "2026 中文尾段");
});


test("binary PGM whitespace pixels and repeated opening scenes preserve later frames", () => {
  const frame = (first, shift = 0) => {
    const pixels = Buffer.alloc(32 * 32, 240);
    for (let y = 0; y < 32; y += 1) pixels[y * 32 + 4 + shift] = 20;
    pixels[0] = first;
    return Buffer.concat([Buffer.from("P5\n32 32\n255\n"), pixels]);
  };
  for (const first of [9, 10, 13, 32]) {
    const opening = frame(first);
    const late = frame(first, 10);
    assert.deepEqual(selectStableTextFrameIndexes([...Array(24).fill(opening), late, late]), [1, 24]);
  }
});


test("FFmpeg 8 empty stream-map diagnostics still identify a missing audio track", () => {
  assert.equal(isNoAudioExecutionError({ stderr: "Stream map '' matches no streams.\nFailed to set value '0:a:0' for option 'map': Invalid argument" }), true);
  assert.equal(isNoAudioExecutionError({ stderr: "Stream map '' matches no streams.\nFailed to set value '0:s:0' for option 'map': Invalid argument" }), false);
});

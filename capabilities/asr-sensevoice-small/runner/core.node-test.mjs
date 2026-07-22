import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  assertProviderWasUsed,
  buildFfmpegArguments,
  buildSenseVoiceArguments,
  executeWithProviderFallback,
  parseSenseVoiceStdout,
  preferredProviders,
  renderTranscript,
  resolveStagingMedia,
} from "./core.mjs";

test("production ASR wrapper has no network client", async () => {
  const source = await fs.readFile(path.join(import.meta.dirname, "index.mjs"), "utf8");
  assert.doesNotMatch(source, /node:(?:net|http|https|http2|dns|tls|dgram)|\bfetch\s*\(|\bWebSocket\b/u);
});

test("accepts only regular media files contained by the staging directory", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-sensevoice-policy-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const staging = path.join(root, "staging");
  await fs.mkdir(staging);
  await fs.writeFile(path.join(staging, "输入.m4a"), "media");
  assert.equal((await resolveStagingMedia(root, "staging", "输入.m4a")).mediaPath, path.join(staging, "输入.m4a"));
  await assert.rejects(resolveStagingMedia(root, "staging", "../outside.m4a"), /IMPORT_ASR_POLICY_BLOCKED/);
  await fs.writeFile(path.join(staging, "input.txt"), "text");
  await assert.rejects(resolveStagingMedia(root, "staging", "input.txt"), /IMPORT_ASR_UNSUPPORTED_MEDIA/);
});

test("builds fixed local-only decode and SenseVoice commands", () => {
  assert.deepEqual(buildFfmpegArguments("input.m4a", "decoded.wav"), [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe", "-i", "input.m4a",
    "-map", "0:a:0", "-vn", "-sn", "-dn", "-t", "7200",
    "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "decoded.wav",
  ]);
  assert.deepEqual(buildSenseVoiceArguments("model", "tokens", "audio", "cpu", 99), [
    "--tokens=tokens", "--sense-voice-model=model", "--sense-voice-language=auto",
    "--sense-voice-use-itn=true", "--provider=cpu", "--num-threads=8",
    "--debug=false", "--print-args=false", "audio",
  ]);
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
  assert.match(markdown, /\[00:00:00\.720\] 开放时间/);
  assert.doesNotMatch(markdown, /-->/);
});

test("rejects ambiguous, non-monotonic, or oversized structured output", () => {
  assert.throws(() => parseSenseVoiceStdout('{"text":"one"}\n{"text":"two"}'), /IMPORT_ASR_OUTPUT_INVALID/);
  assert.throws(() => parseSenseVoiceStdout('{"text":"x","timestamps":[1,0],"tokens":["x","y"]}'), /IMPORT_ASR_OUTPUT_INVALID/);
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
  assert.throws(
    () => assertProviderWasUsed("cuda", "Available providers: CPUExecutionProvider. Fallback to cpu!"),
    /IMPORT_ASR_ACCELERATOR_UNAVAILABLE/,
  );
  assert.doesNotThrow(() => assertProviderWasUsed("cuda", "Available providers: CUDAExecutionProvider, CPUExecutionProvider"));
});

import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

export const ENGINE_VERSION = "sherpa-onnx-1.13.4";
export const MODEL_ID = "SenseVoiceSmall-int8-2024-07-17";
export const MAX_MEDIA_BYTES = 8 * 1024 * 1024 * 1024;
export const MAX_DECODED_BYTES = 256 * 1024 * 1024;
export const MAX_TRANSCRIPT_BYTES = 32 * 1024 * 1024;
export const MAX_TOKENS = 250_000;
export const SENSEVOICE_CHUNK_SECONDS = 20;
export const MAX_SENSEVOICE_CHUNKS = Math.ceil(7_200 / SENSEVOICE_CHUNK_SECONDS);
export const MAX_SENSEVOICE_BATCH_CHUNKS = 24;

const MEDIA_EXTENSIONS = new Set([
  ".aac", ".flac", ".m4a", ".mka", ".mp3", ".ogg", ".opus", ".wav",
  ".avi", ".m4v", ".mkv", ".mov", ".mp4", ".mpeg", ".mpg", ".webm",
]);

function asError(code) {
  return new Error(code);
}

export function isContained(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

export async function resolveStagingMedia(projectRootValue, stagingRootValue, locatorValue) {
  if (typeof projectRootValue !== "string" || typeof stagingRootValue !== "string" || typeof locatorValue !== "string") {
    throw asError("IMPORT_ASR_INVALID_REQUEST");
  }
  const projectRoot = await fs.realpath(path.resolve(projectRootValue));
  const stagingCandidate = path.isAbsolute(stagingRootValue)
    ? path.resolve(stagingRootValue)
    : path.resolve(projectRoot, stagingRootValue);
  const stagingRoot = await fs.realpath(stagingCandidate);
  if (!isContained(projectRoot, stagingRoot)) throw asError("IMPORT_ASR_POLICY_BLOCKED");

  const candidate = path.isAbsolute(locatorValue)
    ? path.resolve(locatorValue)
    : path.resolve(stagingRoot, locatorValue);
  if (!isContained(stagingRoot, candidate)) throw asError("IMPORT_ASR_POLICY_BLOCKED");
  const status = await fs.lstat(candidate).catch(() => null);
  if (!status?.isFile() || status.isSymbolicLink()) throw asError("IMPORT_ASR_INVALID_MEDIA");
  const mediaPath = await fs.realpath(candidate);
  if (!isContained(stagingRoot, mediaPath)) throw asError("IMPORT_ASR_POLICY_BLOCKED");
  if (!MEDIA_EXTENSIONS.has(path.extname(mediaPath).toLowerCase())) throw asError("IMPORT_ASR_UNSUPPORTED_MEDIA");
  if (status.size <= 0 || status.size > MAX_MEDIA_BYTES) throw asError("IMPORT_ASR_MEDIA_TOO_LARGE");
  return { projectRoot, stagingRoot, mediaPath };
}

export async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

export async function verifySignedFile(packRootValue, manifest, relativePath) {
  if (!manifest || !Array.isArray(manifest.files) || typeof relativePath !== "string") {
    throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }
  const declaration = manifest.files.find((item) => item?.path === relativePath);
  if (!declaration || typeof declaration.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(declaration.sha256) ||
      !Number.isSafeInteger(declaration.bytes) || declaration.bytes <= 0) {
    throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }
  const packRoot = await fs.realpath(packRootValue);
  const candidate = path.resolve(packRoot, relativePath);
  if (!isContained(packRoot, candidate)) throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  const status = await fs.lstat(candidate).catch(() => null);
  if (!status?.isFile() || status.isSymbolicLink() || status.size !== declaration.bytes) {
    throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }
  const resolved = await fs.realpath(candidate);
  if (!isContained(packRoot, resolved) || await sha256File(resolved) !== declaration.sha256) {
    throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }
  return resolved;
}

export function ffmpegRelativePath(platform = process.platform) {
  return platform === "win32" ? "runtime/ffmpeg/bin/ffmpeg.exe" : "runtime/ffmpeg/bin/ffmpeg";
}

export function sherpaRelativePath(platform = process.platform) {
  return platform === "win32"
    ? "runtime/sherpa/bin/sherpa-onnx-offline.exe"
    : "runtime/sherpa/bin/sherpa-onnx-offline";
}

export function preferredProviders(platform = process.platform) {
  if (platform === "darwin") return ["coreml", "cpu"];
  if (platform === "win32" || platform === "linux") return ["cuda", "cpu"];
  return ["cpu"];
}

export function buildFfmpegArguments(mediaPath, wavPath) {
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-i", mediaPath,
    "-map", "0:a:0", "-vn", "-sn", "-dn",
    "-t", "7200", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le",
    wavPath,
  ];
}

export function buildChunkedFfmpegArguments(mediaPath, wavPattern) {
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-i", mediaPath,
    "-map", "0:a:0", "-vn", "-sn", "-dn",
    "-t", "7200", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le",
    "-f", "segment", "-segment_time", String(SENSEVOICE_CHUNK_SECONDS),
    "-reset_timestamps", "1",
    wavPattern,
  ];
}

export function buildEmbeddedSubtitleArguments(mediaPath, subtitlePath) {
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-i", mediaPath,
    "-map", "0:s:0", "-vn", "-an", "-dn", "-c:s", "srt", "-f", "srt",
    subtitlePath,
  ];
}

export function buildSenseVoiceArguments(modelPath, tokensPath, wavPath, provider, threads) {
  return buildSenseVoiceBatchArguments(modelPath, tokensPath, [wavPath], provider, threads);
}

export function buildSenseVoiceBatchArguments(modelPath, tokensPath, wavPaths, provider, threads) {
  if (!new Set(["cpu", "cuda", "coreml"]).has(provider)) throw asError("IMPORT_ASR_INVALID_REQUEST");
  if (!Array.isArray(wavPaths) || wavPaths.length === 0 || wavPaths.length > MAX_SENSEVOICE_BATCH_CHUNKS ||
      wavPaths.some((value) => typeof value !== "string" || value.length === 0)) {
    throw asError("IMPORT_ASR_INVALID_REQUEST");
  }
  const safeThreads = Math.min(8, Math.max(1, Number.isSafeInteger(threads) ? threads : 1));
  return [
    `--tokens=${tokensPath}`,
    `--sense-voice-model=${modelPath}`,
    "--sense-voice-language=auto",
    "--sense-voice-use-itn=true",
    `--provider=${provider}`,
    `--num-threads=${safeThreads}`,
    "--debug=false",
    "--print-args=false",
    ...wavPaths,
  ];
}

function cleanText(value) {
  if (typeof value !== "string") return "";
  return Array.from(value, (character) => {
    const codePoint = character.codePointAt(0);
    return codePoint === 0x7f || (codePoint < 0x20 && codePoint !== 0x09 && codePoint !== 0x0a && codePoint !== 0x0d)
      ? " " : character;
  }).join("").replace(/\s+/g, " ").trim();
}

function normalizeTag(value, fallback) {
  const cleaned = cleanText(value).replace(/^<\|/, "").replace(/\|>$/, "").toLowerCase();
  return /^[a-z][a-z0-9_-]{0,31}$/.test(cleaned) ? cleaned : fallback;
}

function senseVoiceCandidates(value) {
  if (typeof value !== "string" || Buffer.byteLength(value, "utf8") > MAX_TRANSCRIPT_BYTES) {
    throw asError("IMPORT_ASR_OUTPUT_INVALID");
  }
  const candidates = [];
  for (const line of value.replace(/\r\n?/g, "\n").split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) continue;
    try {
      const parsed = JSON.parse(trimmed);
      if (typeof parsed?.text === "string") candidates.push(parsed);
    } catch {
      // sherpa writes diagnostic lines alongside its single JSON result.
    }
  }
  return candidates;
}

function normalizeSenseVoiceCandidate(result, allowEmpty = false) {
  const text = cleanText(result.text);
  const timestamps = Array.isArray(result.timestamps) ? result.timestamps.map(Number) : [];
  const tokens = Array.isArray(result.tokens) ? result.tokens.map(cleanText) : [];
  if (timestamps.length > MAX_TOKENS || tokens.length > MAX_TOKENS || timestamps.length > tokens.length ||
      timestamps.some((item, index) => !Number.isFinite(item) || item < 0 || (index > 0 && item < timestamps[index - 1])) ||
      tokens.some((item) => !item)) {
    throw asError("IMPORT_ASR_OUTPUT_INVALID");
  }
  if (!text) {
    if (allowEmpty && timestamps.length === 0 && tokens.length === 0) return null;
    throw asError("IMPORT_ASR_OUTPUT_INVALID");
  }
  return {
    text,
    language: normalizeTag(result.lang, "unknown"),
    emotion: normalizeTag(result.emotion, "unknown"),
    event: normalizeTag(result.event, "speech"),
    tokenTimings: timestamps.map((startSeconds, index) => ({
      startMs: Math.round(startSeconds * 1_000),
      token: tokens[index],
    })),
  };
}

export function parseSenseVoiceStdout(value) {
  const candidates = senseVoiceCandidates(value);
  if (candidates.length !== 1) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  return normalizeSenseVoiceCandidate(candidates[0]);
}

export function parseSenseVoiceBatchStdout(value, chunkStartsMs) {
  if (!Array.isArray(chunkStartsMs) || chunkStartsMs.length === 0 ||
      chunkStartsMs.length > MAX_SENSEVOICE_BATCH_CHUNKS ||
      chunkStartsMs.some((item, index) => !Number.isSafeInteger(item) || item < 0 ||
        (index > 0 && item <= chunkStartsMs[index - 1]))) {
    throw asError("IMPORT_ASR_INVALID_REQUEST");
  }
  const candidates = senseVoiceCandidates(value);
  if (candidates.length !== chunkStartsMs.length) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  const transcripts = [];
  for (let index = 0; index < candidates.length; index += 1) {
    const transcript = normalizeSenseVoiceCandidate(candidates[index], true);
    if (!transcript) continue;
    const offsetMs = chunkStartsMs[index];
    const tokenTimings = transcript.tokenTimings.map((timing) => ({
      startMs: timing.startMs + offsetMs,
      token: timing.token,
    }));
    transcripts.push({
      ...transcript,
      tokenTimings,
      segments: [{
        startMs: tokenTimings[0]?.startMs ?? offsetMs,
        text: transcript.text,
      }],
    });
  }
  return transcripts;
}

export function mergeSenseVoiceTranscripts(transcripts) {
  if (!Array.isArray(transcripts)) throw asError("IMPORT_ASR_INVALID_REQUEST");
  const segments = transcripts.flatMap((item) => Array.isArray(item?.segments) ? item.segments : []);
  const tokenTimings = transcripts.flatMap((item) => Array.isArray(item?.tokenTimings) ? item.tokenTimings : []);
  if (segments.length === 0 || segments.length > MAX_SENSEVOICE_CHUNKS ||
      tokenTimings.length > MAX_TOKENS ||
      segments.some((item, index) => !Number.isSafeInteger(item?.startMs) || item.startMs < 0 ||
        !cleanText(item.text) || (index > 0 && item.startMs <= segments[index - 1].startMs)) ||
      tokenTimings.some((item, index) => !Number.isSafeInteger(item?.startMs) || item.startMs < 0 ||
        !cleanText(item.token) || (index > 0 && item.startMs < tokenTimings[index - 1].startMs))) {
    throw asError("IMPORT_ASR_OUTPUT_INVALID");
  }
  const firstKnown = (key, fallback) =>
    transcripts.map((item) => item?.[key]).find((value) => typeof value === "string" && value !== "unknown") ?? fallback;
  return {
    text: segments.map((segment) => cleanText(segment.text)).join(" "),
    language: firstKnown("language", "unknown"),
    emotion: firstKnown("emotion", "unknown"),
    event: firstKnown("event", "speech"),
    segments: segments.map((segment) => ({ startMs: segment.startMs, text: cleanText(segment.text) })),
    tokenTimings,
  };
}

function timestamp(milliseconds) {
  const value = Math.max(0, Math.round(milliseconds));
  const hours = Math.floor(value / 3_600_000);
  const minutes = Math.floor(value / 60_000) % 60;
  const seconds = Math.floor(value / 1_000) % 60;
  const millis = value % 1_000;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}

function parseSubtitleTimestamp(value) {
  const match = /^(\d{1,2}):(\d{2}):(\d{2})[,.](\d{3})$/u.exec(value.trim());
  if (!match) return null;
  return (((Number(match[1]) * 60 + Number(match[2])) * 60 + Number(match[3])) * 1_000) + Number(match[4]);
}

export function parseEmbeddedSubtitle(value) {
  if (typeof value !== "string" || Buffer.byteLength(value, "utf8") > MAX_TRANSCRIPT_BYTES) {
    throw asError("IMPORT_ASR_OUTPUT_INVALID");
  }
  const segments = [];
  for (const block of value.replace(/^\uFEFF/u, "").replace(/\r\n?/g, "\n").split(/\n{2,}/u)) {
    const lines = block.split("\n").map((line) => line.trim());
    const timingIndex = lines.findIndex((line) => line.includes("-->"));
    if (timingIndex < 0) continue;
    const startMs = parseSubtitleTimestamp(lines[timingIndex].split("-->", 1)[0]);
    const text = cleanText(lines.slice(timingIndex + 1).join(" ").replace(/<[^>]*>/gu, " "));
    if (startMs == null || !text) continue;
    if (segments.length >= MAX_TOKENS || (segments.length > 0 && startMs < segments.at(-1).startMs)) {
      throw asError("IMPORT_ASR_OUTPUT_INVALID");
    }
    segments.push({ startMs, text });
  }
  if (segments.length === 0) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  return { text: segments.map((segment) => segment.text).join(" "), segments };
}

function markdownText(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

export function renderTranscript(result, sourceName, provider) {
  const source = cleanText(sourceName).slice(0, 500);
  const transcriptSegments = Array.isArray(result.segments) && result.segments.length > 0
    ? result.segments
    : [{ startMs: result.tokenTimings[0]?.startMs ?? null, text: result.text }];
  const transcriptLines = transcriptSegments.map((segment) => segment.startMs == null
    ? markdownText(cleanText(segment.text))
    : `[${timestamp(segment.startMs)}] ${markdownText(cleanText(segment.text))}`).join("\n\n");
  return `${[
    "---",
    `engine: ${JSON.stringify(ENGINE_VERSION)}`,
    `model: ${JSON.stringify(MODEL_ID)}`,
    `provider: ${JSON.stringify(provider)}`,
    `language: ${JSON.stringify(result.language)}`,
    `emotion: ${JSON.stringify(result.emotion)}`,
    `event: ${JSON.stringify(result.event)}`,
    `source: ${JSON.stringify(source)}`,
    "timing: token_start",
    "provenance: authorized-local-asr",
    "---",
    "",
    "# Transcript",
    "",
    transcriptLines,
    "",
  ].join("\n")}`;
}

export function renderEmbeddedTranscript(result, sourceName) {
  const source = cleanText(sourceName).slice(0, 500);
  const transcriptLines = result.segments
    .map((segment) => `[${timestamp(segment.startMs)}] ${markdownText(segment.text)}`)
    .join("\n\n");
  return `${[
    "---",
    'engine: "ffmpeg"',
    'provider: "embedded_subtitle"',
    `source: ${JSON.stringify(source)}`,
    "timing: cue_start",
    "provenance: authorized-local-embedded-subtitle",
    "---",
    "",
    "# Transcript",
    "",
    transcriptLines,
    "",
  ].join("\n")}`;
}

export async function executeWithProviderFallback(providers, execute) {
  const attempted = [];
  let lastError;
  for (const provider of providers) {
    attempted.push(provider);
    try {
      return { provider, attemptedProviders: attempted, value: await execute(provider) };
    } catch (error) {
      lastError = error;
    }
  }
  if (typeof lastError?.message === "string" && /^IMPORT_ASR_[A-Z_]+$/.test(lastError.message)) {
    throw lastError;
  }
  throw new Error("IMPORT_ASR_ENGINE_FAILED", { cause: lastError });
}

export function assertProviderWasUsed(provider, stderr) {
  if (provider === "cpu") return;
  const diagnostics = typeof stderr === "string" ? stderr : "";
  if (/fallback\s+to\s+cpu/i.test(diagnostics)) {
    throw asError("IMPORT_ASR_ACCELERATOR_UNAVAILABLE");
  }
}

export function restrictedEnvironment(packRoot, temporaryRoot, platform = process.platform, source = process.env) {
  const result = {
    NO_COLOR: "1",
    OMP_NUM_THREADS: "8",
    ORT_LOG_SEVERITY_LEVEL: "3",
  };
  for (const name of ["SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"]) {
    if (typeof source[name] === "string") result[name] = source[name];
  }
  const sherpaLib = path.join(packRoot, "runtime", "sherpa", "lib");
  const ffmpegLib = path.join(packRoot, "runtime", "ffmpeg", "lib");
  if (platform === "linux") result.LD_LIBRARY_PATH = `${sherpaLib}:${ffmpegLib}`;
  if (platform === "darwin") result.DYLD_LIBRARY_PATH = `${sherpaLib}:${ffmpegLib}`;
  result.TEMP = temporaryRoot;
  result.TMP = temporaryRoot;
  result.TMPDIR = temporaryRoot;
  return result;
}

export function classifyExecutionError(error, stage) {
  if (error?.killed || error?.code === "ETIMEDOUT" || error?.signal === "SIGKILL") return "IMPORT_ASR_TIMEOUT";
  return stage === "decode" ? "IMPORT_ASR_DECODE_FAILED" : "IMPORT_ASR_ENGINE_FAILED";
}

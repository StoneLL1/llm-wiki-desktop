import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { Buffer } from "node:buffer";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

export const ENGINE_VERSION = "whisper.cpp-1.8.3";
export const MODEL_ID = "ggml-small";
export const MODEL_SHA256 = "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b";
export const MAX_MEDIA_BYTES = 8 * 1024 * 1024 * 1024;
export const MAX_TRANSCRIPT_BYTES = 32 * 1024 * 1024;
export const MAX_SEGMENTS = 250_000;

const MEDIA_EXTENSIONS = new Set([
  ".aac", ".flac", ".m4a", ".mka", ".mp3", ".ogg", ".opus", ".wav",
  ".avi", ".m4v", ".mkv", ".mov", ".mp4", ".mpeg", ".mpg", ".webm",
]);

export const FIXED_ARGUMENTS = Object.freeze([
  "--output-json",
  "--no-prints",
]);
const VIDEO_EXTENSIONS = new Set([
  ".avi", ".m4v", ".mkv", ".mov", ".mp4", ".mpeg", ".mpg", ".webm",
]);

function asError(code) {
  return new Error(code);
}

function isContained(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

export function currentRuntimeKey(platform = process.platform, arch = process.arch) {
  return `${platform}-${arch}`;
}

export function ffmpegRelativePath(platform = process.platform) {
  return platform === "win32" ? "runtime/ffmpeg/bin/ffmpeg.exe" : "runtime/ffmpeg/bin/ffmpeg";
}

export function nativeToolPath(value, platform = process.platform) {
  if (platform !== "win32" || typeof value !== "string") return value;
  const normalized = value.replaceAll("/", "\\");
  if (normalized.startsWith("\\\\?\\") || normalized.startsWith("\\\\.\\")) return normalized;
  if (/^\\\\[^\\]+\\[^\\]+(?:\\|$)/u.test(normalized)) {
    return `\\\\?\\UNC\\${path.win32.normalize(normalized).slice(2)}`;
  }
  return /^[A-Za-z]:\\/u.test(normalized) ? `\\\\?\\${path.win32.normalize(normalized)}` : value;
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
  const linkStatus = await fs.lstat(candidate).catch(() => null);
  if (!linkStatus?.isFile() || linkStatus.isSymbolicLink()) throw asError("IMPORT_ASR_INVALID_MEDIA");
  const mediaPath = await fs.realpath(candidate);
  if (!isContained(stagingRoot, mediaPath)) throw asError("IMPORT_ASR_POLICY_BLOCKED");
  if (!MEDIA_EXTENSIONS.has(path.extname(mediaPath).toLowerCase())) throw asError("IMPORT_ASR_UNSUPPORTED_MEDIA");
  if (linkStatus.size <= 0 || linkStatus.size > MAX_MEDIA_BYTES) throw asError("IMPORT_ASR_MEDIA_TOO_LARGE");
  return { projectRoot, stagingRoot, mediaPath };
}

export async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

export function classifyExecutionError(error) {
  if (error?.killed || error?.code === "ETIMEDOUT" || error?.signal === "SIGKILL") return "IMPORT_ASR_TIMEOUT";
  return "IMPORT_ASR_ENGINE_FAILED";
}

export function isNoAudioExecutionError(error) {
  const details = [error?.message, error?.stderr, error?.stdout]
    .filter((value) => typeof value === "string")
    .join("\n");
  return /(?:does not contain any audio stream|no audio stream|audio stream.*not found|failed to find.*audio|failed to load audio)/iu
    .test(details);
}

export async function verifyArtifact(packRoot, declaration, expectedFile) {
  if (!declaration || typeof declaration.file !== "string" || declaration.file !== expectedFile ||
      typeof declaration.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(declaration.sha256) || /^0+$/.test(declaration.sha256)) {
    throw asError("IMPORT_ASR_ENGINE_UNAVAILABLE");
  }
  const root = await fs.realpath(packRoot);
  const candidate = path.resolve(root, declaration.file);
  if (!isContained(root, candidate)) throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  const status = await fs.lstat(candidate).catch(() => null);
  if (!status?.isFile() || status.isSymbolicLink()) throw asError("IMPORT_ASR_ENGINE_UNAVAILABLE");
  const resolved = await fs.realpath(candidate);
  if (!isContained(root, resolved)) throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  if (await sha256File(resolved) !== declaration.sha256) throw asError("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  return resolved;
}

export function buildArguments(modelPath, mediaPath, outputPrefix, language = "auto") {
  if (typeof language !== "string" || !/^(auto|[a-z]{2,3}(?:[-_][a-z0-9]{2,8})?)$/iu.test(language)) {
    throw asError("IMPORT_ASR_INVALID_REQUEST");
  }
  return [
    "--model", nativeToolPath(modelPath),
    "--file", nativeToolPath(mediaPath),
    "--output-file", nativeToolPath(outputPrefix),
    "--language", language.toLowerCase().replace("_", "-"),
    ...FIXED_ARGUMENTS,
  ];
}

export function isVideoMedia(mediaPath) {
  return VIDEO_EXTENSIONS.has(path.extname(mediaPath).toLowerCase());
}

export function buildEmbeddedSubtitleArguments(mediaPath, subtitlePath) {
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-i", nativeToolPath(mediaPath),
    "-map", "0:s:0", "-vn", "-an", "-dn", "-c:s", "srt", "-f", "srt",
    nativeToolPath(subtitlePath),
  ];
}

export function buildVideoTextProbeArguments(mediaPath, outputPattern) {
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-i", nativeToolPath(mediaPath),
    "-an", "-sn", "-dn", "-t", "1800",
    "-vf", "fps=1/10,scale=480:-2:flags=area,format=gray",
    "-frames:v", "180", nativeToolPath(outputPattern),
  ];
}

export function buildVideoOcrFrameArguments(mediaPath, seconds, outputPath) {
  if (!Number.isFinite(seconds) || seconds < 0) throw asError("IMPORT_ASR_INVALID_REQUEST");
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-ss", seconds.toFixed(3), "-i", nativeToolPath(mediaPath),
    "-an", "-sn", "-dn", "-frames:v", "1",
    "-vf", "scale='min(1920,iw)':-2:flags=lanczos",
    nativeToolPath(outputPath),
  ];
}

function parsePortableGraymap(value) {
  if (!Buffer.isBuffer(value) || value.length < 16) throw asError("IMPORT_ASR_VIDEO_PROBE_FAILED");
  let offset = 0;
  const tokens = [];
  while (tokens.length < 4 && offset < value.length) {
    while (offset < value.length && /\s/u.test(String.fromCharCode(value[offset]))) offset += 1;
    if (value[offset] === 0x23) {
      while (offset < value.length && value[offset] !== 0x0a) offset += 1;
      continue;
    }
    const start = offset;
    while (offset < value.length && !/\s/u.test(String.fromCharCode(value[offset]))) offset += 1;
    tokens.push(value.subarray(start, offset).toString("ascii"));
  }
  while (offset < value.length && /\s/u.test(String.fromCharCode(value[offset]))) offset += 1;
  const [magic, widthValue, heightValue, maximumValue] = tokens;
  const width = Number(widthValue);
  const height = Number(heightValue);
  if (magic !== "P5" || !Number.isSafeInteger(width) || !Number.isSafeInteger(height) ||
      width <= 0 || height <= 0 || maximumValue !== "255" || value.length - offset !== width * height) {
    throw asError("IMPORT_ASR_VIDEO_PROBE_FAILED");
  }
  return { width, height, pixels: value.subarray(offset) };
}

export function selectStableTextFrameIndexes(frames) {
  if (!Array.isArray(frames) || frames.length < 2 || frames.length > 180) return [];
  const parsed = frames.map(parsePortableGraymap);
  const selected = [];
  for (let index = 1; index < parsed.length && selected.length < 12; index += 1) {
    const current = parsed[index];
    const previous = parsed[index - 1];
    if (current.width !== previous.width || current.height !== previous.height) continue;
    let edges = 0;
    let difference = 0;
    let samples = 0;
    for (let y = 1; y < current.height; y += 2) {
      for (let x = 1; x < current.width; x += 2) {
        const position = y * current.width + x;
        const pixel = current.pixels[position];
        if (Math.abs(pixel - current.pixels[position - 1]) > 36 ||
            Math.abs(pixel - current.pixels[position - current.width]) > 36) edges += 1;
        difference += Math.abs(pixel - previous.pixels[position]);
        samples += 1;
      }
    }
    const edgeDensity = samples === 0 ? 0 : edges / samples;
    const meanDifference = samples === 0 ? 255 : difference / samples;
    if (edgeDensity >= 0.035 && edgeDensity <= 0.45 && meanDifference <= 12) selected.push(index);
  }
  return selected;
}

function timestamp(milliseconds) {
  const value = Math.max(0, Math.round(milliseconds));
  const hours = Math.floor(value / 3_600_000);
  const minutes = Math.floor(value / 60_000) % 60;
  const seconds = Math.floor(value / 1_000) % 60;
  const millis = value % 1_000;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}

function cleanText(value) {
  if (typeof value !== "string") return "";
  const withoutControls = Array.from(value, (character) => {
    const codePoint = character.codePointAt(0);
    return codePoint === 0x7f || (codePoint < 0x20 && codePoint !== 0x09 && codePoint !== 0x0a && codePoint !== 0x0d)
      ? " "
      : character;
  }).join("");
  return withoutControls.replace(/\s+/g, " ").trim();
}

function markdownText(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function normalizeSegment(startMs, endMs, text) {
  const start = Number(startMs);
  const end = Number(endMs);
  const safeText = cleanText(text);
  if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end < start || !safeText) return null;
  return { startMs: Math.round(start), endMs: Math.round(end), text: safeText };
}

export function parseWhisperJson(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  const source = Array.isArray(value.transcription) ? value.transcription :
    Array.isArray(value.segments) ? value.segments : null;
  if (!source || source.length > MAX_SEGMENTS) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  const segments = source.map((item) => {
    if (item?.offsets && Number.isFinite(item.offsets.from) && Number.isFinite(item.offsets.to)) {
      return normalizeSegment(item.offsets.from, item.offsets.to, item.text);
    }
    if (Number.isFinite(item?.start) && Number.isFinite(item?.end)) {
      return normalizeSegment(item.start * 1_000, item.end * 1_000, item.text);
    }
    if (Number.isFinite(item?.t0) && Number.isFinite(item?.t1)) {
      return normalizeSegment(item.t0 * 10, item.t1 * 10, item.text);
    }
    return null;
  }).filter(Boolean);
  if (!segments.length) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  const language = cleanText(value.result?.language || value.language || "unknown").slice(0, 32) || "unknown";
  const confidenceValue = Number(value.result?.language_probability ?? value.language_probability);
  const languageConfidence = Number.isFinite(confidenceValue) ? Math.min(1, Math.max(0, confidenceValue)) : null;
  return { segments, language, languageConfidence };
}

function parseClock(value) {
  const match = /^(\d{1,3}):(\d{2}):(\d{2})[,.](\d{3})$/.exec(value.trim());
  if (!match) return null;
  return Number(match[1]) * 3_600_000 + Number(match[2]) * 60_000 + Number(match[3]) * 1_000 + Number(match[4]);
}

export function parseTimedText(value) {
  if (typeof value !== "string" || Buffer.byteLength(value, "utf8") > MAX_TRANSCRIPT_BYTES) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  const normalized = value.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
  const blocks = normalized.split(/\n{2,}/);
  const segments = [];
  for (const block of blocks) {
    const lines = block.split("\n").map((line) => line.trim());
    const timingIndex = lines.findIndex((line) => line.includes("-->"));
    if (timingIndex < 0) continue;
    const timing = lines[timingIndex].match(/^(\d{1,3}:\d{2}:\d{2}[,.]\d{3})\s*-->\s*(\d{1,3}:\d{2}:\d{2}[,.]\d{3})/);
    if (!timing) continue;
    const segment = normalizeSegment(parseClock(timing[1]), parseClock(timing[2]), lines.slice(timingIndex + 1).join(" "));
    if (segment) segments.push(segment);
    if (segments.length > MAX_SEGMENTS) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  }
  if (!segments.length) throw asError("IMPORT_ASR_OUTPUT_INVALID");
  return { segments, language: "unknown", languageConfidence: null };
}

export function renderTranscript(result, sourceName, provenance = "authorized-local-asr") {
  const confidence = result.languageConfidence == null ? "unknown" : result.languageConfidence.toFixed(3);
  const safeSource = cleanText(sourceName).replace(/[\r\n]/g, " ").slice(0, 500);
  const lines = [
    "---",
    `engine: ${ENGINE_VERSION}`,
    `model: ${MODEL_ID}`,
    `language: ${JSON.stringify(result.language)}`,
    `languageConfidence: ${confidence}`,
    `source: ${JSON.stringify(safeSource)}`,
    `provenance: ${provenance}`,
    "---",
    "",
    "# Transcript",
    "",
  ];
  let anchorMs = null;
  for (const segment of result.segments) {
    if (anchorMs == null || segment.startMs - anchorMs >= 45_000) {
      anchorMs = segment.startMs;
      lines.push(`## [${timestamp(segment.startMs)}]`, "");
    }
    lines.push(markdownText(segment.text), "");
  }
  return `${lines.join("\n")}\n`;
}

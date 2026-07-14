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
  "--language", "auto",
  "--output-json",
  "--no-prints",
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

export function buildArguments(modelPath, mediaPath, outputPrefix) {
  return [
    "--model", modelPath,
    "--file", mediaPath,
    "--output-file", outputPrefix,
    ...FIXED_ARGUMENTS,
  ];
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

export function renderTranscript(result, sourceName) {
  const confidence = result.languageConfidence == null ? "unknown" : result.languageConfidence.toFixed(3);
  const safeSource = cleanText(sourceName).replace(/[\r\n]/g, " ").slice(0, 500);
  const lines = [
    "---",
    `engine: ${ENGINE_VERSION}`,
    `model: ${MODEL_ID}`,
    `language: ${JSON.stringify(result.language)}`,
    `languageConfidence: ${confidence}`,
    `source: ${JSON.stringify(safeSource)}`,
    "provenance: authorized-local-asr",
    "---",
    "",
    "# Transcript",
    "",
  ];
  for (const segment of result.segments) {
    lines.push(`- [${timestamp(segment.startMs)} --> ${timestamp(segment.endMs)}] ${markdownText(segment.text)}`);
  }
  return `${lines.join("\n")}\n`;
}

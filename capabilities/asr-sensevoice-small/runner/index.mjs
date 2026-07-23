/* global process */
import { Buffer } from "node:buffer";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import {
  ENGINE_VERSION,
  MAX_DECODED_BYTES,
  MODEL_ID,
  assertProviderWasUsed,
  buildEmbeddedSubtitleArguments,
  buildFfmpegArguments,
  buildSenseVoiceArguments,
  classifyExecutionError,
  executeWithProviderFallback,
  ffmpegRelativePath,
  parseEmbeddedSubtitle,
  parseSenseVoiceStdout,
  preferredProviders,
  renderEmbeddedTranscript,
  renderTranscript,
  resolveStagingMedia,
  restrictedEnvironment,
  sherpaRelativePath,
  verifySignedFile,
} from "./core.mjs";

const execFileAsync = promisify(execFile);
const MAX_RPC_BYTES = 1024 * 1024;
const DECODE_TIMEOUT_MS = 30 * 60 * 1000;
const ASR_TIMEOUT_MS = 2 * 60 * 60 * 1000;
const MAX_ENGINE_BUFFER_BYTES = 2 * 1024 * 1024;
const packRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function readRpc() {
  process.stdin.setEncoding("utf8");
  let input = "";
  for await (const chunk of process.stdin) {
    input += chunk;
    if (Buffer.byteLength(input, "utf8") > MAX_RPC_BYTES) throw new Error("IMPORT_ASR_INVALID_REQUEST");
  }
  try { return JSON.parse(input.trim()); } catch { throw new Error("IMPORT_ASR_INVALID_REQUEST"); }
}

function writeFailure(id, code) {
  const message = "The authorized local SenseVoice ASR helper could not complete the request.";
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: id ?? null, result: null, error: { code: -32020, message, data: { code } } })}\n`);
}

async function runFile(program, arguments_, options, stage) {
  try {
    return await execFileAsync(program, arguments_, {
      ...options,
      windowsHide: true,
      killSignal: "SIGKILL",
      encoding: "utf8",
      maxBuffer: MAX_ENGINE_BUFFER_BYTES,
    });
  } catch (error) {
    throw new Error(classifyExecutionError(error, stage), { cause: error });
  }
}

let rpc;
let temporaryRoot;
let completed = false;
try {
  rpc = await readRpc();
  const params = rpc?.params;
  if (rpc?.jsonrpc !== "2.0" || !params || params.operation !== "extract" ||
      params.input?.kind !== "file" || !params.localAsrAuthorized) {
    throw new Error("IMPORT_ASR_INVALID_REQUEST");
  }
  const mediaLocator = params.chainedInput || params.input.locator;
  const { stagingRoot, mediaPath } = await resolveStagingMedia(params.projectRoot, params.stagingRoot, mediaLocator);
  const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
  if (manifest.packId !== "asr-sensevoice-small" || manifest.protocolVersion !== "2") {
    throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }

  const ffmpeg = await verifySignedFile(packRoot, manifest, ffmpegRelativePath());
  temporaryRoot = await fs.mkdtemp(path.join(stagingRoot, ".sensevoice-output-"));
  const environment = restrictedEnvironment(packRoot, temporaryRoot);
  const embeddedSubtitlePath = path.join(temporaryRoot, "embedded.srt");
  let embeddedTranscript = null;
  try {
    await runFile(ffmpeg, buildEmbeddedSubtitleArguments(mediaPath, embeddedSubtitlePath), {
      cwd: temporaryRoot,
      env: environment,
      timeout: DECODE_TIMEOUT_MS,
    }, "decode");
    const embeddedStatus = await fs.lstat(embeddedSubtitlePath).catch(() => null);
    if (embeddedStatus?.isFile() && !embeddedStatus.isSymbolicLink() && embeddedStatus.size > 0 && embeddedStatus.size <= MAX_ENGINE_BUFFER_BYTES) {
      embeddedTranscript = parseEmbeddedSubtitle(await fs.readFile(embeddedSubtitlePath, "utf8"));
    }
  } catch {
    embeddedTranscript = null;
  }
  await fs.rm(embeddedSubtitlePath, { force: true }).catch(() => {});

  let markdown;
  let safeMetadata;
  let warnings = [];
  if (embeddedTranscript) {
    markdown = renderEmbeddedTranscript(embeddedTranscript, path.basename(mediaPath));
    safeMetadata = {
      engine: "ffmpeg",
      model: null,
      provider: "embedded_subtitle",
      attemptedProviders: [],
      language: "unknown",
      languageConfidence: null,
      emotion: "unknown",
      event: "subtitle",
      timingKind: "cue_start",
      text: embeddedTranscript.text,
      segments: embeddedTranscript.segments,
      tokenTimings: [],
      provenance: "authorized-local-embedded-subtitle",
    };
  } else {
    const [sherpa, model, tokens] = await Promise.all([
      verifySignedFile(packRoot, manifest, sherpaRelativePath()),
      verifySignedFile(packRoot, manifest, "models/model.int8.onnx"),
      verifySignedFile(packRoot, manifest, "models/tokens.txt"),
    ]);
    const modelSha256 = manifest.files.find((item) => item?.path === "models/model.int8.onnx")?.sha256;
    const tokensSha256 = manifest.files.find((item) => item?.path === "models/tokens.txt")?.sha256;
    if (!/^[0-9a-f]{64}$/.test(modelSha256 || "") || !/^[0-9a-f]{64}$/.test(tokensSha256 || "")) {
      throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
    }
    const decodedPath = path.join(temporaryRoot, "decoded.wav");
    await runFile(ffmpeg, buildFfmpegArguments(mediaPath, decodedPath), {
      cwd: temporaryRoot,
      env: environment,
      timeout: DECODE_TIMEOUT_MS,
    }, "decode");
    const decodedStatus = await fs.lstat(decodedPath).catch(() => null);
    if (!decodedStatus?.isFile() || decodedStatus.isSymbolicLink() || decodedStatus.size <= 44 || decodedStatus.size > MAX_DECODED_BYTES) {
      throw new Error("IMPORT_ASR_DECODE_FAILED");
    }

    const threads = Math.min(8, Math.max(1, os.availableParallelism?.() || os.cpus().length || 1));
    const execution = await executeWithProviderFallback(preferredProviders(), async (provider) => {
      const result = await runFile(
        sherpa,
        buildSenseVoiceArguments(model, tokens, decodedPath, provider, threads),
        { cwd: temporaryRoot, env: environment, timeout: ASR_TIMEOUT_MS },
        "asr",
      );
      assertProviderWasUsed(provider, result.stderr);
      return parseSenseVoiceStdout(result.stdout);
    });
    await fs.rm(decodedPath, { force: true });
    const transcript = execution.value;
    markdown = renderTranscript(transcript, path.basename(mediaPath), execution.provider);
    const firstTimestamp = transcript.tokenTimings[0]?.startMs ?? null;
    safeMetadata = {
      engine: ENGINE_VERSION,
      model: MODEL_ID,
      modelSha256,
      tokensSha256,
      provider: execution.provider,
      attemptedProviders: execution.attemptedProviders,
      language: transcript.language,
      languageConfidence: null,
      emotion: transcript.emotion,
      event: transcript.event,
      timingKind: "token_start",
      text: transcript.text,
      segments: [{ startMs: firstTimestamp, text: transcript.text }],
      tokenTimings: transcript.tokenTimings,
      provenance: "authorized-local-asr",
    };
    warnings = execution.provider === "cpu" && execution.attemptedProviders.length > 1
      ? ["IMPORT_ASR_ACCELERATOR_FALLBACK"] : [];
  }
  const candidatePath = path.join(temporaryRoot, "candidate.md");
  const sourcePath = path.join(temporaryRoot, "source.json");
  const metadataPath = path.join(temporaryRoot, "metadata.json");
  await Promise.all([
    fs.writeFile(candidatePath, markdown, { encoding: "utf8", flag: "wx" }),
    fs.writeFile(sourcePath, JSON.stringify(safeMetadata), { encoding: "utf8", flag: "wx" }),
    fs.writeFile(metadataPath, JSON.stringify(safeMetadata), { encoding: "utf8", flag: "wx" }),
  ]);
  const relative = (value) => path.relative(stagingRoot, value).split(path.sep).join("/");
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: {
    sourceSnapshotPath: relative(sourcePath),
    markdownPath: relative(candidatePath),
    assetPaths: [],
    metadataPath: relative(metadataPath),
    title: `Transcript - ${path.basename(mediaPath)}`,
    textCoverage: 1,
    warnings,
  }, error: null })}\n`);
  completed = true;
} catch (error) {
  const code = typeof error?.message === "string" && /^IMPORT_ASR_[A-Z_]+$/.test(error.message)
    ? error.message : "IMPORT_ASR_ENGINE_FAILED";
  writeFailure(rpc?.id, code);
} finally {
  if (temporaryRoot && !completed) await fs.rm(temporaryRoot, { recursive: true, force: true }).catch(() => {});
}

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
  MAX_SENSEVOICE_BATCH_CHUNKS,
  MAX_SENSEVOICE_CHUNKS,
  MODEL_ID,
  SENSEVOICE_CHUNK_SECONDS,
  assertProviderWasUsed,
  buildChunkedFfmpegArguments,
  buildEmbeddedSubtitleArguments,
  buildSenseVoiceBatchArguments,
  buildVideoOcrFrameArguments,
  buildVideoTextProbeArguments,
  classifyExecutionError,
  executeWithProviderFallback,
  ffmpegRelativePath,
  isNoAudioExecutionError,
  isVideoMedia,
  mergeSenseVoiceTranscripts,
  parseEmbeddedSubtitle,
  parseSenseVoiceBatchStdout,
  preferredProviders,
  renderEmbeddedTranscript,
  renderTranscript,
  resolveStagingMedia,
  restrictedEnvironment,
  selectStableTextFrameIndexes,
  sha256File,
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

let lastProgress = -1;
function writeProgress(current, label) {
  const bounded = Math.max(lastProgress, Math.min(99, Math.max(0, Math.round(current))));
  lastProgress = bounded;
  process.stdout.write(`${JSON.stringify({
    jsonrpc: "2.0",
    method: "import.progress",
    params: { current: bounded, total: 100, label },
  })}\n`);
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

async function decodedChunks(temporaryRoot) {
  const entries = (await fs.readdir(temporaryRoot, { withFileTypes: true }))
    .filter((entry) => entry.isFile() && /^decoded-\d{4}\.wav$/u.test(entry.name))
    .sort((left, right) => left.name.localeCompare(right.name, "en"));
  if (entries.length === 0 || entries.length > MAX_SENSEVOICE_CHUNKS) {
    throw new Error("IMPORT_ASR_DECODE_FAILED");
  }
  let totalBytes = 0;
  const chunks = [];
  for (let index = 0; index < entries.length; index += 1) {
    const expectedName = `decoded-${String(index).padStart(4, "0")}.wav`;
    if (entries[index].name !== expectedName) throw new Error("IMPORT_ASR_DECODE_FAILED");
    const chunkPath = path.join(temporaryRoot, entries[index].name);
    const status = await fs.lstat(chunkPath).catch(() => null);
    if (!status?.isFile() || status.isSymbolicLink() || status.size <= 44) {
      throw new Error("IMPORT_ASR_DECODE_FAILED");
    }
    totalBytes += status.size;
    if (totalBytes > MAX_DECODED_BYTES) throw new Error("IMPORT_ASR_DECODE_FAILED");
    chunks.push({
      path: chunkPath,
      startMs: index * SENSEVOICE_CHUNK_SECONDS * 1_000,
    });
  }
  return chunks;
}

async function readJson(filePath) {
  try { return JSON.parse(await fs.readFile(filePath, "utf8")); }
  catch { return null; }
}

async function writeJsonAtomic(filePath, value) {
  const temporary = `${filePath}.tmp-${process.pid}-${Date.now()}`;
  await fs.writeFile(temporary, JSON.stringify(value), { encoding: "utf8", flag: "wx" });
  await fs.rename(temporary, filePath);
}

async function prepareVideoOcrContinuation(
  ffmpeg,
  mediaPath,
  stagingRoot,
  temporaryRoot,
  environment,
  localOcrAuthorized,
) {
  const probeRoot = path.join(temporaryRoot, "video-text-probe");
  await fs.mkdir(probeRoot, { recursive: true });
  await runFile(
    ffmpeg,
    buildVideoTextProbeArguments(mediaPath, path.join(probeRoot, "probe-%04d.pgm")),
    { cwd: packRoot, env: environment, timeout: DECODE_TIMEOUT_MS },
    "decode",
  );
  const probeFiles = (await fs.readdir(probeRoot))
    .filter((name) => /^probe-\d{4}\.pgm$/u.test(name))
    .sort((left, right) => left.localeCompare(right, "en"));
  const selected = selectStableTextFrameIndexes(
    await Promise.all(probeFiles.map((name) => fs.readFile(path.join(probeRoot, name)))),
  );
  if (selected.length === 0) throw new Error("IMPORT_ASR_NO_SPEECH");
  if (!localOcrAuthorized) throw new Error("IMPORT_VIDEO_FRAME_OCR_REQUIRED");
  const ocrRoot = await fs.mkdtemp(path.join(stagingRoot, ".ocr-input-"));
  const temporaryInputPaths = [];
  for (const [outputIndex, probeIndex] of selected.slice(0, 6).entries()) {
    const output = path.join(ocrRoot, `frame-${String(outputIndex + 1).padStart(3, "0")}.png`);
    await runFile(
      ffmpeg,
      buildVideoOcrFrameArguments(mediaPath, probeIndex * 10, output),
      { cwd: packRoot, env: environment, timeout: DECODE_TIMEOUT_MS },
      "decode",
    );
    temporaryInputPaths.push(path.relative(stagingRoot, output).split(path.sep).join("/"));
  }
  return temporaryInputPaths;
}

let rpc;
let temporaryRoot;
let completed = false;
try {
  rpc = await readRpc();
  if (rpc?.method === "capability.health") {
    const route = rpc?.params?.route;
    if (rpc?.params?.protocolVersion !== "2" || rpc?.params?.capabilityId !== "asr-sensevoice-small" || route !== "media.asr") throw new Error("IMPORT_ASR_INVALID_REQUEST");
    const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
    if (manifest.packId !== "asr-sensevoice-small" || manifest.protocolVersion !== "2") throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
    await Promise.all([
      verifySignedFile(packRoot, manifest, ffmpegRelativePath()),
      verifySignedFile(packRoot, manifest, sherpaRelativePath()),
      verifySignedFile(packRoot, manifest, "models/model.int8.onnx"),
      verifySignedFile(packRoot, manifest, "models/tokens.txt"),
    ]);
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { healthy: true, protocolVersion: "2", capabilityId: "asr-sensevoice-small", route }, error: null })}\n`);
    process.exit(0);
  }
  const params = rpc?.params;
  if (rpc?.jsonrpc !== "2.0" || !params || params.operation !== "extract" ||
      params.input?.kind !== "file" ||
      (!params.localAsrAuthorized && !params.asrProbeOnly)) {
    throw new Error("IMPORT_ASR_INVALID_REQUEST");
  }
  const mediaLocator = params.chainedInput || params.input.locator;
  const { stagingRoot, mediaPath } = await resolveStagingMedia(params.projectRoot, params.stagingRoot, mediaLocator);
  const recognitionLanguage = typeof params.recognitionLanguage === "string"
    ? params.recognitionLanguage : "auto";
  const asrProfile = typeof params.asrProfile === "string" ? params.asrProfile : "balanced";
  writeProgress(2, "asr.preparing");
  const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
  if (manifest.packId !== "asr-sensevoice-small" || manifest.protocolVersion !== "2") {
    throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }

  const ffmpeg = await verifySignedFile(packRoot, manifest, ffmpegRelativePath());
  temporaryRoot = await fs.mkdtemp(path.join(stagingRoot, ".sensevoice-output-"));
  const environment = restrictedEnvironment(packRoot, temporaryRoot);
  const embeddedSubtitlePath = path.join(temporaryRoot, "embedded.srt");
  let embeddedTranscript = null;
  writeProgress(5, "asr.checking_subtitles");
  try {
    await runFile(ffmpeg, buildEmbeddedSubtitleArguments(mediaPath, embeddedSubtitlePath), {
      cwd: packRoot,
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
  if (!embeddedTranscript && params.asrProbeOnly) {
    throw new Error("IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE");
  }
  writeProgress(10, embeddedTranscript ? "asr.finalizing" : "asr.preparing");

  let markdown;
  let safeMetadata;
  let warnings = [];
  let continuation = null;
  if (embeddedTranscript) {
    markdown = renderEmbeddedTranscript(embeddedTranscript, path.basename(mediaPath));
    safeMetadata = {
      engine: "ffmpeg",
      model: null,
      provider: "embedded_subtitle",
      attemptedProviders: [],
      language: "unknown",
      requestedLanguage: recognitionLanguage,
      profile: asrProfile,
      languageConfidence: null,
      emotion: "unknown",
      event: "subtitle",
      timingKind: "cue_start",
      text: embeddedTranscript.text,
      segments: embeddedTranscript.segments,
      tokenTimings: [],
      provenance: "local-embedded-subtitle",
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
    const mediaSha256 = await sha256File(mediaPath);
    const shardRoot = path.join(stagingRoot, "asr-shards", `${mediaSha256}-${MODEL_ID}`);
    await fs.mkdir(shardRoot, { recursive: true });
    const decodeMarkerPath = path.join(shardRoot, "decode.complete.json");
    let chunks = [];
    const decodeMarker = await readJson(decodeMarkerPath);
    if (decodeMarker?.mediaSha256 === mediaSha256 &&
        decodeMarker?.chunkSeconds === SENSEVOICE_CHUNK_SECONDS) {
      try { chunks = await decodedChunks(shardRoot); } catch { chunks = []; }
    }
    let execution;
    try {
    if (chunks.length === 0) {
      await fs.rm(shardRoot, { recursive: true, force: true });
      await fs.mkdir(shardRoot, { recursive: true });
      const decodedPattern = path.join(shardRoot, "decoded-%04d.wav");
      writeProgress(15, "asr.decoding");
      await runFile(ffmpeg, buildChunkedFfmpegArguments(mediaPath, decodedPattern), {
        cwd: packRoot,
        env: environment,
        timeout: DECODE_TIMEOUT_MS,
      }, "decode");
      chunks = await decodedChunks(shardRoot);
      await writeJsonAtomic(decodeMarkerPath, {
        schemaVersion: 1,
        complete: true,
        mediaSha256,
        chunkSeconds: SENSEVOICE_CHUNK_SECONDS,
        chunks: chunks.length,
      });
    } else {
      writeProgress(20, "asr.reusing_shards");
    }
    writeProgress(22, "asr.recognizing");

    const threads = Math.min(8, Math.max(1, os.availableParallelism?.() || os.cpus().length || 1));
      execution = await executeWithProviderFallback(preferredProviders(), async (provider) => {
      const transcripts = [];
      for (let offset = 0; offset < chunks.length; offset += MAX_SENSEVOICE_BATCH_CHUNKS) {
        const batch = chunks.slice(offset, offset + MAX_SENSEVOICE_BATCH_CHUNKS);
        const batchPath = path.join(
          shardRoot,
          `batch-${String(offset).padStart(4, "0")}-${provider}-${recognitionLanguage}.complete.json`,
        );
        const cached = await readJson(batchPath);
        if (cached?.schemaVersion === 1 && cached?.complete === true &&
            cached?.mediaSha256 === mediaSha256 && Array.isArray(cached.transcripts)) {
          transcripts.push(...cached.transcripts);
        } else {
          const result = await runFile(
            sherpa,
            buildSenseVoiceBatchArguments(
              model,
              tokens,
              batch.map((chunk) => chunk.path),
              provider,
              threads,
              recognitionLanguage,
            ),
            { cwd: packRoot, env: environment, timeout: ASR_TIMEOUT_MS },
            "asr",
          );
          assertProviderWasUsed(provider, result.stderr);
          const parsed = parseSenseVoiceBatchStdout(
            result.stdout,
            batch.map((chunk) => chunk.startMs),
          );
          await writeJsonAtomic(batchPath, {
            schemaVersion: 1,
            complete: true,
            mediaSha256,
            provider,
            recognitionLanguage,
            transcripts: parsed,
          });
          transcripts.push(...parsed);
        }
        const recognizedChunks = Math.min(chunks.length, offset + batch.length);
        writeProgress(22 + Math.round((recognizedChunks / chunks.length) * 70), "asr.recognizing");
      }
      return mergeSenseVoiceTranscripts(transcripts);
      });
    } catch (error) {
      const noAudioTrack = error?.message === "IMPORT_ASR_DECODE_FAILED" &&
        isNoAudioExecutionError(error);
      if (error?.message !== "IMPORT_ASR_OUTPUT_INVALID" && !noAudioTrack) throw error;
      if (!isVideoMedia(mediaPath)) throw new Error("IMPORT_ASR_NO_SPEECH");
      const temporaryInputPaths = await prepareVideoOcrContinuation(
        ffmpeg,
        mediaPath,
        stagingRoot,
        temporaryRoot,
        environment,
        params.localOcrAuthorized === true,
      );
      markdown = "# Video text\n\nNo speech was detected. Stable frame text candidates were selected without running hidden OCR.\n";
      safeMetadata = {
        engine: ENGINE_VERSION,
        model: MODEL_ID,
        provider: null,
        requestedLanguage: recognitionLanguage,
        profile: asrProfile,
        speechDetected: false,
        stableFrameCandidates: temporaryInputPaths.length,
        provenance: "authorized-local-video-text-probe",
      };
      continuation = {
        type: "local_ocr",
        temporary_input_paths: temporaryInputPaths,
      };
      warnings = ["IMPORT_ASR_NO_SPEECH_VIDEO_OCR"];
    }
    if (!continuation) {
      const transcript = execution.value;
      markdown = renderTranscript(transcript, path.basename(mediaPath), execution.provider);
      safeMetadata = {
        engine: ENGINE_VERSION,
        model: MODEL_ID,
        modelSha256,
        tokensSha256,
        provider: execution.provider,
        attemptedProviders: execution.attemptedProviders,
        language: transcript.language,
        requestedLanguage: recognitionLanguage,
        profile: asrProfile,
        languageConfidence: null,
        emotion: transcript.emotion,
        event: transcript.event,
        timingKind: "token_start",
        text: transcript.text,
        segments: transcript.segments,
        tokenTimings: transcript.tokenTimings,
        provenance: "authorized-local-asr",
      };
      warnings = execution.provider === "cpu" && execution.attemptedProviders.length > 1
        ? ["IMPORT_ASR_ACCELERATOR_FALLBACK"] : [];
    }
  }
  writeProgress(96, "asr.finalizing");
  const candidatePath = path.join(temporaryRoot, "candidate.md");
  const sourcePath = path.join(temporaryRoot, "source.json");
  const metadataPath = path.join(temporaryRoot, "metadata.json");
  await Promise.all([
    fs.writeFile(candidatePath, markdown, { encoding: "utf8", flag: "wx" }),
    fs.writeFile(sourcePath, JSON.stringify(safeMetadata), { encoding: "utf8", flag: "wx" }),
    fs.writeFile(metadataPath, JSON.stringify(safeMetadata), { encoding: "utf8", flag: "wx" }),
  ]);
  writeProgress(99, "asr.finalizing");
  const relative = (value) => path.relative(stagingRoot, value).split(path.sep).join("/");
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: {
    sourceSnapshotPath: relative(sourcePath),
    markdownPath: relative(candidatePath),
    assetPaths: [],
    metadataPath: relative(metadataPath),
    title: `Transcript - ${path.basename(mediaPath)}`,
    textCoverage: continuation ? null : 1,
    continuation,
    warnings,
  }, error: null })}\n`);
  completed = true;
} catch (error) {
  const code = typeof error?.message === "string" &&
      /^(?:IMPORT_ASR_[A-Z_]+|IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE|IMPORT_VIDEO_FRAME_OCR_REQUIRED)$/.test(error.message)
    ? error.message : "IMPORT_ASR_ENGINE_FAILED";
  writeFailure(rpc?.id, code);
} finally {
  if (temporaryRoot && !completed) await fs.rm(temporaryRoot, { recursive: true, force: true }).catch(() => {});
}

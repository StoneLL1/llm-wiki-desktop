import { Buffer } from "node:buffer";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import {
  ENGINE_VERSION,
  MAX_TRANSCRIPT_BYTES,
  MODEL_ID,
  MODEL_SHA256,
  buildArguments,
  buildEmbeddedSubtitleArguments,
  buildVideoOcrFrameArguments,
  buildVideoTextProbeArguments,
  classifyExecutionError,
  ffmpegRelativePath,
  isNoAudioExecutionError,
  isVideoMedia,
  parseTimedText,
  parseWhisperJson,
  renderTranscript,
  resolveStagingMedia,
  selectStableTextFrameIndexes,
  sha256File,
  verifyArtifact,
} from "./core.mjs";

const execFileAsync = promisify(execFile);
const MAX_RPC_BYTES = 1024 * 1024;
const EXECUTION_TIMEOUT_MS = 30 * 60 * 1000;
const packRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function inventoryDeclaration(manifest, file) {
  const matches = (manifest.files || []).filter((item) => item?.path === file);
  if (matches.length !== 1) throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  return { file, sha256: matches[0].sha256 };
}

async function releaseRuntime(manifest) {
  const binary = process.platform === "win32" ? "bin/whisper-cli.exe" : "bin/whisper-cli";
  const provenancePath = await verifyArtifact(
    packRoot,
    inventoryDeclaration(manifest, "BUILD-PROVENANCE.json"),
    "BUILD-PROVENANCE.json",
  );
  const provenance = JSON.parse(await fs.readFile(provenancePath, "utf8"));
  if (provenance.packId !== "asr-whisper" || provenance.runtimeNetwork !== false ||
      !provenance.buildFeatures?.includes("WHISPER_FFMPEG")) {
    throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }
  return {
    ...inventoryDeclaration(manifest, binary),
    ffmpeg: inventoryDeclaration(manifest, ffmpegRelativePath()),
    model: inventoryDeclaration(manifest, "models/ggml-small.bin"),
  };
}

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
  const message = "The authorized local ASR helper could not complete the request.";
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: id ?? null, result: null, error: { code: -32020, message, data: { code } } })}\n`);
}

function restrictedEnvironment() {
  const result = { NO_COLOR: "1" };
  for (const name of ["SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"]) {
    if (typeof process.env[name] === "string") result[name] = process.env[name];
  }
  return result;
}

async function readBounded(filePath) {
  const status = await fs.stat(filePath).catch(() => null);
  if (!status?.isFile() || status.size <= 0 || status.size > MAX_TRANSCRIPT_BYTES) throw new Error("IMPORT_ASR_OUTPUT_INVALID");
  return fs.readFile(filePath, "utf8");
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

async function runFfmpeg(binary, args, options) {
  try {
    return await execFileAsync(binary, args, {
      ...options,
      windowsHide: true,
      killSignal: "SIGKILL",
      maxBuffer: 1024 * 1024,
      encoding: "utf8",
    });
  } catch (error) {
    throw new Error(classifyExecutionError(error));
  }
}

async function prepareVideoOcrContinuation(
  ffmpeg,
  mediaPath,
  stagingRoot,
  temporaryRoot,
  localOcrAuthorized,
) {
  const probeRoot = path.join(temporaryRoot, "video-text-probe");
  await fs.mkdir(probeRoot, { recursive: true });
  await runFfmpeg(
    ffmpeg,
    buildVideoTextProbeArguments(mediaPath, path.join(probeRoot, "probe-%04d.pgm")),
    { cwd: packRoot, env: restrictedEnvironment(), timeout: EXECUTION_TIMEOUT_MS },
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
    await runFfmpeg(
      ffmpeg,
      buildVideoOcrFrameArguments(mediaPath, probeIndex * 10, output),
      { cwd: packRoot, env: restrictedEnvironment(), timeout: EXECUTION_TIMEOUT_MS },
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
  const params = rpc?.params;
  if (rpc?.method === "capability.health") {
    const route = params?.route;
    if (params?.protocolVersion !== "2" || params?.capabilityId !== "asr-whisper" || route !== "media.asr") {
      throw new Error("IMPORT_ASR_INVALID_REQUEST");
    }
    const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
    const runtimeDeclaration = await releaseRuntime(manifest);
    const binaryName = process.platform === "win32" ? "bin/whisper-cli.exe" : "bin/whisper-cli";
    if (manifest.packId !== "asr-whisper" || manifest.protocolVersion !== "2") {
      throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
    }
    await Promise.all([
      verifyArtifact(packRoot, runtimeDeclaration, binaryName),
      verifyArtifact(packRoot, runtimeDeclaration.ffmpeg, ffmpegRelativePath()),
    ]);
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { healthy: true, protocolVersion: "2", capabilityId: "asr-whisper", route }, error: null })}\n`);
    completed = true;
    process.exit(0);
  }
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

  const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
  const runtimeDeclaration = await releaseRuntime(manifest);
  const runtimeTemp = path.join(stagingRoot, "runtime-temp");
  await fs.mkdir(runtimeTemp, { recursive: true });
  temporaryRoot = await fs.mkdtemp(path.join(runtimeTemp, "asr-output-"));
  const outputPrefix = path.join(temporaryRoot, "transcript");
  if (params.asrProbeOnly) {
    const ffmpeg = await verifyArtifact(
      packRoot,
      runtimeDeclaration.ffmpeg,
      ffmpegRelativePath(),
    );
    const embeddedPath = path.join(temporaryRoot, "embedded.srt");
    let transcript = null;
    try {
      await runFfmpeg(
        ffmpeg,
        buildEmbeddedSubtitleArguments(mediaPath, embeddedPath),
        { cwd: packRoot, env: restrictedEnvironment(), timeout: EXECUTION_TIMEOUT_MS },
      );
      transcript = parseTimedText(await readBounded(embeddedPath));
    } catch {
      transcript = null;
    }
    if (!transcript) throw new Error("IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE");
    const markdown = renderTranscript(
      transcript,
      path.basename(mediaPath),
      "local-embedded-subtitle",
    );
    const metadata = {
      engine: "ffmpeg",
      model: null,
      language: transcript.language,
      requestedLanguage: recognitionLanguage,
      profile: asrProfile,
      languageConfidence: transcript.languageConfidence,
      segmentCount: transcript.segments.length,
      provenance: "local-embedded-subtitle",
    };
    const candidatePath = path.join(temporaryRoot, "candidate.md");
    const sourcePath = path.join(temporaryRoot, "source.json");
    const metadataPath = path.join(temporaryRoot, "metadata.json");
    await Promise.all([
      fs.writeFile(candidatePath, markdown, { encoding: "utf8", flag: "wx" }),
      fs.writeFile(sourcePath, JSON.stringify(metadata), { encoding: "utf8", flag: "wx" }),
      fs.writeFile(metadataPath, JSON.stringify(metadata), { encoding: "utf8", flag: "wx" }),
    ]);
    const relative = (value) => path.relative(stagingRoot, value).split(path.sep).join("/");
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: {
      sourceSnapshotPath: relative(sourcePath),
      markdownPath: relative(candidatePath),
      assetPaths: [],
      metadataPath: relative(metadataPath),
      title: `Embedded transcript - ${path.basename(mediaPath)}`,
      textCoverage: 1,
      continuation: null,
      warnings: [],
    }, error: null })}\n`);
    completed = true;
  }
  if (completed) {
    // The extraction-only probe must not verify or execute the ASR model.
    process.exitCode = 0;
  } else {
  const binaryName = process.platform === "win32" ? "bin/whisper-cli.exe" : "bin/whisper-cli";
  const binary = await verifyArtifact(packRoot, runtimeDeclaration, binaryName);
  if (runtimeDeclaration.model.sha256 !== MODEL_SHA256) throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  const model = await verifyArtifact(packRoot, runtimeDeclaration.model, "models/ggml-small.bin");

  const mediaSha256 = await sha256File(mediaPath);
  const shardRoot = path.join(stagingRoot, "asr-shards");
  await fs.mkdir(shardRoot, { recursive: true });
  const shardPath = path.join(
    shardRoot,
    `${mediaSha256}-${MODEL_ID}-${recognitionLanguage}.complete.json`,
  );
  let transcript = await readJson(shardPath);
  let markdown;
  let safeMetadata;
  let continuation = null;
  let warnings = [];
  if (transcript?.schemaVersion === 1 && transcript?.complete === true &&
      transcript?.mediaSha256 === mediaSha256 && transcript?.recognitionLanguage === recognitionLanguage) {
    transcript = transcript.transcript;
  } else {
    try {
      await execFileAsync(binary, buildArguments(model, mediaPath, outputPrefix, recognitionLanguage), {
        cwd: packRoot,
        env: restrictedEnvironment(),
        windowsHide: true,
        timeout: EXECUTION_TIMEOUT_MS,
        killSignal: "SIGKILL",
        maxBuffer: 1024 * 1024,
        encoding: "utf8",
      });
    } catch (error) {
      if (isVideoMedia(mediaPath) && isNoAudioExecutionError(error)) {
        const ffmpeg = await verifyArtifact(
          packRoot,
          runtimeDeclaration.ffmpeg,
          ffmpegRelativePath(),
        );
        const temporaryInputPaths = await prepareVideoOcrContinuation(
          ffmpeg,
          mediaPath,
          stagingRoot,
          temporaryRoot,
          params.localOcrAuthorized === true,
        );
        markdown = "# Video text\n\nNo audio track was present. Stable frame text candidates were selected without running hidden OCR.\n";
        safeMetadata = {
          engine: ENGINE_VERSION,
          model: MODEL_ID,
          language: "unknown",
          requestedLanguage: recognitionLanguage,
          profile: asrProfile,
          speechDetected: false,
          audioTrackPresent: false,
          stableFrameCandidates: temporaryInputPaths.length,
          provenance: "authorized-local-video-text-probe",
        };
        continuation = {
          type: "local_ocr",
          temporary_input_paths: temporaryInputPaths,
        };
        warnings = ["IMPORT_ASR_NO_AUDIO_TRACK_VIDEO_OCR"];
      } else {
        throw new Error(classifyExecutionError(error));
      }
    }
    if (!continuation) try {
      try {
        transcript = parseWhisperJson(JSON.parse(await readBounded(`${outputPrefix}.json`)));
      } catch (jsonError) {
        const timedTextPath = (await fs.stat(`${outputPrefix}.vtt`).catch(() => null)) ? `${outputPrefix}.vtt` : `${outputPrefix}.srt`;
        try { transcript = parseTimedText(await readBounded(timedTextPath)); }
        catch { throw new Error("IMPORT_ASR_OUTPUT_INVALID", { cause: jsonError }); }
      }
    } catch (error) {
      if (error?.message !== "IMPORT_ASR_OUTPUT_INVALID") throw error;
      if (!isVideoMedia(mediaPath)) throw new Error("IMPORT_ASR_NO_SPEECH");
      const ffmpeg = await verifyArtifact(
        packRoot,
        runtimeDeclaration.ffmpeg,
        ffmpegRelativePath(),
      );
      const temporaryInputPaths = await prepareVideoOcrContinuation(
        ffmpeg,
        mediaPath,
        stagingRoot,
        temporaryRoot,
        params.localOcrAuthorized === true,
      );
      markdown = "# Video text\n\nNo speech was detected. Stable frame text candidates were selected without running hidden OCR.\n";
      safeMetadata = {
        engine: ENGINE_VERSION,
        model: MODEL_ID,
        language: "unknown",
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
      transcript = null;
    }
    if (transcript) {
      await writeJsonAtomic(shardPath, {
        schemaVersion: 1,
        complete: true,
        mediaSha256,
        recognitionLanguage,
        transcript,
      });
    }
  }
  if (!continuation) {
    markdown = renderTranscript(transcript, path.basename(mediaPath));
    safeMetadata = {
      engine: ENGINE_VERSION,
      model: MODEL_ID,
      language: transcript.language,
      requestedLanguage: recognitionLanguage,
      profile: asrProfile,
      languageConfidence: transcript.languageConfidence,
      segmentCount: transcript.segments.length,
      provenance: "authorized-local-asr",
    };
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
    textCoverage: continuation ? null : 1,
    continuation,
    warnings,
  }, error: null })}\n`);
  completed = true;
  }
} catch (error) {
  const code = typeof error?.message === "string" &&
      /^(?:IMPORT_ASR_[A-Z_]+|IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE|IMPORT_VIDEO_FRAME_OCR_REQUIRED)$/.test(error.message)
    ? error.message : "IMPORT_ASR_ENGINE_FAILED";
  writeFailure(rpc?.id, code);
} finally {
  if (temporaryRoot && !completed) await fs.rm(temporaryRoot, { recursive: true, force: true }).catch(() => {});
}

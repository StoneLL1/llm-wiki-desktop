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
  classifyExecutionError,
  currentRuntimeKey,
  parseTimedText,
  parseWhisperJson,
  renderTranscript,
  resolveStagingMedia,
  verifyArtifact,
} from "./core.mjs";

const execFileAsync = promisify(execFile);
const MAX_RPC_BYTES = 1024 * 1024;
const EXECUTION_TIMEOUT_MS = 30 * 60 * 1000;
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

let rpc;
let temporaryRoot;
let completed = false;
try {
  rpc = await readRpc();
  const params = rpc?.params;
  if (rpc?.jsonrpc !== "2.0" || !params || params.operation !== "extract" ||
      params.input?.kind !== "file" || !params.localAsrAuthorized) throw new Error("IMPORT_ASR_INVALID_REQUEST");
  const mediaLocator = params.chainedInput || params.input.locator;
  const { stagingRoot, mediaPath } = await resolveStagingMedia(params.projectRoot, params.stagingRoot, mediaLocator);

  const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
  const runtimeDeclaration = manifest.runtimeArtifacts?.[currentRuntimeKey()];
  if (manifest.audioDecoding?.requiredBuildFeature !== "WHISPER_FFMPEG" ||
      !runtimeDeclaration?.buildFeatures?.includes("WHISPER_FFMPEG") ||
      runtimeDeclaration?.qualificationFixture !== manifest.audioDecoding.qualificationFixture) {
    throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  }
  const binaryName = process.platform === "win32" ? "bin/whisper-cli.exe" : "bin/whisper-cli";
  const binary = await verifyArtifact(packRoot, runtimeDeclaration, binaryName);
  const modelDeclaration = manifest.models?.find((model) => model.id === "small");
  if (modelDeclaration?.sha256 !== MODEL_SHA256) throw new Error("IMPORT_ASR_ENGINE_INTEGRITY_FAILED");
  const model = await verifyArtifact(packRoot, modelDeclaration, "models/ggml-small.bin");

  const runtimeTemp = path.join(stagingRoot, "runtime-temp");
  await fs.mkdir(runtimeTemp, { recursive: true });
  temporaryRoot = await fs.mkdtemp(path.join(runtimeTemp, "asr-output-"));
  const outputPrefix = path.join(temporaryRoot, "transcript");
  try {
    await execFileAsync(binary, buildArguments(model, mediaPath, outputPrefix), {
      cwd: temporaryRoot,
      env: restrictedEnvironment(),
      windowsHide: true,
      timeout: EXECUTION_TIMEOUT_MS,
      killSignal: "SIGKILL",
      maxBuffer: 1024 * 1024,
      encoding: "utf8",
    });
  } catch (error) {
    throw new Error(classifyExecutionError(error));
  }

  let transcript;
  try {
    transcript = parseWhisperJson(JSON.parse(await readBounded(`${outputPrefix}.json`)));
  } catch (jsonError) {
    const timedTextPath = (await fs.stat(`${outputPrefix}.vtt`).catch(() => null)) ? `${outputPrefix}.vtt` : `${outputPrefix}.srt`;
    try { transcript = parseTimedText(await readBounded(timedTextPath)); }
    catch { throw new Error("IMPORT_ASR_OUTPUT_INVALID", { cause: jsonError }); }
  }
  const markdown = renderTranscript(transcript, path.basename(mediaPath));
  const safeMetadata = {
    engine: ENGINE_VERSION,
    model: MODEL_ID,
    language: transcript.language,
    languageConfidence: transcript.languageConfidence,
    segmentCount: transcript.segments.length,
    provenance: "authorized-local-asr",
  };
  const candidatePath = path.join(temporaryRoot, "candidate.md");
  const sourcePath = path.join(temporaryRoot, "source.json");
  const metadataPath = path.join(temporaryRoot, "metadata.json");
  await Promise.all([
    fs.writeFile(candidatePath, markdown, { encoding: "utf8", flag: "wx" }),
    fs.writeFile(sourcePath, JSON.stringify(safeMetadata), { encoding: "utf8", flag: "wx" }),
    fs.writeFile(metadataPath, JSON.stringify(safeMetadata), { encoding: "utf8", flag: "wx" }),
  ]);
  const relative = (value) => path.relative(stagingRoot, value).split(path.sep).join("/");
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: relative(sourcePath), markdownPath: relative(candidatePath), assetPaths: [], metadataPath: relative(metadataPath), title: `Transcript - ${path.basename(mediaPath)}`, textCoverage: 1, warnings: [] }, error: null })}\n`);
  completed = true;
} catch (error) {
  const code = typeof error?.message === "string" && /^IMPORT_ASR_[A-Z_]+$/.test(error.message)
    ? error.message : "IMPORT_ASR_ENGINE_FAILED";
  writeFailure(rpc?.id, code);
} finally {
  if (temporaryRoot && !completed) await fs.rm(temporaryRoot, { recursive: true, force: true }).catch(() => {});
}

/* global process */
import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import {
  FIXED_ARGS,
  YTDLP_WINDOWS_X64_SHA256,
  parseYtDlpMetadata,
  validateBilibiliUrl,
} from "./core.mjs";

const execFileAsync = promisify(execFile);
const MAX_RPC_BYTES = 1024 * 1024;
const MAX_YTDLP_OUTPUT_BYTES = 8 * 1024 * 1024;

async function readRpc() {
  process.stdin.setEncoding("utf8");
  let input = "";
  for await (const chunk of process.stdin) {
    input += chunk;
    if (Buffer.byteLength(input, "utf8") > MAX_RPC_BYTES) throw new Error("IMPORT_WEB_RESPONSE_TOO_LARGE");
  }
  return JSON.parse(input.trim());
}

function failure(id, code, message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: id ?? null, result: null, error: { code: -32010, message, data: { code } } })}\n`);
}

function containedRoot(projectRoot, stagingRoot) {
  const project = path.resolve(projectRoot);
  const staging = path.resolve(project, stagingRoot);
  if (staging !== project && !staging.startsWith(`${project}${path.sep}`)) throw new Error("IMPORT_WEB_POLICY_BLOCKED");
  return staging;
}

async function verifiedBinary() {
  if (process.platform !== "win32" || process.arch !== "x64") throw new Error("IMPORT_WEB_ENGINE_UNAVAILABLE");
  const binary = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "bin", "yt-dlp.exe");
  const bytes = await fs.readFile(binary);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== YTDLP_WINDOWS_X64_SHA256) throw new Error("IMPORT_WEB_ENGINE_INTEGRITY_FAILED");
  return binary;
}

function restrictedEnvironment() {
  const environment = {};
  for (const name of ["SystemRoot", "WINDIR", "TEMP", "TMP"]) {
    if (typeof process.env[name] === "string") environment[name] = process.env[name];
  }
  environment.NO_COLOR = "1";
  return environment;
}

let rpc;
try {
  rpc = await readRpc();
  const params = rpc?.params;
  if (rpc?.jsonrpc !== "2.0" || !params || params.input?.kind !== "url") throw new Error("IMPORT_WEB_INVALID_REQUEST");
  const requestedUrl = validateBilibiliUrl(params.input.normalizedLocator || params.input.locator);
  const stagingRoot = containedRoot(params.projectRoot, params.stagingRoot);
  const binary = await verifiedBinary();
  let stdout;
  try {
    ({ stdout } = await execFileAsync(binary, [...FIXED_ARGS, requestedUrl], {
      cwd: stagingRoot,
      env: restrictedEnvironment(),
      windowsHide: true,
      timeout: 120_000,
      killSignal: "SIGKILL",
      maxBuffer: MAX_YTDLP_OUTPUT_BYTES,
      encoding: "utf8",
    }));
  } catch {
    throw new Error("IMPORT_WEB_UPSTREAM_FAILED");
  }
  let raw;
  try { raw = JSON.parse(stdout); } catch { throw new Error("IMPORT_WEB_STRUCTURE_CHANGED"); }
  const { safe, markdown, remoteAssets } = parseYtDlpMetadata(raw, requestedUrl);
  for (const asset of remoteAssets) {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: asset })}\n`);
  }
  await Promise.all([
    fs.writeFile(path.join(stagingRoot, "candidate.md"), markdown, { encoding: "utf8", flag: "wx" }),
    fs.writeFile(path.join(stagingRoot, "source.json"), JSON.stringify(safe), { encoding: "utf8", flag: "wx" }),
    fs.writeFile(path.join(stagingRoot, "metadata.json"), JSON.stringify(safe), { encoding: "utf8", flag: "wx" }),
  ]);
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: "source.json", markdownPath: "candidate.md", assetPaths: [], metadataPath: "metadata.json", title: safe.title, textCoverage: safe.description ? 0.3 : 0.1, warnings: remoteAssets.length ? [] : ["subtitle_unavailable"] }, error: null })}\n`);
} catch (error) {
  const code = typeof error?.message === "string" && /^IMPORT_WEB_[A-Z_]+$/.test(error.message)
    ? error.message : "IMPORT_WEB_ENGINE_FAILED";
  failure(rpc?.id, code, "The restricted Bilibili metadata helper could not complete the request.");
}

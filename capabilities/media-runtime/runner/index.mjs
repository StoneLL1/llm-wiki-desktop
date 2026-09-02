import { execFile } from "node:child_process";
import { Buffer } from "node:buffer";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { promisify, TextDecoder } from "node:util";

const execFileAsync = promisify(execFile);
const packRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ffmpeg = path.join(packRoot, "runtime", "ffmpeg", "bin", process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg");
const allowedExtensions = new Set(["gif", "wma", "wmv", "srt", "vtt", "ass", "ssa", "lrc"]);
const MAX_RPC_BYTES = 1024 * 1024;

async function readRpc() {
  process.stdin.setEncoding("utf8");
  let input = "";
  for await (const chunk of process.stdin) {
    input += chunk;
    if (Buffer.byteLength(input, "utf8") > MAX_RPC_BYTES) throw new Error("IMPORT_MEDIA_INVALID_REQUEST");
  }
  return JSON.parse(input.trim());
}

function failure(id, code) {
  return { jsonrpc: "2.0", id: id ?? null, result: null, error: { code: -32020, message: "The local media helper could not complete the request.", data: { code } } };
}

function contained(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function subtitleMarkdown(text) {
  return text.replace(/^WEBVTT[^\n]*\n/iu, "").replace(/\d{1,2}:\d{2}(?::\d{2})?[.,]\d{3}\s+-->[^\n]+/gu, "")
    .replace(/^Dialogue:[^,]*(?:,[^,]*){8},/gmu, "").replace(/^\[\d{1,2}:\d{2}(?:\.\d{1,3})?\]/gmu, "")
    .split(/\r?\n/u).map((line) => line.trim()).filter((line) => line && !/^\d+$/u.test(line) && !line.startsWith("[")).join("\n\n");
}

let rpc;
try {
  rpc = await readRpc();
  const params = rpc?.params;
  if (rpc?.method === "capability.health") {
    if (params?.protocolVersion !== "2" || params?.capabilityId !== "media-runtime" || !["media.subtitle", "media.keyframes"].includes(params?.route)) throw new Error("IMPORT_MEDIA_INVALID_REQUEST");
    if (!(await fs.stat(ffmpeg).catch(() => null))?.isFile()) throw new Error("IMPORT_MEDIA_ENGINE_INTEGRITY_FAILED");
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { healthy: true, protocolVersion: "2", capabilityId: "media-runtime", route: params.route }, error: null })}\n`);
    process.exit(0);
  }
  if (rpc?.jsonrpc !== "2.0" || params?.operation !== "extract" || params?.input?.kind !== "file") throw new Error("IMPORT_MEDIA_INVALID_REQUEST");
  const projectRoot = path.resolve(params.projectRoot);
  const stagingRoot = path.resolve(projectRoot, params.stagingRoot);
  const source = path.resolve(stagingRoot, params.chainedInput || params.input.locator);
  if (!contained(projectRoot, stagingRoot) || !contained(stagingRoot, source) || !(await fs.stat(source).catch(() => null))?.isFile()) throw new Error("IMPORT_MEDIA_POLICY_BLOCKED");
  const extension = path.extname(source).slice(1).toLowerCase();
  if (!allowedExtensions.has(extension) || (await fs.stat(source)).size <= 0) throw new Error("IMPORT_MEDIA_INVALID_INPUT");
  const output = await fs.mkdtemp(path.join(stagingRoot, ".media-output-"));
  const sourcePath = path.join(output, `source.${extension || "bin"}`);
  await fs.copyFile(source, sourcePath);
  let markdown;
  let continuation = null;
  const assets = [];
  if (["srt", "vtt", "ass", "ssa", "lrc"].includes(extension)) {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(await fs.readFile(source));
    if (!text.trim()) throw new Error("IMPORT_MEDIA_INVALID_INPUT");
    markdown = `# Subtitle\n\n${subtitleMarkdown(text)}\n`;
  } else {
    const frames = path.join(output, "frames");
    await fs.mkdir(frames);
    await execFileAsync(ffmpeg, ["-nostdin", "-hide_banner", "-loglevel", "error", "-i", source, "-vf", "fps=1/10,scale='min(1280,iw)':-2", "-frames:v", "6", path.join(frames, "frame-%03d.png")], { shell: false, windowsHide: true, timeout: 10 * 60 * 1000, maxBuffer: 1024 * 1024 });
    const framePaths = (await fs.readdir(frames)).filter((name) => /^frame-\d{3}\.png$/u.test(name)).sort().map((name) => path.relative(stagingRoot, path.join(frames, name)).split(path.sep).join("/"));
    if (!framePaths.length) throw new Error("IMPORT_MEDIA_NO_STABLE_FRAMES");
    assets.push(...framePaths);
    continuation = { type: "local_ocr", temporary_input_paths: framePaths };
    markdown = "# Media keyframes\n\nStable frames were selected for explicitly authorized local OCR.\n";
  }
  const markdownPath = path.join(output, "candidate.md");
  const metadataPath = path.join(output, "metadata.json");
  await Promise.all([fs.writeFile(markdownPath, markdown), fs.writeFile(metadataPath, JSON.stringify({ engine: "ffmpeg", version: "8.1.2", route: continuation ? "media.keyframes" : "media.subtitle" }))]);
  const relative = (value) => path.relative(stagingRoot, value).split(path.sep).join("/");
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: relative(sourcePath), markdownPath: relative(markdownPath), assetPaths: assets, metadataPath: relative(metadataPath), title: path.parse(source).name, textCoverage: continuation ? null : 1, continuation, warnings: [] }, error: null })}\n`);
} catch (error) {
  const code = /^IMPORT_MEDIA_[A-Z_]+$/u.test(error?.message || "") ? error.message : "IMPORT_MEDIA_ENGINE_FAILED";
  process.stdout.write(`${JSON.stringify(failure(rpc?.id, code))}\n`);
}

/* global process */
import { Buffer } from "node:buffer";
import fs from "node:fs/promises";
import path from "node:path";
import {
  parseBilibiliHtml,
  parseYtDlpMetadata,
  selectTemporaryAudio,
  validateBilibiliUrl,
} from "./core.mjs";

const MAX_RPC_BYTES = 1024 * 1024;

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

let rpc;
try {
  rpc = await readRpc();
  if (rpc?.method === "capability.health") {
    const route = rpc?.params?.route;
    if (rpc?.params?.protocolVersion !== "2" || rpc?.params?.capabilityId !== "media-metadata" || route !== "web.bilibili.metadata") throw new Error("IMPORT_WEB_INVALID_REQUEST");
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { healthy: true, protocolVersion: "2", capabilityId: "media-metadata", route }, error: null })}\n`);
    process.exit(0);
  }
  const params = rpc?.params;
  if (rpc?.jsonrpc !== "2.0" || !params || params.input?.kind !== "url") throw new Error("IMPORT_WEB_INVALID_REQUEST");
  const requestedUrl = validateBilibiliUrl(params.input.normalizedLocator || params.input.locator);
  const stagingRoot = containedRoot(params.projectRoot, params.stagingRoot);
  const inputPath = path.resolve(stagingRoot, params.chainedInput || "fetched.html");
  if (!inputPath.startsWith(`${stagingRoot}${path.sep}`)) throw new Error("IMPORT_WEB_POLICY_BLOCKED");
  const status = await fs.lstat(inputPath).catch(() => null);
  if (!status?.isFile() || status.isSymbolicLink() || status.size > 16 * 1024 * 1024) throw new Error("IMPORT_WEB_RESPONSE_TOO_LARGE");
  const raw = parseBilibiliHtml(await fs.readFile(inputPath, "utf8"));
  const { safe, markdown, remoteAssets } = parseYtDlpMetadata(raw, requestedUrl);
  if (remoteAssets.length === 0) {
    if (!params.localAsrAuthorized) throw new Error("IMPORT_WEB_SUBTITLE_UNAVAILABLE");
    const audioUrl = selectTemporaryAudio(raw);
    if (!audioUrl) throw new Error("IMPORT_WEB_SUBTITLE_UNAVAILABLE");
    remoteAssets.push({ placeholder: "local-asr-input", url: audioUrl, kind: "temporary_media" });
  }
  for (const asset of remoteAssets) {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: asset })}\n`);
  }
  await Promise.all([
    fs.writeFile(path.join(stagingRoot, "candidate.md"), markdown, { encoding: "utf8", flag: "wx" }),
    fs.writeFile(path.join(stagingRoot, "source.json"), JSON.stringify(safe), { encoding: "utf8", flag: "wx" }),
    fs.writeFile(path.join(stagingRoot, "metadata.json"), JSON.stringify(safe), { encoding: "utf8", flag: "wx" }),
  ]);
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: "source.json", markdownPath: "candidate.md", assetPaths: [], metadataPath: "metadata.json", title: safe.title, textCoverage: safe.description ? 0.3 : 0.1, warnings: [] }, error: null })}\n`);
} catch (error) {
  const code = typeof error?.message === "string" && /^IMPORT_WEB_[A-Z_]+$/.test(error.message)
    ? error.message : "IMPORT_WEB_ENGINE_FAILED";
  failure(rpc?.id, code, "The restricted Bilibili metadata helper could not complete the request.");
}

import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { fetchNodeRuntime } from "./fetch-capability-runtime.mjs";
import { fetchSenseVoiceSources } from "./fetch-sensevoice-sources.mjs";
import { stageSenseVoiceCapability } from "./stage-sensevoice-capability.mjs";

const VERSION = "1.13.4+2024.07.17";
const NODE_VERSION = "22.17.0";
const PREPARATION_REVISION = 3;

function comparePaths(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function targetTriple() {
  const key = `${process.platform}-${process.arch}`;
  return ({
    "win32-x64": "x86_64-pc-windows-msvc",
    "darwin-arm64": "aarch64-apple-darwin",
    "darwin-x64": "x86_64-apple-darwin",
    "linux-x64": "x86_64-unknown-linux-gnu",
  })[key] ?? null;
}

function run(program, arguments_, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, arguments_, { cwd, shell: false, stdio: "inherit", windowsHide: true });
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? resolve() : reject(new Error(`${program} exited with ${code}`)));
  });
}

async function sha256(file) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

async function inventory(root) {
  const files = [];
  async function visit(directory) {
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const candidate = path.join(directory, entry.name);
      const status = await fs.lstat(candidate);
      if (status.isSymbolicLink()) throw new Error(`development capability contains a symbolic link: ${candidate}`);
      if (status.isDirectory()) await visit(candidate);
      else if (status.isFile()) {
        const relative = path.relative(root, candidate).split(path.sep).join("/");
        if (relative !== "manifest.json") files.push({ path: relative, sha256: await sha256(candidate), bytes: status.size });
      }
    }
  }
  await visit(root);
  return files.sort((left, right) => comparePaths(left.path, right.path));
}

async function slimDevelopmentPack(packRoot) {
  // The published Windows sherpa archive duplicates its DLL set under lib/
  // and bundles CUDA/TensorRT providers that are hundreds of MiB. Development
  // qualification requires the CPU path; accelerators remain release-pack
  // concerns when target-specific runtimes are intentionally present.
  await fs.rm(path.join(packRoot, "runtime", "sherpa", "lib"), { recursive: true, force: true });
  if (process.platform === "win32") {
    for (const provider of ["onnxruntime_providers_cuda.dll", "onnxruntime_providers_tensorrt.dll"]) {
      await fs.rm(path.join(packRoot, "runtime", "sherpa", "bin", provider), { force: true });
    }
  }
}

async function validPrepared(packRoot, publicKeyPath, preparationStatePath, target) {
  try {
    const manifest = JSON.parse(await fs.readFile(path.join(packRoot, "manifest.json"), "utf8"));
    const key = (await fs.readFile(publicKeyPath, "utf8")).trim();
    const preparationState = JSON.parse(await fs.readFile(preparationStatePath, "utf8"));
    return manifest.packId === "asr-sensevoice-small" && manifest.version === VERSION
      && manifest.targetTriples?.includes(target) && /^[0-9a-f]{64}$/.test(key)
      && preparationState.revision === PREPARATION_REVISION
      && preparationState.version === VERSION
      && preparationState.target === target;
  } catch {
    return false;
  }
}

export async function prepareSenseVoiceDevelopmentCapability() {
  const target = targetTriple();
  if (!target) throw new Error(`SenseVoice development capability is unsupported on ${process.platform}-${process.arch}`);
  const repositoryRoot = path.join(import.meta.dirname, "..");
  const developmentRoot = path.join(repositoryRoot, ".dev-capabilities");
  const publicKeyPath = path.join(developmentRoot, "development-public-key.hex");
  const preparationStatePath = path.join(developmentRoot, "prepared-state.json");
  const packRoot = path.join(developmentRoot, "installed", "asr-sensevoice-small", VERSION);
  if (await validPrepared(packRoot, publicKeyPath, preparationStatePath, target)) {
    process.stdout.write("SenseVoice development capability is ready.\n");
    return packRoot;
  }

  await fs.mkdir(path.join(developmentRoot, "cache"), { recursive: true });
  const stagedPayloadReady = (await Promise.all([
    path.join(packRoot, "runtime", process.platform === "win32" ? "node.exe" : "node"),
    path.join(packRoot, "models", "model.int8.onnx"),
    path.join(packRoot, "runtime", "ffmpeg", "bin", process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg"),
    path.join(packRoot, "runtime", "sherpa", "bin", process.platform === "win32" ? "sherpa-onnx-offline.exe" : "sherpa-onnx-offline"),
  ].map((candidate) => fs.stat(candidate).catch(() => null)))).every((status) => status?.isFile());
  if (!stagedPayloadReady) {
    await fs.rm(path.join(developmentRoot, "installed"), { recursive: true, force: true });
  }
  await fs.rm(publicKeyPath, { force: true });
  await fs.rm(preparationStatePath, { force: true });
  await fs.rm(path.join(packRoot, "manifest.json"), { force: true });
  const nodeRoot = path.join(developmentRoot, "cache", `node-${target}-${NODE_VERSION}`);
  const cachedNode = path.join(nodeRoot, process.platform === "win32" ? "node.exe" : "bin/node");
  const sourcesRoot = path.join(developmentRoot, "cache", `sensevoice-${target}-${VERSION}`);
  if (!(await fs.stat(cachedNode).catch(() => null))?.isFile()) {
    await fs.rm(nodeRoot, { recursive: true, force: true });
    await fetchNodeRuntime({ target, output: nodeRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json") });
  }
  if (!(await fs.stat(path.join(sourcesRoot, "SOURCE-PROVENANCE.json")).catch(() => null))?.isFile()) {
    await fs.rm(sourcesRoot, { recursive: true, force: true });
    await fetchSenseVoiceSources({ target, output: sourcesRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json") });
  }
  let staged = {
    entrypoint: `runtime/${process.platform === "win32" ? "node.exe" : "node"}`,
    entrypointArgs: ["runner/index.mjs"],
  };
  if (!stagedPayloadReady) {
    await fs.mkdir(path.dirname(packRoot), { recursive: true });
    staged = await stageSenseVoiceCapability({
      target,
      nodeVersion: NODE_VERSION,
      nodeRoot,
      sourcesRoot,
      output: packRoot,
      repositoryRoot,
    });
  }
  await slimDevelopmentPack(packRoot);
  const node = path.join(packRoot, "runtime", process.platform === "win32" ? "node.exe" : "node");
  await run(node, ["--test", path.join(packRoot, "runner", "core.node-test.mjs")], packRoot);
  await run(node, [path.join(packRoot, "runner", "qualification.mjs")], packRoot);

  const files = await inventory(packRoot);
  const executableFiles = [
    staged.entrypoint,
    process.platform === "win32" ? "runtime/ffmpeg/bin/ffmpeg.exe" : "runtime/ffmpeg/bin/ffmpeg",
    process.platform === "win32" ? "runtime/sherpa/bin/sherpa-onnx-offline.exe" : "runtime/sherpa/bin/sherpa-onnx-offline",
  ].sort(comparePaths);
  const manifest = {
    schemaVersion: 2,
    packId: "asr-sensevoice-small",
    version: VERSION,
    protocolVersion: "2",
    targetTriples: [target],
    archiveSha256: "",
    licenseExpression: "Apache-2.0 AND LGPL-3.0-or-later AND MIT",
    entrypoint: staged.entrypoint,
    entrypointArgs: staged.entrypointArgs,
    executableFiles,
    compressedBytes: 0,
    installedBytes: 0,
    signingKeyId: "development-local",
    signature: "",
    files,
  };
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  manifest.signature = sign(null, Buffer.from(JSON.stringify(manifest)), privateKey).toString("hex");
  const publicDer = publicKey.export({ format: "der", type: "spki" });
  await fs.writeFile(path.join(packRoot, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await fs.writeFile(publicKeyPath, `${publicDer.subarray(publicDer.length - 32).toString("hex")}\n`, { encoding: "utf8", flag: "wx" });
  await fs.writeFile(preparationStatePath, `${JSON.stringify({
    revision: PREPARATION_REVISION,
    version: VERSION,
    target,
  }, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await fs.rm(sourcesRoot, { recursive: true, force: true });
  process.stdout.write(`SenseVoice development capability prepared at ${packRoot}\n`);
  return packRoot;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  prepareSenseVoiceDevelopmentCapability().catch((error) => {
    process.stderr.write(`prepare-sensevoice-dev: ${error.message}\n`);
    process.exitCode = 1;
  });
}

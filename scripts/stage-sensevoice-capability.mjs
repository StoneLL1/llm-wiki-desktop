import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

function parse(arguments_) {
  if (arguments_.length % 2 !== 0) throw new Error("every option requires one value");
  const values = new Map();
  for (let index = 0; index < arguments_.length; index += 2) {
    const key = arguments_[index].match(/^--([a-z-]+)$/)?.[1];
    if (!key || values.has(key)) throw new Error("options must be unique --name value pairs");
    values.set(key, arguments_[index + 1]);
  }
  const required = (name) => {
    const value = values.get(name)?.trim();
    if (!value) throw new Error(`--${name} is required`);
    return value;
  };
  return {
    target: required("target"),
    nodeVersion: required("node-version"),
    nodeRoot: path.resolve(required("node-root")),
    sourcesRoot: path.resolve(required("sources-root")),
    output: path.resolve(required("output")),
    repositoryRoot: path.resolve(values.get("repository-root") || path.join(import.meta.dirname, "..")),
  };
}

async function requireFile(candidate, label) {
  const status = await fs.stat(candidate).catch(() => null);
  if (!status?.isFile()) throw new Error(`${label} is missing: ${candidate}`);
  return status;
}

async function requireDirectory(candidate, label) {
  if (!(await fs.stat(candidate).catch(() => null))?.isDirectory()) throw new Error(`${label} is missing: ${candidate}`);
}

async function copyDereferenced(source, destination) {
  await fs.cp(source, destination, {
    recursive: true,
    dereference: true,
    errorOnExist: true,
    force: false,
    preserveTimestamps: true,
  });
}

async function assertNoLinks(directory) {
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    const candidate = path.join(directory, entry.name);
    const status = await fs.lstat(candidate);
    if (status.isSymbolicLink()) throw new Error(`staged payload contains a symbolic link: ${candidate}`);
    if (status.isDirectory()) await assertNoLinks(candidate);
  }
}

async function pruneExecutables(directory, keepPattern, removePattern) {
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    if (!entry.isFile() || keepPattern.test(entry.name) || !removePattern.test(entry.name)) continue;
    await fs.rm(path.join(directory, entry.name));
  }
}

async function findLicenseEvidence(root) {
  const found = [];
  async function visit(directory) {
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory()) await visit(candidate);
      else if (/^(?:copying|license|notice)/i.test(entry.name)) found.push(path.relative(root, candidate).split(path.sep).join("/"));
    }
  }
  await visit(root);
  return found.sort();
}

async function writeCompliance(output, provenance, modelBytes, ffmpegEvidence) {
  const packages = [
    { name: "Node.js", SPDXID: "SPDXRef-NodeJS", versionInfo: provenance.nodeVersion, licenseDeclared: "MIT" },
    { name: "sherpa-onnx", SPDXID: "SPDXRef-SherpaONNX", versionInfo: provenance.sherpa.version, licenseDeclared: "Apache-2.0" },
    { name: "SenseVoiceSmall int8 model", SPDXID: "SPDXRef-SenseVoiceModel", versionInfo: provenance.model.version, licenseDeclared: "Apache-2.0" },
    { name: "FFmpeg", SPDXID: "SPDXRef-FFmpeg", versionInfo: provenance.ffmpeg.version, licenseDeclared: "LGPL-3.0-or-later" },
  ].map((item) => ({ ...item, downloadLocation: "NOASSERTION", filesAnalyzed: false, licenseConcluded: item.licenseDeclared }));
  await fs.writeFile(path.join(output, "SBOM.spdx.json"), `${JSON.stringify({
    spdxVersion: "SPDX-2.3",
    dataLicense: "CC0-1.0",
    SPDXID: "SPDXRef-DOCUMENT",
    name: `llm-wiki-asr-sensevoice-small-${provenance.target}`,
    documentNamespace: `https://llm-wiki.invalid/sbom/asr-sensevoice-small/${provenance.target}`,
    creationInfo: { created: "1970-01-01T00:00:00Z", creators: ["Tool: stage-sensevoice-capability.mjs"] },
    packages,
  }, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await fs.writeFile(path.join(output, "NOTICE.md"), [
    "# SenseVoiceSmall capability third-party notices",
    "",
    "This offline pack contains the sherpa-onnx runtime and an exported SenseVoiceSmall int8 model under Apache-2.0.",
    "The full Apache-2.0 text is included at `licenses/APACHE-2.0.txt`; upstream model attribution is preserved at `models/UPSTREAM-README.md`.",
    "Node.js is distributed under MIT and bundled third-party terms; its complete license is at `runtime/NODE-LICENSE`.",
    "FFmpeg is an LGPLv3-or-later shared build. GPL and nonfree variants are forbidden by the pinned release recipe.",
    `FFmpeg license evidence included by its distributor/build: ${ffmpegEvidence.join(", ")}.`,
    "The exact upstream archives, hashes, target, accelerator preference, CPU fallback, and build kind are recorded in `BUILD-PROVENANCE.json`.",
    "The signed runner and its fixed FFmpeg/sherpa commands initiate no network access; media and model data remain local. Capability packs are maintainer-signed trusted code, not an operating-system sandbox boundary.",
    "",
    `Model bytes: ${modelBytes}.`,
    "",
  ].join("\n"), { encoding: "utf8", flag: "wx" });
}

export async function stageSenseVoiceCapability(options) {
  if (!/^v?\d+\.\d+\.\d+$/.test(options.nodeVersion)) throw new Error("nodeVersion must be exact");
  if (await fs.stat(options.output).catch(() => null)) throw new Error(`output must not already exist: ${options.output}`);
  const source = path.join(options.repositoryRoot, "capabilities", "asr-sensevoice-small");
  const nodeName = process.platform === "win32" ? "node.exe" : "node";
  const nodeSource = process.platform === "win32" ? path.join(options.nodeRoot, "node.exe") : path.join(options.nodeRoot, "bin", "node");
  const sherpaSource = path.join(options.sourcesRoot, "sherpa");
  const modelSource = path.join(options.sourcesRoot, "model");
  const ffmpegSource = path.join(options.sourcesRoot, "ffmpeg");
  await Promise.all([
    requireDirectory(path.join(source, "runner"), "SenseVoice runner"),
    requireDirectory(path.join(source, "licenses"), "SenseVoice licenses"),
    requireFile(nodeSource, "Node executable"),
    requireFile(path.join(options.nodeRoot, "LICENSE"), "Node license"),
    requireDirectory(sherpaSource, "sherpa source"),
    requireDirectory(modelSource, "model source"),
    requireDirectory(ffmpegSource, "FFmpeg source"),
  ]);
  const sourceProvenance = JSON.parse(await fs.readFile(path.join(options.sourcesRoot, "SOURCE-PROVENANCE.json"), "utf8"));
  if (sourceProvenance.schemaVersion !== 1 || sourceProvenance.target !== options.target ||
      !new Set(["cuda", "coreml"]).has(sourceProvenance.sherpa?.accelerator) || sourceProvenance.sherpa?.cpuFallback !== true) {
    throw new Error("SenseVoice source provenance does not match the staged target policy");
  }

  await fs.mkdir(path.join(options.output, "runtime"), { recursive: true });
  await copyDereferenced(path.join(source, "runner"), path.join(options.output, "runner"));
  await copyDereferenced(path.join(source, "licenses"), path.join(options.output, "licenses"));
  await fs.copyFile(nodeSource, path.join(options.output, "runtime", nodeName), fs.constants.COPYFILE_EXCL);
  await fs.copyFile(path.join(options.nodeRoot, "LICENSE"), path.join(options.output, "runtime", "NODE-LICENSE"), fs.constants.COPYFILE_EXCL);
  await copyDereferenced(sherpaSource, path.join(options.output, "runtime", "sherpa"));
  await copyDereferenced(ffmpegSource, path.join(options.output, "runtime", "ffmpeg"));

  const executableSuffix = process.platform === "win32" ? "\\.exe$" : "$";
  await pruneExecutables(
    path.join(options.output, "runtime", "sherpa", "bin"),
    new RegExp(`^sherpa-onnx-offline${executableSuffix}`, "i"),
    process.platform === "win32" ? /^sherpa-onnx.*\.exe$/i : /^sherpa-onnx/,
  );
  await pruneExecutables(
    path.join(options.output, "runtime", "ffmpeg", "bin"),
    new RegExp(`^ffmpeg${executableSuffix}`, "i"),
    process.platform === "win32" ? /^ff(?:play|probe)\.exe$/i : /^ff(?:play|probe)$/i,
  );

  await fs.mkdir(path.join(options.output, "models"));
  const modelStatus = await requireFile(path.join(modelSource, "model.int8.onnx"), "SenseVoice model");
  await fs.copyFile(path.join(modelSource, "model.int8.onnx"), path.join(options.output, "models", "model.int8.onnx"), fs.constants.COPYFILE_EXCL);
  await fs.copyFile(path.join(modelSource, "tokens.txt"), path.join(options.output, "models", "tokens.txt"), fs.constants.COPYFILE_EXCL);
  await fs.copyFile(path.join(modelSource, "README.md"), path.join(options.output, "models", "UPSTREAM-README.md"), fs.constants.COPYFILE_EXCL);
  await fs.mkdir(path.join(options.output, "qualification"));
  await fs.copyFile(path.join(modelSource, "test_wavs", "zh.wav"), path.join(options.output, "qualification", "zh.wav"), fs.constants.COPYFILE_EXCL);

  const ffmpegEvidence = await findLicenseEvidence(path.join(options.output, "runtime", "ffmpeg"));
  if (!ffmpegEvidence.length) throw new Error("FFmpeg payload has no bundled license evidence");
  const provenance = {
    ...sourceProvenance,
    nodeVersion: options.nodeVersion.replace(/^v/, ""),
    runtimeNetwork: false,
    callerFlags: false,
    mediaLimitSeconds: 7200,
  };
  await fs.writeFile(path.join(options.output, "BUILD-PROVENANCE.json"), `${JSON.stringify(provenance, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await writeCompliance(options.output, provenance, modelStatus.size, ffmpegEvidence);
  if (process.platform !== "win32") {
    await Promise.all([
      fs.chmod(path.join(options.output, "runtime", nodeName), 0o755),
      fs.chmod(path.join(options.output, "runtime", "sherpa", "bin", "sherpa-onnx-offline"), 0o755),
      fs.chmod(path.join(options.output, "runtime", "ffmpeg", "bin", "ffmpeg"), 0o755),
    ]);
  }
  await assertNoLinks(options.output);
  return { entrypoint: `runtime/${nodeName}`, entrypointArgs: ["runner/index.mjs"], modelBytes: modelStatus.size };
}

async function main() {
  const result = await stageSenseVoiceCapability(parse(process.argv.slice(2)));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`stage-sensevoice-capability: ${error.message}\n`);
    process.exitCode = 1;
  });
}

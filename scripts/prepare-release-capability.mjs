import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { fetchNodeRuntime } from "./fetch-capability-runtime.mjs";
import { fetchRapidOcrSources } from "./fetch-rapidocr-sources.mjs";
import { fetchSenseVoiceSources } from "./fetch-sensevoice-sources.mjs";
import { stageNodeCapability } from "./stage-node-capability.mjs";
import { stagePreparedCapability } from "./stage-prepared-capability.mjs";
import { stageRapidOcrCapability } from "./stage-rapidocr-capability.mjs";
import { stageSenseVoiceCapability } from "./stage-sensevoice-capability.mjs";

const repositoryRoot = path.resolve(import.meta.dirname, "..");

function parse(values) {
  const options = new Map();
  for (let index = 0; index < values.length; index += 2) options.set(values[index]?.replace(/^--/u, ""), values[index + 1]);
  const required = (name) => {
    const value = options.get(name)?.trim();
    if (!value) throw new Error(`--${name} is required`);
    return value;
  };
  return { pack: required("pack"), target: required("target"), output: path.resolve(required("output")) };
}

function run(program, arguments_, cwd = repositoryRoot, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, arguments_, {
      cwd, shell: false, windowsHide: true, stdio: "inherit",
      env: { ...process.env, ...extraEnv, UV_LINK_MODE: "copy", UV_NO_CONFIG: "1", UV_NO_PROGRESS: "1" },
    });
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? resolve() : reject(new Error(`${program} exited with ${code}`)));
  });
}

async function download(url, declaration, destination) {
  const response = await fetch(url, { redirect: "follow", signal: AbortSignal.timeout(45 * 60 * 1000) });
  if (!response.ok || !response.body || new URL(response.url).protocol !== "https:") throw new Error(`download failed: ${url}`);
  const handle = await fs.open(destination, "wx");
  const hash = createHash("sha256");
  let bytes = 0;
  try {
    for await (const chunk of response.body) {
      bytes += chunk.byteLength;
      if (Number.isSafeInteger(declaration.bytes) && bytes > declaration.bytes) throw new Error("download exceeded locked bytes");
      hash.update(chunk);
      await handle.write(chunk);
    }
  } finally {
    await handle.close();
  }
  if (hash.digest("hex") !== declaration.sha256 || (declaration.bytes && bytes !== declaration.bytes)) {
    await fs.rm(destination, { force: true });
    throw new Error("download differs from the locked SHA-256 or byte count");
  }
}

async function fetchPython(sources, target, work) {
  const declaration = sources.pythonStandalone.distributions[target];
  if (!declaration) throw new Error(`Python does not support ${target}`);
  const archive = path.join(work, declaration.file);
  await download(new URL(declaration.file, sources.pythonStandalone.source), declaration, archive);
  const extracted = path.join(work, "python-extracted");
  await fs.mkdir(extracted);
  await run("tar", ["-xf", archive, "-C", extracted], work);
  const root = path.join(extracted, declaration.root);
  const executable = process.platform === "win32" ? path.join(root, "python.exe") : path.join(root, "bin", "python3");
  if (!(await fs.stat(executable).catch(() => null))?.isFile()) throw new Error("Python archive omitted its executable");
  return { root, executable };
}

async function copyRunner(pack, prepared, sourcePack = pack) {
  await fs.cp(path.join(repositoryRoot, "capabilities", sourcePack, "runner"), path.join(prepared, "runner"), {
    recursive: true, dereference: true, errorOnExist: true, force: false,
  });
}

async function fileInventory(root) {
  const result = [];
  async function visit(directory) {
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory()) await visit(candidate);
      else if (entry.isFile()) result.push({
        path: path.relative(root, candidate).split(path.sep).join("/"),
        bytes: (await fs.stat(candidate)).size,
        sha256: createHash("sha256").update(await fs.readFile(candidate)).digest("hex"),
      });
      else throw new Error("prepared release payload contains a link or special file");
    }
  }
  await visit(root);
  return result.sort((left, right) => left.path.localeCompare(right.path, "en"));
}

async function writeCompliance(prepared, pack, target, runtimeNetwork, sourceNames, sources, details = {}) {
  const evidence = (await fileInventory(prepared)).filter((item) => /(?:^|\/)(?:copying|license|notice)[^/]*$/iu.test(item.path));
  if (!evidence.length) throw new Error(`${pack} contains no redistributed license evidence`);
  const packages = sourceNames.map((name, index) => ({
    name, SPDXID: `SPDXRef-Source-${index + 1}`, versionInfo: sources[name].version,
    downloadLocation: typeof sources[name].source === "string" && sources[name].source.startsWith("https://") ? sources[name].source : "NOASSERTION",
    filesAnalyzed: false, licenseConcluded: "NOASSERTION", licenseDeclared: sources[name].license,
  }));
  await fs.writeFile(path.join(prepared, "NOTICE.md"), [
    `# ${pack} third-party notices`, "",
    "Every runtime, model, dependency lock, upstream coordinate and license expression is frozen in the signed CAPABILITY-CONTRACT and BUILD-PROVENANCE records.",
    "Redistributed license evidence:", ...evidence.map((item) => `- \`${item.path}\``), "",
  ].join("\n"));
  await fs.writeFile(path.join(prepared, "SBOM.spdx.json"), `${JSON.stringify({
    spdxVersion: "SPDX-2.3", dataLicense: "CC0-1.0", SPDXID: "SPDXRef-DOCUMENT",
    name: `llm-wiki-${pack}-${target}`, documentNamespace: `https://llm-wiki.invalid/sbom/${pack}/${target}`,
    creationInfo: { created: "1970-01-01T00:00:00Z", creators: ["Tool: prepare-release-capability.mjs"] }, packages,
  }, null, 2)}\n`);
  await fs.writeFile(path.join(prepared, "BUILD-PROVENANCE.json"), `${JSON.stringify({
    schemaVersion: 1, packId: pack, target, runtimeNetwork, sourceNames, ...details,
    preparedInventory: await fileInventory(prepared),
  }, null, 2)}\n`);
}

async function preparePythonPack(pack, target, work, prepared, sources) {
  const python = await fetchPython(sources, target, work);
  await fs.mkdir(path.join(prepared, "runtime"), { recursive: true });
  await fs.rename(python.root, path.join(prepared, "runtime", "python"));
  const executable = process.platform === "win32"
    ? path.join(prepared, "runtime", "python", "python.exe")
    : path.join(prepared, "runtime", "python", "bin", "python3");
  const layout = pack === "document-layout";
  const lock = path.join(repositoryRoot, layout
    ? "capabilities/document-layout/runner/requirements.lock"
    : "capabilities/document-standard/requirements.lock");
  const sitePackages = path.join(prepared, "runtime", "site-packages");
  await run("uv", ["pip", "install", "--python", executable, "--target", sitePackages, "--require-hashes", "-r", lock]);
  await copyRunner(pack, prepared);
  if (layout) {
    const model = sources.documentLayout.models;
    await run(executable, ["-c", [
      "from huggingface_hub import snapshot_download",
      `snapshot_download(repo_id='ds4sd/docling-models',revision='${model.revision}',local_dir=r'${path.join(prepared, "models").replaceAll("'", "\\'")}')`,
    ].join(";")], prepared);
  }
  await writeCompliance(
    prepared, pack, target, false,
    ["pythonStandalone", layout ? "documentLayout" : "documentStandard"], sources,
    layout ? { modelRevision: sources.documentLayout.models.revision, modelInventory: await fileInventory(path.join(prepared, "models")) } : {},
  );
  return { entrypoint: process.platform === "win32" ? "runtime/python/python.exe" : "runtime/python/bin/python3", entrypointArgs: [layout ? "runner/docling_pack.py" : "runner/markitdown_pack.py"] };
}

function libreOfficeUrl(source, target, file) {
  const folder = target === "x86_64-pc-windows-msvc" ? "win/x86_64/"
    : target === "aarch64-apple-darwin" ? "mac/aarch64/"
      : target === "x86_64-apple-darwin" ? "mac/x86_64/" : "deb/x86_64/";
  return new URL(`${folder}${file}`, source).href;
}

async function findDirectory(root, predicate) {
  const pending = [root];
  while (pending.length) {
    const directory = pending.pop();
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory() && predicate(candidate, entry.name)) return candidate;
      if (entry.isDirectory()) pending.push(candidate);
    }
  }
  return null;
}

async function prepareOffice(target, work, prepared, sources) {
  const python = await fetchPython(sources, target, work);
  await fs.mkdir(path.join(prepared, "runtime"), { recursive: true });
  await fs.rename(python.root, path.join(prepared, "runtime", "python"));
  const declaration = sources.libreOffice.distributions[target];
  const archive = path.join(work, declaration.file);
  await download(libreOfficeUrl(sources.libreOffice.source, target, declaration.file), declaration, archive);
  if (process.platform === "win32") {
    const extracted = path.join(work, "office-msi");
    await fs.mkdir(extracted);
    await run("msiexec.exe", ["/a", archive, "/qn", `TARGETDIR=${extracted}`], work);
    const office = await findDirectory(extracted, (candidate) => candidate.endsWith(`${path.sep}LibreOffice`));
    if (!office) throw new Error("LibreOffice MSI omitted the application root");
    await fs.cp(office, path.join(prepared, "runtime", "libreoffice"), { recursive: true, dereference: true });
  } else if (process.platform === "darwin") {
    const mount = path.join(work, "office-mount");
    await fs.mkdir(mount);
    await run("hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mount, archive], work);
    try { await fs.cp(path.join(mount, "LibreOffice.app"), path.join(prepared, "runtime", "LibreOffice.app"), { recursive: true, dereference: true }); }
    finally { await run("hdiutil", ["detach", mount], work); }
  } else {
    const extracted = path.join(work, "office-debs");
    const installed = path.join(work, "office-installed");
    await Promise.all([fs.mkdir(extracted), fs.mkdir(installed)]);
    await run("tar", ["-xf", archive, "-C", extracted], work);
    const debRoot = await findDirectory(extracted, (_candidate, name) => name === "DEBS");
    if (!debRoot) throw new Error("LibreOffice archive omitted DEBS");
    for (const name of (await fs.readdir(debRoot)).filter((item) => item.endsWith(".deb")).sort()) {
      await run("dpkg-deb", ["-x", path.join(debRoot, name), installed], work);
    }
    await fs.cp(path.join(installed, "opt", "libreoffice26.2"), path.join(prepared, "runtime", "libreoffice"), { recursive: true, dereference: true });
  }
  await copyRunner("office-legacy", prepared);
  await writeCompliance(prepared, "office-legacy", target, false, ["pythonStandalone", "libreOffice"], sources);
  return { entrypoint: process.platform === "win32" ? "runtime/python/python.exe" : "runtime/python/bin/python3", entrypointArgs: ["runner/office_legacy_pack.py"] };
}

async function fetchFfmpeg(target, work, prepared, sources) {
  const declaration = sources.ffmpeg.distributions[target];
  const archive = path.join(work, declaration.file);
  await download(new URL(declaration.file, declaration.source), declaration, archive);
  const extracted = path.join(work, "ffmpeg-extracted");
  await fs.mkdir(extracted);
  await run("tar", ["-xf", archive, "-C", extracted], work);
  const sourceRoot = path.join(extracted, declaration.root);
  const destination = path.join(prepared, "runtime", "ffmpeg");
  if (declaration.kind === "source") {
    await run("./configure", ["--disable-doc", "--disable-debug", "--disable-programs", "--enable-ffmpeg", "--disable-gpl", "--disable-nonfree", `--prefix=${destination}`], sourceRoot);
    await run("make", ["-j2"], sourceRoot);
    await run("make", ["install"], sourceRoot);
    await fs.copyFile(path.join(sourceRoot, "COPYING.LGPLv3"), path.join(destination, "FFMPEG-LICENSE"));
  } else {
    await fs.cp(sourceRoot, destination, { recursive: true, dereference: true });
  }
}

async function installNodeRuntime(target, work, prepared) {
  const nodeRoot = path.join(work, "node");
  await fetchNodeRuntime({ target, output: nodeRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json") });
  const nodeName = process.platform === "win32" ? "node.exe" : "node";
  const node = process.platform === "win32" ? path.join(nodeRoot, nodeName) : path.join(nodeRoot, "bin", nodeName);
  await fs.mkdir(path.join(prepared, "runtime"), { recursive: true });
  await fs.copyFile(node, path.join(prepared, "runtime", nodeName));
  await fs.copyFile(path.join(nodeRoot, "LICENSE"), path.join(prepared, "runtime", "NODE-LICENSE"));
  return `runtime/${nodeName}`;
}

async function prepareMedia(pack, target, work, prepared, sources) {
  const entrypoint = await installNodeRuntime(target, work, prepared);
  await fetchFfmpeg(target, work, prepared, sources);
  await copyRunner(pack, prepared);
  if (pack === "asr-whisper") {
    const source = path.join(work, "whisper-src");
    await run("git", ["init", source], work);
    await run("git", ["-C", source, "remote", "add", "origin", sources.whisper.source], work);
    await run("git", ["-C", source, "fetch", "--depth", "1", "origin", sources.whisper.commit], work);
    await run("git", ["-C", source, "checkout", "--detach", "FETCH_HEAD"], work);
    const build = path.join(work, "whisper-build");
    await run("cmake", ["-S", source, "-B", build, "-DWHISPER_FFMPEG=ON", "-DWHISPER_BUILD_TESTS=OFF", "-DWHISPER_BUILD_EXAMPLES=ON", "-DCMAKE_BUILD_TYPE=Release"], work);
    await run("cmake", ["--build", build, "--config", "Release", "--target", "whisper-cli", "--parallel", "2"], work);
    const binaryName = process.platform === "win32" ? "whisper-cli.exe" : "whisper-cli";
    const pending = [build]; let binary = null;
    while (pending.length && !binary) {
      const directory = pending.pop();
      for (const item of await fs.readdir(directory, { withFileTypes: true })) {
        const candidate = path.join(directory, item.name);
        if (item.isDirectory()) pending.push(candidate);
        else if (item.name === binaryName) { binary = candidate; break; }
      }
    }
    if (!binary) throw new Error("whisper.cpp build omitted whisper-cli");
    await fs.mkdir(path.join(prepared, "bin"));
    await fs.copyFile(binary, path.join(prepared, "bin", binaryName));
    await fs.mkdir(path.join(prepared, "models"));
    await download(sources.whisper.model.source, sources.whisper.model, path.join(prepared, "models", "ggml-small.bin"));
    await fs.copyFile(path.join(source, "LICENSE"), path.join(prepared, "WHISPER-LICENSE"));
    await writeCompliance(prepared, pack, target, false, ["node", "whisper", "ffmpeg"], sources, { buildFeatures: ["WHISPER_FFMPEG"], sourceCommit: sources.whisper.commit });
  } else {
    await writeCompliance(prepared, pack, target, false, ["node", "ffmpeg"], sources);
  }
  return { entrypoint, entrypointArgs: ["runner/index.mjs"] };
}

export async function prepareReleaseCapability({ pack, target, output }) {
  if (await fs.stat(output).catch(() => null)) throw new Error("output already exists");
  const [sources, product] = await Promise.all([
    fs.readFile(path.join(repositoryRoot, "capabilities", "release-sources.json"), "utf8").then(JSON.parse),
    fs.readFile(path.join(repositoryRoot, "capabilities", "product-manifest.json"), "utf8").then(JSON.parse),
  ]);
  const definition = product.definitions.find((item) => item.capabilityId === pack && item.distributionTier === "published");
  if (!definition?.supportedTargets.includes(target)) throw new Error("pack/target is not published");
  const work = `${output}.work-${process.pid}`;
  const prepared = path.join(work, "prepared");
  await fs.mkdir(work, { recursive: true });
  try {
    let staged;
    if (["browser-runtime", "browser-runtime-lite", "media-metadata"].includes(pack)) {
      const nodeRoot = path.join(work, "node");
      await fetchNodeRuntime({ target, output: nodeRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json") });
      const lock = path.join(repositoryRoot, "capabilities", pack, "package-lock.json");
      if ((await fs.stat(lock).catch(() => null))?.isFile()) await run("npm", ["ci", "--ignore-scripts", "--prefix", path.join(repositoryRoot, "capabilities", pack)]);
      let browserRoot = null;
      if (pack === "browser-runtime") {
        browserRoot = path.join(work, "ms-playwright");
        await run("npm", ["exec", "--prefix", path.join(repositoryRoot, "capabilities", pack), "--", "playwright", "install", "chromium"], repositoryRoot, { PLAYWRIGHT_BROWSERS_PATH: browserRoot });
      }
      staged = await stageNodeCapability({ pack, target, nodeVersion: sources.node.version, nodeRoot, output: prepared, browserRoot, repositoryRoot });
    } else if (pack === "asr-sensevoice-small") {
      const nodeRoot = path.join(work, "node");
      const sourceRoot = path.join(work, "sensevoice");
      await fetchNodeRuntime({ target, output: nodeRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json") });
      await fetchSenseVoiceSources({ target, output: sourceRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json") });
      staged = await stageSenseVoiceCapability({ target, nodeVersion: sources.node.version, nodeRoot, sourcesRoot: sourceRoot, output: prepared, repositoryRoot });
    } else if (["ocr-basic", "ocr-cjk-accurate"].includes(pack)) {
      const sourceRoot = path.join(work, "rapidocr");
      await fetchRapidOcrSources({ target, output: sourceRoot, config: path.join(repositoryRoot, "capabilities", "release-sources.json"), repositoryRoot });
      staged = await stageRapidOcrCapability({ pack, target, sourcesRoot: sourceRoot, output: prepared, repositoryRoot });
    } else if (["document-standard", "document-layout"].includes(pack)) {
      staged = await preparePythonPack(pack, target, work, prepared, sources);
    } else if (pack === "office-legacy") {
      staged = await prepareOffice(target, work, prepared, sources);
    } else if (["media-runtime", "asr-whisper"].includes(pack)) {
      staged = await prepareMedia(pack, target, work, prepared, sources);
    } else throw new Error(`unsupported release pack: ${pack}`);
    const modelRoot = path.join(prepared, "models");
    const modelBytes = (await fs.stat(modelRoot).catch(() => null))?.isDirectory()
      ? (await fileInventory(modelRoot)).reduce((sum, item) => sum + item.bytes, 0) : null;
    const result = await stagePreparedCapability({
      pack, target, preparedRoot: prepared, output,
      entrypoint: staged.entrypoint, entrypointArgs: staged.entrypointArgs,
    });
    return { ...result, modelBytes };
  } finally {
    await fs.rm(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const result = await prepareReleaseCapability(parse(process.argv.slice(2)));
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    process.stderr.write(`prepare-release-capability: ${error.message}\n`);
    process.exitCode = 1;
  }
}

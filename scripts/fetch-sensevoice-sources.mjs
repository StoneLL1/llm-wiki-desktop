import { createHash } from "node:crypto";
import { Buffer } from "node:buffer";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
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
    output: path.resolve(required("output")),
    config: path.resolve(values.get("config") || path.join(import.meta.dirname, "..", "capabilities", "release-sources.json")),
  };
}

function run(program, arguments_, cwd, capture = false) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, arguments_, {
      cwd,
      shell: false,
      windowsHide: true,
      // Non-captured child stdout is forwarded to stderr so caller stdout stays parseable.
      stdio: capture ? ["ignore", "pipe", "pipe"] : ["inherit", "pipe", "inherit"],
    });
    if (!capture) child.stdout.on("data", (chunk) => { process.stderr.write(chunk); });
    let stdout = "";
    let stderr = "";
    if (capture) {
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
        if (Buffer.byteLength(stdout, "utf8") > 32 * 1024 * 1024) child.kill("SIGKILL");
      });
      child.stderr.on("data", (chunk) => { stderr += chunk; });
    }
    child.once("error", reject);
    child.once("exit", (code) => code === 0
      ? resolve(stdout)
      : reject(new Error(`${program} exited with ${code}${capture ? `: ${stderr.slice(0, 1000)}` : ""}`)));
  });
}

function validateDeclaration(declaration, label) {
  if (!declaration || typeof declaration.file !== "string" || typeof declaration.root !== "string" ||
      !/^[0-9a-f]{64}$/.test(declaration.sha256) || !Number.isSafeInteger(declaration.bytes) || declaration.bytes <= 0) {
    throw new Error(`${label} declaration is invalid`);
  }
  const source = new URL(declaration.source);
  if (source.protocol !== "https:" || source.username || source.password || source.search || source.hash) {
    throw new Error(`${label} source must be a public HTTPS base URL`);
  }
  return source;
}

async function download(declaration, destination, label) {
  const source = validateDeclaration(declaration, label);
  const url = new URL(declaration.file, source);
  const response = await fetch(url, { redirect: "follow", signal: AbortSignal.timeout(30 * 60 * 1000) });
  if (!response.ok || !response.body || new URL(response.url).protocol !== "https:") {
    throw new Error(`${label} download failed with HTTP ${response.status}`);
  }
  const handle = await fs.open(destination, "wx");
  const hash = createHash("sha256");
  let downloaded = 0;
  try {
    for await (const chunk of response.body) {
      downloaded += chunk.byteLength;
      if (downloaded > declaration.bytes) throw new Error(`${label} exceeds its pinned size`);
      hash.update(chunk);
      await handle.write(chunk);
    }
  } catch (error) {
    await handle.close();
    await fs.rm(destination, { force: true });
    throw error;
  }
  await handle.close();
  if (downloaded !== declaration.bytes || hash.digest("hex") !== declaration.sha256) {
    await fs.rm(destination, { force: true });
    throw new Error(`${label} digest or byte count differs from the pinned release source`);
  }
}

function validateArchiveListing(listing, expectedRoot, label) {
  const prefix = `${expectedRoot}/`;
  const entries = listing.replace(/\r\n?/g, "\n").split("\n").filter(Boolean);
  if (!entries.length) throw new Error(`${label} archive is empty`);
  for (const entry of entries) {
    const normalized = entry.replaceAll("\\", "/");
    if (normalized.includes("\0") || normalized.startsWith("/") ||
        (!normalized.startsWith(prefix) && normalized !== expectedRoot && normalized !== prefix) ||
        normalized.split("/").some((part) => part === "..")) {
      throw new Error(`${label} archive contains a path outside its declared root`);
    }
  }
}

function isContained(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

async function assertSafeLinks(directory, root = directory) {
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    const candidate = path.join(directory, entry.name);
    const status = await fs.lstat(candidate);
    if (status.isSymbolicLink()) {
      const resolved = await fs.realpath(candidate).catch(() => null);
      if (!resolved || !isContained(root, resolved)) throw new Error(`release source link escapes its root: ${candidate}`);
    } else if (status.isDirectory()) {
      await assertSafeLinks(candidate, root);
    }
  }
}

async function fetchArchive(declaration, temporaryRoot, destination, label) {
  const archive = path.join(temporaryRoot, declaration.file);
  await download(declaration, archive, label);
  const listing = await run("tar", ["-tf", archive], temporaryRoot, true);
  validateArchiveListing(listing, declaration.root, label);
  const extraction = path.join(temporaryRoot, `${label}-extracted`);
  await fs.mkdir(extraction);
  await run("tar", ["-xf", archive, "-C", extraction], temporaryRoot);
  const sourceRoot = path.join(extraction, declaration.root);
  if (!(await fs.stat(sourceRoot).catch(() => null))?.isDirectory()) {
    throw new Error(`${label} archive omitted its declared root`);
  }
  await assertSafeLinks(sourceRoot);
  await fs.rename(sourceRoot, destination);
}

async function buildMacFfmpeg(sourceRoot, destination) {
  if (process.platform !== "darwin") throw new Error("source FFmpeg builds are supported only on macOS release runners");
  const configure = path.join(sourceRoot, "configure");
  await run(configure, [
    `--prefix=${destination}`,
    "--disable-debug", "--disable-doc", "--disable-ffplay", "--disable-ffprobe",
    "--disable-static", "--enable-shared", "--enable-small",
    "--disable-network", "--disable-autodetect", "--disable-gpl", "--disable-nonfree",
    "--disable-sdl2", "--disable-vulkan", "--disable-videotoolbox", "--disable-audiotoolbox",
    "--disable-avdevice", "--disable-swscale", "--disable-x86asm",
    "--install-name-dir=@loader_path/../lib",
  ], sourceRoot);
  await run("make", ["-j2"], sourceRoot);
  await run("make", ["install"], sourceRoot);
  await fs.rm(path.join(destination, "include"), { recursive: true, force: true });
  await fs.rm(path.join(destination, "share"), { recursive: true, force: true });
  await fs.rm(path.join(destination, "lib", "pkgconfig"), { recursive: true, force: true });
  const licenses = path.join(destination, "licenses");
  await fs.mkdir(licenses);
  for (const name of ["LICENSE.md", "COPYING.LGPLv2.1", "COPYING.LGPLv3"]) {
    await fs.copyFile(path.join(sourceRoot, name), path.join(licenses, name));
  }
  const compact = (value) => value.replace(/\r\n?/g, "\n").trim().slice(0, 4096);
  return {
    runnerPlatform: `${process.platform}-${process.arch}`,
    osRelease: os.release(),
    compiler: compact(await run("cc", ["--version"], sourceRoot, true)),
    make: compact(await run("make", ["--version"], sourceRoot, true)),
    xcode: compact(await run("xcodebuild", ["-version"], sourceRoot, true)),
    sdk: compact(await run("xcrun", ["--show-sdk-version"], sourceRoot, true)),
  };
}

async function requireFile(candidate, label) {
  if (!(await fs.stat(candidate).catch(() => null))?.isFile()) throw new Error(`${label} is missing: ${candidate}`);
}

export async function fetchSenseVoiceSources({ target, output, config }) {
  if (await fs.stat(output).catch(() => null)) throw new Error(`output must not already exist: ${output}`);
  const sources = JSON.parse(await fs.readFile(config, "utf8"));
  if (sources.schemaVersion !== 1) throw new Error("unsupported release source schema");
  const runtime = sources.senseVoice?.distributions?.[target];
  const model = sources.senseVoice?.model;
  const ffmpeg = sources.ffmpeg?.distributions?.[target];
  if (!runtime || !model || !ffmpeg) throw new Error(`SenseVoice release sources do not support ${target}`);
  runtime.source = sources.senseVoice.source;
  model.source ||= sources.senseVoice.model.source;
  const temporary = `${output}.installing-${process.pid}`;
  await fs.mkdir(temporary, { recursive: false });
  try {
    await fs.mkdir(output);
    const sherpaRoot = path.join(output, "sherpa");
    const modelRoot = path.join(output, "model");
    const ffmpegRoot = path.join(output, "ffmpeg");
    await fetchArchive(runtime, temporary, sherpaRoot, "sherpa-runtime");
    await fetchArchive(model, temporary, modelRoot, "sensevoice-model");
    let ffmpegBuildEnvironment = null;
    if (ffmpeg.kind === "prebuilt") {
      await fetchArchive(ffmpeg, temporary, ffmpegRoot, "ffmpeg-runtime");
    } else if (ffmpeg.kind === "source") {
      const sourceRoot = path.join(output, "ffmpeg-source");
      await fetchArchive(ffmpeg, temporary, sourceRoot, "ffmpeg-source");
      ffmpegBuildEnvironment = await buildMacFfmpeg(sourceRoot, ffmpegRoot);
      await fs.rm(sourceRoot, { recursive: true, force: true });
    } else {
      throw new Error("unsupported FFmpeg release source kind");
    }
    const executable = process.platform === "win32" ? ".exe" : "";
    await requireFile(path.join(sherpaRoot, "bin", `sherpa-onnx-offline${executable}`), "sherpa CLI");
    await requireFile(path.join(modelRoot, "model.int8.onnx"), "SenseVoice model");
    await requireFile(path.join(modelRoot, "tokens.txt"), "SenseVoice tokens");
    await requireFile(path.join(modelRoot, "test_wavs", "zh.wav"), "SenseVoice qualification fixture");
    await requireFile(path.join(ffmpegRoot, "bin", `ffmpeg${executable}`), "FFmpeg CLI");
    await fs.writeFile(path.join(output, "SOURCE-PROVENANCE.json"), `${JSON.stringify({
      schemaVersion: 1,
      target,
      sherpa: { version: sources.senseVoice.version, file: runtime.file, sha256: runtime.sha256, accelerator: runtime.accelerator, cpuFallback: runtime.cpuFallback },
      model: { version: model.version, file: model.file, sha256: model.sha256 },
      ffmpeg: {
        version: sources.ffmpeg.version,
        file: ffmpeg.file,
        sha256: ffmpeg.sha256,
        kind: ffmpeg.kind,
        buildEnvironment: ffmpegBuildEnvironment,
      },
    }, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
    return { output, accelerator: runtime.accelerator };
  } catch (error) {
    await fs.rm(output, { recursive: true, force: true });
    throw error;
  } finally {
    await fs.rm(temporary, { recursive: true, force: true });
  }
}

async function main() {
  const result = await fetchSenseVoiceSources(parse(process.argv.slice(2)));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`fetch-sensevoice-sources: ${error.message}\n`);
    process.exitCode = 1;
  });
}

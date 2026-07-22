import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
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
    output: path.resolve(required("output")),
    config: path.resolve(values.get("config") || path.join(import.meta.dirname, "..", "capabilities", "release-sources.json")),
    repositoryRoot: path.resolve(values.get("repository-root") || path.join(import.meta.dirname, "..")),
  };
}

function run(program, arguments_, cwd, capture = false) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, arguments_, {
      cwd,
      shell: false,
      windowsHide: true,
      env: { ...process.env, UV_LINK_MODE: "copy", UV_NO_CONFIG: "1", UV_NO_PROGRESS: "1" },
      stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
    });
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

function validateHttps(value, label) {
  const url = new URL(value);
  if (url.protocol !== "https:" || url.username || url.password || url.hash) {
    throw new Error(`${label} must use public HTTPS`);
  }
  return url;
}

function validateFileDeclaration(declaration, label) {
  if (!declaration || typeof declaration.file !== "string" ||
      !/^[0-9a-f]{64}$/.test(declaration.sha256) ||
      !Number.isSafeInteger(declaration.bytes) || declaration.bytes <= 0 ||
      typeof declaration.source !== "string") {
    throw new Error(`${label} declaration is invalid`);
  }
  validateHttps(declaration.source, label);
}

async function download(declaration, destination, label, sourceOverride) {
  const source = validateHttps(sourceOverride || declaration.source, label);
  const url = sourceOverride ? new URL(declaration.file, source) : source;
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
    throw new Error(`${label} digest or byte count differs from the pinned source`);
  }
}

function validateArchiveListing(listing, expectedRoot) {
  const prefix = `${expectedRoot}/`;
  const entries = listing.replace(/\r\n?/g, "\n").split("\n").filter(Boolean);
  if (!entries.length) throw new Error("Python archive is empty");
  for (const entry of entries) {
    const normalized = entry.replaceAll("\\", "/");
    if (normalized.includes("\0") || normalized.startsWith("/") ||
        (!normalized.startsWith(prefix) && normalized !== expectedRoot && normalized !== prefix) ||
        normalized.split("/").some((part) => part === "..")) {
      throw new Error("Python archive contains a path outside its declared root");
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
      if (!resolved || !isContained(root, resolved)) throw new Error(`Python runtime link escapes its root: ${candidate}`);
    } else if (status.isDirectory()) {
      await assertSafeLinks(candidate, root);
    }
  }
}

async function findDirectory(root, name) {
  const pending = [root];
  while (pending.length) {
    const directory = pending.pop();
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const candidate = path.join(directory, entry.name);
      if (entry.isDirectory() && entry.name === name) return candidate;
      if (entry.isDirectory() && !entry.name.startsWith("__pycache__")) pending.push(candidate);
    }
  }
  return null;
}

async function requireFile(candidate, label) {
  if (!(await fs.stat(candidate).catch(() => null))?.isFile()) throw new Error(`${label} is missing: ${candidate}`);
}

export async function fetchRapidOcrSources({ target, output, config, repositoryRoot }) {
  if (await fs.stat(output).catch(() => null)) throw new Error(`output must not already exist: ${output}`);
  const declaration = JSON.parse(await fs.readFile(config, "utf8"));
  if (declaration.schemaVersion !== 1) throw new Error("unsupported release source schema");
  const python = declaration.pythonStandalone?.distributions?.[target];
  const rapidOcr = declaration.rapidOcr;
  if (!python || !rapidOcr) throw new Error(`RapidOCR release sources do not support ${target}`);
  python.source = declaration.pythonStandalone.source;
  validateFileDeclaration(python, "Python runtime");
  for (const [name, model] of Object.entries(rapidOcr.models || {})) validateFileDeclaration(model, `${name} model`);
  validateFileDeclaration(rapidOcr.qualificationFixture, "qualification fixture");
  validateFileDeclaration(rapidOcr.wheel, "RapidOCR wheel");

  const lockPath = path.join(repositoryRoot, "capabilities", "ocr-cjk-accurate", "requirements.lock");
  await requireFile(lockPath, "OCR dependency lock");
  const temporary = `${output}.installing-${process.pid}`;
  await fs.mkdir(temporary, { recursive: false });
  try {
    await fs.mkdir(output);
    const archive = path.join(temporary, python.file);
    await download(python, archive, "Python runtime", python.source);
    const listing = await run("tar", ["-tf", archive], temporary, true);
    validateArchiveListing(listing, python.root);
    const extracted = path.join(temporary, "python-extracted");
    await fs.mkdir(extracted);
    await run("tar", ["-xf", archive, "-C", extracted], temporary);
    const pythonRoot = path.join(extracted, python.root);
    if (!(await fs.stat(pythonRoot).catch(() => null))?.isDirectory()) throw new Error("Python archive omitted its declared root");
    await assertSafeLinks(pythonRoot);
    const runtimeRoot = path.join(output, "python");
    await fs.rename(pythonRoot, runtimeRoot);
    const pythonExecutable = process.platform === "win32"
      ? path.join(runtimeRoot, "python.exe")
      : path.join(runtimeRoot, "bin", "python3");
    await requireFile(pythonExecutable, "Python executable");
    await run("uv", [
      "pip", "install", "--python", pythonExecutable, "--system", "--no-build",
      "--require-hashes", "--no-config", "-r", lockPath,
    ], repositoryRoot);

    const rapidOcrPackage = await findDirectory(runtimeRoot, "rapidocr");
    if (!rapidOcrPackage || !isContained(runtimeRoot, rapidOcrPackage)) throw new Error("RapidOCR package was not installed into the staged Python runtime");
    const bundledModels = path.join(rapidOcrPackage, "models");
    const dictionarySource = path.join(bundledModels, rapidOcr.dictionary.file);
    await requireFile(dictionarySource, "PP-OCRv5 dictionary");
    const dictionaryStatus = await fs.stat(dictionarySource);
    const dictionaryHash = createHash("sha256").update(await fs.readFile(dictionarySource)).digest("hex");
    if (dictionaryStatus.size !== rapidOcr.dictionary.bytes || dictionaryHash !== rapidOcr.dictionary.sha256) {
      throw new Error("RapidOCR dictionary differs from its pinned wheel declaration");
    }

    const modelsRoot = path.join(output, "models");
    const qualificationRoot = path.join(output, "qualification");
    await Promise.all([fs.mkdir(modelsRoot), fs.mkdir(qualificationRoot)]);
    for (const [name, model] of Object.entries(rapidOcr.models)) {
      await download(model, path.join(modelsRoot, model.file), `${name} model`);
    }
    await fs.copyFile(dictionarySource, path.join(modelsRoot, rapidOcr.dictionary.file));
    await fs.rm(bundledModels, { recursive: true, force: true });
    await download(
      rapidOcr.qualificationFixture,
      path.join(qualificationRoot, rapidOcr.qualificationFixture.file),
      "qualification fixture",
    );
    await fs.copyFile(lockPath, path.join(output, "requirements.lock"));
    const lockSha256 = createHash("sha256").update(await fs.readFile(lockPath)).digest("hex");
    await fs.writeFile(path.join(output, "SOURCE-PROVENANCE.json"), `${JSON.stringify({
      schemaVersion: 1,
      target,
      python: { version: declaration.pythonStandalone.version, file: python.file, sha256: python.sha256 },
      rapidOcr: { version: rapidOcr.version, wheel: rapidOcr.wheel, dependencyLockSha256: lockSha256 },
      models: rapidOcr.models,
      dictionary: rapidOcr.dictionary,
      qualificationFixture: rapidOcr.qualificationFixture,
      provider: "cpu",
      runtimeNetwork: false,
    }, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
    return { output, provider: "cpu" };
  } catch (error) {
    await fs.rm(output, { recursive: true, force: true });
    throw error;
  } finally {
    await fs.rm(temporary, { recursive: true, force: true });
  }
}

async function main() {
  const result = await fetchRapidOcrSources(parse(process.argv.slice(2)));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`fetch-rapidocr-sources: ${error.message}\n`);
    process.exitCode = 1;
  });
}

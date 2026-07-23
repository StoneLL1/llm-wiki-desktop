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
  };
}

function run(program, arguments_, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, arguments_, { cwd, shell: false, stdio: "inherit", windowsHide: true });
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? resolve() : reject(new Error(`${program} exited with ${code}`)));
  });
}

async function download(url, destination, expectedSha256) {
  const response = await fetch(url, { redirect: "follow", signal: AbortSignal.timeout(10 * 60 * 1000) });
  if (!response.ok || !response.body) throw new Error(`download failed with HTTP ${response.status}`);
  if (new URL(response.url).protocol !== "https:") throw new Error("download redirected outside HTTPS");
  const maximumBytes = 128 * 1024 * 1024;
  const declaredBytes = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredBytes) && declaredBytes > maximumBytes) throw new Error("download exceeds the Node runtime size limit");
  const reader = response.body.getReader();
  const chunks = [];
  const hash = createHash("sha256");
  let downloaded = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    downloaded += value.byteLength;
    if (downloaded > maximumBytes) throw new Error("download exceeds the Node runtime size limit");
    hash.update(value);
    chunks.push(Buffer.from(value));
  }
  const actual = hash.digest("hex");
  if (actual !== expectedSha256) throw new Error(`download digest mismatch: expected ${expectedSha256}, got ${actual}`);
  await fs.writeFile(destination, Buffer.concat(chunks, downloaded), { flag: "wx" });
}

export async function fetchNodeRuntime({ target, output, config }) {
  if (await fs.stat(output).catch(() => null)) throw new Error(`output must not already exist: ${output}`);
  const declaration = JSON.parse(await fs.readFile(config, "utf8"));
  if (declaration.schemaVersion !== 1) throw new Error("unsupported release source schema");
  const distribution = declaration.node?.distributions?.[target];
  if (!distribution || !/^[0-9a-f]{64}$/.test(distribution.sha256)) {
    throw new Error(`no pinned Node distribution for ${target}`);
  }
  const temporary = `${output}.installing-${process.pid}`;
  await fs.mkdir(temporary, { recursive: false });
  try {
    const archive = path.join(temporary, distribution.file);
    await download(new URL(distribution.file, declaration.node.source), archive, distribution.sha256);
    const extraction = path.join(temporary, "extracted");
    await fs.mkdir(extraction);
    await run("tar", ["-xf", archive, "-C", extraction], temporary);
    const sourceRoot = path.join(extraction, distribution.root);
    const status = await fs.stat(sourceRoot).catch(() => null);
    if (!status?.isDirectory()) throw new Error("Node archive did not contain its declared root");
    try {
      await fs.rename(sourceRoot, output);
    } catch (error) {
      if (process.platform !== "win32" || !["EPERM", "EACCES"].includes(error?.code)) throw error;
      try {
        await fs.cp(sourceRoot, output, {
          recursive: true,
          dereference: true,
          errorOnExist: true,
          force: false,
          preserveTimestamps: true,
        });
      } catch (copyError) {
        await fs.rm(output, { recursive: true, force: true });
        throw copyError;
      }
    }
    return { version: declaration.node.version, output };
  } finally {
    await fs.rm(temporary, { recursive: true, force: true });
  }
}

async function main() {
  const result = await fetchNodeRuntime(parse(process.argv.slice(2)));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`fetch-capability-runtime: ${error.message}\n`);
    process.exitCode = 1;
  });
}

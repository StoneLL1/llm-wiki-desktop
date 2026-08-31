import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const NODE_PACKS = new Set(["browser-runtime", "browser-runtime-lite", "media-metadata"]);
const LINUX_CHROMIUM_LIBRARIES = [
  "libasound.so.2", "libatk-1.0.so.0", "libatk-bridge-2.0.so.0", "libcairo.so.2",
  "libcups.so.2", "libdbus-1.so.3", "libexpat.so.1", "libgbm.so.1", "libglib-2.0.so.0",
  "libnss3.so", "libnspr4.so", "libpango-1.0.so.0", "libX11.so.6", "libxcb.so.1",
  "libXcomposite.so.1", "libXdamage.so.1", "libXext.so.6", "libXfixes.so.3",
  "libxkbcommon.so.0", "libXrandr.so.2",
];

function parseOptions(arguments_) {
  if (arguments_.length % 2 !== 0) throw new Error("every option requires one value");
  const options = new Map();
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index].match(/^--([a-z-]+)$/)?.[1];
    if (!name || options.has(name)) throw new Error("options must be unique --name value pairs");
    options.set(name, arguments_[index + 1]);
  }
  const required = (name) => {
    const value = options.get(name)?.trim();
    if (!value) throw new Error(`--${name} is required`);
    return value;
  };
  return {
    pack: required("pack"),
    target: required("target"),
    nodeVersion: required("node-version"),
    nodeRoot: path.resolve(required("node-root")),
    output: path.resolve(required("output")),
    browserRoot: options.has("browser-root") ? path.resolve(required("browser-root")) : null,
    repositoryRoot: options.has("repository-root")
      ? path.resolve(required("repository-root"))
      : path.resolve(import.meta.dirname, ".."),
  };
}

async function requireDirectory(candidate, label) {
  const status = await fs.stat(candidate).catch(() => null);
  if (!status?.isDirectory()) throw new Error(`${label} is not a directory: ${candidate}`);
}

async function requireFile(candidate, label) {
  const status = await fs.stat(candidate).catch(() => null);
  if (!status?.isFile()) throw new Error(`${label} is not a file: ${candidate}`);
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

async function writeComplianceEvidence(output, source, pack, nodeVersion, browserBundled) {
  const lockPath = path.join(source, "package-lock.json");
  const lock = JSON.parse(await fs.readFile(lockPath, "utf8").catch(() => '{"packages":{}}'));
  const packages = Object.entries(lock.packages || {})
    .map(([location, declaration], index) => ({
      name: declaration.name || (location ? path.basename(location) : pack),
      SPDXID: `SPDXRef-Npm-${index}`,
      versionInfo: declaration.version || "NOASSERTION",
      downloadLocation: "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: "NOASSERTION",
      licenseDeclared: typeof declaration.license === "string" ? declaration.license : "NOASSERTION",
    }));
  packages.push({
    name: "Node.js",
    SPDXID: "SPDXRef-NodeJS",
    versionInfo: nodeVersion.replace(/^v/, ""),
    downloadLocation: "https://nodejs.org/dist/",
    filesAnalyzed: false,
    licenseConcluded: "NOASSERTION",
    licenseDeclared: "MIT",
  });
  if (browserBundled) {
    packages.push({
      name: "Chromium (Playwright distribution)",
      SPDXID: "SPDXRef-Chromium",
      versionInfo: "Playwright-locked",
      downloadLocation: "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: "NOASSERTION",
      licenseDeclared: "BSD-3-Clause",
    });
  }
  await fs.writeFile(path.join(output, "SBOM.spdx.json"), `${JSON.stringify({
    spdxVersion: "SPDX-2.3",
    dataLicense: "CC0-1.0",
    SPDXID: "SPDXRef-DOCUMENT",
    name: `llm-wiki-${pack}-${nodeVersion.replace(/^v/, "")}`,
    documentNamespace: `https://llm-wiki.invalid/sbom/${pack}/${nodeVersion.replace(/^v/, "")}`,
    creationInfo: { created: "1970-01-01T00:00:00Z", creators: ["Tool: stage-node-capability.mjs"] },
    packages,
  }, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await fs.writeFile(path.join(output, "NOTICE.md"), [
    `# ${pack} third-party notices`,
    "",
    "This payload includes the official Node.js distribution; its complete license and bundled third-party notices are in `runtime/NODE-LICENSE`.",
    "JavaScript package licenses are represented in `SBOM.spdx.json` and their license files remain inside `node_modules/`.",
    browserBundled
      ? "The Playwright Chromium distribution is included under `runtime/ms-playwright/`; Chromium and bundled third-party license files remain in that tree."
      : "No browser binary is included in this payload.",
    "",
    "The signed manifest inventory covers this notice, the SBOM, and all license evidence.",
    "",
  ].join("\n"), { encoding: "utf8", flag: "wx" });
}

export async function stageNodeCapability(options) {
  if (!NODE_PACKS.has(options.pack)) throw new Error(`unsupported Node capability: ${options.pack}`);
  const source = path.join(options.repositoryRoot, "capabilities", options.pack);
  await requireDirectory(source, "capability source");
  await requireDirectory(options.nodeRoot, "Node distribution root");
  if (!/^v?\d+\.\d+\.\d+$/.test(options.nodeVersion)) {
    throw new Error("nodeVersion must be an exact semantic version");
  }
  if (await fs.stat(options.output).catch(() => null)) {
    throw new Error(`output must not already exist: ${options.output}`);
  }

  const nodeName = process.platform === "win32" ? "node.exe" : "node";
  const nodeSource = process.platform === "win32"
    ? path.join(options.nodeRoot, "node.exe")
    : path.join(options.nodeRoot, "bin", "node");
  const nodeLicense = path.join(options.nodeRoot, "LICENSE");
  await requireFile(nodeSource, "Node executable");
  await requireFile(nodeLicense, "Node license");
  await requireDirectory(path.join(source, "runner"), "capability runner");

  await fs.mkdir(path.join(options.output, "runtime"), { recursive: true });
  await copyDereferenced(path.join(source, "runner"), path.join(options.output, "runner"));
  await fs.copyFile(nodeSource, path.join(options.output, "runtime", nodeName), fs.constants.COPYFILE_EXCL);
  await fs.copyFile(nodeLicense, path.join(options.output, "runtime", "NODE-LICENSE"), fs.constants.COPYFILE_EXCL);

  for (const name of ["package.json", "package-lock.json", "node_modules"]) {
    const candidate = path.join(source, name);
    if (await fs.stat(candidate).catch(() => null)) {
      await copyDereferenced(candidate, path.join(options.output, name));
    }
  }

  if (options.pack === "browser-runtime") {
    if (!options.browserRoot) throw new Error("--browser-root is required for browser-runtime");
    await requireDirectory(options.browserRoot, "Playwright browser root");
    const browserEntries = (await fs.readdir(options.browserRoot))
      .filter((name) => /^chromium(?:_headless_shell)?-/.test(name))
      .sort();
    if (!browserEntries.length) throw new Error("Playwright browser root contains no pinned Chromium payload");
    const destination = path.join(options.output, "runtime", "ms-playwright");
    await fs.mkdir(destination, { recursive: true });
    for (const name of browserEntries) {
      await copyDereferenced(path.join(options.browserRoot, name), path.join(destination, name));
    }
  }

  if (process.platform !== "win32") {
    await fs.chmod(path.join(options.output, "runtime", nodeName), 0o755);
  }
  await fs.writeFile(
    path.join(options.output, "BUILD-PROVENANCE.json"),
    `${JSON.stringify({
      schemaVersion: 1,
      packId: options.pack,
      target: options.target,
      nodeVersion: options.nodeVersion.replace(/^v/, ""),
      browserBundled: options.pack === "browser-runtime",
      networkAtRuntime: options.pack === "browser-runtime",
      runtimeNetwork: options.pack === "browser-runtime",
      linuxSystemLibraries: options.pack === "browser-runtime" ? LINUX_CHROMIUM_LIBRARIES : [],
      linuxSupportContract: options.pack === "browser-runtime"
        ? "A glibc desktop supported by Playwright Chromium with the listed shared libraries; system libraries are not bundled."
        : null,
    }, null, 2)}\n`,
    { encoding: "utf8", flag: "wx" },
  );
  await writeComplianceEvidence(
    options.output,
    source,
    options.pack,
    options.nodeVersion,
    options.pack === "browser-runtime",
  );
  await assertNoLinks(options.output);
  return { entrypoint: `runtime/${nodeName}`, entrypointArgs: ["runner/index.mjs"] };
}

async function main() {
  const result = await stageNodeCapability(parseOptions(process.argv.slice(2)));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`stage-node-capability: ${error.message}\n`);
    process.exitCode = 1;
  });
}

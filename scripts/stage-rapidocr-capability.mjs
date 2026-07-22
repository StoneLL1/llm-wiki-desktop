import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const DEPENDENCY_LICENSES = new Map(Object.entries({
  certifi: "MPL-2.0",
  "charset-normalizer": "MIT",
  colorama: "BSD-3-Clause",
  coloredlogs: "MIT",
  colorlog: "MIT",
  flatbuffers: "Apache-2.0",
  humanfriendly: "MIT",
  idna: "BSD-3-Clause",
  mpmath: "BSD-3-Clause",
  numpy: "BSD-3-Clause",
  omegaconf: "BSD-3-Clause",
  onnxruntime: "MIT",
  "opencv-python": "Apache-2.0 AND LGPL-2.1-only AND LGPL-3.0-only",
  packaging: "Apache-2.0 OR BSD-2-Clause",
  pillow: "HPND",
  protobuf: "BSD-3-Clause",
  pyclipper: "MIT",
  pyreadline3: "BSD-3-Clause",
  pyyaml: "MIT",
  rapidocr: "Apache-2.0",
  requests: "Apache-2.0",
  shapely: "BSD-3-Clause AND LGPL-2.1-only",
  six: "MIT",
  sympy: "BSD-3-Clause",
  tqdm: "MPL-2.0 AND MIT",
  "typing-extensions": "PSF-2.0",
  urllib3: "MIT",
}));

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

function lockedDependencies(value) {
  const packages = [];
  for (const line of value.replace(/\r\n?/g, "\n").split("\n")) {
    const match = line.match(/^([a-z0-9][a-z0-9._-]+)==([^\s\\;]+)/i);
    if (!match) continue;
    const name = match[1].toLowerCase().replaceAll("_", "-");
    const licenseDeclared = DEPENDENCY_LICENSES.get(name);
    if (!licenseDeclared) throw new Error(`OCR dependency license is not reviewed: ${name}`);
    packages.push({ name, versionInfo: match[2], licenseDeclared });
  }
  if (packages.length !== DEPENDENCY_LICENSES.size) throw new Error("OCR dependency lock does not match the reviewed license inventory");
  return packages;
}

async function writeCompliance(output, provenance, dependencyLock, modelBytes, evidence) {
  const dependencies = lockedDependencies(dependencyLock);
  const packages = [
    { name: "CPython", versionInfo: provenance.python.version, licenseDeclared: "PSF-2.0" },
    { name: "python-build-standalone", versionInfo: provenance.python.version.split("+")[1], licenseDeclared: "MPL-2.0" },
    ...dependencies,
    { name: "PP-OCRv5 mobile models", versionInfo: "v3.8.0", licenseDeclared: "Apache-2.0" },
  ].map((item, index) => ({
    ...item,
    SPDXID: `SPDXRef-Package-${index + 1}`,
    downloadLocation: "NOASSERTION",
    filesAnalyzed: false,
    licenseConcluded: item.licenseDeclared,
  }));
  await fs.writeFile(path.join(output, "SBOM.spdx.json"), `${JSON.stringify({
    spdxVersion: "SPDX-2.3",
    dataLicense: "CC0-1.0",
    SPDXID: "SPDXRef-DOCUMENT",
    name: `llm-wiki-ocr-cjk-accurate-${provenance.target}`,
    documentNamespace: `https://llm-wiki.invalid/sbom/ocr-cjk-accurate/${provenance.target}`,
    creationInfo: { created: "1970-01-01T00:00:00Z", creators: ["Tool: stage-rapidocr-capability.mjs"] },
    packages,
  }, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await fs.writeFile(path.join(output, "NOTICE.md"), [
    "# RapidOCR PP-OCRv5 capability third-party notices",
    "",
    "This offline pack contains a relocatable CPython runtime built by python-build-standalone, hash-locked Python wheels, RapidOCR 3.8.1, ONNX Runtime 1.23.2, OpenCV 4.12, and PP-OCRv5 mobile models.",
    "RapidOCR engineering code and the Baidu PP-OCR model family are distributed under Apache-2.0. ONNX Runtime and most helper libraries are MIT/BSD-family software.",
    "The opencv-python wheels redistribute FFmpeg under LGPL-2.1 and, on Linux/macOS, Qt and related libraries under LGPL-3.0. Shapely wheels redistribute GEOS under LGPL-2.1. Their complete license notices remain inside the staged Python runtime.",
    "CPython uses the PSF license; python-build-standalone and selected dependencies include MPL-2.0 terms. Exact package versions and declared SPDX licenses are in `SBOM.spdx.json`.",
    "The bundled runtime preserves package and runtime license files. The signed dependency lock, source hashes, target, and model hashes are recorded in `DEPENDENCY-LOCK.txt` and `BUILD-PROVENANCE.json`.",
    `Runtime license/notice evidence files found: ${evidence.length}.`,
    "The signed runner blocks Python socket APIs and has no cloud fallback. Capability packs are maintainer-signed trusted code, not an operating-system sandbox boundary.",
    "",
    `Model bytes: ${modelBytes}.`,
    "",
  ].join("\n"), { encoding: "utf8", flag: "wx" });
}

export async function stageRapidOcrCapability(options) {
  if (await fs.stat(options.output).catch(() => null)) throw new Error(`output must not already exist: ${options.output}`);
  const source = path.join(options.repositoryRoot, "capabilities", "ocr-cjk-accurate");
  const pythonSource = path.join(options.sourcesRoot, "python");
  const modelsSource = path.join(options.sourcesRoot, "models");
  const qualificationSource = path.join(options.sourcesRoot, "qualification");
  await Promise.all([
    requireDirectory(path.join(source, "runner"), "RapidOCR runner"),
    requireDirectory(pythonSource, "Python runtime"),
    requireDirectory(modelsSource, "PP-OCRv5 models"),
    requireDirectory(qualificationSource, "OCR qualification fixture"),
    requireFile(path.join(options.sourcesRoot, "requirements.lock"), "OCR dependency lock"),
  ]);
  const provenance = JSON.parse(await fs.readFile(path.join(options.sourcesRoot, "SOURCE-PROVENANCE.json"), "utf8"));
  if (provenance.schemaVersion !== 1 || provenance.target !== options.target ||
      provenance.provider !== "cpu" || provenance.runtimeNetwork !== false ||
      provenance.rapidOcr?.version !== "3.8.1") {
    throw new Error("RapidOCR source provenance does not match the staged target policy");
  }

  await fs.mkdir(options.output, { recursive: false });
  await copyDereferenced(path.join(source, "runner"), path.join(options.output, "runner"));
  await fs.mkdir(path.join(options.output, "runtime"));
  await copyDereferenced(pythonSource, path.join(options.output, "runtime", "python"));
  await copyDereferenced(modelsSource, path.join(options.output, "models"));
  await copyDereferenced(qualificationSource, path.join(options.output, "qualification"));
  const dependencyLock = await fs.readFile(path.join(options.sourcesRoot, "requirements.lock"), "utf8");
  const lockSha256 = createHash("sha256").update(dependencyLock).digest("hex");
  if (lockSha256 !== provenance.rapidOcr.dependencyLockSha256) throw new Error("OCR dependency lock differs from source provenance");
  await fs.writeFile(path.join(options.output, "DEPENDENCY-LOCK.txt"), dependencyLock, { encoding: "utf8", flag: "wx" });
  await fs.writeFile(path.join(options.output, "BUILD-PROVENANCE.json"), `${JSON.stringify(provenance, null, 2)}\n`, { encoding: "utf8", flag: "wx" });

  const modelNames = [
    "ch_PP-OCRv5_det_mobile.onnx",
    "ch_PP-OCRv5_rec_mobile.onnx",
    "ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
  ];
  const modelStatuses = await Promise.all(modelNames.map((name) => requireFile(path.join(options.output, "models", name), name)));
  await requireFile(path.join(options.output, "models", "ppocrv5_dict.txt"), "PP-OCRv5 dictionary");
  await requireFile(path.join(options.output, "qualification", "ch_en_num.jpg"), "OCR qualification fixture");
  const modelBytes = modelStatuses.reduce((sum, status) => sum + status.size, 0);
  const evidence = await findLicenseEvidence(path.join(options.output, "runtime", "python"));
  if (evidence.length < 5) throw new Error("Python OCR runtime has insufficient bundled license evidence");
  await writeCompliance(options.output, provenance, dependencyLock, modelBytes, evidence);

  const entrypoint = process.platform === "win32" ? "runtime/python/python.exe" : "runtime/python/bin/python3";
  await requireFile(path.join(options.output, ...entrypoint.split("/")), "staged Python entrypoint");
  if (process.platform !== "win32") await fs.chmod(path.join(options.output, ...entrypoint.split("/")), 0o755);
  await assertNoLinks(options.output);
  return { entrypoint, entrypointArgs: ["runner/index.py"], modelBytes };
}

async function main() {
  const result = await stageRapidOcrCapability(parse(process.argv.slice(2)));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`stage-rapidocr-capability: ${error.message}\n`);
    process.exitCode = 1;
  });
}

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { stageRapidOcrCapability } from "./stage-rapidocr-capability.mjs";

const dependencyLicenses = [
  "certifi", "charset-normalizer", "colorama", "coloredlogs", "colorlog", "flatbuffers",
  "humanfriendly", "idna", "mpmath", "numpy", "omegaconf", "onnxruntime", "opencv-python",
  "packaging", "pillow", "pillow-heif", "protobuf", "pyclipper", "pypdfium2", "pyreadline3", "pyyaml", "rapidocr",
  "requests", "shapely", "six", "sympy", "tqdm", "typing-extensions", "urllib3",
];

async function writeFile(filePath, value = "fixture") {
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, value);
}

async function fixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-stage-rapidocr-"));
  const repositoryRoot = path.join(root, "repository");
  const source = path.join(repositoryRoot, "capabilities", "ocr-cjk-accurate");
  const sourcesRoot = path.join(root, "sources");
  const pythonExecutable = process.platform === "win32"
    ? path.join(sourcesRoot, "python", "python.exe")
    : path.join(sourcesRoot, "python", "bin", "python3");
  const lock = dependencyLicenses.map((name) => `${name}==1.0.0 \\\n    --hash=sha256:${"a".repeat(64)}`).join("\n");
  const lockSha256 = createHash("sha256").update(lock).digest("hex");
  await Promise.all([
    writeFile(path.join(source, "runner", "index.py"), "raise SystemExit(0)\n"),
    writeFile(pythonExecutable),
    writeFile(path.join(sourcesRoot, "python", "LICENSE.txt"), "Python license"),
    ...Array.from({ length: 5 }, (_, index) => writeFile(path.join(sourcesRoot, "python", `package-${index}.dist-info`, "LICENSE"), "license")),
    writeFile(path.join(sourcesRoot, "models", "ch_PP-OCRv5_det_mobile.onnx"), "det"),
    writeFile(path.join(sourcesRoot, "models", "ch_PP-OCRv5_rec_mobile.onnx"), "rec"),
    writeFile(path.join(sourcesRoot, "models", "ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx"), "cls"),
    writeFile(path.join(sourcesRoot, "models", "ppocrv5_dict.txt"), "dictionary"),
    writeFile(path.join(sourcesRoot, "qualification", "ch_en_num.jpg"), "image"),
    writeFile(path.join(sourcesRoot, "requirements.lock"), lock),
    writeFile(path.join(sourcesRoot, "SOURCE-PROVENANCE.json"), `${JSON.stringify({
      schemaVersion: 1,
      target: "x86_64-pc-windows-msvc",
      python: { version: "3.12.13+20260718" },
      rapidOcr: { version: "3.8.1", dependencyLockSha256: lockSha256 },
      provider: "cpu",
      runtimeNetwork: false,
    })}\n`),
  ]);
  return { root, repositoryRoot, sourcesRoot, output: path.join(root, "payload") };
}

test("stages a self-contained signed-inventory-ready RapidOCR payload", async (context) => {
  const value = await fixture();
  context.after(() => fs.rm(value.root, { recursive: true, force: true }));
  const result = await stageRapidOcrCapability({
    target: "x86_64-pc-windows-msvc",
    sourcesRoot: value.sourcesRoot,
    output: value.output,
    repositoryRoot: value.repositoryRoot,
  });
  const entrypoint = process.platform === "win32" ? "runtime/python/python.exe" : "runtime/python/bin/python3";
  assert.deepEqual(result, { entrypoint, entrypointArgs: ["runner/index.py"], modelBytes: 9 });
  assert.equal((await fs.stat(path.join(value.output, "SBOM.spdx.json"))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "NOTICE.md"))).isFile(), true);
  assert.equal((await fs.stat(path.join(value.output, "DEPENDENCY-LOCK.txt"))).isFile(), true);
  const sbom = JSON.parse(await fs.readFile(path.join(value.output, "SBOM.spdx.json"), "utf8"));
  assert.equal(sbom.packages.some((item) => item.name === "rapidocr"), true);
  assert.equal(sbom.packages.some((item) => item.name === "PP-OCRv5 mobile models"), true);
  assert.equal(
    sbom.packages.find((item) => item.name === "opencv-python").licenseDeclared,
    "Apache-2.0 AND LGPL-2.1-only AND LGPL-3.0-only",
  );
  assert.equal(
    sbom.packages.find((item) => item.name === "shapely").licenseDeclared,
    "BSD-3-Clause AND LGPL-2.1-only",
  );
});

test("rejects provenance that allows runtime networking", async (context) => {
  const value = await fixture();
  context.after(() => fs.rm(value.root, { recursive: true, force: true }));
  const provenancePath = path.join(value.sourcesRoot, "SOURCE-PROVENANCE.json");
  const provenance = JSON.parse(await fs.readFile(provenancePath, "utf8"));
  provenance.runtimeNetwork = true;
  await fs.writeFile(provenancePath, JSON.stringify(provenance));
  await assert.rejects(stageRapidOcrCapability({
    target: "x86_64-pc-windows-msvc",
    sourcesRoot: value.sourcesRoot,
    output: value.output,
    repositoryRoot: value.repositoryRoot,
  }), /provenance/);
});

test("pins Python, RapidOCR, PP-OCRv5, and the fixture for four targets", async () => {
  const sources = JSON.parse(await fs.readFile(
    path.join(import.meta.dirname, "..", "capabilities", "release-sources.json"),
    "utf8",
  ));
  const targets = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
  ];
  assert.deepEqual(Object.keys(sources.pythonStandalone.distributions).sort(), targets);
  for (const declaration of [
    ...Object.values(sources.pythonStandalone.distributions),
    sources.rapidOcr.wheel,
    ...Object.values(sources.rapidOcr.models),
    sources.rapidOcr.qualificationFixture,
  ]) {
    assert.match(declaration.sha256, /^[0-9a-f]{64}$/);
    assert.equal(Number.isSafeInteger(declaration.bytes) && declaration.bytes > 0, true);
  }
  assert.equal(sources.rapidOcr.version, "3.8.1");
  assert.equal(sources.rapidOcr.models.recognizer.bytes, 16_631_306);
  assert.equal(sources.rapidOcr.qualificationFixture.sha256, "b94c1bf68af9ceb3c550b86931d7b48d4181c26c9f5ace2492e64042f36a02ca");
  assert.equal(
    sources.rapidOcr.qualificationFixture.source,
    "https://github.com/RapidAI/RapidOCR/releases/download/v1.1.0/ch_en_num.jpg",
  );
});

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { stageCapabilityDownloads } from "./stage-capability-downloads.mjs";
import { expectedReleaseMatrix, MODEL_CAPABILITY_PACKS, verifyCapabilityCatalog } from "./verify-capability-catalog.mjs";
import { PRODUCT_MANIFEST } from "./verify-product-capabilities.mjs";

const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const baseUrl = "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/resources-v1.2.3/";
const python = process.platform === "win32" ? "python" : "python3";

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "capability downloads 中文-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const input = path.join(root, "resources");
  const output = path.join(root, "downloads");
  fs.mkdirSync(input);
  const modelBytes = Buffer.from("shared 模型 bytes");
  const digest = hash(modelBytes);
  const modelPath = "models/中文 模型.bin";
  const offlinePath = `models/${digest}/${path.posix.basename(modelPath)}`;
  const modelSource = path.join(input, ...offlinePath.split("/"));
  fs.mkdirSync(path.dirname(modelSource), { recursive: true });
  fs.writeFileSync(modelSource, modelBytes);
  const catalog = { schemaVersion: 1, entries: expectedReleaseMatrix().map(({ capabilityId, targetTriple }) => {
    const name = `${capabilityId}-1.2.3-${targetTriple} build.zip`;
    const bytes = Buffer.from(`program fixture: ${capabilityId}/${targetTriple}`);
    fs.writeFileSync(path.join(input, name), bytes);
    const hasModel = MODEL_CAPABILITY_PACKS.includes(capabilityId);
    return {
      capabilityId, targetTriple, version: "1.2.3",
      url: `https://cdn.llmwiki.cn/resources/${encodeURIComponent(name)}`,
      archiveSha256: hash(bytes), compressedBytes: bytes.length, installedBytes: bytes.length + 200,
      license: PRODUCT_MANIFEST.definitions.find((item) => item.capabilityId === capabilityId).licensePolicy.expression,
      ...(hasModel ? { modelBytes: modelBytes.length, modelFiles: [{
        path: modelPath, sha256: digest, bytes: modelBytes.length,
        urls: [`https://cdn.llmwiki.cn/${encodeURI(offlinePath)}`],
      }] } : {}),
    };
  }) };
  const writeCatalog = () => fs.writeFileSync(path.join(input, "install-catalog.json"), JSON.stringify(catalog));
  writeCatalog();
  return { input, output, baseUrl, catalog, writeCatalog, modelSource, modelBytes, modelPath, offlinePath };
}

test("CLI stages a complete flat release, deduplicates models and supplies the offline model tree", (t) => {
  const f = fixture(t);
  const sourceCatalog = fs.readFileSync(path.join(f.input, "install-catalog.json"), "utf8");
  execFileSync(process.execPath, [fileURLToPath(new URL("./stage-capability-downloads.mjs", import.meta.url)),
    "--input", f.input, "--output", f.output, "--base-url", f.baseUrl], { stdio: "pipe" });
  const files = fs.readdirSync(f.output);
  assert.equal(files.length, f.catalog.entries.length + 4);
  assert.ok(files.every((name) => fs.statSync(path.join(f.output, name)).isFile()));
  const catalog = JSON.parse(fs.readFileSync(path.join(f.output, "install-catalog.json")));
  assert.deepEqual(verifyCapabilityCatalog({ catalog, trustedKeys: {}, mode: "release" }).errors, []);
  const modelUrls = new Set();
  for (const entry of catalog.entries) {
    assert.ok(entry.url.startsWith(f.baseUrl));
    const name = decodeURIComponent(entry.url.slice(f.baseUrl.length));
    assert.ok(!name.includes("/") && !name.includes(" "));
    assert.equal(hash(fs.readFileSync(path.join(f.output, name))), entry.archiveSha256);
    for (const model of entry.modelFiles ?? []) {
      assert.equal(model.path, f.modelPath);
      assert.equal(model.urls.length, 1);
      modelUrls.add(model.urls[0]);
      const name = decodeURIComponent(model.urls[0].slice(f.baseUrl.length));
      assert.ok(!name.includes("/"));
      assert.ok(name.startsWith(model.sha256 + "-"));
      assert.deepEqual(fs.readFileSync(path.join(f.output, name)), f.modelBytes);
    }
  }
  assert.equal(modelUrls.size, 1);
  const archived = JSON.parse(execFileSync(python, ["-c", `
import json,sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    print(json.dumps({name: archive.read(name).decode('utf-8') for name in archive.namelist()}))
`, path.join(f.output, "models.zip")], { encoding: "utf8" }));
  assert.deepEqual(archived, { [f.offlinePath]: f.modelBytes.toString() });
  const checksums = fs.readFileSync(path.join(f.output, "CHECKSUMS.sha256"), "utf8").trimEnd().split("\n");
  assert.equal(checksums.length, files.length - 1);
  for (const line of checksums) {
    const [, digest, name] = /^([a-f0-9]{64}) {2}(.+)$/u.exec(line);
    assert.equal(hash(fs.readFileSync(path.join(f.output, name))), digest);
  }
  assert.equal(fs.readFileSync(path.join(f.input, "install-catalog.json"), "utf8"), sourceCatalog);
});

test("existing output is never overwritten or removed", async (t) => {
  const f = fixture(t);
  fs.mkdirSync(f.output);
  fs.writeFileSync(path.join(f.output, "keep.txt"), "user data");
  await assert.rejects(stageCapabilityDownloads(f), /output must be a new directory/u);
  assert.equal(fs.readFileSync(path.join(f.output, "keep.txt"), "utf8"), "user data");
});

test("model SHA mismatch is rejected before creating the output", async (t) => {
  const f = fixture(t);
  fs.writeFileSync(f.modelSource, Buffer.alloc(f.modelBytes.length));
  await assert.rejects(stageCapabilityDownloads(f), /SHA-256 does not match/u);
  assert.equal(fs.existsSync(f.output), false);
});

test("program size mismatch is rejected before creating the output", async (t) => {
  const f = fixture(t);
  f.catalog.entries[0].compressedBytes += 1;
  f.writeCatalog();
  await assert.rejects(stageCapabilityDownloads(f), /size or path does not match/u);
  assert.equal(fs.existsSync(f.output), false);
});

test("normalization cannot silently replace a different program", async (t) => {
  const f = fixture(t);
  for (const [index, name] of ["conflict build.zip", "conflict.build.zip"].entries()) {
    const entry = f.catalog.entries[index];
    const old = decodeURIComponent(path.posix.basename(new URL(entry.url).pathname));
    fs.renameSync(path.join(f.input, old), path.join(f.input, name));
    entry.url = "https://cdn.llmwiki.cn/" + encodeURIComponent(name);
  }
  f.writeCatalog();
  await assert.rejects(stageCapabilityDownloads(f), /conflicting attachment name/u);
  assert.equal(fs.existsSync(f.output), false);
});

test("GitHub's per-attachment limit fails with a useful error without reading oversized data", async (t) => {
  const f = fixture(t);
  f.catalog.entries[0].compressedBytes = 2 * 1024 ** 3;
  f.writeCatalog();
  await assert.rejects(stageCapabilityDownloads(f), /smaller than 2 GiB/u);
  assert.equal(fs.existsSync(f.output), false);
});

test("requires a complete catalog and an exact GitHub download directory", async (t) => {
  const f = fixture(t);
  for (const baseUrl of ["https://cdn.llmwiki.cn/resources/", "https://github.com/owner/repo/releases/", f.baseUrl + "?token=secret"]) {
    await assert.rejects(stageCapabilityDownloads({ ...f, baseUrl }), /exact public GitHub/u);
  }
  f.catalog.entries.pop();
  f.writeCatalog();
  await assert.rejects(stageCapabilityDownloads(f), /exact matrix/u);
  assert.equal(fs.existsSync(f.output), false);
});

import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  CAPABILITY_PACKS,
  CAPABILITY_TARGETS,
  MODEL_CAPABILITY_PACKS,
  emitCatalogProvenance,
  expectedReleaseMatrix,
  repositoryRoot,
  verifyCapabilityCatalog,
} from "./verify-capability-catalog.mjs";
import { PRODUCT_MANIFEST } from "./verify-product-capabilities.mjs";

const trustedKeys = { release: "c".repeat(64) };
const productDefinitions = new Map(PRODUCT_MANIFEST.definitions.map((definition) => [definition.capabilityId, definition]));

const releaseEntry = (capabilityId, targetTriple, overrides = {}) => ({
  capabilityId,
  targetTriple,
  version: "1.2.3",
  url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/"
    + capabilityId + "-1.2.3-" + targetTriple + ".zip",
  archiveSha256: "a".repeat(64),
  manifestSha256: "b".repeat(64),
  signingKeyId: "release",
  compressedBytes: 1234,
  installedBytes: 2345,
  modelBytes: MODEL_CAPABILITY_PACKS.includes(capabilityId)
    ? 640
    : null,
  license: productDefinitions.get(capabilityId)?.licensePolicy.expression ?? "MIT",
  ...overrides,
});

const releaseCatalog = (entries) => ({ schemaVersion: 1, entries });
// The release matrix is per-definition (document-layout ships three targets),
// so fixtures are derived from the manifest instead of a cartesian product.
const fullMatrix = () => expectedReleaseMatrix().map(
  ({ capabilityId, targetTriple }) => releaseEntry(capabilityId, targetTriple),
);

const verify = (overrides = {}) => verifyCapabilityCatalog({
  catalog: releaseCatalog(fullMatrix()),
  trustedKeys,
  mode: "release",
  expectedTag: "app-v0.1.0",
  ...overrides,
});

test("release mode requires the complete unique product-manifest matrix", () => {
  assert.deepEqual(verify().errors, []);

  const incomplete = fullMatrix().slice(0, -1);
  assert.equal(verify({ catalog: releaseCatalog(incomplete) }).errors.length > 0, true);

  const duplicated = [...fullMatrix(), fullMatrix()[0]];
  assert.equal(verify({ catalog: releaseCatalog(duplicated) }).errors.length > 0, true);

  const wrongTarget = fullMatrix().with(3, releaseEntry("browser-runtime", "wasm32-unknown-unknown"));
  assert.equal(verify({ catalog: releaseCatalog(wrongTarget) }).errors.length > 0, true);

  const wrongPack = fullMatrix().with(4, releaseEntry("video-transcode", CAPABILITY_TARGETS[0]));
  assert.equal(verify({ catalog: releaseCatalog(wrongPack) }).errors.length > 0, true);
});

test("catalog accepts independent HTTPS storage and rejects unusable public URLs", () => {
  for (const url of ["https://cdn.llmwiki.cn/engines/browser.zip", "https://github.com/another/project/releases/download/v2/browser.zip"]) {
    const entries = fullMatrix(); entries[0].url = url;
    assert.deepEqual(verify({ catalog: releaseCatalog(entries) }).errors, []);
  }
  for (const url of ["", "http://cdn.llmwiki.cn/a.zip", "https://localhost/a.zip", "https://example.com/a.zip", "https://user:token@cdn.llmwiki.cn/a.zip", "https://cdn.llmwiki.cn/a.zip?token=secret"]) {
    const entries = fullMatrix(); entries[0].url = url;
    assert.ok(verify({ catalog: releaseCatalog(entries) }).errors.length > 0, url);
  }
});

test("catalog accepts SemVer build metadata in an exact asset name", () => {
  const capabilityId = "asr-sensevoice-small";
  const targetTriple = CAPABILITY_TARGETS[0];
  const version = "1.13.4+2024.07.17";
  const entry = releaseEntry(capabilityId, targetTriple, {
    version,
    url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/"
      + capabilityId + "-" + version + "-" + targetTriple + ".zip",
  });
  const index = fullMatrix().findIndex((candidate) => candidate.capabilityId === capabilityId
    && candidate.targetTriple === targetTriple);

  assert.deepEqual(
    verify({ catalog: releaseCatalog(fullMatrix().with(index, entry)) }).errors,
    [],
  );
});

test("resource versions are independent of the desktop release batch", () => {
  assert.deepEqual(verify({ expectedTag: "app-v9.0.0" }).errors, []);
  assert.deepEqual(verify({ expectedTag: null }).errors, []);
});

test("entry measurements and identities must be complete", () => {
  const invalidEntries = [
    { archiveSha256: "z".repeat(64) },
    { archiveSha256: "0".repeat(64) },
    { manifestSha256: "short" },
    { compressedBytes: 0 },
    { installedBytes: -1 },
    { compressedBytes: 1.5 },
    { license: "   " },
    { version: "not-semver" },
    { targetTriple: "x86_64-pc-windows-gnu" },
    { capabilityId: "bad pack id!" },
    { modelBytes: 0 },
  ];
  for (const overrides of invalidEntries) {
    const catalog = releaseCatalog(
      fullMatrix().with(5, releaseEntry(CAPABILITY_PACKS[0], CAPABILITY_TARGETS[1], overrides)),
    );
    assert.equal(verify({ catalog }).errors.length > 0, true);
  }

  const modelPackEntry = releaseEntry(MODEL_CAPABILITY_PACKS[0], CAPABILITY_TARGETS[0]);
  delete modelPackEntry.modelBytes;
  const modelIndex = fullMatrix().findIndex((entry) => entry.capabilityId === MODEL_CAPABILITY_PACKS[0]
    && entry.targetTriple === CAPABILITY_TARGETS[0]);
  const removed = fullMatrix().with(modelIndex, modelPackEntry);
  assert.equal(verify({ catalog: releaseCatalog(removed) }).errors.length > 0, true);
});

test("source mode keeps the development fallback explicit", () => {
  const emptyCatalog = { schemaVersion: 1, entries: [] };
  assert.deepEqual(
    verifyCapabilityCatalog({ catalog: emptyCatalog, trustedKeys: {}, mode: "source" }).errors,
    [],
  );

  const badSchema = { schemaVersion: 2, entries: [] };
  assert.equal(
    verifyCapabilityCatalog({ catalog: badSchema, trustedKeys: {}, mode: "source" }).errors.length > 0,
    true,
  );

  const committedStyle = releaseCatalog([releaseEntry(CAPABILITY_PACKS[0], CAPABILITY_TARGETS[0])]);
  assert.deepEqual(
    verifyCapabilityCatalog({ catalog: committedStyle, trustedKeys, mode: "source" }).errors,
    [],
  );

  const brokenCommitted = releaseCatalog([
    releaseEntry(CAPABILITY_PACKS[0], CAPABILITY_TARGETS[0], { url: "https://example.com/pack.zip" }),
  ]);
  assert.equal(
    verifyCapabilityCatalog({ catalog: brokenCommitted, trustedKeys, mode: "source" }).errors.length > 0,
    true,
  );

  const malformedKeys = { release: "nothex" };
  assert.equal(
    verifyCapabilityCatalog({ catalog: committedStyle, trustedKeys: malformedKeys, mode: "source" }).errors.length > 0,
    true,
  );

  const releaseWithoutTag = verifyCapabilityCatalog({
    catalog: releaseCatalog(fullMatrix()),
    trustedKeys,
    mode: "release",
  });
  assert.deepEqual(releaseWithoutTag.errors, []);
});

test("archive-hash catalogs need no custom signature metadata", () => {
  const entries = fullMatrix().map((original) => {
    const entry = { ...original };
    delete entry.signingKeyId;
    delete entry.manifestSha256;
    return entry;
  });
  assert.deepEqual(verify({ catalog: releaseCatalog(entries), trustedKeys: {} }).errors, []);
  assert.ok(verify({ trustedKeys: { release: "0".repeat(64) } }).errors.length > 0);
});

test("provenance binds the catalog artifact to one run, tag, and commit", () => {
  const provenance = {
    schemaVersion: 1,
    releaseTag: "app-v0.1.0",
    commitSha: "a".repeat(40),
    workflowRunId: "1234567890",
  };
  assert.deepEqual(verify({ provenance }).errors, []);
  assert.deepEqual(verify({
    provenance,
    expectedCommit: "a".repeat(40),
    expectedRunId: "1234567890",
  }).errors, []);

  assert.equal(verify({ provenance: { ...provenance, releaseTag: "app-v0.2.0" } }).errors.length > 0, true);
  assert.equal(verify({
    provenance,
    expectedCommit: "b".repeat(40),
  }).errors.length > 0, true);
  assert.equal(verify({
    provenance,
    expectedRunId: "9876543210",
  }).errors.length > 0, true);
  assert.equal(verify({ provenance: { ...provenance, commitSha: "short" } }).errors.length > 0, true);
  assert.equal(verify({ provenance: { ...provenance, workflowRunId: "run-abc" } }).errors.length > 0, true);
  assert.equal(verify({ provenance: { schemaVersion: 2 } }).errors.length > 0, true);
});

test("provenance emission is deterministic", async (context) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-catalog-provenance-"));
  context.after(() => fs.rm(directory, { recursive: true, force: true }));
  const outputPath = path.join(directory, "catalog-provenance.json");
  emitCatalogProvenance({
    outputPath,
    releaseTag: "app-v0.1.0",
    commitSha: "a".repeat(40),
    workflowRunId: "1234567890",
  });
  const emitted = await fs.readFile(outputPath, "utf8");
  assert.deepEqual(JSON.parse(emitted), {
    schemaVersion: 1,
    releaseTag: "app-v0.1.0",
    commitSha: "a".repeat(40),
    workflowRunId: "1234567890",
  });
  assert.equal(emitted.endsWith("\n"), true);
});

test("the repository catalog is valid for source builds and public releases", async () => {
  const catalog = JSON.parse(
    await fs.readFile(path.join(repositoryRoot, "capabilities/install-catalog.json"), "utf8"),
  );
  const keys = JSON.parse(
    await fs.readFile(path.join(repositoryRoot, "capabilities/trusted-keys.json"), "utf8"),
  );
  for (const mode of ["source", "release"]) {
    assert.deepEqual(verifyCapabilityCatalog({ catalog, trustedKeys: keys, mode }).errors, []);
  }
});

test("independent model resources require safe paths and pinned identities", () => {
  const entries = fullMatrix();
  const index = entries.findIndex((entry) => entry.modelBytes != null);
  entries[index].modelFiles = [{ path: "models/中文/model.bin", bytes: 640, sha256: "d".repeat(64), urls: ["https://cdn.llmwiki.cn/models/a.bin"] }];
  assert.deepEqual(verify({ catalog: releaseCatalog(entries) }).errors, []);
  const duplicate = structuredClone(entries);
  duplicate[index].modelFiles.push({ ...duplicate[index].modelFiles[0], path: "models/中文/MODEL.bin" });
  duplicate[index].modelBytes *= 2;
  assert.ok(verify({ catalog: releaseCatalog(duplicate) }).errors.some((error) => error.includes("unique data")));
  for (const override of [{ path: "models/../runner" }, { path: "models/a:b" }, { path: "models/script.py" }, { bytes: 8 * 1024 ** 3 + 1 }, { urls: Array(9).fill("https://cdn.llmwiki.cn/model.bin") }, { bytes: 639 }, { sha256: "bad" }, { urls: [] }]) {
    const bad = structuredClone(entries);
    Object.assign(bad[index].modelFiles[0], override);
    assert.ok(verify({ catalog: releaseCatalog(bad) }).errors.length > 0);
  }
});

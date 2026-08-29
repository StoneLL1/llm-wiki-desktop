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
  compressedBytes: 1234,
  installedBytes: 2345,
  modelBytes: MODEL_CAPABILITY_PACKS.includes(capabilityId)
    ? 640
    : null,
  license: productDefinitions.get(capabilityId)?.licensePolicy.expression ?? "MIT",
  ...overrides,
});

const releaseCatalog = (entries) => ({ schemaVersion: 1, entries });
const fullMatrix = () => CAPABILITY_TARGETS.flatMap(
  (targetTriple) => CAPABILITY_PACKS.map((capabilityId) => releaseEntry(capabilityId, targetTriple)),
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

test("catalog urls must pin one exact immutable canonical tag", () => {
  const wrongTag = fullMatrix().with(
    0,
    releaseEntry(CAPABILITY_PACKS[0], CAPABILITY_TARGETS[0], {
      url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v9.9.9/browser-runtime-1.2.3.zip",
    }),
  );
  assert.equal(verify({ catalog: releaseCatalog(wrongTag) }).errors.length > 0, true);

  const mutableLatest = fullMatrix().with(
    1,
    releaseEntry(CAPABILITY_PACKS[0], CAPABILITY_TARGETS[1], {
      url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/browser-runtime-1.2.3.zip",
    }),
  );
  assert.equal(verify({ catalog: releaseCatalog(mutableLatest) }).errors.length > 0, true);

  const forbiddenUrls = [
    "",
    "http://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/pack.zip",
    "https://localhost/releases/download/app-v0.1.0/pack.zip",
    "https://example.com/releases/download/app-v0.1.0/pack.zip",
    "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/pack.zip?token=1",
    "https://user:token@github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/pack.zip",
    "https://github.com/example/fork/releases/download/app-v0.1.0/pack.zip",
    "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/other-pack-name.zip",
  ];
  for (const url of forbiddenUrls) {
    const catalog = releaseCatalog(
      fullMatrix().with(2, releaseEntry(CAPABILITY_PACKS[0], CAPABILITY_TARGETS[2], { url })),
    );
    assert.equal(verify({ catalog }).errors.length > 0, true);
  }
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
  assert.equal(releaseWithoutTag.errors.length > 0, true);
});

test("release mode requires committed trusted keys", () => {
  assert.equal(verify({ trustedKeys: {} }).errors.length > 0, true);
  assert.equal(verify({ trustedKeys: { release: "0".repeat(64) } }).errors.length > 0, true);
  assert.equal(verify({ trustedKeys: null }).errors.length > 0, true);
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

test("the repository source catalog stays a valid development fallback", async () => {
  const catalog = JSON.parse(
    await fs.readFile(path.join(repositoryRoot, "capabilities/install-catalog.json"), "utf8"),
  );
  const keys = JSON.parse(
    await fs.readFile(path.join(repositoryRoot, "capabilities/trusted-keys.json"), "utf8"),
  );
  assert.deepEqual(
    verifyCapabilityCatalog({ catalog, trustedKeys: keys, mode: "source" }).errors,
    [],
  );
  assert.equal(
    verifyCapabilityCatalog({ catalog, trustedKeys: keys, mode: "release", expectedTag: "app-v0.1.0" })
      .errors.length > 0,
    true,
  );
});

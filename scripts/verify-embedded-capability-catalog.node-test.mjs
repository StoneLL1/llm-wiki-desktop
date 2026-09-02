import assert from "node:assert/strict";
import test from "node:test";

import {
  expectedReleaseMatrix,
} from "./verify-capability-catalog.mjs";
import { verifyEmbeddedCapabilityCatalog } from "./verify-embedded-capability-catalog.mjs";

const catalogText = (entries) => JSON.stringify({ schemaVersion: 1, entries }, null, 2) + "\n";

const entry = (capabilityId, targetTriple) => ({
  capabilityId,
  targetTriple,
  version: "1.2.3",
  url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.1.0/"
    + capabilityId + "-1.2.3-" + targetTriple + ".zip",
  archiveSha256: "a".repeat(64),
  manifestSha256: "b".repeat(64),
  signingKeyId: "release-key",
  compressedBytes: 1234,
  installedBytes: 2345,
  modelBytes: null,
  license: "MIT",
});

// The release matrix is per-definition (document-layout ships three targets),
// so the fixture mirrors the manifest instead of a cartesian product.
const fullCatalogText = () => catalogText(
  expectedReleaseMatrix().map(
    ({ capabilityId, targetTriple }) => entry(capabilityId, targetTriple),
  ),
);

const binaryWith = (text) => Buffer.concat([
  Buffer.from("0".repeat(256)),
  Buffer.from(text, "utf8"),
  Buffer.from("0".repeat(128)),
]);

test("release binaries must embed the exact staged catalog", () => {
  const catalog = fullCatalogText();
  const binary = binaryWith(catalog);
  assert.deepEqual(
    verifyEmbeddedCapabilityCatalog({ binary, catalogText: catalog, mode: "release" }).errors,
    [],
  );

  const truncated = binary.subarray(0, 256 + Buffer.byteLength(catalog) - 16);
  assert.equal(
    verifyEmbeddedCapabilityCatalog({ binary: truncated, catalogText: catalog, mode: "release" }).errors.length > 0,
    true,
  );

  const catalogMissingEntry = fullCatalogText().replace("ocr-cjk-accurate", "other-pack");
  assert.equal(
    verifyEmbeddedCapabilityCatalog({ binary, catalogText: catalogMissingEntry, mode: "release" })
      .errors.length > 0,
    true,
  );
});

test("release binaries cannot embed an empty catalog", () => {
  const empty = catalogText([]);
  assert.equal(
    verifyEmbeddedCapabilityCatalog({ binary: binaryWith(empty), catalogText: empty, mode: "release" }).errors.length > 0,
    true,
  );
});

test("source binaries embed the exact source fallback", () => {
  const empty = catalogText([]);
  assert.deepEqual(
    verifyEmbeddedCapabilityCatalog({ binary: binaryWith(empty), catalogText: empty, mode: "source" }).errors,
    [],
  );

  const replaced = catalogText([{ capabilityId: "browser-runtime", targetTriple: "x86_64-pc-windows-msvc" }]);
  assert.equal(
    verifyEmbeddedCapabilityCatalog({ binary: binaryWith(empty), catalogText: replaced, mode: "source" }).errors.length > 0,
    true,
  );
});

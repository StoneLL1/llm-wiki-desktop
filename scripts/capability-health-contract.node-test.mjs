import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const runnerByPack = {
  "asr-sensevoice-small": "capabilities/asr-sensevoice-small/runner/index.mjs",
  "asr-whisper": "capabilities/asr-whisper/runner/index.mjs",
  "browser-runtime": "capabilities/browser-runtime/runner/index.mjs",
  "browser-runtime-lite": "capabilities/browser-runtime-lite/runner/index.mjs",
  "document-layout": "capabilities/document-layout/runner/docling_pack.py",
  "document-standard": "capabilities/document-standard/runner/markitdown_pack.py",
  "media-metadata": "capabilities/media-metadata/runner/index.mjs",
  "media-runtime": "capabilities/media-runtime/runner/index.mjs",
  "ocr-basic": "capabilities/ocr-cjk-accurate/runner/index.py",
  "ocr-cjk-accurate": "capabilities/ocr-cjk-accurate/runner/index.py",
  "office-legacy": "capabilities/office-legacy/runner/office_legacy_pack.py",
};
const product = JSON.parse(fs.readFileSync(path.join(root, "capabilities/product-manifest.json"), "utf8"));
const published = product.definitions.filter((definition) => definition.distributionTier === "published");

test("every Batch 3A published capability implements the Batch 3B health protocol", () => {
  for (const { capabilityId, routes } of published) {
    const relativePath = runnerByPack[capabilityId];
    assert.ok(relativePath, `${capabilityId} has no formal runner mapping`);
    const source = fs.readFileSync(path.join(root, relativePath), "utf8");
    assert.match(source, /capability\.health/u, `${capabilityId} has no health method`);
    assert.ok(source.includes(capabilityId), `${capabilityId} does not bind its health identity`);
    for (const route of routes) assert.ok(source.includes(route), `${capabilityId} does not bind ${route}`);
    assert.match(source, /healthy/u, `${capabilityId} does not return a readiness result`);
    assert.match(source, /protocolVersion/u, `${capabilityId} does not bind protocol v2`);
  }
});

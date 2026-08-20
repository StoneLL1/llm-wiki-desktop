import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const published = [
  ["browser-runtime", "capabilities/browser-runtime/runner/index.mjs", "web.generic.browser"],
  ["browser-runtime-lite", "capabilities/browser-runtime-lite/runner/index.mjs", "web.generic.readability"],
  ["media-metadata", "capabilities/media-metadata/runner/index.mjs", "web.bilibili.metadata"],
  ["asr-sensevoice-small", "capabilities/asr-sensevoice-small/runner/index.mjs", "media.asr"],
  ["ocr-cjk-accurate", "capabilities/ocr-cjk-accurate/runner/index.py", "ocr.cjk-accurate"],
];

test("every Batch 3A published capability implements the Batch 3B health protocol", () => {
  for (const [capabilityId, relativePath, route] of published) {
    const source = fs.readFileSync(path.join(root, relativePath), "utf8");
    assert.match(source, /capability\.health/u, `${capabilityId} has no health method`);
    assert.ok(source.includes(capabilityId), `${capabilityId} does not bind its health identity`);
    assert.ok(source.includes(route), `${capabilityId} does not bind its published health route`);
    assert.match(source, /healthy/u, `${capabilityId} does not return a readiness result`);
    assert.match(source, /protocolVersion/u, `${capabilityId} does not bind protocol v2`);
  }
});

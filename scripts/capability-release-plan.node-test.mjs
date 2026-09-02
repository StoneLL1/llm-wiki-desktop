import assert from "node:assert/strict";
import test from "node:test";

import { buildCapabilityReleasePlan } from "./capability-release-plan.mjs";

test("materializes exactly one release entry per published definition target", async () => {
  const result = await buildCapabilityReleasePlan();
  assert.deepEqual(result.errors, []);
  assert.equal(result.entries.length, result.expectedEntryCount);
  const expected = result.manifest.definitions
    .filter((definition) => definition.distributionTier === "published")
    .reduce((total, definition) => total + definition.supportedTargets.length, 0);
  assert.equal(result.expectedEntryCount, expected);
  assert.equal(
    new Set(result.entries.map((entry) => `${entry.capabilityId}:${entry.targetTriple}`)).size,
    result.entries.length,
  );
  // document-layout drops Intel macOS (PyTorch stopped shipping x86_64 mac wheels),
  // so it must contribute exactly three entries — the first per-capability target subset.
  const layout = result.entries.filter((entry) => entry.capabilityId === "document-layout");
  assert.equal(layout.length, 3);
  assert.ok(!layout.some((entry) => entry.targetTriple === "x86_64-apple-darwin"));
});

test("keeps runner routes and formats on every matrix entry", async () => {
  const result = await buildCapabilityReleasePlan();
  const browser = result.entries.find((entry) => entry.capabilityId === "browser-runtime");
  assert.deepEqual(browser.routes, ["web.generic.browser", "web.wechat.article", "web.x.post"]);
  const accurateOcr = result.entries.find((entry) => entry.capabilityId === "ocr-cjk-accurate");
  assert.ok(accurateOcr.extensions.includes("heic"));
  assert.ok(accurateOcr.extensions.includes("heif"));
  const whisper = result.entries.find((entry) => entry.capabilityId === "asr-whisper");
  assert.ok(whisper.extensions.includes("wma"));
  assert.ok(whisper.extensions.includes("wmv"));
});

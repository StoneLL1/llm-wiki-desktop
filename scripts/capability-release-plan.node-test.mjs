import assert from "node:assert/strict";
import test from "node:test";

import { buildCapabilityReleasePlan } from "./capability-release-plan.mjs";

test("materializes exactly one four-target release entry for every published definition", async () => {
  const result = await buildCapabilityReleasePlan();
  assert.deepEqual(result.errors, []);
  assert.equal(result.entries.length, result.expectedEntryCount);
  const publishedCount = result.manifest.definitions.filter((definition) => definition.distributionTier === "published").length;
  assert.equal(result.expectedEntryCount, publishedCount * result.manifest.supportedTargets.length);
  assert.equal(
    new Set(result.entries.map((entry) => `${entry.capabilityId}:${entry.targetTriple}`)).size,
    result.entries.length,
  );
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

/* global fetch, AbortSignal */
import assert from "node:assert/strict";
import process from "node:process";
import { URL } from "node:url";

import { classifyPlatformPage, extractXPayload } from "./platform-extract.mjs";

const publicFixture = `<meta property="og:title" content="Release owner on X: qualification"><meta property="og:description" content="Public production route qualification"><meta property="og:image" content="https://pbs.twimg.com/media/fixture.jpg">`;
assert.equal(extractXPayload(publicFixture, "https://x.com/release/status/123")?.platformId, "123");
assert.equal(classifyPlatformPage("x", "Login required"), "IMPORT_WEB_LOGIN_REQUIRED");
assert.equal(classifyPlatformPage("x", "These posts are protected"), "IMPORT_WEB_CONTENT_RESTRICTED");
assert.equal(extractXPayload(publicFixture, "https://example.com/unknown"), null);

const productionUrl = process.env.LLM_WIKI_X_PRODUCTION_SAMPLE_URL;
if (!productionUrl) {
  throw new Error("LLM_WIKI_X_PRODUCTION_SAMPLE_URL is required for release qualification; fixture success is not production evidence");
}
const reviewedHosts = new Set(["x.com", "www.x.com", "twitter.com", "www.twitter.com", "t.co", "www.t.co"]);
let current = new URL(productionUrl);
assert.equal(current.protocol, "https:");
assert.equal(reviewedHosts.has(current.hostname), true, "X production sample host is not reviewed");
assert.match(current.pathname, /^\/[^/]+\/status\/\d+\/?$/u);
let response;
for (let redirect = 0; redirect <= 5; redirect += 1) {
  response = await fetch(current, {
    redirect: "manual",
    headers: { "user-agent": "LLM-Wiki-Desktop-Release-Qualification/1" },
    signal: AbortSignal.timeout(45_000),
  });
  if (![301, 302, 303, 307, 308].includes(response.status)) break;
  const location = response.headers.get("location");
  assert.ok(location, "X production redirect omitted Location");
  current = new URL(location, current);
  assert.equal(current.protocol, "https:");
  assert.equal(reviewedHosts.has(current.hostname), true, "X production redirect left the endpoint policy");
}
assert.ok(response);
assert.equal(["x.com", "www.x.com", "twitter.com", "www.twitter.com"].includes(current.hostname), true);
assert.equal(response.ok, true, `X production sample returned ${response.status}`);
const payload = extractXPayload(await response.text(), current.href);
assert.ok(payload?.platformId, "X production sample did not expose the official public page metadata route");
process.stdout.write(`${JSON.stringify({ qualified: true, route: "web.x.post", production: true })}\n`);

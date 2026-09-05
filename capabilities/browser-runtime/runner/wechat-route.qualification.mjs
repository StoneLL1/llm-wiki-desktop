/* global fetch, AbortSignal */
import assert from "node:assert/strict";
import process from "node:process";
import { URL } from "node:url";

const sample = process.env.LLM_WIKI_WECHAT_PRODUCTION_SAMPLE_URL;
if (!sample) throw new Error("LLM_WIKI_WECHAT_PRODUCTION_SAMPLE_URL is required for release qualification");
const url = new URL(sample);
assert.equal(url.protocol, "https:");
assert.equal(url.hostname, "mp.weixin.qq.com");
assert.match(url.pathname, /^\/s(?:\/[^/]+)?$/u);
const response = await fetch(url, { redirect: "manual", signal: AbortSignal.timeout(45_000) });
let productionState;
if (response.ok && response.status < 300) {
  const html = await response.text();
  assert.match(html, /(?:id=["']js_content["']|var\s+msg_title\s*=|property=["']og:title["'])/iu);
  productionState = "public-content";
} else {
  assert.equal([301, 302, 303, 307, 308].includes(response.status), true, `WeChat production sample returned ${response.status}`);
  const location = response.headers.get("location");
  assert.ok(location, "WeChat production challenge omitted Location");
  const challenge = new URL(location, url);
  assert.equal(challenge.protocol, "https:");
  assert.equal(challenge.hostname, "mp.weixin.qq.com");
  assert.equal(challenge.pathname, "/mp/wappoc_appmsgcaptcha");
  assert.equal(new URL(challenge.searchParams.get("target_url")).href, url.href);
  productionState = "official-challenge";
}
process.stdout.write(`${JSON.stringify({ qualified: true, route: "web.wechat.article", production: true, productionState, endpoint: "mp.weixin.qq.com" })}\n`);

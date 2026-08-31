/* global fetch, AbortSignal */
import assert from "node:assert/strict";
import process from "node:process";
import { URL } from "node:url";

const sample = process.env.LLM_WIKI_WECHAT_PRODUCTION_SAMPLE_URL;
if (!sample) throw new Error("LLM_WIKI_WECHAT_PRODUCTION_SAMPLE_URL is required for release qualification");
const url = new URL(sample);
assert.equal(url.protocol, "https:");
assert.equal(url.hostname, "mp.weixin.qq.com");
assert.equal(url.pathname, "/s");
const response = await fetch(url, { redirect: "manual", signal: AbortSignal.timeout(45_000) });
assert.equal(response.ok && response.status < 300, true, `WeChat production sample returned ${response.status}`);
const html = await response.text();
assert.match(html, /(?:id=["']js_content["']|var\s+msg_title\s*=|property=["']og:title["'])/iu);
process.stdout.write(`${JSON.stringify({ qualified: true, route: "web.wechat.article", production: true, endpoint: "mp.weixin.qq.com" })}\n`);

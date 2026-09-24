// Uses real Chromium with controlled page acquisition; the WeChat CDN script
// is fetched live by the production pinned subresource handler.
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { spawn } from "node:child_process";
import assert from "node:assert/strict";
const [packRoot] = process.argv.slice(2);
if (!packRoot) throw new Error("Expected isolated pack from prepare-local-acceptance.mjs");
const root = path.resolve(packRoot);
const wrapper = `import { chromium } from './acceptance-runtime.mjs';
import fs from 'node:fs/promises';
const fixture = JSON.parse(await fs.readFile(process.argv[2], 'utf8'));
const launch = chromium.launchPersistentContext.bind(chromium);
chromium.launchPersistentContext = async (...args) => {
 const context = await launch(...args);
 const newPage = context.newPage.bind(context);
 context.newPage = async () => {
  const page = await newPage(); const goto = page.goto.bind(page);
  page.goto = async (url, options) => {
   if (fixture.requireCookie) {
    const cookies = await context.cookies(url);
    if (!cookies.some(cookie => cookie.name === fixture.requireCookie && cookie.value === "authenticated")) throw new Error("Session cookie backup was not consumed");
   }
   await page.route(url, route => route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: fixture.html }));
   const result = await goto(url, options);
   if (fixture.liveScript) await page.waitForFunction(() => Boolean(window.wx), { timeout: 15000 });
   return result;
  }; return page;
 }; return context;
};
await import('./index.mjs');
`;
await fs.writeFile(path.join(root, "runner/acceptance-fixture-entry.mjs"), wrapper);
const xhsPayload = (kind) => `<script>window.__INITIAL_STATE__=${JSON.stringify({ note: { noteDetailMap: { "65a0bcde0123456789abcdef": { note: { noteId: "65a0bcde0123456789abcdef", title: "视频字幕", type: "video", desc: "真实平台字幕证据", user: { nickname: "作者" }, video: { media: { stream: { h264: [{ masterUrl: "https://sns-video-qc.xhscdn.com/video.mp4" }] } }, mediaV2: JSON.stringify({ subtitles: { [kind === "source" ? "source" : "en-US"]: [{ url: "https://sns-subtitle-s2.xhscdn.com/source.srt", automatic: true, language: kind === "source" ? "zh-CN" : "en-US" }] } }) } } } } } })}</script>`;
const fixtures = [
 { name: "wechat-text-cdn", url: "https://mp.weixin.qq.com/s/acceptance", liveScript: true,
   html: '<title>页面</title><h1 id="activity-name">浏览器文章</h1><div id="js_content"><p>安全验证与 captcha 是本文讨论的内容。</p></div><script src="https://res.wx.qq.com/open/js/jweixin-1.6.0.js"></script>' },
 { name: "wechat-image", url: "https://mp.weixin.qq.com/s/acceptance",
   html: '<h1 id="activity-name">图文文章</h1><div id="js_content"><p>配图前的文字。</p><img src="data:image/png;base64," data-src="https://mmbiz.qpic.cn/acceptance-image.png"><p>配图后的文字。</p></div>' },
 { name: "xhs-original", requireCookie: "web_session", url: "https://www.xiaohongshu.com/explore/65a0bcde0123456789abcdef", html: xhsPayload("source"), subtitleKind: "platform_auto_original" },
 { name: "xhs-translation", url: "https://www.xiaohongshu.com/explore/65a0bcde0123456789abcdef", html: xhsPayload("translation"), subtitleKind: "machine_translation" },
];
for (const fixture of fixtures) {
 const stage = path.join(root, "acceptance", fixture.name);
 await fs.mkdir(stage, { recursive: true });
 const fixturePath = path.join(stage, "fixture.json");
 await fs.writeFile(fixturePath, JSON.stringify(fixture));
 const input = JSON.stringify({ jsonrpc: "2.0", id: "r1", method: "import.extract", params: {
  protocolVersion: "2", projectRoot: root, stagingRoot: path.relative(root, stage), mediaSaveMode: "extract_only",
  cookieBackup: fixture.requireCookie ? [{ name: fixture.requireCookie, value: "authenticated", domain: ".xiaohongshu.com", path: "/", expires: -1, httpOnly: true, secure: true }] : null,
  input: { kind: "url", locator: fixture.url, normalizedLocator: fixture.url },
 } });
 const child = spawn(path.join(root, "node"), [path.join(root, "runner/acceptance-fixture-entry.mjs"), fixturePath], { stdio: ["pipe", "pipe", "pipe"], env: { ...process.env, ...(fixture.requireCookie ? { LLM_WIKI_CONNECTOR_PROFILE: path.join(stage, "profile") } : {}) } });
 let stdout = "", stderr = "";
 child.stdout.on("data", chunk => stdout += chunk); child.stderr.on("data", chunk => stderr += chunk);
 const status = new Promise(resolve => child.on("close", resolve));
 child.stdin.end(input);
 assert.equal(await status, 0, stderr);
 await fs.writeFile(path.join(stage, "rpc.jsonl"), stdout);
 const messages = stdout.trim().split("\n").map(line => JSON.parse(line));
 const response = messages.find(message => message.id === "r1");
 assert.equal(response?.error, null, JSON.stringify(response));
 const markdown = await fs.readFile(path.join(stage, response.result.markdownPath), "utf8");
 if (fixture.name === "wechat-text-cdn") assert.match(markdown, /安全验证与 captcha/);
 if (fixture.name === "wechat-image") {
  assert.match(markdown, /asset:\/\/webasset-0/);
  assert.ok(messages.some(message => message.params?.url === "https://mmbiz.qpic.cn/acceptance-image.png" && message.params.kind === "image"));
 }
 if (fixture.subtitleKind) assert.ok(messages.some(message => message.params?.subtitleKind === fixture.subtitleKind), stdout);
 process.stdout.write(JSON.stringify({ sample: fixture.name, result: "passed", realChromium: true, liveCdnScript: Boolean(fixture.liveScript), subtitleKind: fixture.subtitleKind || null }) + "\n");
}

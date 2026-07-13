/* global process */
import fs from "node:fs/promises";
import path from "node:path";
import { URL } from "node:url";
import { chromium } from "playwright";
import { JSDOM } from "jsdom";
import { Readability } from "@mozilla/readability";
import createDOMPurify from "dompurify";
import TurndownService from "turndown";
import { hasPlatformAuthentication, isPinnedTargetHost, isPlatformTargetHost, loginSentinels, resolvePinnedAddress } from "./policy.mjs";

const line = await new Promise((resolve) => { let data = ""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => { data += chunk; }); process.stdin.on("end", () => resolve(data.trim())); });
const rpc = JSON.parse(line);
const params = rpc.params;

async function launchPinned(profile, target, headless) {
  const address = await resolvePinnedAddress(target.hostname);
  return chromium.launchPersistentContext(profile, {
    headless,
    acceptDownloads: false,
    ignoreHTTPSErrors: false,
    args: ["--disable-extensions", "--disable-background-networking", "--disable-component-update", "--disable-sync", "--disable-default-apps", `--host-resolver-rules=MAP ${target.hostname} ${address}, EXCLUDE localhost`],
  });
}

async function confinePage(page, target) {
  page.on("popup", (popup) => popup.close());
  await page.route("**/*", async (route) => {
    let requestUrl;
    try { requestUrl = new URL(route.request().url()); } catch { await route.abort("blockedbyclient"); return; }
    if (!["http:", "https:"].includes(requestUrl.protocol) || !isPinnedTargetHost(target.hostname, requestUrl.hostname)) {
      await route.abort("blockedbyclient");
      return;
    }
    await route.continue();
  });
}

if (rpc.method === "browser.login") {
  const sourceUrl = params.url;
  const target = new URL(sourceUrl);
  const platform = params.platform;
  if (!isPlatformTargetHost(platform, target.hostname)) throw new Error("browser platform does not match target host");
  const profile = path.resolve(params.profilePath);
  await fs.mkdir(profile, { recursive: true });
  const context = await launchPinned(profile, target, false);
  try {
    const page = context.pages()[0] || await context.newPage();
    await confinePage(page, target);
    await page.goto(sourceUrl, { waitUntil: "domcontentloaded", timeout: 45_000 });
    const deadline = Date.now() + Math.min(params.timeoutMs || 600_000, 600_000);
    let authenticated = false;
    while (Date.now() < deadline) {
      if (page.isClosed()) break;
      const cookies = await context.cookies([sourceUrl]);
      const visible = [];
      for (const selector of loginSentinels(platform)) {
        if (await page.locator(selector).first().isVisible().catch(() => false)) visible.push(selector);
      }
      if (hasPlatformAuthentication(platform, target.hostname, cookies, visible)) { authenticated = true; break; }
      await page.waitForTimeout(1000);
    }
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { authenticated }, error: null })}\n`);
  } finally { await context.close(); }
  process.exit(0);
}

const requestUrl = params.input.locator;
const publicUrl = params.input.normalizedLocator || requestUrl;
const target = new URL(requestUrl);
const platform = target.hostname === "b23.tv" || target.hostname === "bilibili.com" || target.hostname.endsWith(".bilibili.com")
  ? "bilibili" : "generic";
const stagingRoot = path.resolve(params.projectRoot, params.stagingRoot);
const profile = process.env.LLM_WIKI_CONNECTOR_PROFILE
  ? path.resolve(process.env.LLM_WIKI_CONNECTOR_PROFILE)
  : path.join(stagingRoot, "browser-profile");
await fs.mkdir(profile, { recursive: true });
const context = await launchPinned(profile, target, true);
try {
  const page = await context.newPage();
  await confinePage(page, target);
  await page.goto(requestUrl, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.waitForTimeout(750);
  const bodyText = await page.locator("body").innerText().catch(() => "");
  if (/captcha|challenge|login required|sign in/i.test(bodyText)) {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "Login or challenge required", data: { code: "IMPORT_WEB_LOGIN_REQUIRED" } } })}\n`);
    process.exitCode = 0;
  } else if (platform === "bilibili") {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "A verified subtitle is required", data: { code: "IMPORT_WEB_SUBTITLE_UNAVAILABLE" } } })}\n`);
    process.exitCode = 0;
  } else {
    const html = await page.content();
    const dom = new JSDOM(html, { url: requestUrl, runScripts: "outside-only", resources: undefined });
    const article = new Readability(dom.window.document.cloneNode(true)).parse();
    if (!article?.content || !article.title) throw new Error("dynamic content root missing");
    const clean = createDOMPurify(dom.window).sanitize(article.content, { FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form", "template"], FORBID_ATTR: ["style"] });
    const cleanDom = new JSDOM(clean, { url: requestUrl });
    let assetIndex = 0;
    for (const image of cleanDom.window.document.querySelectorAll("img")) {
      const raw = image.getAttribute("src") || image.getAttribute("data-src");
      image.removeAttribute("srcset"); image.removeAttribute("data-src");
      if (!raw) { image.remove(); continue; }
      const resolved = new URL(raw, requestUrl);
      if (!isPinnedTargetHost(target.hostname, resolved.hostname)) { image.remove(); continue; }
      const placeholder = `webasset-${assetIndex++}`;
      image.setAttribute("src", `asset://${placeholder}`);
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: { placeholder, url: resolved.href, kind: "image" } })}\n`);
    }
    const persistedHtml = cleanDom.window.document.body.innerHTML;
    const markdown = new TurndownService({ codeBlockStyle: "fenced", headingStyle: "atx" }).turndown(persistedHtml);
    await fs.writeFile(path.join(stagingRoot, "candidate.md"), `# ${article.title}\n\n${markdown}\n`);
    await fs.writeFile(path.join(stagingRoot, "source.html"), persistedHtml);
    await fs.writeFile(path.join(stagingRoot, "metadata.json"), JSON.stringify({ title: article.title, byline: article.byline, publicUrl }));
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: "source.html", markdownPath: "candidate.md", assetPaths: [], metadataPath: "metadata.json", title: article.title, textCoverage: 1, warnings: [] }, error: null })}\n`);
  }
} finally { await context.close(); }

/* global process, URL */
import fs from "node:fs/promises";
import path from "node:path";
import { JSDOM } from "jsdom";
import { Readability } from "@mozilla/readability";
import createDOMPurify from "dompurify";
import TurndownService from "turndown";

const inputLine = await new Promise((resolve) => {
  let data = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => { data += chunk; });
  process.stdin.on("end", () => resolve(data.trim()));
});
const rpc = JSON.parse(inputLine);
const params = rpc.params;
const stagingRoot = path.resolve(params.projectRoot, params.stagingRoot);
const inputPath = path.resolve(stagingRoot, params.chainedInput || "fetched.html");
if (!inputPath.startsWith(`${stagingRoot}${path.sep}`)) throw new Error("input escaped staging");

const html = await fs.readFile(inputPath, "utf8");
const sourceUrl = params.input.normalizedLocator || params.input.locator;
const dom = new JSDOM(html, { url: sourceUrl, runScripts: "outside-only", resources: undefined });
const document = dom.window.document;
const host = new URL(sourceUrl).hostname.toLowerCase();
const pageText = document.body?.textContent || "";

function rpcFailure(code, message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message, data: { code } } })}\n`);
  process.exit(0);
}
if (/环境异常|访问过于频繁|安全验证|captcha|challenge/i.test(pageText)) rpcFailure("IMPORT_WEB_CHALLENGE_DETECTED", "The site returned a challenge page.");
if (/登录后|signflow|login required/i.test(pageText)) rpcFailure("IMPORT_WEB_LOGIN_REQUIRED", "The site requires login.");

let title = "";
let byline = "";
let contentRoot = null;
if (host === "mp.weixin.qq.com") {
  title = document.querySelector("#activity-name")?.textContent?.trim() || "";
  byline = document.querySelector("#js_name")?.textContent?.trim() || "";
  contentRoot = document.querySelector("#js_content");
} else if (host === "zhihu.com" || host.endsWith(".zhihu.com")) {
  title = document.querySelector(".Post-Title,.QuestionHeader-title,h1")?.textContent?.trim() || "";
  byline = document.querySelector("[data-author]")?.getAttribute("data-author") || document.querySelector(".AuthorInfo-name")?.textContent?.trim() || "";
  contentRoot = document.querySelector(".RichContent-inner,.Post-RichText,.RichText");
} else if (host === "bilibili.com" || host.endsWith(".bilibili.com") || host === "b23.tv") {
  rpcFailure("IMPORT_WEB_SUBTITLE_UNAVAILABLE", "Bilibili metadata without a verified subtitle is not a complete import.");
} else {
  const article = new Readability(document.cloneNode(true)).parse();
  if (article?.content) {
    title = article.title || "";
    byline = article.byline || "";
    contentRoot = new JSDOM(article.content, { url: sourceUrl }).window.document.body;
  }
}
if (!title || !contentRoot || (contentRoot.textContent || "").trim().length < 20) rpcFailure("IMPORT_WEB_STRUCTURE_CHANGED", "The page did not contain a complete supported content root.");

const clean = createDOMPurify(dom.window).sanitize(contentRoot.innerHTML, {
  FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form", "template"],
  FORBID_ATTR: ["style"],
});
const cleanDom = new JSDOM(clean, { url: sourceUrl });
let assetIndex = 0;
for (const image of cleanDom.window.document.querySelectorAll("img")) {
  const raw = image.getAttribute("src") || image.getAttribute("data-src") || image.getAttribute("data-original");
  image.removeAttribute("srcset"); image.removeAttribute("data-src"); image.removeAttribute("data-original");
  if (!raw) { image.remove(); continue; }
  const absolute = new URL(raw, sourceUrl).href;
  const placeholder = `webasset-${assetIndex++}`;
  image.setAttribute("src", `asset://${placeholder}`);
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: { placeholder, url: absolute, kind: "image" } })}\n`);
}
const persistedHtml = cleanDom.window.document.body.innerHTML;
const markdown = new TurndownService({ codeBlockStyle: "fenced", headingStyle: "atx" }).turndown(persistedHtml);
await fs.writeFile(path.join(stagingRoot, "candidate.md"), `# ${title}\n\n${markdown}\n`);
await fs.writeFile(path.join(stagingRoot, "source.html"), persistedHtml);
await fs.writeFile(path.join(stagingRoot, "metadata.json"), JSON.stringify({ title, byline, publicUrl: sourceUrl }));
process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: "source.html", markdownPath: "candidate.md", assetPaths: [], metadataPath: "metadata.json", title, textCoverage: 1, warnings: [] }, error: null })}\n`);

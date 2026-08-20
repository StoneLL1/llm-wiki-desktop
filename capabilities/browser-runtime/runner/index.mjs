/* global process */
import fs from "node:fs/promises";
import { rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { Buffer } from "node:buffer";
import { URL, fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";
import { Readability } from "@mozilla/readability";
import createDOMPurify from "dompurify";
import TurndownService from "turndown";
import { extractAccountSummary, hasPlatformAuthentication, isConfinedTargetUrl, isPinnedTargetHost, isPlatformNavigationHost, isSecureAssetProtocol, isTrustedPlatformAssetHost, platformNavigationHosts, privateTargetAuthorityMatches, resolvePinnedAddress, sanitizeCookieBackup } from "./policy.mjs";
import { bilibiliMediaPolicy, classifyPlatformPage, classifyRemoteImageKind, extractBilibiliPlayerEvidenceFromHtml, extractPlatformPayload, extractPlatformPayloadFromValue, extractRelevantBilibiliPlayerEvidence, isBilibiliPlayerApiUrl, mergeBilibiliPlayerEvidence, platformHasVideoEvidence, renderPlatformMarkdown, resolveSubtitleReference, selectRelevantApiEvidence, xiaohongshuImageEvidenceReady, xiaohongshuImageOcrRequired } from "./platform-extract.mjs";
import { isLoginChallengeState, redactJsonValue, redactSensitiveText, sanitizePublicUrl } from "./snapshot-policy.mjs";
import { assertLinuxBrowserDependencies } from "./linux-deps.mjs";

const packRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const bundledBrowsers = path.join(packRoot, "runtime", "ms-playwright");
if ((await fs.stat(bundledBrowsers).catch(() => null))?.isDirectory()) {
  process.env.PLAYWRIGHT_BROWSERS_PATH = bundledBrowsers;
}
const { chromium } = await import("playwright");

const line = await new Promise((resolve) => { let data = ""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => { data += chunk; }); process.stdin.on("end", () => resolve(data.trim())); });
const rpc = JSON.parse(line);
if (rpc?.method === "capability.health") {
  const route = rpc?.params?.route;
  const allowedRoutes = new Set(["web.generic.browser", "web.wechat.article"]);
  if (rpc?.params?.protocolVersion !== "2" || rpc?.params?.capabilityId !== "browser-runtime" || !allowedRoutes.has(route)) throw new Error("invalid health request");
  const executable = chromium.executablePath();
  if (!(await fs.stat(executable).catch(() => null))?.isFile()) throw new Error("browser runtime is unavailable");
  assertLinuxBrowserDependencies(executable);
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { healthy: true, protocolVersion: "2", capabilityId: "browser-runtime", route }, error: null })}\n`);
  process.exit(0);
}
const params = rpc.params;
const privateTargetAuthority = (() => {
  try {
    const parsed = JSON.parse(process.env.LLM_WIKI_PRIVATE_TARGET_AUTHORITY || "null");
    if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.resolvedIps)) return null;
    return parsed;
  } catch {
    return null;
  }
})();

function escapeHtml(value) {
  return String(value).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

class RpcHandled extends Error {}

function resolutionOptions(url, platform) {
  const reviewedPlatformHost = url.protocol === "https:"
    && platform !== "generic"
    && isPlatformNavigationHost(platform, url.hostname);
  return {
    allowBenchmarkFakeIp: reviewedPlatformHost,
    allowedAddresses: privateTargetAuthorityMatches(privateTargetAuthority, url) ? privateTargetAuthority.resolvedIps : [],
  };
}

async function launchPinned(profile, target, headless, platform = "generic") {
  assertLinuxBrowserDependencies(chromium.executablePath());
  const hosts = new Set([target.hostname, ...platformNavigationHosts(platform)]);
  const mappings = [];
  for (const host of hosts) {
    try {
      const candidate = host === target.hostname ? target : new URL(`https://${host}/`);
      mappings.push(`MAP ${host} ${await resolvePinnedAddress(host, undefined, resolutionOptions(candidate, platform))}`);
    } catch (error) {
      if (host === target.hostname) throw error;
    }
  }
  return chromium.launchPersistentContext(profile, {
    headless,
    acceptDownloads: false,
    ignoreHTTPSErrors: false,
    args: ["--disable-extensions", "--disable-background-networking", "--disable-component-update", "--disable-sync", "--disable-default-apps", `--host-resolver-rules=${mappings.join(", ")}, EXCLUDE localhost`],
  });
}

async function confinePage(page, target, platform) {
  page.on("popup", (popup) => popup.close());
  await page.route("**/*", async (route) => {
    let requestUrl;
    try { requestUrl = new URL(route.request().url()); } catch { await route.abort("blockedbyclient"); return; }
    let allowed = isConfinedTargetUrl(target, requestUrl, privateTargetAuthority);
    const navigationHost = platform !== "generic" && isPlatformNavigationHost(platform, requestUrl.hostname);
    const assetHost = platform !== "generic"
      && isTrustedPlatformAssetHost(platform, target.hostname, requestUrl.hostname)
      && !navigationHost;
    if (navigationHost && requestUrl.protocol !== "https:") {
      await route.abort("blockedbyclient");
      return;
    }
    if (assetHost) {
      // Media/CDN URLs are emitted as remoteAsset notifications and fetched
      // by Rust's pinned HTTP client. Chromium must not resolve them again.
      await route.abort("blockedbyclient");
      return;
    }
    if (!allowed && navigationHost) {
      try {
        if (requestUrl.protocol !== "https:") throw new Error("platform navigation requires HTTPS");
        await resolvePinnedAddress(requestUrl.hostname, undefined, resolutionOptions(requestUrl, platform));
        allowed = true;
      } catch {
        allowed = false;
      }
    }
    if (!["http:", "https:"].includes(requestUrl.protocol) || !allowed) {
      await route.abort("blockedbyclient");
      return;
    }
    await route.continue();
  });
}

async function isAllowedAssetUrl(platform, target, candidate) {
  if (!isSecureAssetProtocol(candidate.protocol)) return false;
  if (platform === "generic" && isPinnedTargetHost(target.hostname, candidate.hostname)) {
    try {
      await resolvePinnedAddress(candidate.hostname, undefined, resolutionOptions(candidate, platform));
      return true;
    } catch {
      return false;
    }
  }
  if (!isTrustedPlatformAssetHost(platform, target.hostname, candidate.hostname)) return false;
  try {
    await resolvePinnedAddress(candidate.hostname, undefined, { allowBenchmarkFakeIp: true });
    return true;
  } catch {
    return false;
  }
}

function subtitleCandidates(document, baseUrl) {
  const candidates = new Set();
  for (const node of document.querySelectorAll('track[kind="subtitles"][src], track[kind="captions"][src], meta[name="subtitle"], meta[name="subtitleUrl"], meta[property="og:subtitle"]')) {
    const raw = node.getAttribute("src") || node.getAttribute("content");
    if (raw) candidates.add(raw);
  }
  const scripts = Array.from(document.scripts).map((script) => script.textContent || "").join("\n");
  const keyed = /["'](?:subtitleUrl|subtitle_url|captionUrl|subtitle|captions?|subtitle_src|caption_url)["']\s*:\s*["']([^"']+)["']/gi;
  for (const match of scripts.matchAll(keyed)) candidates.add(match[1]);
  const fileUrl = /https?:\/\/[^"'\\\s]+\.(?:vtt|srt|ass|ssa)(?:\?[^"'\\\s]*)?/gi;
  for (const match of scripts.matchAll(fileUrl)) candidates.add(match[0]);
  return Array.from(candidates).flatMap((raw) => {
    const resolved = resolveSubtitleReference(raw, baseUrl);
    return resolved ? [resolved] : [];
  });
}

if (rpc.method === "browser.login") {
  let sourceUrl = params.url;
  const target = new URL(sourceUrl);
  const platform = params.platform;
  if (!isPlatformNavigationHost(platform, target.hostname)) throw new Error("browser platform does not match target host");
  if (target.protocol === "http:" && !target.port) {
    target.protocol = "https:";
    sourceUrl = target.href;
  }
  const profile = path.resolve(params.profilePath);
  await fs.mkdir(profile, { recursive: true });
  const context = await launchPinned(profile, target, false, platform);
  try {
    if (params.cookieBackup) {
  const backup = sanitizeCookieBackup(platform, params.cookieBackup);
      if (backup.length) await context.addCookies(backup).catch(() => {});
    }
    const page = context.pages()[0] || await context.newPage();
    await confinePage(page, target, platform);
    await page.goto(sourceUrl, { waitUntil: "domcontentloaded", timeout: 45_000 });
    const deadline = Date.now() + Math.min(params.timeoutMs || 600_000, 600_000);
    let authenticated = false;
    const authUrls = [sourceUrl, ...platformNavigationHosts(platform).map((host) => `https://${host}/`)]
      .filter((value, index, values) => values.indexOf(value) === index);
    let clearedStaleCookies = false;
    while (Date.now() < deadline) {
      if (page.isClosed()) break;
      const cookies = await context.cookies(authUrls);
      const cookieProof = authUrls.some((value) => hasPlatformAuthentication(platform, new URL(value).hostname, cookies));
      const loginChallenge = isLoginChallengeState(page.url(), await page.locator("body").innerText().catch(() => ""));
      if (cookieProof && !loginChallenge) {
        authenticated = true;
        break;
      }
      if (cookieProof && loginChallenge && !clearedStaleCookies) {
        await context.clearCookies();
        clearedStaleCookies = true;
        await page.goto(sourceUrl, { waitUntil: "domcontentloaded", timeout: 45_000 }).catch(() => {});
      }
      await page.waitForTimeout(1000);
    }
    const cookies = authenticated ? await context.cookies() : [];
    const summary = authenticated ? await extractAccountSummary(page, platform) : null;
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { authenticated, cookies: sanitizeCookieBackup(platform, cookies), ...(summary ? { accountSummary: summary } : {}) }, error: null })}\n`);
  } finally { await context.close(); }
  process.exit(0);
}

function platformForHost(hostname) {
  const host = hostname.toLowerCase();
  if (host === "mp.weixin.qq.com") return "wechat";
  if (host === "zhihu.com" || host.endsWith(".zhihu.com")) return "zhihu";
  if (host === "b23.tv" || host === "bilibili.com" || host.endsWith(".bilibili.com")) return "bilibili";
  if (host === "xiaohongshu.com"
    || host.endsWith(".xiaohongshu.com")
    || host === "xhslink.com"
    || host.endsWith(".xhslink.com")
    || host === "xhslink.cn"
    || host.endsWith(".xhslink.cn")) return "xiaohongshu";
  if (host === "douyin.com" || host.endsWith(".douyin.com") || host === "iesdouyin.com" || host.endsWith(".iesdouyin.com")) return "douyin";
  if (host === "x.com" || host.endsWith(".x.com") || host === "twitter.com" || host.endsWith(".twitter.com") || host === "t.co") return "x";
  return "generic";
}

function normalizePlatformAssetUrl(platform, raw, baseUrl) {
  const candidate = new URL(raw, baseUrl);
  const host = candidate.hostname.toLowerCase();
  const xhsCdn = host === "xhscdn.com"
    || host.endsWith(".xhscdn.com")
    || host === "xhscdn.net"
    || host.endsWith(".xhscdn.net");
  if (platform === "xiaohongshu"
    && candidate.protocol === "http:"
    && xhsCdn
    && (!candidate.port || candidate.port === "80")) {
    candidate.protocol = "https:";
    candidate.port = "";
  }
  return candidate;
}
const requestUrl = params.input.locator;
const publicUrl = params.input.normalizedLocator || requestUrl;
const target = new URL(requestUrl);
let platform = platformForHost(target.hostname);
const stagingRoot = path.resolve(params.projectRoot, params.stagingRoot);
const retainedProfile = Boolean(process.env.LLM_WIKI_CONNECTOR_PROFILE);
const profile = retainedProfile
  ? path.resolve(process.env.LLM_WIKI_CONNECTOR_PROFILE)
  : await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-browser-"));
if (retainedProfile) await fs.mkdir(profile, { recursive: true });
if (!retainedProfile) {
  process.once("exit", () => {
    try { rmSync(profile, { recursive: true, force: true }); } catch { /* best-effort temp cleanup */ }
  });
}
const context = await launchPinned(profile, target, true, platform);
try {
  const page = await context.newPage();
  await confinePage(page, target, platform);
  const apiCandidates = [];
  const bilibiliPlayerCandidates = [];
  const pendingApiCaptures = new Set();
  if (platform !== "generic") {
    page.on("response", (response) => {
      const capture = (async () => {
        const resourceType = response.request().resourceType();
        if (!['fetch', 'xhr'].includes(resourceType) || response.status() < 200 || response.status() >= 300) return;
        let responseUrl;
        try { responseUrl = new URL(response.url()); } catch { return; }
        if (!isPlatformNavigationHost(platform, responseUrl.hostname)) return;
        const headers = await response.allHeaders();
        if (!String(headers['content-type'] || '').toLowerCase().includes('json')) return;
        const rawLength = headers['content-length'];
        if (rawLength !== undefined) {
          const declaredLength = Number(rawLength);
          if (!Number.isFinite(declaredLength) || declaredLength < 0 || declaredLength > 2 * 1024 * 1024) return;
        }
        const body = await response.text().catch(() => null);
        if (!body || Buffer.byteLength(body, 'utf8') > 2 * 1024 * 1024) return;
        let value;
        try { value = JSON.parse(body); } catch { return; }
        const candidate = { url: responseUrl.href, value };
        if (platform === "bilibili" && isBilibiliPlayerApiUrl(responseUrl.href)) {
          bilibiliPlayerCandidates.push(candidate);
          if (bilibiliPlayerCandidates.length > 16) bilibiliPlayerCandidates.shift();
          return;
        }
        const relevant = extractPlatformPayloadFromValue(platform, value, page.url());
        if (relevant) apiCandidates.unshift(candidate);
        else if (apiCandidates.length < 16) apiCandidates.push(candidate);
        if (apiCandidates.length > 16) apiCandidates.length = 16;
      })().catch(() => {});
      pendingApiCaptures.add(capture);
      capture.finally(() => pendingApiCaptures.delete(capture));
    });
  }
  await page.goto(requestUrl, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.waitForTimeout(750);
  await Promise.allSettled([...pendingApiCaptures]);
  const finalUrl = page.url();
  const finalPlatform = platformForHost(new URL(finalUrl).hostname);
  if (finalPlatform !== "generic") platform = finalPlatform;
  const bodyText = await page.locator("body").innerText().catch(() => "");
  const html = await page.content();
  const capturedApiCandidates = [...bilibiliPlayerCandidates, ...apiCandidates];
  let platformPayload = extractPlatformPayload(platform, html, finalUrl);
  if (!platformPayload && platform !== "generic") {
    platformPayload = capturedApiCandidates
      .map((candidate) => extractPlatformPayloadFromValue(platform, candidate.value, finalUrl))
      .find(Boolean) || null;
  }
  if (platform === "bilibili" && platformPayload) {
    platformPayload = mergeBilibiliPlayerEvidence(platformPayload, [
      extractBilibiliPlayerEvidenceFromHtml(html, finalUrl, platformPayload.targetAliases),
      ...capturedApiCandidates.map((candidate) =>
        extractRelevantBilibiliPlayerEvidence(candidate, finalUrl, platformPayload.targetAliases)),
    ]);
  }
  const platformFailure = platformPayload ? null : classifyPlatformPage(platform, bodyText);
  if (platformFailure) {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "Platform access requires user action", data: { code: platformFailure } } })}\n`);
    process.exitCode = 0;
  } else {
    const dom = new JSDOM(html, { url: finalUrl, runScripts: "outside-only", resources: undefined });
    if (platform === "xiaohongshu" && !platformPayload) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "The requested Xiaohongshu note payload was not found", data: { code: "IMPORT_WEB_STRUCTURE_CHANGED" } } })}\n`);
      throw new RpcHandled();
    }
    if (platform === "xiaohongshu"
      && xiaohongshuImageOcrRequired(platformPayload, params.localOcrAuthorized)) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "Xiaohongshu image posts require verified local OCR before preview", data: { code: "IMPORT_WEB_OCR_UNAVAILABLE" } } })}\n`);
      throw new RpcHandled();
    }
    let subtitleCandidateEmitted = false;
    let subtitleIndex = 0;
    const subtitleAssets = new Map();
    for (const candidate of platformPayload?.subtitleCandidates || []) {
      try {
        const subtitleUrl = normalizePlatformAssetUrl(platform, candidate.url, finalUrl);
        subtitleAssets.set(subtitleUrl.href, {
          url: subtitleUrl,
          automatic: Boolean(candidate.automatic),
          language: candidate.language || null,
          label: candidate.label || null,
        });
      } catch { /* ignored */ }
    }
    for (const raw of platformPayload?.subtitles || []) {
      try {
        const subtitleUrl = normalizePlatformAssetUrl(platform, raw, finalUrl);
        if (!subtitleAssets.has(subtitleUrl.href)) {
          subtitleAssets.set(subtitleUrl.href, {
            url: subtitleUrl,
            automatic: null,
            language: null,
            label: null,
          });
        }
      } catch { /* ignored */ }
    }
    for (const raw of subtitleCandidates(dom.window.document, finalUrl)) {
      const subtitleUrl = normalizePlatformAssetUrl(platform, raw.href, finalUrl);
      if (!subtitleAssets.has(subtitleUrl.href)) {
        subtitleAssets.set(subtitleUrl.href, {
          url: subtitleUrl,
          automatic: null,
          language: null,
          label: null,
        });
      }
    }
    for (const subtitle of subtitleAssets.values()) {
      const subtitleUrl = subtitle.url;
      if (!(await isAllowedAssetUrl(platform, target, subtitleUrl))) continue;
      process.stdout.write(`${JSON.stringify({
        jsonrpc: "2.0",
        method: "import.remoteAsset",
        params: {
          placeholder: `platform-subtitle-${subtitleIndex++}`,
          url: subtitleUrl.href,
          kind: "subtitle",
          automatic: subtitle.automatic,
          language: subtitle.language,
          label: subtitle.label,
        },
      })}\n`);
      subtitleCandidateEmitted = true;
      if (subtitleIndex >= 4) break;
    }
    const mediaRaw = platformPayload?.mediaUrl
      || dom.window.document.querySelector('meta[property="og:video"], meta[name="twitter:player:stream"]')?.getAttribute("content")
      || dom.window.document.querySelector("video[src], video source[src]")?.getAttribute("src");
    const asrMediaRaw = platform === "bilibili"
      ? platformPayload?.asrMediaUrl || mediaRaw
      : mediaRaw;
    if (platform === "xiaohongshu"
      && platformPayload?.contentType === "video"
      && !mediaRaw
      && !subtitleCandidateEmitted) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "The Xiaohongshu video did not expose a playable media stream", data: { code: "IMPORT_WEB_STRUCTURE_CHANGED" } } })}\n`);
      throw new RpcHandled();
    }
    const bilibiliPolicy = platform === "bilibili"
      ? bilibiliMediaPolicy(
        { mediaUrl: mediaRaw, asrMediaUrl: asrMediaRaw },
        {
          mediaSaveMode: params.mediaSaveMode,
          hasSubtitle: subtitleCandidateEmitted,
          localAsrAuthorized: Boolean(params.localAsrAuthorized),
          allowMissingTranscript: Boolean(params.allowMissingTranscript),
        },
      )
      : null;
    if (bilibiliPolicy?.errorCode) {
      const message = bilibiliPolicy.errorCode === "IMPORT_WEB_MEDIA_UNAVAILABLE"
        ? "Bilibili exposed only separate DASH tracks, so a complete original video could not be preserved"
        : "A verified subtitle or local ASR capability is required";
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message, data: { code: bilibiliPolicy.errorCode } } })}\n`);
      throw new RpcHandled();
    }
    if (platform !== "bilibili"
      && (mediaRaw || asrMediaRaw)
      && !params.localAsrAuthorized
      && !subtitleCandidateEmitted
      && (!params.allowMissingTranscript || platform === "xiaohongshu")) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "A verified subtitle or local ASR capability is required", data: { code: "IMPORT_WEB_SUBTITLE_UNAVAILABLE" } } })}\n`);
      throw new RpcHandled();
    }
    let originalMediaPlaceholder = false;
    if (mediaRaw) {
      const mediaUrl = normalizePlatformAssetUrl(platform, mediaRaw, finalUrl);
      if (await isAllowedAssetUrl(platform, target, mediaUrl)) {
        // Always report the durable media candidate. Rust drops it in
        // extraction-only mode and preserves it only when explicitly requested.
        process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: { placeholder: "original-media", url: mediaUrl.href, kind: "media" } })}\n`);
        originalMediaPlaceholder = params.mediaSaveMode === "preserve_original";
      } else if (platform !== "generic") {
        process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "The platform media host is not in the verified allowlist", data: { code: "IMPORT_WEB_MEDIA_HOST_UNSUPPORTED" } } })}\n`);
        throw new RpcHandled();
      }
    }
    const authorizedAsrMediaRaw = platform === "bilibili"
      ? bilibiliPolicy?.asrMediaUrl
      : params.localAsrAuthorized
        && (!params.allowMissingTranscript || platform === "xiaohongshu")
        ? asrMediaRaw
        : null;
    if (authorizedAsrMediaRaw) {
      const asrMediaUrl = normalizePlatformAssetUrl(platform, authorizedAsrMediaRaw, finalUrl);
      if (await isAllowedAssetUrl(platform, target, asrMediaUrl)) {
        process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: { placeholder: "temporary-media", url: asrMediaUrl.href, kind: "temporary_media" } })}\n`);
      } else if (platform !== "generic") {
        process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "The platform ASR media host is not in the verified allowlist", data: { code: "IMPORT_WEB_MEDIA_HOST_UNSUPPORTED" } } })}\n`);
        throw new RpcHandled();
      }
    }
    const article = new Readability(dom.window.document.cloneNode(true)).parse();
    const warnings = [];
    const title = platformPayload?.title || article?.title || dom.window.document.querySelector('meta[property="og:title"]')?.getAttribute("content") || target.pathname;
    const safePublicUrl = sanitizePublicUrl(finalUrl || publicUrl);
    const readableContent = platformPayload?.description
      ? `<p>${escapeHtml(platformPayload.description)}</p>`
      : article?.content || `<p>Media page imported from <a href="${safePublicUrl}">${safePublicUrl}</a>.</p>`;
    const clean = createDOMPurify(dom.window).sanitize(readableContent, { FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form", "template"], FORBID_ATTR: ["style"] });
    const cleanDom = new JSDOM(clean, { url: finalUrl });
    for (const imageUrl of platformPayload?.images || []) {
      const image = cleanDom.window.document.createElement("img");
      image.setAttribute("src", imageUrl);
      cleanDom.window.document.body.appendChild(image);
    }
    let assetIndex = 0;
    const seenImages = new Set();
    const platformImageLinks = [];
    for (const image of cleanDom.window.document.querySelectorAll("img")) {
      const raw = image.getAttribute("src") || image.getAttribute("data-src");
      image.removeAttribute("srcset"); image.removeAttribute("data-src");
      if (!raw) { image.remove(); continue; }
      let resolved;
      try { resolved = normalizePlatformAssetUrl(platform, raw, finalUrl); } catch { image.remove(); continue; }
      if (seenImages.has(resolved.href)) { image.remove(); continue; }
      seenImages.add(resolved.href);
      if (!(await isAllowedAssetUrl(platform, target, resolved))) {
        image.remove();
        if (platform !== "generic" && !warnings.includes("PLATFORM_IMAGE_HOST_UNSUPPORTED")) {
          warnings.push("PLATFORM_IMAGE_HOST_UNSUPPORTED");
        }
        continue;
      }
      const kind = classifyRemoteImageKind(
        platform,
        platformHasVideoEvidence(platformPayload, mediaRaw, asrMediaRaw),
        Boolean(params.localOcrAuthorized),
        params.mediaSaveMode,
      );
      if (!kind) { image.remove(); continue; }
      const placeholder = `webasset-${assetIndex++}`;
      image.setAttribute("src", `asset://${placeholder}`);
      platformImageLinks.push(`asset://${placeholder}`);
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", method: "import.remoteAsset", params: { placeholder, url: resolved.href, kind } })}\n`);
    }
    if (platform === "xiaohongshu"
      && !xiaohongshuImageEvidenceReady(platformPayload, assetIndex)) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "No Xiaohongshu note image was available for required OCR", data: { code: "IMPORT_WEB_MEDIA_UNAVAILABLE" } } })}\n`);
      throw new RpcHandled();
    }
    if (platform !== "generic"
      && !mediaRaw
      && !asrMediaRaw
      && subtitleIndex === 0
      && assetIndex === 0
      && !(platformPayload?.images?.length)
      && !(platform === "bilibili" && params.allowMissingTranscript && platformPayload)) {
      process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: null, error: { code: -32010, message: "The platform page did not expose media, subtitles, or usable images", data: { code: "IMPORT_WEB_STRUCTURE_CHANGED" } } })}\n`);
      throw new RpcHandled();
    }
    if (originalMediaPlaceholder) {
      const paragraph = cleanDom.window.document.createElement("p");
      const link = cleanDom.window.document.createElement("a");
      link.setAttribute("href", "asset://original-media");
      link.textContent = "Original media";
      paragraph.appendChild(link);
      cleanDom.window.document.body.appendChild(paragraph);
    }
    const persistedHtml = cleanDom.window.document.body.innerHTML;
    const markdown = platform !== "generic" && platformPayload
      ? renderPlatformMarkdown(
        platform,
        platformPayload,
        safePublicUrl,
        platformImageLinks,
        originalMediaPlaceholder ? "asset://original-media" : null,
        params.mediaSaveMode,
      )
      : new TurndownService({ codeBlockStyle: "fenced", headingStyle: "atx" }).turndown(persistedHtml);
    const extractedText = platform === "generic"
      ? String(article?.textContent || bodyText || "").trim()
      : String(platformPayload?.description || "").trim();
    if (platform === "generic" && !article) warnings.push("READABILITY_FALLBACK");
    await fs.writeFile(path.join(stagingRoot, "candidate.md"), platform !== "generic" && platformPayload ? markdown : `# ${title}\n\n${markdown}\n`);
    // `source.html` is the immutable raw evidence snapshot. Keep the
    // sanitized article separately for diagnostics; Markdown is derived from
    // the sanitized DOM and never replaces the original response.
    await fs.writeFile(path.join(stagingRoot, "source.html"), redactSensitiveText(html));
    await fs.writeFile(path.join(stagingRoot, "sanitized.html"), redactSensitiveText(persistedHtml));
    await fs.writeFile(path.join(stagingRoot, "metadata.json"), JSON.stringify({
      title,
      author: platformPayload?.author || article?.byline || null,
      publishedAt: platformPayload?.publishedAt || null,
      publicUrl: safePublicUrl,
      platform,
      platformId: platformPayload?.platformId || null,
      contentType: platformPayload?.contentType || null,
      titleSource: platformPayload?.titleSource || null,
      hashtags: platformPayload?.hashtags || [],
      imageCount: platformPayload?.images?.length || 0,
      mediaPresent: Boolean(platformPayload?.mediaUrl || platformPayload?.asrMediaUrl),
    }));
    const sourceEvidencePaths = [];
    for (const candidate of selectRelevantApiEvidence(
      platform,
      capturedApiCandidates,
      finalUrl,
      3,
      platformPayload?.targetAliases,
    )) {
      const evidencePath = `source-evidence/${platform}-api-${sourceEvidencePaths.length + 1}.json`;
      await fs.mkdir(path.join(stagingRoot, "source-evidence"), { recursive: true });
      await fs.writeFile(path.join(stagingRoot, evidencePath), JSON.stringify(redactJsonValue(candidate.value)));
      sourceEvidencePaths.push(evidencePath);
      if (sourceEvidencePaths.length >= 3) break;
    }
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result: { sourceSnapshotPath: "source.html", markdownPath: "candidate.md", assetPaths: sourceEvidencePaths, metadataPath: "metadata.json", title, textCoverage: extractedText ? 1 : 0, warnings }, error: null })}\n`);
  }
} catch (error) {
  if (!(error instanceof RpcHandled)) throw error;
} finally {
  await context.close();
  if (!retainedProfile) await fs.rm(profile, { recursive: true, force: true }).catch(() => {});
}

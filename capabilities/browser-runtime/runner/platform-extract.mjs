import { Buffer } from "node:buffer";
import { URL } from "node:url";

const MAX_SCRIPT_BYTES = 4 * 1024 * 1024;
const MAX_VALUES = 128;
const MAX_WALK_VALUES = 50_000;

function balancedJson(input) {
  const start = input.search(/\{|\[/);
  if (start < 0) return null;
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let index = start; index < input.length; index += 1) {
    const character = input[index];
    if (inString) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') inString = false;
      continue;
    }
    if (character === '"') inString = true;
    else if (character === "{" || character === "[") depth += 1;
    else if (character === "}" || character === "]") {
      depth -= 1;
      if (depth === 0) {
        try { return JSON.parse(input.slice(start, index + 1)); } catch { return null; }
      }
    }
  }
  return null;
}

function scriptValues(html) {
  const values = [];
  const scripts = String(html).matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi);
  for (const match of scripts) {
    if (values.length >= MAX_VALUES) break;
    const source = match[1].trim();
    if (!source || Buffer.byteLength(source, "utf8") > MAX_SCRIPT_BYTES) continue;
    let value;
    try { value = JSON.parse(source); } catch { value = balancedJson(source); }
    if (value && typeof value === "object") values.push(value);
  }
  return values;
}

function walk(value, callback) {
  if (!value || typeof value !== "object") return;
  const pending = [value];
  let visited = 0;
  while (pending.length > 0 && visited < MAX_WALK_VALUES) {
    const candidate = pending.pop();
    if (!candidate || typeof candidate !== "object") continue;
    visited += 1;
    callback(candidate);
    for (const child of Object.values(candidate)) {
      if (child && typeof child === "object") pending.push(child);
    }
  }
}

function stringValue(value) {
  if (typeof value === "string") return value.trim() || null;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return null;
}

function firstString(value, keys) {
  let found = null;
  walk(value, (candidate) => {
    if (found || Array.isArray(candidate)) return;
    for (const key of keys) {
      const actual = Object.keys(candidate).find((name) => name.toLowerCase() === key.toLowerCase());
      if (!actual) continue;
      const direct = stringValue(candidate[actual]);
      if (direct) { found = direct; return; }
      const object = candidate[actual];
      if (object && typeof object === "object") {
        for (const nested of ["name", "nickname", "displayName", "uname"]) {
          const nestedValue = stringValue(object[nested]);
          if (nestedValue) { found = nestedValue; return; }
        }
      }
    }
  });
  return found;
}

function urlsFrom(value, baseUrl, output, limit) {
  const pending = [value];
  let visited = 0;
  while (pending.length > 0 && output.length < limit && visited < MAX_WALK_VALUES) {
    const candidate = pending.pop();
    visited += 1;
    if (typeof candidate === "string") {
      try {
        const url = new URL(candidate, baseUrl);
        if (["http:", "https:"].includes(url.protocol) && !output.includes(url.href)) output.push(url.href);
      } catch { /* ignored */ }
    } else if (candidate && typeof candidate === "object") {
      for (const child of Object.values(candidate)) pending.push(child);
    }
  }
}

function collectKeyUrls(value, keys, baseUrl, limit) {
  const output = [];
  walk(value, (candidate) => {
    if (output.length >= limit || Array.isArray(candidate)) return;
    for (const [key, child] of Object.entries(candidate)) {
      if (keys.some((expected) => key.toLowerCase() === expected.toLowerCase())) {
        urlsFrom(child, baseUrl, output, limit);
      }
    }
  });
  return output.slice(0, limit);
}

function looksLike(platform, value) {
  if (platform === "bilibili") return Boolean(firstString(value, ["bvid", "aid"]) && firstString(value, ["title"]));
  if (platform === "xiaohongshu") return Boolean(firstString(value, ["noteId", "note_id", "xsecToken", "userId"]) && firstString(value, ["title", "desc", "description"]));
  if (platform === "douyin") return Boolean(firstString(value, ["awemeId", "aweme_id", "secUid", "sec_uid"]) && firstString(value, ["title", "desc", "description"]));
  return false;
}

function targetIdentity(platform, baseUrl) {
  let pathname;
  try { pathname = new URL(baseUrl).pathname; } catch { return null; }
  const pattern = platform === "bilibili"
    ? /\/video\/(BV[a-zA-Z0-9]+|av\d+)/i
    : platform === "xiaohongshu"
      ? /\/(?:explore|discovery\/item)\/([a-zA-Z0-9]+)/i
      : /\/(?:video|note)\/(\d+)/i;
  const match = pathname.match(pattern);
  if (!match) return null;
  const raw = match[1];
  return platform === "bilibili" && /^av\d+$/i.test(raw) ? raw.slice(2) : raw;
}

function idKeys(platform) {
  if (platform === "bilibili") return ["bvid", "aid"];
  if (platform === "xiaohongshu") return ["noteId", "note_id", "id"];
  return ["awemeId", "aweme_id", "itemId", "item_id"];
}

function matchingTarget(value, platform, expectedId) {
  if (!expectedId) return null;
  let found = null;
  const expected = String(expectedId).toLowerCase();
  walk(value, (candidate) => {
    if (found || Array.isArray(candidate)) return;
    for (const key of idKeys(platform)) {
      const actual = Object.keys(candidate).find((name) => name.toLowerCase() === key.toLowerCase());
      if (!actual) continue;
      const candidateId = stringValue(candidate[actual]);
      if (candidateId?.toLowerCase() === expected) { found = candidate; return; }
    }
  });
  return found;
}

export function extractPlatformPayloadFromValue(platform, value, baseUrl) {
  if (!["bilibili", "xiaohongshu", "douyin"].includes(platform) || !value || typeof value !== "object") return null;
  const expectedId = targetIdentity(platform, baseUrl);
  const scope = expectedId ? matchingTarget(value, platform, expectedId) : (looksLike(platform, value) ? value : null);
  if (!scope) return null;
  const title = firstString(scope, ["title", "noteTitle", "videoTitle", "desc", "description"]);
  if (!title) return null;
  const description = firstString(scope, ["desc", "description", "content", "text", "caption"]) || "";
  const author = firstString(scope, ["author", "nickname", "nickName", "uname", "username", "name"]);
  const publishedAt = firstString(scope, ["publishedAt", "publishTime", "createTime", "create_time", "pubdate"]);
  const imageKeys = platform === "bilibili"
    ? ["pic", "cover", "thumbnail", "coverUrl", "cover_url"]
    : platform === "xiaohongshu"
      ? ["imageList", "image_list", "urlDefault", "url_default", "images"]
      : ["images", "imageList", "displayImage", "display_image", "cover"];
  const mediaKeys = platform === "bilibili"
    ? ["durl", "baseUrl", "base_url", "playUrl", "play_url", "videoUrl", "video_url"]
    : platform === "xiaohongshu"
      ? ["masterUrl", "master_url", "videoUrl", "video_url", "playUrl", "play_url"]
      : ["playAddr", "play_addr", "playUrl", "play_url"];
  return {
    title,
    description,
    author,
    publishedAt,
    images: collectKeyUrls(scope, imageKeys, baseUrl, 100),
    mediaUrl: collectKeyUrls(scope, mediaKeys, baseUrl, 1)[0] || null,
    subtitles: collectKeyUrls(scope, ["subtitles", "subtitle", "captions", "captionUrl", "subtitleUrl"], baseUrl, 20),
  };
}

export function extractPlatformPayload(platform, html, baseUrl) {
  if (!["bilibili", "xiaohongshu", "douyin"].includes(platform)) return null;
  for (const value of scriptValues(html)) {
    const payload = extractPlatformPayloadFromValue(platform, value, baseUrl);
    if (payload) return payload;
  }
  return null;
}

export function selectRelevantApiEvidence(platform, candidates, baseUrl, limit = 3) {
  if (!Array.isArray(candidates) || !Number.isSafeInteger(limit) || limit <= 0) return [];
  return candidates
    .filter((candidate) => candidate && typeof candidate === "object"
      && extractPlatformPayloadFromValue(platform, candidate.value, baseUrl))
    .slice(0, limit);
}

export function classifyRemoteImageKind(platform, _hasPlayableMedia, localOcrAuthorized, mediaSaveMode = "preserve_original") {
  if (platform === "generic") return "image";
  if (localOcrAuthorized) return "temporary_image";
  return mediaSaveMode === "preserve_original" ? "image" : null;
}

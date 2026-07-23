import { Buffer } from "node:buffer";
import { URL } from "node:url";

const MAX_SCRIPT_BYTES = 4 * 1024 * 1024;
const MAX_VALUES = 128;
const MAX_WALK_VALUES = 50_000;

function normalizeUndefinedLiterals(input) {
  let output = "";
  let inString = false;
  let escaped = false;
  for (let index = 0; index < input.length; index += 1) {
    const character = input[index];
    if (inString) {
      output += character;
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') inString = false;
      continue;
    }
    if (character === '"') {
      inString = true;
      output += character;
      continue;
    }
    if (input.startsWith("undefined", index)
      && !/[A-Za-z0-9_$]/.test(input[index - 1] || "")
      && !/[A-Za-z0-9_$]/.test(input[index + 9] || "")) {
      output += "null";
      index += 8;
      continue;
    }
    output += character;
  }
  return output;
}

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
        const candidate = input.slice(start, index + 1);
        try { return JSON.parse(candidate); } catch {
          try { return JSON.parse(normalizeUndefinedLiterals(candidate)); } catch { return null; }
        }
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
    for (const child of Object.values(candidate).reverse()) {
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
      for (const child of Object.values(candidate).reverse()) pending.push(child);
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

function normalizedAssetUrl(raw, baseUrl) {
  if (!raw) return null;
  try {
    const request = new URL(raw, baseUrl);
    if (!["http:", "https:"].includes(request.protocol)) return null;
    const identity = new URL(request);
    identity.search = "";
    identity.hash = "";
    return { request: request.href, identity: identity.href };
  } catch {
    return null;
  }
}

function preferredXhsImageUrl(image) {
  if (typeof image === "string") return image;
  if (!image || typeof image !== "object") return null;
  for (const key of ["urlDefault", "url_default", "urlPre", "url_pre", "url"]) {
    const value = stringValue(image[key]);
    if (value) return value;
  }
  const infoList = image.infoList || image.info_list;
  if (!Array.isArray(infoList)) return null;
  const preferred = infoList.find((entry) =>
    String(entry?.imageScene || entry?.image_scene || "").toUpperCase() === "WB_DFT"
      && stringValue(entry?.url));
  return stringValue(preferred?.url)
    || infoList.map((entry) => stringValue(entry?.url)).find(Boolean)
    || null;
}

function collectXhsImages(value, baseUrl, limit) {
  let selected = null;
  walk(value, (candidate) => {
    if (selected || Array.isArray(candidate)) return;
    const key = Object.keys(candidate)
      .find((name) => ["imagelist", "image_list"].includes(name.toLowerCase()));
    if (key && Array.isArray(candidate[key])) selected = candidate[key];
  });
  if (!selected) {
    return collectKeyUrls(value, ["urlDefault", "url_default", "images"], baseUrl, limit);
  }
  const output = [];
  const seen = new Set();
  for (const image of selected) {
    const normalized = normalizedAssetUrl(preferredXhsImageUrl(image), baseUrl);
    if (!normalized || seen.has(normalized.identity)) continue;
    seen.add(normalized.identity);
    output.push(normalized.request);
    if (output.length >= limit) break;
  }
  return output;
}

function inferTitle(description) {
  const line = String(description || "")
    .split(/\r?\n/)
    .map((value) => value.trim())
    .find(Boolean);
  return line ? Array.from(line).slice(0, 80).join("") : null;
}

function hashtagsFrom(description) {
  return [...new Set(String(description || "").match(/#[\p{L}\p{N}_]+/gu) || [])];
}

function firstNestedString(value, containerKeys, valueKeys) {
  let result = null;
  walk(value, (candidate) => {
    if (result || Array.isArray(candidate)) return;
    const container = Object.keys(candidate)
      .find((name) => containerKeys.some((key) => name.toLowerCase() === key.toLowerCase()));
    if (container) result = firstString(candidate[container], valueKeys);
  });
  return result;
}

function collectXhsTags(value, limit = 100) {
  const result = [];
  walk(value, (candidate) => {
    if (result.length >= limit || Array.isArray(candidate)) return;
    const key = Object.keys(candidate)
      .find((name) => ["taglist", "tag_list", "tags"].includes(name.toLowerCase()));
    if (!key || !Array.isArray(candidate[key])) return;
    for (const tag of candidate[key]) {
      const raw = typeof tag === "string"
        ? tag.trim()
        : firstString(tag, ["tagName", "tag_name", "name", "title"]);
      if (!raw) continue;
      const normalized = raw.startsWith("#") ? raw : `#${raw}`;
      if (!result.includes(normalized)) result.push(normalized);
      if (result.length >= limit) break;
    }
  });
  return result;
}

function normalizePublishedAt(value) {
  if (!value || !/^\d+$/.test(value)) return value;
  const numeric = Number(value);
  if (!Number.isSafeInteger(numeric)) return value;
  const millis = numeric >= 100_000_000_000 ? numeric : numeric * 1000;
  const date = new Date(millis);
  return Number.isNaN(date.valueOf()) ? value : date.toISOString();
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
      ? /\/(?:explore|discovery\/item)\/([a-zA-Z0-9]+)|\/user\/profile\/[^/]+\/([a-zA-Z0-9]+)/i
      : /\/(?:video|note)\/(\d+)/i;
  const match = pathname.match(pattern);
  if (!match) return null;
  const raw = match[1] || match[2];
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
  if (platform === "xiaohongshu" && !expectedId) return null;
  const scope = expectedId ? matchingTarget(value, platform, expectedId) : (looksLike(platform, value) ? value : null);
  if (!scope) return null;
  const description = firstString(scope, ["desc", "description", "content", "text", "caption"]) || "";
  const explicitTitle = firstString(scope, ["title", "noteTitle", "videoTitle"]);
  const title = explicitTitle
    || (["xiaohongshu", "douyin"].includes(platform) ? inferTitle(description) : null);
  if (!title) return null;
  const author = platform === "xiaohongshu"
    ? firstNestedString(scope, ["user", "author"], ["nickname", "nickName", "username", "name"])
    : firstString(scope, ["author", "nickname", "nickName", "uname", "username", "name"]);
  const publishedAt = normalizePublishedAt(firstString(scope, ["publishedAt", "publishTime", "createTime", "create_time", "time", "timestamp", "pubdate"]));
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
  const images = platform === "xiaohongshu"
    ? collectXhsImages(scope, baseUrl, 100)
    : collectKeyUrls(scope, imageKeys, baseUrl, 100);
  const mediaUrl = collectKeyUrls(scope, mediaKeys, baseUrl, 1)[0] || null;
  const declaredVideo = platform === "xiaohongshu"
    && String(firstString(scope, ["type", "noteType", "note_type"]) || "").toLowerCase() === "video";
  const hashtags = hashtagsFrom(description);
  if (platform === "xiaohongshu") {
    for (const tag of collectXhsTags(scope)) if (!hashtags.includes(tag)) hashtags.push(tag);
  }
  return {
    title,
    titleSource: explicitTitle ? "platform" : "inferred",
    description,
    author,
    publishedAt,
    platformId: expectedId,
    contentType: declaredVideo || mediaUrl ? "video" : images.length ? "image_post" : "article",
    images,
    mediaUrl,
    subtitles: collectKeyUrls(scope, ["subtitles", "subtitle", "captions", "captionUrl", "subtitleUrl"], baseUrl, 20),
    hashtags,
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

export function classifyPlatformPage(platform, pageText) {
  const text = String(pageText || "").toLowerCase();
  if (/captcha|challenge|滑块验证|请通过验证|请完成验证|安全验证|访问过于频繁/i.test(text)) {
    return "IMPORT_WEB_CAPTCHA_REQUIRED";
  }
  if (/login required|signflow|请先登录|登录后查看|登录后浏览/i.test(text)) {
    return "IMPORT_WEB_LOGIN_REQUIRED";
  }
  if (platform === "xiaohongshu"
    && /note has been deleted|note not found|笔记已删除|该笔记已被删除|内容不存在|当前内容无法展示/i.test(text)) {
    return "IMPORT_WEB_CONTENT_REMOVED";
  }
  return null;
}

function yaml(value) {
  return JSON.stringify(String(value ?? ""));
}

export function renderPlatformMarkdown(
  platform,
  payload,
  publicUrl,
  imageLinks = [],
  mediaLink = null,
  mediaSaveMode = "preserve_original",
) {
  const platformName = platform === "xiaohongshu" ? "小红书" : platform === "douyin" ? "抖音" : "Bilibili";
  const lines = [
    "---",
    "type: source",
    `title: ${yaml(payload.title)}`,
    `title_source: ${yaml(payload.titleSource || "platform")}`,
    `source_url: ${yaml(publicUrl)}`,
    `source_platform: ${yaml(platform)}`,
    `content_type: ${yaml(payload.contentType || (payload.mediaUrl ? "video" : payload.images?.length ? "image_post" : "article"))}`,
    `route: ${yaml("web.generic.browser")}`,
    `engine_id: ${yaml("browser-runtime")}`,
    `engine_version: ${yaml("0.1.0")}`,
  ];
  if (payload.platformId) lines.push(`source_id: ${yaml(payload.platformId)}`);
  if (payload.author) lines.push(`author: ${yaml(payload.author)}`);
  if (payload.publishedAt) lines.push(`published_at: ${yaml(payload.publishedAt)}`);
  lines.push("---", "", `# ${payload.title}`, "", `> 来源：[${platformName}](${publicUrl})`, "", "## 来源信息", "", `- 平台：${platformName}`);
  if (payload.platformId) lines.push(`- 平台 ID：${payload.platformId}`);
  if (payload.author) lines.push(`- 作者：${payload.author}`);
  if (payload.publishedAt) lines.push(`- 发布时间：${payload.publishedAt}`);
  lines.push(`- 来源：${publicUrl}`, "- 导入路线：`web.generic.browser`");
  if (payload.titleSource === "inferred") lines.push("- 标题来源：由原始正文首行推断");
  if (payload.description) {
    lines.push("", platform === "xiaohongshu" ? "## 原始正文" : "## 原始描述", "", payload.description.trim());
  }
  if (payload.hashtags?.length) {
    lines.push("", "## 话题", "", payload.hashtags.join(" "));
  }
  if (imageLinks.length && payload.contentType !== "video") {
    lines.push("", "## 图片", "");
    imageLinks.forEach((link, index) => lines.push(`${index + 1}. ![第 ${index + 1} 张](${link})`));
  } else if (imageLinks.length) {
    lines.push("", "## 封面", "", `![视频封面](${imageLinks[0]})`);
  } else if (mediaSaveMode === "extract_only" && payload.images?.length && payload.contentType !== "video") {
    lines.push("", "## 图片", "");
    payload.images.forEach((_, index) => lines.push(`${index + 1}. （原图未保留）`));
  } else if (mediaSaveMode === "extract_only" && payload.images?.length) {
    lines.push("", "## 封面", "", "（原图未保留）");
  }
  if (mediaLink) lines.push("", "## 视频 / 音频", "", `[平台媒体](${mediaLink})`);
  else if (mediaSaveMode === "extract_only" && payload.mediaUrl) {
    lines.push("", "## 视频 / 音频", "", "（原始媒体未保留）");
  }
  return `${lines.join("\n")}\n`;
}

export function selectRelevantApiEvidence(platform, candidates, baseUrl, limit = 3) {
  if (!Array.isArray(candidates) || !Number.isSafeInteger(limit) || limit <= 0) return [];
  return candidates
    .filter((candidate) => candidate && typeof candidate === "object"
      && extractPlatformPayloadFromValue(platform, candidate.value, baseUrl))
    .slice(0, limit);
}

export function classifyRemoteImageKind(platform, hasPlayableMedia, localOcrAuthorized, mediaSaveMode = "preserve_original") {
  if (platform === "generic") return "image";
  if (hasPlayableMedia) return mediaSaveMode === "preserve_original" ? "image" : null;
  if (localOcrAuthorized) return "temporary_image";
  return mediaSaveMode === "preserve_original" ? "image" : null;
}

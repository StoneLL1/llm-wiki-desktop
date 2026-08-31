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

function scriptEntries(html) {
  const entries = [];
  const scripts = String(html).matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi);
  for (const match of scripts) {
    if (entries.length >= MAX_VALUES) break;
    const source = match[2].trim();
    if (!source || Buffer.byteLength(source, "utf8") > MAX_SCRIPT_BYTES) continue;
    let value;
    try { value = JSON.parse(source); } catch { value = balancedJson(source); }
    if (value && typeof value === "object") {
      entries.push({ attributes: match[1], source, value });
    }
  }
  return entries;
}

function scriptValues(html) {
  return scriptEntries(html).map((entry) => entry.value);
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

function firstDirectString(value, keys) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  for (const expected of keys) {
    const actual = Object.keys(value)
      .find((name) => name.toLowerCase() === expected.toLowerCase());
    const direct = actual ? stringValue(value[actual]) : null;
    if (direct) return direct;
  }
  return null;
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

function collectBilibiliSubtitleCandidates(value, baseUrl, directOnly = false) {
  const output = [];
  const seen = new Set();
  const collect = (candidate) => {
    if (output.length >= 20 || Array.isArray(candidate)) return;
    for (const [key, child] of Object.entries(candidate)) {
      if (!["subtitle_url", "subtitleurl", "caption_url", "captionurl"].includes(key.toLowerCase())) continue;
      const values = Array.isArray(child) ? child : [child];
      for (const raw of values) {
        if (typeof raw !== "string") continue;
        const normalized = normalizedAssetUrl(raw, baseUrl);
        if (!normalized || seen.has(normalized.identity)) continue;
        seen.add(normalized.identity);
        const numericFlag = ["ai_status", "ai_type", "type"]
          .map((name) => Object.keys(candidate).find((key) => key.toLowerCase() === name))
          .filter(Boolean)
          .map((name) => Number(candidate[name]))
          .find((flag) => Number.isFinite(flag) && flag !== 0);
        const numericTranslationFlag = ["ai_type", "type"]
          .map((name) => Object.keys(candidate).find((key) => key.toLowerCase() === name))
          .filter(Boolean)
          .map((name) => Number(candidate[name]))
          .find((flag) => Number.isFinite(flag) && flag >= 2);
        const label = firstDirectString(candidate, ["lan_doc", "label", "languageLabel"]);
        const explicitTranslation = numericTranslationFlag !== undefined ||
          Boolean(candidate.is_translate ?? candidate.isTranslate) ||
          isTranslationLabel(label);
        output.push({
          url: normalized.request,
          automatic: numericFlag !== undefined,
          kind: explicitTranslation ? "machine_translation" : "author_original",
          language: firstDirectString(candidate, ["lan", "language", "lang"]),
          label,
        });
        if (output.length >= 20) return;
      }
    }
  };
  const directSubtitleContainer = value && typeof value === "object"
    ? Object.entries(value)
      .find(([key]) => ["subtitle", "subtitles", "captions"].includes(key.toLowerCase()))?.[1]
    : null;
  if (directSubtitleContainer && typeof directSubtitleContainer === "object") {
    walk(directSubtitleContainer, collect);
  } else if (directOnly) {
    collect(value);
  } else {
    walk(value, collect);
  }
  const originalLanguage =
    output.find((subtitle) => subtitle.kind !== "machine_translation" && !subtitle.automatic)?.language ||
    output.find((subtitle) => subtitle.kind !== "machine_translation")?.language;
  for (const subtitle of output) {
    if (subtitle.kind === "machine_translation") continue;
    const originalTrack = !originalLanguage || !subtitle.language ||
      String(subtitle.language).toLowerCase() === String(originalLanguage).toLowerCase();
    subtitle.kind = originalTrack
      ? (subtitle.automatic ? "platform_auto_original" : "author_original")
      : (subtitle.automatic ? "machine_translation" : "author_other");
  }
  output.sort((left, right) => {
    return subtitleKindRank(left.kind) - subtitleKindRank(right.kind);
  });
  return output;
}

function isTranslationLabel(value) {
  const label = String(value || "").toLowerCase();
  return label.includes("translation") || label.includes("translated") ||
    label.includes("翻译") || label.includes("机翻") || label.includes("译制");
}

function subtitleKindRank(kind) {
  return {
    author_original: 0,
    platform_auto_original: 1,
    author_other: 2,
    machine_translation: 3,
  }[kind] ?? 3;
}

function firstStreamUrl(streams, baseUrl) {
  if (!Array.isArray(streams)) return null;
  for (const stream of streams) {
    if (!stream || typeof stream !== "object") continue;
    for (const key of ["url", "baseUrl", "base_url"]) {
      const normalized = normalizedAssetUrl(stringValue(stream[key]), baseUrl);
      if (normalized) return normalized.request;
    }
  }
  return null;
}

function bilibiliMediaUrls(value, baseUrl, directOnly = false) {
  const find = (candidate) => {
    if (Array.isArray(candidate)) return;
    const progressive = firstStreamUrl(candidate.durl, baseUrl);
    let audio = null;
    let video = null;
    if (candidate.dash && typeof candidate.dash === "object") {
      audio = firstStreamUrl(candidate.dash.audio, baseUrl);
      video = firstStreamUrl(candidate.dash.video, baseUrl);
    }
    return { progressive, audio, video };
  };
  let streams = find(value) || { progressive: null, audio: null, video: null };
  if (!directOnly && !streams.progressive && !streams.audio && !streams.video) {
    walk(value, (candidate) => {
      if (streams.progressive || streams.audio || streams.video) return;
      streams = find(candidate) || streams;
    });
  }
  return {
    mediaUrl: streams.progressive,
    asrMediaUrl: streams.audio || streams.progressive || streams.video,
  };
}

export function extractBilibiliPlayerEvidence(value, baseUrl, directOnly = false) {
  if (!value || typeof value !== "object") {
    return {
      mediaUrl: null,
      asrMediaUrl: null,
      subtitleCandidates: [],
      subtitles: [],
    };
  }
  const media = bilibiliMediaUrls(value, baseUrl, directOnly);
  const subtitleCandidates = collectBilibiliSubtitleCandidates(value, baseUrl, directOnly);
  return {
    ...media,
    subtitleCandidates,
    subtitles: subtitleCandidates.map((subtitle) => subtitle.url),
  };
}

export function extractBilibiliPlayerEvidenceFromHtml(html, baseUrl, aliases = {}) {
  const evidence = [];
  const identities = bilibiliTargetAliases(baseUrl, aliases);
  for (const entry of scriptEntries(html)) {
    const explicitPlayInfo = /(?:window\.)?__playinfo__\s*=/i.test(entry.source)
      || /\bid\s*=\s*["']__playinfo__["']/i.test(entry.attributes);
    if (explicitPlayInfo) {
      const playerValue = entry.value?.data && typeof entry.value.data === "object"
        ? entry.value.data
        : entry.value;
      evidence.push(extractBilibiliPlayerEvidence(playerValue, baseUrl, true));
      continue;
    }
    const scope = matchingBilibiliTarget(entry.value, identities);
    if (scope) evidence.push(extractBilibiliPlayerEvidence(scope, baseUrl));
  }
  return mergeBilibiliPlayerEvidence(
    {
      mediaUrl: null,
      asrMediaUrl: null,
      subtitleCandidates: [],
      subtitles: [],
    },
    evidence,
  );
}

export function extractRelevantBilibiliPlayerEvidence(candidate, baseUrl, aliases = {}) {
  if (!candidate || typeof candidate !== "object") {
    return extractBilibiliPlayerEvidence(null, baseUrl);
  }
  const identities = bilibiliTargetAliases(baseUrl, aliases);
  if (!identities.bvid && !identities.aid && !identities.cid) {
    return extractBilibiliPlayerEvidence(null, baseUrl);
  }
  let requestMatches = false;
  try {
    const requestUrl = new URL(candidate.url);
    requestMatches = [
      ["bvid", identities.bvid],
      ["aid", identities.aid],
      ["avid", identities.aid],
      ["cid", identities.cid],
    ].some(([key, expected]) => expected
      && requestUrl.searchParams.get(key)?.toLowerCase() === expected.toLowerCase());
  } catch {
    requestMatches = false;
  }
  const responseScope = matchingBilibiliTarget(candidate.value, identities);
  if (responseScope) {
    return extractBilibiliPlayerEvidence(responseScope, baseUrl);
  }
  if (!requestMatches) return extractBilibiliPlayerEvidence(null, baseUrl);
  const playerValue = candidate.value?.data && typeof candidate.value.data === "object"
    ? candidate.value.data
    : candidate.value;
  return extractBilibiliPlayerEvidence(playerValue, baseUrl, true);
}

export function mergeBilibiliPlayerEvidence(payload, evidenceValues) {
  const merged = {
    ...payload,
    subtitleCandidates: [...(payload?.subtitleCandidates || [])],
    subtitles: [...new Set(payload?.subtitles || [])],
  };
  for (const evidence of evidenceValues || []) {
    if (!evidence || typeof evidence !== "object") continue;
    if (!merged.mediaUrl && evidence.mediaUrl) merged.mediaUrl = evidence.mediaUrl;
    if (!merged.asrMediaUrl && evidence.asrMediaUrl) {
      merged.asrMediaUrl = evidence.asrMediaUrl;
    }
    for (const subtitle of evidence.subtitleCandidates || []) {
      if (!merged.subtitleCandidates.some((candidate) => candidate.url === subtitle.url)) {
        merged.subtitleCandidates.push(subtitle);
      }
    }
  }
  merged.subtitleCandidates.sort((left, right) => {
    return subtitleKindRank(left.kind) - subtitleKindRank(right.kind);
  });
  merged.subtitles = merged.subtitleCandidates.length
    ? merged.subtitleCandidates.map((subtitle) => subtitle.url)
    : merged.subtitles;
  if (merged.mediaUrl || merged.asrMediaUrl || merged.subtitles.length) {
    merged.contentType = "video";
  }
  return merged;
}

export function bilibiliMediaPolicy(payload, options) {
  const mediaUrl = payload?.mediaUrl || null;
  const asrMediaUrl = payload?.asrMediaUrl || mediaUrl;
  if (options.mediaSaveMode === "preserve_original" && !mediaUrl && asrMediaUrl) {
    return {
      errorCode: "IMPORT_WEB_MEDIA_UNAVAILABLE",
      asrMediaUrl: null,
    };
  }
  if ((mediaUrl || asrMediaUrl)
    && !options.hasSubtitle
    && !options.localAsrAuthorized
    && !options.allowMissingTranscript) {
    return {
      errorCode: "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
      asrMediaUrl: null,
    };
  }
  return {
    errorCode: null,
    asrMediaUrl: options.localAsrAuthorized && !options.allowMissingTranscript
      ? asrMediaUrl
      : null,
  };
}

export function isBilibiliPlayerApiUrl(value) {
  try {
    const candidate = new URL(value);
    return candidate.pathname.startsWith("/x/player/")
      && ["bvid", "aid", "avid", "cid"].some((key) => candidate.searchParams.has(key));
  } catch {
    return false;
  }
}

export function resolveSubtitleReference(raw, baseUrl) {
  const value = String(raw || "").trim();
  if (!/^(?:https?:)?\/\//i.test(value)
    && !/\.(?:json|vtt|srt|ass|ssa)(?:[?#]|$)/i.test(value)) {
    return null;
  }
  try {
    return new URL(value, baseUrl);
  } catch {
    return null;
  }
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

function normalizedXhsAssetUrl(raw, baseUrl) {
  const normalized = normalizedAssetUrl(raw, baseUrl);
  if (!normalized) return null;
  const request = new URL(normalized.request);
  const host = request.hostname.toLowerCase();
  const trustedCdn = host === "xhscdn.com"
    || host.endsWith(".xhscdn.com")
    || host === "xhscdn.net"
    || host.endsWith(".xhscdn.net");
  if (request.protocol === "http:" && trustedCdn && (!request.port || request.port === "80")) {
    request.protocol = "https:";
    request.port = "";
    return normalizedAssetUrl(request.href, baseUrl);
  }
  return normalized;
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
    return collectKeyUrls(value, ["urlDefault", "url_default", "images"], baseUrl, limit)
      .flatMap((raw) => {
        const normalized = normalizedXhsAssetUrl(raw, baseUrl);
        return normalized ? [normalized.request] : [];
      });
  }
  const output = [];
  const seen = new Set();
  for (const image of selected) {
    const normalized = normalizedXhsAssetUrl(preferredXhsImageUrl(image), baseUrl);
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

function collectXhsSubtitleCandidates(value, baseUrl, limit = 20) {
  const MAX_MEDIA_V2_BYTES = 2 * 1024 * 1024;
  const embedded = [];
  walk(value, (candidate) => {
    if (embedded.length >= 8 || Array.isArray(candidate)) return;
    for (const [key, child] of Object.entries(candidate)) {
      if (!["mediav2", "media_v2"].includes(key.toLowerCase())) continue;
      if (child && typeof child === "object") {
        embedded.push(child);
      } else if (typeof child === "string" && child.length <= MAX_MEDIA_V2_BYTES) {
        try {
          const parsed = JSON.parse(child);
          if (parsed && typeof parsed === "object") embedded.push(parsed);
        } catch {
          // A malformed optional mediaV2 string is ignored; the caller can
          // still use direct subtitle fields or local ASR.
        }
      }
    }
  });

  const output = [];
  const seen = new Set();
  const addEntry = (entry, label) => {
    if (output.length >= limit || !entry || typeof entry !== "object" || Array.isArray(entry)) return;
    const raw = firstDirectString(entry, ["url", "subtitleUrl", "subtitle_url"]);
    if (!raw) {
      for (const child of Object.values(entry)) {
        if (Array.isArray(child)) {
          for (const nested of child) addEntry(nested, label);
        } else if (child && typeof child === "object") {
          addEntry(child, label);
        }
      }
      return;
    }
    const normalized = normalizedXhsAssetUrl(raw, baseUrl);
    if (!normalized || seen.has(normalized.request)) return;
    seen.add(normalized.request);
    const language = firstDirectString(entry, ["language", "languageCode", "language_code", "lang"])
      || (String(label).toLowerCase() === "source" ? null : String(label));
    const automaticKey = Object.keys(entry)
      .find((key) => ["automatic", "isauto", "is_auto"].includes(key.toLowerCase()));
    const automatic = automaticKey ? Boolean(entry[automaticKey]) : true;
    const sourceTrack = String(label).toLowerCase() === "source";
    output.push({
      url: normalized.request,
      automatic,
      kind: sourceTrack
        ? (automatic ? "platform_auto_original" : "author_original")
        : (automatic ? "machine_translation" : "author_other"),
      language,
      label: String(label),
    });
  };

  for (const media of embedded) {
    walk(media, (candidate) => {
      if (output.length >= limit || Array.isArray(candidate)) return;
      const key = Object.keys(candidate).find((name) => name.toLowerCase() === "subtitles");
      const subtitles = key ? candidate[key] : null;
      if (!subtitles || typeof subtitles !== "object" || Array.isArray(subtitles)) return;
      for (const [label, entries] of Object.entries(subtitles)) {
        for (const entry of Array.isArray(entries) ? entries : [entries]) addEntry(entry, label);
      }
    });
  }

  for (const raw of collectKeyUrls(
    value,
    ["subtitles", "subtitle", "captions", "caption_url", "captionUrl", "subtitle_url", "subtitleUrl"],
    baseUrl,
    limit,
  )) {
    if (output.length >= limit) break;
    const normalized = normalizedXhsAssetUrl(raw, baseUrl);
    if (!normalized || seen.has(normalized.request)) continue;
    seen.add(normalized.request);
    output.push({
      url: normalized.request,
      automatic: false,
      kind: "author_original",
      language: null,
      label: null,
    });
  }

  output.sort((left, right) => {
    const kind = subtitleKindRank(left.kind) - subtitleKindRank(right.kind);
    if (kind !== 0) return kind;
    const priority = (subtitle) => {
      const label = String(subtitle.label || "").toLowerCase();
      const language = String(subtitle.language || "").toLowerCase();
      if (label === "source") return 0;
      if (label === "zh-cn" || language === "zh-cn") return 1;
      if (label.startsWith("zh") || language.startsWith("zh")) return 2;
      return 3;
    };
    return priority(left) - priority(right);
  });
  return output.slice(0, limit);
}

function normalizePublishedAt(value) {
  if (!value || !/^\d+$/.test(value)) return value;
  const numeric = Number(value);
  if (!Number.isSafeInteger(numeric)) return value;
  const millis = numeric >= 100_000_000_000 ? numeric : numeric * 1000;
  const date = new Date(millis);
  return Number.isNaN(date.valueOf()) ? value : date.toISOString();
}

function htmlAttribute(value, name) {
  const match = String(value).match(new RegExp(`\\b${name}\\s*=\\s*(["'])(.*?)\\1`, "iu"));
  return match?.[2] || null;
}

function decodeHtmlAttribute(value) {
  return String(value || "")
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'")
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">");
}

/**
 * Extract the public X post contract from server-rendered OpenGraph/Twitter
 * metadata. The signed runner intentionally does not call an undocumented API:
 * login walls and restricted posts remain typed recovery outcomes.
 */
export function extractXPayload(html, baseUrl) {
  let target;
  try { target = new URL(baseUrl); } catch { return null; }
  const identity = target.pathname.match(/^\/(?:i\/web\/status|[^/]+\/status)\/(\d+)/iu)?.[1];
  if (!identity) return null;
  const metadata = new Map();
  for (const tag of String(html || "").match(/<meta\b[^>]*>/giu) || []) {
    const key = htmlAttribute(tag, "property") || htmlAttribute(tag, "name");
    const content = htmlAttribute(tag, "content");
    if (key && content && !metadata.has(key.toLowerCase())) {
      metadata.set(key.toLowerCase(), decodeHtmlAttribute(content).trim());
    }
  }
  const description = metadata.get("og:description") || metadata.get("twitter:description") || "";
  const rawTitle = metadata.get("og:title") || metadata.get("twitter:title") || "";
  if (!description && !rawTitle) return null;
  const author = metadata.get("twitter:creator") || rawTitle.match(/^(.+?)\s+on\s+X:/iu)?.[1] || null;
  const title = inferTitle(description) || rawTitle || `X post ${identity}`;
  const images = [metadata.get("og:image"), metadata.get("twitter:image")]
    .filter(Boolean)
    .filter((value, index, values) => values.indexOf(value) === index);
  const mediaUrl = metadata.get("og:video:url") || metadata.get("og:video") || null;
  return {
    title,
    titleSource: description ? "inferred" : "platform",
    description,
    author,
    publishedAt: null,
    platformId: identity,
    targetAliases: null,
    contentType: mediaUrl ? "video" : images.length ? "image_post" : "article",
    images,
    mediaUrl,
    asrMediaUrl: mediaUrl,
    subtitleCandidates: [],
    subtitles: [],
    hashtags: hashtagsFrom(description),
  };
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

function bilibiliTargetAliases(baseUrl, aliases = {}) {
  const pathIdentity = targetIdentity("bilibili", baseUrl);
  const bvid = aliases.bvid
    || (pathIdentity?.toLowerCase().startsWith("bv") ? pathIdentity : null);
  const aid = aliases.aid
    || (pathIdentity && !pathIdentity.toLowerCase().startsWith("bv") ? pathIdentity : null);
  return {
    bvid: bvid ? String(bvid) : null,
    aid: aid ? String(aid) : null,
    cid: aliases.cid ? String(aliases.cid) : null,
  };
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

function matchingBilibiliTarget(value, aliases) {
  const identities = [
    [["bvid"], aliases.bvid],
    [["aid", "avid"], aliases.aid],
    [["cid"], aliases.cid],
  ];
  for (const [keys, expected] of identities) {
    if (!expected) continue;
    let found = null;
    walk(value, (candidate) => {
      if (found || Array.isArray(candidate)) return;
      const actual = Object.keys(candidate)
        .find((name) => keys.includes(name.toLowerCase()));
      const candidateId = actual ? stringValue(candidate[actual]) : null;
      if (candidateId?.toLowerCase() === String(expected).toLowerCase()) {
        found = candidate;
      }
    });
    if (found) return found;
  }
  return null;
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
  const bilibiliPlayerEvidence = platform === "bilibili"
    ? extractBilibiliPlayerEvidence(scope, baseUrl)
    : null;
  const targetAliases = platform === "bilibili"
    ? bilibiliTargetAliases(baseUrl, {
      bvid: firstDirectString(scope, ["bvid"]),
      aid: firstDirectString(scope, ["aid", "avid"]),
      cid: firstDirectString(scope, ["cid"]),
    })
    : null;
  const images = platform === "xiaohongshu"
    ? collectXhsImages(scope, baseUrl, 100)
    : collectKeyUrls(scope, imageKeys, baseUrl, 100);
  const rawMediaUrl = bilibiliPlayerEvidence?.mediaUrl
    || collectKeyUrls(scope, mediaKeys, baseUrl, 1)[0]
    || null;
  const mediaUrl = platform === "xiaohongshu" && rawMediaUrl
    ? normalizedXhsAssetUrl(rawMediaUrl, baseUrl)?.request || null
    : rawMediaUrl;
  const declaredVideo = (platform === "bilibili" && Boolean(expectedId))
    || (platform === "xiaohongshu"
      && String(firstString(scope, ["type", "noteType", "note_type"]) || "").toLowerCase() === "video");
  const hashtags = hashtagsFrom(description);
  if (platform === "xiaohongshu") {
    for (const tag of collectXhsTags(scope)) if (!hashtags.includes(tag)) hashtags.push(tag);
  }
  const xhsSubtitleCandidates = platform === "xiaohongshu"
    ? collectXhsSubtitleCandidates(scope, baseUrl)
    : [];
  return {
    title,
    titleSource: explicitTitle ? "platform" : "inferred",
    description,
    author,
    publishedAt,
    platformId: expectedId,
    targetAliases,
    contentType: declaredVideo || mediaUrl
      ? "video"
      : platform === "xiaohongshu" || images.length
        ? "image_post"
        : "article",
    images,
    mediaUrl,
    asrMediaUrl: bilibiliPlayerEvidence?.asrMediaUrl || mediaUrl,
    subtitleCandidates: bilibiliPlayerEvidence?.subtitleCandidates || xhsSubtitleCandidates,
    subtitles: bilibiliPlayerEvidence?.subtitles
      || (platform === "xiaohongshu"
        ? xhsSubtitleCandidates.map((subtitle) => subtitle.url)
        : collectKeyUrls(
          scope,
          ["subtitles", "subtitle", "captions", "caption_url", "captionUrl", "subtitle_url", "subtitleUrl"],
          baseUrl,
          20,
        )),
    hashtags,
  };
}

export function extractPlatformPayload(platform, html, baseUrl) {
  if (platform === "x") return extractXPayload(html, baseUrl);
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
  if (platform === "x" && /this post is from an account you don.t follow|these posts are protected|account suspended|post unavailable|page doesn.t exist/iu.test(text)) {
    return "IMPORT_WEB_CONTENT_RESTRICTED";
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

export function selectRelevantApiEvidence(platform, candidates, baseUrl, limit = 3, aliases = {}) {
  if (!Array.isArray(candidates) || !Number.isSafeInteger(limit) || limit <= 0) return [];
  return candidates
    .filter((candidate) => candidate && typeof candidate === "object"
      && (extractPlatformPayloadFromValue(platform, candidate.value, baseUrl)
        || (platform === "bilibili" && (() => {
          const evidence = extractRelevantBilibiliPlayerEvidence(candidate, baseUrl, aliases);
          return Boolean(evidence.mediaUrl || evidence.asrMediaUrl || evidence.subtitles.length);
        })())))
    .slice(0, limit);
}

export function classifyRemoteImageKind(platform, hasPlayableMedia, localOcrAuthorized, mediaSaveMode = "preserve_original") {
  if (platform === "generic") return "image";
  if (hasPlayableMedia) return mediaSaveMode === "preserve_original" ? "image" : null;
  if (localOcrAuthorized) return "temporary_image";
  return mediaSaveMode === "preserve_original" ? "image" : null;
}

export function platformHasVideoEvidence(payload, mediaUrl, asrMediaUrl) {
  return payload?.contentType === "video" || Boolean(mediaUrl || asrMediaUrl);
}

export function xiaohongshuImageOcrRequired(payload, localOcrAuthorized) {
  return payload?.contentType === "image_post" && !localOcrAuthorized;
}

export function xiaohongshuImageEvidenceReady(payload, localizedImageCount) {
  return payload?.contentType !== "image_post" || localizedImageCount > 0;
}

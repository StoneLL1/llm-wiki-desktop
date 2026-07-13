import { Buffer } from "node:buffer";

const MAX_TITLE = 500;
const MAX_DESCRIPTION = 100_000;
const MAX_CHAPTERS = 500;
const MAX_SUBTITLES = 16;

export const YTDLP_RELEASE = "2026.06.09";
export const YTDLP_WINDOWS_X64_SHA256 = "3a48cb955d55c8821b60ccbdbbc6f61bc958f2f3d3b7ad5eaf3d83a543293a27";

export const FIXED_ARGS = Object.freeze([
  "--dump-single-json",
  "--skip-download",
  "--simulate",
  "--write-subs",
  "--write-auto-subs",
  "--sub-langs", "all",
  "--sub-format", "vtt/best",
  "--no-playlist",
  "--no-config",
  "--ignore-config",
  "--no-plugin-dirs",
  "--no-warnings",
]);

function clipped(value, maximum) {
  if (typeof value !== "string") return "";
  const printable = [...scrubSecrets(value)]
    .filter((character) => {
      const code = character.charCodeAt(0);
      return code === 9 || code === 10 || code === 13 || (code >= 32 && code !== 127);
    })
    .join("");
  return printable.trim().slice(0, maximum);
}

function scrubSecrets(value) {
  const headerSafe = value.replace(/\b(?:authorization|proxy-authorization|cookie|set-cookie)\s*:\s*[^\r\n]+/gi, "[redacted credential]");
  return headerSafe.replace(/https:\/\/[^\s<>()]+/gi, (candidate) => {
    try {
      const url = new URL(candidate);
      let removed = false;
      for (const key of [...url.searchParams.keys()]) {
        if (/^(?:access_token|auth|authorization|cookie|credential|expires?|key|policy|signature|sig|token|x-amz-.+)$/i.test(key)) {
          url.searchParams.delete(key);
          removed = true;
        }
      }
      return removed ? url.href : candidate;
    } catch { return candidate; }
  });
}

export function validateBilibiliUrl(raw) {
  if (typeof raw !== "string" || raw.length === 0 || raw.length > 2048) {
    throw new Error("IMPORT_WEB_UNSUPPORTED_URL");
  }
  let url;
  try { url = new URL(raw); } catch { throw new Error("IMPORT_WEB_UNSUPPORTED_URL"); }
  if (url.protocol !== "https:" || url.username || url.password || url.hash || (url.port && url.port !== "443")) {
    throw new Error("IMPORT_WEB_UNSUPPORTED_URL");
  }
  const host = url.hostname.toLowerCase();
  if (host === "www.bilibili.com") {
    if (!/^\/video\/(?:BV[0-9A-Za-z]+|av[0-9]+)\/?$/.test(url.pathname)) {
      throw new Error("IMPORT_WEB_UNSUPPORTED_URL");
    }
    for (const [key, value] of url.searchParams) {
      if (key !== "p" || !/^[1-9][0-9]{0,3}$/.test(value)) throw new Error("IMPORT_WEB_UNSUPPORTED_URL");
    }
  } else if (host === "b23.tv") {
    if (!/^\/[0-9A-Za-z_-]{4,64}\/?$/.test(url.pathname) || url.search) {
      throw new Error("IMPORT_WEB_UNSUPPORTED_URL");
    }
  } else {
    throw new Error("IMPORT_WEB_UNSUPPORTED_URL");
  }
  return url.href;
}

function safeRemoteUrl(raw) {
  if (typeof raw !== "string" || raw.length > 4096) return null;
  try {
    const parsed = new URL(raw.startsWith("//") ? `https:${raw}` : raw);
    if (parsed.protocol !== "https:" || parsed.username || parsed.password) return null;
    return parsed.href;
  } catch { return null; }
}

function extractAssignedJson(html, marker) {
  const markerIndex = html.indexOf(marker);
  if (markerIndex < 0) return null;
  let index = markerIndex + marker.length;
  while (/\s/.test(html[index] || "")) index += 1;
  if (html[index] !== "{") return null;
  const start = index;
  let depth = 0;
  let quoted = false;
  let escaped = false;
  for (; index < html.length; index += 1) {
    const character = html[index];
    if (quoted) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') quoted = false;
      continue;
    }
    if (character === '"') quoted = true;
    else if (character === "{") depth += 1;
    else if (character === "}" && --depth === 0) {
      try { return JSON.parse(html.slice(start, index + 1)); } catch { return null; }
    }
  }
  return null;
}

export function parseBilibiliHtml(html) {
  if (typeof html !== "string" || Buffer.byteLength(html, "utf8") > 16 * 1024 * 1024) {
    throw new Error("IMPORT_WEB_RESPONSE_TOO_LARGE");
  }
  const initial = extractAssignedJson(html, "window.__INITIAL_STATE__=") || extractAssignedJson(html, "window.__INITIAL_STATE__ =");
  const play = extractAssignedJson(html, "window.__playinfo__=") || extractAssignedJson(html, "window.__playinfo__ =");
  const video = initial?.videoData || initial?.videoInfo;
  const playData = play?.data;
  if (!video || !playData) throw new Error("IMPORT_WEB_STRUCTURE_CHANGED");
  const requestedSubtitles = {};
  for (const subtitle of playData.subtitle?.subtitles || []) {
    const key = clipped(subtitle?.lan_doc || subtitle?.lan || `subtitle-${Object.keys(requestedSubtitles).length}`, 80);
    const url = safeRemoteUrl(subtitle?.subtitle_url || subtitle?.subtitleUrl);
    if (key && url) requestedSubtitles[key] = { ext: "json", url };
  }
  const formats = (playData.dash?.audio || []).slice(0, 32).map((audio) => ({
    url: safeRemoteUrl(audio?.baseUrl || audio?.base_url),
    vcodec: "none",
    acodec: clipped(audio?.codecs || audio?.mimeType || "aac", 80),
    abr: Number.isFinite(audio?.bandwidth) ? audio.bandwidth / 1000 : 0,
    filesize: null,
  })).filter((audio) => audio.url);
  const published = Number(video.pubdate);
  const uploadDate = Number.isFinite(published)
    ? new Date(published * 1000).toISOString().slice(0, 10).replace(/-/g, "")
    : null;
  return {
    __extractor: "bilibili-embedded",
    title: video.title,
    uploader: video.owner?.name,
    description: video.desc,
    duration: video.duration,
    upload_date: uploadDate,
    chapters: Array.isArray(video.pages) ? video.pages.map((page) => ({ title: page?.part })) : [],
    requested_subtitles: requestedSubtitles,
    formats,
  };
}

function subtitleCandidates(info) {
  const candidates = [];
  const requested = info?.requested_subtitles;
  if (requested && typeof requested === "object") {
    for (const [language, item] of Object.entries(requested)) candidates.push([language, item]);
  }
  if (candidates.length === 0) {
    for (const groupName of ["subtitles", "automatic_captions"]) {
      const group = info?.[groupName];
      if (!group || typeof group !== "object") continue;
      for (const [language, formats] of Object.entries(group)) {
        if (!Array.isArray(formats)) continue;
        const preferred = formats.find((item) => item?.ext === "vtt") || formats[0];
        candidates.push([language, preferred]);
      }
    }
  }
  return candidates;
}

export function selectTemporaryAudio(info) {
  if (!Array.isArray(info?.formats)) return null;
  const candidates = info.formats
    .filter((format) => format && format.vcodec === "none" && format.acodec && format.acodec !== "none")
    .map((format) => ({
      url: safeRemoteUrl(format.url),
      bytes: Number.isFinite(format.filesize) ? format.filesize : Number.isFinite(format.filesize_approx) ? format.filesize_approx : null,
      bitrate: Number.isFinite(format.abr) ? format.abr : 0,
    }))
    .filter((format) => format.url && (format.bytes === null || format.bytes <= 256 * 1024 * 1024))
    .sort((left, right) => right.bitrate - left.bitrate);
  return candidates[0]?.url || null;
}

export function parseYtDlpMetadata(info, requestedUrl) {
  if (!info || typeof info !== "object" || Array.isArray(info)) throw new Error("IMPORT_WEB_STRUCTURE_CHANGED");
  const publicUrl = validateBilibiliUrl(requestedUrl);
  const title = clipped(info.title, MAX_TITLE);
  if (!title) throw new Error("IMPORT_WEB_STRUCTURE_CHANGED");

  const chapters = Array.isArray(info.chapters) ? info.chapters.slice(0, MAX_CHAPTERS).map((chapter) => ({
    title: clipped(chapter?.title, 500),
    startTime: Number.isFinite(chapter?.start_time) && chapter.start_time >= 0 ? chapter.start_time : null,
    endTime: Number.isFinite(chapter?.end_time) && chapter.end_time >= 0 ? chapter.end_time : null,
  })).filter((chapter) => chapter.title) : [];

  const remoteAssets = [];
  const subtitleLinks = [];
  const seen = new Set();
  for (const [rawLanguage, item] of subtitleCandidates(info)) {
    if (remoteAssets.length >= MAX_SUBTITLES) break;
    const url = safeRemoteUrl(item?.url);
    if (!url || seen.has(url)) continue;
    seen.add(url);
    const language = clipped(rawLanguage, 80) || "unknown";
    const placeholder = `subtitle-${remoteAssets.length}`;
    remoteAssets.push({ placeholder, url, kind: "subtitle" });
    subtitleLinks.push(`- [${language}](asset://${placeholder})`);
  }

  const safe = {
    title,
    uploader: clipped(info.uploader || info.channel, 500),
    description: clipped(info.description, MAX_DESCRIPTION),
    publicUrl,
    durationSeconds: Number.isFinite(info.duration) && info.duration >= 0 ? Math.min(info.duration, 604_800) : null,
    uploadDate: typeof info.upload_date === "string" && /^\d{8}$/.test(info.upload_date) ? info.upload_date : null,
    chapters,
    subtitleCount: remoteAssets.length,
    extractor: info.__extractor === "bilibili-embedded" ? "bilibili-embedded" : "yt-dlp",
    extractorVersion: info.__extractor === "bilibili-embedded" ? "1" : YTDLP_RELEASE,
  };

  const lines = [`# ${title}`, "", `Source: ${publicUrl}`];
  if (safe.uploader) lines.push("", `Uploader: ${safe.uploader}`);
  if (safe.description) lines.push("", "## Description", "", safe.description);
  if (chapters.length) {
    lines.push("", "## Chapters", "");
    for (const chapter of chapters) lines.push(`- ${chapter.startTime ?? 0}s — ${chapter.title}`);
  }
  if (subtitleLinks.length) lines.push("", "## Subtitles", "", ...subtitleLinks);

  return { safe, markdown: `${lines.join("\n")}\n`, remoteAssets };
}
import { URL } from "node:url";

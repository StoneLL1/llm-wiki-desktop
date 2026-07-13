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
    const parsed = new URL(raw);
    if (parsed.protocol !== "https:" || parsed.username || parsed.password) return null;
    return parsed.href;
  } catch { return null; }
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
    extractor: "yt-dlp",
    extractorVersion: YTDLP_RELEASE,
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

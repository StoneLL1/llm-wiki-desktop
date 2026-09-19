import path from "node:path";
import process from "node:process";

export function restrictedEnvironment(packRoot, platform = process.platform, source = process.env) {
  const result = { NO_COLOR: "1" };
  for (const name of ["SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"]) {
    if (typeof source[name] === "string") result[name] = source[name];
  }
  const ffmpegLib = path.join(packRoot, "runtime", "ffmpeg", "lib");
  if (platform === "linux") result.LD_LIBRARY_PATH = ffmpegLib;
  if (platform === "darwin") result.DYLD_LIBRARY_PATH = ffmpegLib;
  if (platform === "win32") {
    const ffmpegBin = path.join(packRoot, "runtime", "ffmpeg", "bin");
    result.PATH = `${ffmpegBin}${path.delimiter}${source.PATH ?? ""}`;
  }
  return result;
}

const TEXT_SUBTITLE_CODECS = new Set(["subrip", "srt", "ass", "ssa", "mov_text", "webvtt", "text", "ttml", "sami", "microdvd", "subviewer"]);

// FFmpeg is already shipped by this decoder capability; inspecting its stream
// inventory avoids requiring ffprobe or an ASR model just to extract subtitles.
export function textSubtitleTracks(inventory, language = "auto") {
  const languageKey = (value) => {
    try { return new Intl.Locale(value).language; } catch { return "unknown"; }
  };
  const streams = inventory.split(/(?=^\s*Stream #0:\d+)/mu).flatMap((block) => {
    const match = block.match(/^\s*Stream #0:(\d+)(?:\[[^\]]*\])?(?:\(([^)]+)\))?: (Audio|Subtitle): ([\w]+)([^\n]*)/mu);
    if (!match) return [];
    const declaredKind = block.match(/^\s*(?:kind|subtitle_kind)\s*:\s*(\S+)\s*$/imu)?.[1]?.toLowerCase();
    return [{ index: Number(match[1]), language: match[2] || "unknown", type: match[3], codec: match[4],
      default: match[5].includes("(default)"), original: match[5].includes("(original)"),
      comment: /\((?:comment|commentary)\)/u.test(match[5]), declaredKind }];
  });
  const audio = streams.filter((track) => track.type === "Audio" && !track.comment && !["unknown", "und"].includes(track.language))
    .sort((a, b) => Number(b.original) - Number(a.original) || Number(b.default) - Number(a.default) || a.index - b.index)[0];
  const preferred = languageKey(language === "auto" ? audio?.language : language);
  return streams.filter((track) => track.type === "Subtitle" && TEXT_SUBTITLE_CODECS.has(track.codec) &&
      !track.comment && !["machine_translation", "commentary"].includes(track.declaredKind))
    .sort((a, b) => (["unknown", "und"].includes(preferred) ? 0 :
      Number(languageKey(b.language) === preferred) - Number(languageKey(a.language) === preferred)) ||
      Number(b.original) - Number(a.original) || Number(b.default) - Number(a.default) || a.index - b.index)
    .map(({ index, language, default: isDefault }) => ({ index, language, default: isDefault }));
}

export function renderSrt(value) {
  const segments = [];
  for (const block of value.replace(/^\uFEFF/u, "").replace(/\r\n?/gu, "\n").split(/\n{2,}/u)) {
    const lines = block.split("\n");
    const timing = lines.findIndex((line) => /^\s*\d+:\d{2}:\d{2}[,.]\d{3}\s+-->/u.test(line));
    if (timing < 0) continue;
    const match = lines[timing].match(/(\d+):(\d{2}):(\d{2})[,.](\d{3})/u);
    const text = lines.slice(timing + 1).join("\n").replace(/<\/?(?:b|i|u|font)(?:\s+[^>]*)?>/giu, "").trim();
    if (!text) continue;
    const startMs = ((Number(match[1]) * 60 + Number(match[2])) * 60 + Number(match[3])) * 1000 + Number(match[4]);
    segments.push({ startMs, text });
  }
  if (!segments.length) throw new Error("IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE");
  const markdown = "# Transcript\n\n" + segments.map(({ startMs, text }) => {
    const seconds = Math.floor(startMs / 1000);
    const stamp = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60].map((v) => String(v).padStart(2, "0")).join(":");
    return `## [${stamp}]\n\n${text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;")}\n`;
  }).join("\n");
  return { markdown, segments };
}

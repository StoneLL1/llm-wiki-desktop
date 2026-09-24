import { Buffer } from "node:buffer";
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

// Same bounded frame selection contract as the ASR resources.
export function buildVideoTextProbeArguments(mediaPath, outputPattern, intervalSeconds = 10) {
  if (!Number.isFinite(intervalSeconds) || intervalSeconds <= 0) throw new Error("IMPORT_ASR_INVALID_REQUEST");
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-i", (mediaPath),
    "-an", "-sn", "-dn",
    "-vf", `fps=1/${intervalSeconds}:start_time=0:round=down,scale=480:-2:flags=area,format=gray`,
    "-frames:v", "180", (outputPattern),
  ];
}

export function buildVideoOcrFrameArguments(mediaPath, seconds, outputPath) {
  if (!Number.isFinite(seconds) || seconds < 0) throw new Error("IMPORT_ASR_INVALID_REQUEST");
  return [
    "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
    "-protocol_whitelist", "file,pipe",
    "-ss", seconds.toFixed(3), "-i", (mediaPath),
    "-an", "-sn", "-dn", "-frames:v", "1",
    "-vf", "scale='min(1920,iw)':-2:flags=lanczos",
    (outputPath),
  ];
}

function parsePortableGraymap(value) {
  if (!Buffer.isBuffer(value) || value.length < 16) throw new Error("IMPORT_ASR_VIDEO_PROBE_FAILED");
  let offset = 0;
  const tokens = [];
  while (tokens.length < 4 && offset < value.length) {
    while (offset < value.length && /\s/u.test(String.fromCharCode(value[offset]))) offset += 1;
    if (value[offset] === 0x23) {
      while (offset < value.length && value[offset] !== 0x0a) offset += 1;
      continue;
    }
    const start = offset;
    while (offset < value.length && !/\s/u.test(String.fromCharCode(value[offset]))) offset += 1;
    tokens.push(value.subarray(start, offset).toString("ascii"));
  }
  // The P5 raster starts after exactly one separator; pixel bytes may be whitespace.
  if (value[offset] === 13 && value[offset + 1] === 10) offset += 2;
  else if (/\s/u.test(String.fromCharCode(value[offset]))) offset += 1;
  const [magic, widthValue, heightValue, maximumValue] = tokens;
  const width = Number(widthValue);
  const height = Number(heightValue);
  if (magic !== "P5" || !Number.isSafeInteger(width) || !Number.isSafeInteger(height) ||
      width <= 0 || height <= 0 || maximumValue !== "255" || value.length - offset !== width * height) {
    throw new Error("IMPORT_ASR_VIDEO_PROBE_FAILED");
  }
  return { width, height, pixels: value.subarray(offset) };
}

export function selectStableTextFrameIndexes(frames) {
  if (!Array.isArray(frames) || frames.length < 2 || frames.length > 180) return [];
  const parsed = frames.map(parsePortableGraymap);
  const selected = [];
  for (let index = 1; index < parsed.length; index += 1) {
    const current = parsed[index];
    const previous = parsed[index - 1];
    if (current.width !== previous.width || current.height !== previous.height) continue;
    let edges = 0;
    let difference = 0;
    let samples = 0;
    for (let y = 1; y < current.height; y += 2) {
      for (let x = 1; x < current.width; x += 2) {
        const position = y * current.width + x;
        const pixel = current.pixels[position];
        if (Math.abs(pixel - current.pixels[position - 1]) > 36 ||
            Math.abs(pixel - current.pixels[position - current.width]) > 36) edges += 1;
        difference += Math.abs(pixel - previous.pixels[position]);
        samples += 1;
      }
    }
    const edgeDensity = samples === 0 ? 0 : edges / samples;
    const meanDifference = samples === 0 ? 255 : difference / samples;
    if (edgeDensity >= 0.035 && edgeDensity <= 0.45 && meanDifference <= 12) {
      // Deduplicate consecutive scenes before applying the OCR quota.
      const duplicate = selected.slice(-1).some((otherIndex) => {
        const other = parsed[otherIndex];
        if (other.width !== current.width || other.height !== current.height) return false;
        let delta = 0;
        let count = 0;
        for (let position = 0; position < current.pixels.length; position += 3) {
          delta += Math.abs(current.pixels[position] - other.pixels[position]);
          count += 1;
        }
        return delta / count <= 2;
      });
      if (!duplicate) selected.push(index);
    }
  }
  return selected;
}

import fs from "node:fs/promises";
import path from "node:path";
import { buildVideoTextProbeArguments, buildVideoOcrFrameArguments, selectStableTextFrameIndexes } from "./core.mjs";

// This file is bundled in each independent decoder/ASR resource. It emits
// evidence only; the host alone decides whether an OCR continuation is allowed.
export async function prepareVideoFrames(run, mediaPath, stagingRoot, temporaryRoot, authorized) {
  let inventory;
  try {
    inventory = (await run(["-nostdin", "-hide_banner", "-protocol_whitelist", "file,pipe", "-i", mediaPath])).stderr;
  } catch (error) {
    const detail = error.cause || error;
    if (detail.code !== 1 || !detail.stderr?.includes("At least one output file must be specified")) throw error;
    inventory = detail.stderr;
  }
  const duration = inventory?.match(/Duration: (\d+):(\d{2}):(\d{2}(?:\.\d+)?)/u);
  if (!duration) throw new Error("IMPORT_ASR_VIDEO_PROBE_FAILED");
  const durationMs = Math.round((Number(duration[1]) * 3600 + Number(duration[2]) * 60 + Number(duration[3])) * 1000);
  if (durationMs <= 0) throw new Error("IMPORT_ASR_VIDEO_PROBE_FAILED");
  // Bounded lightweight sampling covers the whole media, including long videos.
  const interval = Math.max(0.1, durationMs / 1000 / 180);
  const probeRoot = path.join(temporaryRoot, "video-text-probe");
  await fs.mkdir(probeRoot, { recursive: true });
  await run(buildVideoTextProbeArguments(mediaPath, path.join(probeRoot, "probe-%04d.pgm"), interval));
  const names = (await fs.readdir(probeRoot)).filter((name) => /^probe-\d{4}\.pgm$/u.test(name)).sort();
  const selected = selectStableTextFrameIndexes(await Promise.all(names.map((name) => fs.readFile(path.join(probeRoot, name)))));
  if (!selected.length) throw new Error("IMPORT_ASR_NO_SPEECH");
  if (!authorized) throw new Error("IMPORT_VIDEO_FRAME_OCR_REQUIRED");
  // Apply the budget across distinct scenes, keeping the final scene too.
  const indexes = selected.length <= 6 ? selected : Array.from({ length: 6 }, (_, i) => selected[Math.round(i * (selected.length - 1) / 5)]);
  const ocrRoot = await fs.mkdtemp(path.join(stagingRoot, ".ocr-input-"));
  const frames = [];
  try {
    for (const index of indexes) {
      const timestampMs = Math.round(index * interval * 1000);
      const output = path.join(ocrRoot, `frame-at-${timestampMs}.png`);
      await run(buildVideoOcrFrameArguments(mediaPath, timestampMs / 1000, output));
      frames.push({ path: path.relative(stagingRoot, output).split(path.sep).join("/"), timestampMs });
    }
  } catch (error) {
    await fs.rm(ocrRoot, { recursive: true, force: true });
    throw error;
  }
  return { frames, durationMs, sampledFrameCount: names.length };
}

import assert from "node:assert/strict";
import test from "node:test";
import process from "node:process";
import { Buffer } from "node:buffer";
import { textSubtitleTracks, renderSrt, selectStableTextFrameIndexes } from "./core.mjs";

test("decoder skips bitmap tracks and retains text track language and priority", () => {
  const inventory = "  Stream #0:1: Subtitle: hdmv_pgs_subtitle (default)\n  Stream #0:2(eng): Subtitle: subrip\n  Stream #0:3(zho): Subtitle: ass (default)\n";
  assert.deepEqual(textSubtitleTracks(inventory).map((track) => track.index), [3, 2]);
  assert.deepEqual(textSubtitleTracks(inventory, "eng").map((track) => track.index), [2, 3]);
});

test("auto prefers the main audio language and excludes only explicit translation or commentary evidence", () => {
  const inventory = "  Stream #0:0(zho): Audio: aac (default)\n  Stream #0:1(eng): Subtitle: subrip (default)\n  Stream #0:2(chi): Subtitle: subrip\n  Stream #0:3(zho): Subtitle: subrip (default)\n    Metadata:\n      KIND : machine_translation\n  Stream #0:4(zho): Subtitle: subrip (comment)\n";
  assert.deepEqual(textSubtitleTracks(inventory).map((track) => track.index), [2, 1]);
  assert.deepEqual(textSubtitleTracks(inventory, "en").map((track) => track.index), [1, 2]);
  assert.deepEqual(textSubtitleTracks("  Stream #0:1: Subtitle: subrip\n  Stream #0:2(eng): Subtitle: subrip (default)\n").map((track) => track.index), [2, 1]);
  assert.deepEqual(textSubtitleTracks("  Stream #0:1: Subtitle: subrip\n    Metadata:\n      title : An article discussing machine translation\n").map((track) => track.index), [1]);
});

test("embedded subtitle keeps numeric/bracket content and accurate time anchors", () => {
  const result = renderSrt("1\n00:00:00,000 --> 00:00:01,000\n2026\n\n2\n02:01:00,125 --> 02:01:01,000\n42\n[中文正文]\n");
  assert.match(result.markdown, /2026/u);
  assert.match(result.markdown, /42\n\[中文正文\]/u);
  assert.match(result.markdown, /\[02:01:00\]/u);
  assert.equal(result.segments[1].startMs, 7260125);
  assert.throws(() => renderSrt("1\n00:00:00,000 --> 00:00:01,000\n"), /IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE/u);
});

test("real decoder RPC extracts MP4 and MKV text without models, including a preceding bitmap track", { skip: !process.env.LLM_WIKI_TEST_FFMPEG }, async (t) => {
  const fs = await import("node:fs/promises");
  const path = await import("node:path");
  const os = await import("node:os");
  const { execFileSync } = await import("node:child_process");
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "媒体-decoder-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const runner = path.join(root, "pack", "runner");
  const bin = path.join(root, "pack", "runtime", "ffmpeg", "bin");
  const staging = path.join(root, "staging");
  await fs.mkdir(bin, { recursive: true }); await fs.mkdir(staging);
  await fs.cp(import.meta.dirname, runner, { recursive: true });
  await fs.symlink(process.env.LLM_WIKI_TEST_FFMPEG, path.join(bin, process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg"));
  const sourceSubtitle = "1\n00:00:00,000 --> 00:00:01,000\n2026\n\n2\n00:00:01,000 --> 00:00:02,000\n42 [中文正文]\n";
  await fs.writeFile(path.join(staging, "input.srt"), sourceSubtitle);
  for (const extension of ["mp4", "mkv"]) {
    const source = path.join(staging, `原件.${extension}`);
    execFileSync(process.env.LLM_WIKI_TEST_FFMPEG, ["-v", "error", "-f", "lavfi", "-i", "color=size=16x16:rate=1:duration=2", "-i", path.join(staging, "input.srt"), "-i", path.join(staging, "input.srt"), "-map", "0", "-map", "1", "-map", "2", "-c:v", "mpeg4", "-c:s", extension === "mp4" ? "mov_text" : "srt", "-metadata:s:s:1", "language=zho", source]);
    if (extension === "mkv") {
      // A valid Matroska track inventory with bitmap first; its packets need
      // not decode because selecting the subsequent text track must skip it.
      const bytes = await fs.readFile(source);
      const offset = bytes.indexOf(Buffer.from("S_TEXT/UTF8"));
      assert.ok(offset > 0);
      Buffer.from("S_HDMV/PGS\0").copy(bytes, offset);
      await fs.writeFile(source, bytes);
    }
    const original = await fs.readFile(source);
    const response = JSON.parse(execFileSync(process.execPath, [path.join(runner, "index.mjs")], { input: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "import.extract", params: { operation: "extract", projectRoot: root, stagingRoot: "staging", chainedInput: `原件.${extension}`, input: { kind: "file", locator: source }, asrProbeOnly: true } }), encoding: "utf8" }));
    assert.equal(response.error, null, JSON.stringify(response.error));
    const result = response.result;
    const markdown = await fs.readFile(path.join(staging, result.markdownPath), "utf8");
    assert.match(markdown, /2026/u); assert.match(markdown, /42 \[中文正文\]/u);
    assert.deepEqual(await fs.readFile(path.join(staging, result.sourceSnapshotPath)), original);
    const metadata = JSON.parse(await fs.readFile(path.join(staging, result.metadataPath), "utf8"));
    assert.equal(metadata.provenance, "local-embedded-subtitle");
    assert.ok(metadata.originalSubtitle.includes("中文正文"));
    if (extension === "mkv") assert.equal(metadata.trackIndex, 2);
    assert.ok((await fs.readFile(path.join(staging, result.assetPaths[0]), "utf8")).includes("中文正文"));
  }
  const english = path.join(staging, "english.srt");
  const translated = path.join(staging, "translated.srt");
  await fs.writeFile(english, "1\n00:00:00,000 --> 00:00:02,000\nOther language caption\n");
  await fs.writeFile(translated, "1\n00:00:00,000 --> 00:00:02,000\nMachine translation should not be selected\n");
  const multilingual = path.join(staging, "multilingual.mkv");
  execFileSync(process.env.LLM_WIKI_TEST_FFMPEG, ["-v", "error", "-f", "lavfi", "-i", "color=size=16x16:rate=1:duration=2", "-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-i", english, "-i", path.join(staging, "input.srt"), "-i", translated, "-map", "0", "-map", "1", "-map", "2", "-map", "3", "-map", "4", "-t", "2", "-c:v", "mpeg4", "-c:a", "aac", "-c:s", "srt", "-metadata:s:a:0", "language=zho", "-metadata:s:s:0", "language=eng", "-metadata:s:s:1", "language=zho", "-metadata:s:s:2", "language=zho", "-metadata:s:s:2", "kind=machine_translation", "-disposition:s:0", "default", "-disposition:s:1", "0", "-disposition:s:2", "default", multilingual]);
  const response = JSON.parse(execFileSync(process.execPath, [path.join(runner, "index.mjs")], { input: JSON.stringify({ jsonrpc: "2.0", id: 2, method: "import.extract", params: { operation: "extract", projectRoot: root, stagingRoot: "staging", chainedInput: "multilingual.mkv", input: { kind: "file", locator: multilingual }, asrProbeOnly: true } }), encoding: "utf8" }));
  assert.equal(response.error, null, JSON.stringify(response.error));
  const metadata = JSON.parse(await fs.readFile(path.join(staging, response.result.metadataPath), "utf8"));
  assert.equal(metadata.trackIndex, 3);
  assert.equal(metadata.language, "zho");
  assert.ok(metadata.originalSubtitle.includes("中文正文"));

});


test("binary PGM whitespace pixels and repeated opening scenes preserve later frames", () => {
  const frame = (first, shift = 0) => {
    const pixels = Buffer.alloc(32 * 32, 240);
    for (let y = 0; y < 32; y += 1) pixels[y * 32 + 4 + shift] = 20;
    pixels[0] = first;
    return Buffer.concat([Buffer.from("P5\n32 32\n255\n"), pixels]);
  };
  for (const first of [9, 10, 13, 32]) {
    const opening = frame(first);
    const late = frame(first, 10);
    assert.deepEqual(selectStableTextFrameIndexes([...Array(24).fill(opening), late, late]), [1, 24]);
  }
});

test("real video RPC requires OCR consent and deduplicates the opening across the full duration", { skip: !process.env.LLM_WIKI_TEST_FFMPEG }, async (t) => {
  const fs = await import("node:fs/promises");
  const path = await import("node:path");
  const os = await import("node:os");
  const { execFileSync } = await import("node:child_process");
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "video-frames-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const runner = path.join(root, "pack", "runner");
  const bin = path.join(root, "pack", "runtime", "ffmpeg", "bin");
  const staging = path.join(root, "staging");
  await fs.mkdir(bin, { recursive: true }); await fs.mkdir(staging);
  await fs.cp(import.meta.dirname, runner, { recursive: true });
  await fs.symlink(process.env.LLM_WIKI_TEST_FFMPEG, path.join(bin, process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg"));
  for (let scene = 0; scene < 2; scene += 1) {
    const pixels = Buffer.alloc(480 * 240, 240);
    for (let y = 20; y < 220; y += 1) for (let x = 20 + scene * 160; x < 160 + scene * 160; x += 8) pixels[y * 480 + x] = 20;
    await fs.writeFile(path.join(staging, `scene-${scene}.pgm`), Buffer.concat([Buffer.from("P5\n480 240\n255\n"), pixels]));
  }
  const source = path.join(staging, "movie.mp4");
  execFileSync(process.env.LLM_WIKI_TEST_FFMPEG, ["-v", "error", "-loop", "1", "-framerate", "1", "-t", "1900", "-i", path.join(staging, "scene-0.pgm"), "-loop", "1", "-framerate", "1", "-t", "100", "-i", path.join(staging, "scene-1.pgm"), "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0", "-r", "1", "-c:v", "mpeg4", source]);
  const original = await fs.readFile(source);
  for (const authorized of [false, true]) {
    const rpc = JSON.parse(execFileSync(process.execPath, [path.join(runner, "index.mjs")], { input: JSON.stringify({ jsonrpc: "2.0", id: "frames", method: "import.extract", params: { operation: "extract", projectRoot: root, stagingRoot: "staging", chainedInput: "movie.mp4", input: { kind: "file", locator: source }, localOcrAuthorized: authorized } }), encoding: "utf8" }));
    if (!authorized) { assert.equal(rpc.error?.data?.code, "IMPORT_VIDEO_FRAME_OCR_REQUIRED", JSON.stringify(rpc)); continue; }
    assert.equal(rpc.error, null, JSON.stringify(rpc));
    assert.equal(rpc.result.continuation, null);
    const evidence = rpc.result.videoFrames;
    assert.equal(evidence.frames.length, 2);
    assert.ok(evidence.frames[1].timestampMs >= 1900000, JSON.stringify(evidence));
    assert.ok(evidence.durationMs >= 2000000);
    for (const frame of evidence.frames) {
      assert.match(frame.path, /^\.ocr-input-[^/]+\/frame-at-\d+\.png$/u);
      assert.deepEqual((await fs.readFile(path.join(staging, frame.path))).subarray(0, 8), Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
    }
    assert.deepEqual(await fs.readFile(path.join(staging, rpc.result.sourceSnapshotPath)), original);
  }
});

test("real FFmpeg PGM preserves whitespace-valued first raster bytes", { skip: !process.env.LLM_WIKI_TEST_FFMPEG }, async () => {
  const { execFileSync } = await import("node:child_process");
  for (const first of [9, 10, 13, 32]) {
    const raster = Buffer.alloc(16 * 16, first);
    const pgm = execFileSync(process.env.LLM_WIKI_TEST_FFMPEG, ["-v", "error", "-f", "rawvideo", "-pixel_format", "gray", "-video_size", "16x16", "-i", "pipe:0", "-frames:v", "1", "-f", "image2pipe", "-c:v", "pgm", "pipe:1"], { input: raster });
    assert.equal(pgm.at(-256), first);
    assert.deepEqual(selectStableTextFrameIndexes([pgm, pgm]), []);
  }
});

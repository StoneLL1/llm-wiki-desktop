import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { URL } from "node:url";
import { FIXED_ARGS, parseBilibiliHtml, parseYtDlpMetadata, selectTemporaryAudio, validateBilibiliUrl } from "./core.mjs";

test("accepts only exact supported Bilibili video URLs", () => {
  assert.equal(validateBilibiliUrl("https://www.bilibili.com/video/BV1Ab411c7de?p=2"), "https://www.bilibili.com/video/BV1Ab411c7de?p=2");
  assert.equal(validateBilibiliUrl("https://b23.tv/AbCd12"), "https://b23.tv/AbCd12");
  for (const invalid of [
    "http://www.bilibili.com/video/BV1Ab411c7de",
    "https://evil.example/video/BV1Ab411c7de",
    "https://bilibili.com.evil.example/video/BV1Ab411c7de",
    "https://www.bilibili.com/read/cv1",
    "https://user:secret@www.bilibili.com/video/BV1Ab411c7de",
    "https://www.bilibili.com/video/BV1Ab411c7de?token=secret",
  ]) assert.throws(() => validateBilibiliUrl(invalid), /IMPORT_WEB_UNSUPPORTED_URL/);
});

test("fixed arguments cannot execute, load credentials, plugins, download media, or post-process", () => {
  const joined = FIXED_ARGS.join(" ");
  for (const forbidden of ["--exec", "--cookies", "--netrc", "--plugin-dirs default", "--external-downloader", "--output", "--postprocessor-args"]) {
    assert.equal(joined.includes(forbidden), false, forbidden);
  }
  assert.ok(FIXED_ARGS.includes("--no-plugin-dirs"));
  assert.ok(FIXED_ARGS.includes("--skip-download"));
  assert.ok(FIXED_ARGS.includes("--no-config"));
});

test("parsing returns bounded public metadata and separate subtitle notifications", () => {
  const parsed = parseYtDlpMetadata({
    title: ` Example\0 title ${"x".repeat(800)}`,
    uploader: "Uploader",
    description: "description https://example.test/page?signature=secret&article=1\nAuthorization: Bearer hidden",
    duration: 42,
    upload_date: "20260713",
    formats: [{ url: "https://signed.example/video?secret=must-not-persist" }],
    requested_subtitles: {
      "zh-CN": { ext: "vtt", url: "https://subtitle.example/a.vtt?signature=stdout-only" },
      broken: { url: "file:///etc/passwd" },
    },
    chapters: [{ title: "Intro", start_time: 0, end_time: 5 }],
  }, "https://www.bilibili.com/video/BV1Ab411c7de");
  assert.equal(parsed.safe.title.length, 500);
  assert.equal(parsed.safe.subtitleCount, 1);
  assert.equal(parsed.remoteAssets.length, 1);
  assert.equal(parsed.remoteAssets[0].kind, "subtitle");
  assert.match(parsed.markdown, /asset:\/\/subtitle-0/);
  assert.doesNotMatch(JSON.stringify(parsed.safe), /signature|signed\.example|formats/);
  assert.doesNotMatch(JSON.stringify(parsed.safe), /secret|Bearer hidden/);
  assert.match(parsed.safe.description, /article=1/);
});

test("parsing caps subtitle and chapter fan-out", () => {
  const subtitles = Object.fromEntries(Array.from({ length: 40 }, (_, index) => [`lang-${index}`, { url: `https://subtitle.example/${index}.vtt` }]));
  const chapters = Array.from({ length: 700 }, (_, index) => ({ title: `Chapter ${index}`, start_time: index }));
  const parsed = parseYtDlpMetadata({ title: "Video", requested_subtitles: subtitles, chapters }, "https://b23.tv/AbCd12");
  assert.equal(parsed.remoteAssets.length, 16);
  assert.equal(parsed.safe.chapters.length, 500);
});

test("authorized ASR selects one bounded audio-only HTTPS format", () => {
  assert.equal(selectTemporaryAudio({ formats: [
    { url: "https://cdn.example/video", vcodec: "avc", acodec: "aac", abr: 500 },
    { url: "http://cdn.example/insecure", vcodec: "none", acodec: "aac", abr: 300 },
    { url: "https://cdn.example/huge", vcodec: "none", acodec: "aac", filesize: 300 * 1024 * 1024, abr: 250 },
    { url: "https://cdn.example/audio", vcodec: "none", acodec: "aac", filesize: 10_000, abr: 128 },
  ] }), "https://cdn.example/audio");
});

test("parses Bilibili embedded state offline without granting yt-dlp a network socket", () => {
  const html = `<script>window.__INITIAL_STATE__={"videoData":{"title":"Offline video","owner":{"name":"Author"},"desc":"Description","duration":12,"pubdate":1783900800,"pages":[{"part":"Part 1"}]}};</script>
    <script>window.__playinfo__={"data":{"dash":{"audio":[{"baseUrl":"https://cdn.example/audio.m4a?token=runtime-only","codecs":"mp4a.40.2","bandwidth":128000}]},"subtitle":{"subtitles":[{"lan":"zh-CN","lan_doc":"中文","subtitle_url":"//subtitle.example/sub.json?signature=runtime-only"}]}}};</script>`;
  const parsed = parseBilibiliHtml(html);
  assert.equal(parsed.title, "Offline video");
  assert.equal(parsed.formats[0].vcodec, "none");
  assert.match(parsed.formats[0].url, /^https:\/\/cdn\.example/);
  assert.match(parsed.requested_subtitles["中文"].url, /^https:\/\/subtitle\.example/);
});

test("production runner has no child process or direct network client", () => {
  const source = fs.readFileSync(new URL("./index.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(source, /child_process|execFile|spawn\(|fetch\(|https?\.request|yt-dlp/);
});

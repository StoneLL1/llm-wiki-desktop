import assert from "node:assert/strict";
import test from "node:test";
import { FIXED_ARGS, parseYtDlpMetadata, validateBilibiliUrl } from "./core.mjs";

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

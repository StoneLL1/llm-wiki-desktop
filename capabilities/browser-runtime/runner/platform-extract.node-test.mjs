import assert from "node:assert/strict";
import test from "node:test";
import { classifyRemoteImageKind, extractPlatformPayload, selectRelevantApiEvidence } from "./platform-extract.mjs";

test("persists platform images only when the selected media mode allows it", () => {
  assert.equal(classifyRemoteImageKind("xiaohongshu", false, false, "preserve_original"), "image");
  assert.equal(classifyRemoteImageKind("douyin", false, false, "extract_only"), null);
  assert.equal(classifyRemoteImageKind("bilibili", false, false, "extract_only"), null);
});

test("requests temporary OCR inputs only for an explicit image-post OCR run", () => {
  assert.equal(classifyRemoteImageKind("xiaohongshu", false, true, "extract_only"), "temporary_image");
  assert.equal(classifyRemoteImageKind("douyin", true, true, "preserve_original"), "temporary_image");
  assert.equal(classifyRemoteImageKind("generic", false, true), "image");
});

test("anchors extraction to the content id in the URL", () => {
  const html = `<script>{"feed":[{"aweme_id":"999","desc":"Recommended","video":{"play_addr":{"url_list":["https://v3-dy-o-abtest.zjcdn.com/wrong.mp4"]}}},{"aweme_id":"123","desc":"Requested","video":{"play_addr":{"url_list":["https://v3-dy-o-abtest.zjcdn.com/right.mp4"]}}}]}</script>`;
  const result = extractPlatformPayload("douyin", html, "https://www.douyin.com/video/123");
  assert.equal(result.title, "Requested");
  assert.equal(result.mediaUrl, "https://v3-dy-o-abtest.zjcdn.com/right.mp4");
});

test("does not fall back to a recommended item when the requested id is absent", () => {
  const html = `<script>{"noteCard":{"noteId":"other","title":"Wrong","desc":"Wrong body"}}</script>`;
  assert.equal(extractPlatformPayload("xiaohongshu", html, "https://www.xiaohongshu.com/explore/requested"), null);
});

test("keeps only API responses anchored to the requested work", () => {
  const candidates = [
    { url: "https://www.douyin.com/api/recommend", value: { aweme_id: "999", desc: "Wrong" } },
    { url: "https://www.douyin.com/api/detail", value: { aweme_id: "123", desc: "Right" } },
  ];
  assert.deepEqual(
    selectRelevantApiEvidence("douyin", candidates, "https://www.douyin.com/video/123"),
    [candidates[1]],
  );
});

test("extracts XHS playable media separately from the cover", () => {
  const html = `<script>{"noteCard":{"noteId":"n1","title":"Title","desc":"Body","user":{"nickname":"Author"},"imageList":[{"urlDefault":"https://sns-img-qc.xhscdn.com/cover.jpg"}],"video":{"media":{"stream":{"h264":[{"masterUrl":"https://sns-video-qc.xhscdn.com/video.mp4"}]}}}}}</script>`;
  const result = extractPlatformPayload("xiaohongshu", html, "https://www.xiaohongshu.com/explore/n1");
  assert.equal(result.author, "Author");
  assert.equal(result.images[0], "https://sns-img-qc.xhscdn.com/cover.jpg");
  assert.equal(result.mediaUrl, "https://sns-video-qc.xhscdn.com/video.mp4");
});

test("extracts Douyin play_addr instead of its cover", () => {
  const html = `<script>{"aweme_detail":{"awemeId":"1","desc":"Video","video":{"cover":{"url_list":["https://p3-sign.douyinpic.com/cover.jpg"]},"play_addr":{"url_list":["https://v3-dy-o-abtest.zjcdn.com/video.mp4"]}}}}</script>`;
  const result = extractPlatformPayload("douyin", html, "https://www.douyin.com/video/1");
  assert.equal(result.mediaUrl, "https://v3-dy-o-abtest.zjcdn.com/video.mp4");
  assert.notEqual(result.mediaUrl, result.images[0]);
});

test("extracts Bilibili title, owner, subtitle, and progressive stream", () => {
  const html = `<script>{"data":{"bvid":"BV1abc","title":"Video","owner":{"name":"UP"},"subtitle":{"list":[{"subtitle_url":"https://aisubtitle.hdslb.com/subtitle.vtt"}]},"durl":[{"url":"https://upos-sz-mirrorali.bilivideo.com/video.mp4"}]}}</script>`;
  const result = extractPlatformPayload("bilibili", html, "https://www.bilibili.com/video/BV1abc");
  assert.equal(result.author, "UP");
  assert.equal(result.mediaUrl, "https://upos-sz-mirrorali.bilivideo.com/video.mp4");
  assert.equal(result.subtitles[0], "https://aisubtitle.hdslb.com/subtitle.vtt");
});

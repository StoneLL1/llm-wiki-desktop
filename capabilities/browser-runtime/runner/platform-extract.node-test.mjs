import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { URL } from "node:url";
import { classifyPlatformPage, classifyRemoteImageKind, extractPlatformPayload, renderPlatformMarkdown, selectRelevantApiEvidence } from "./platform-extract.mjs";

function fixture(name) {
  return readFileSync(new URL(`../../../tests/fixtures/import-v2/web/xiaohongshu/${name}`, import.meta.url), "utf8");
}

test("persists platform images only when the selected media mode allows it", () => {
  assert.equal(classifyRemoteImageKind("xiaohongshu", false, false, "preserve_original"), "image");
  assert.equal(classifyRemoteImageKind("douyin", false, false, "extract_only"), null);
  assert.equal(classifyRemoteImageKind("bilibili", false, false, "extract_only"), null);
});

test("requests temporary OCR inputs only for an explicit image-post OCR run", () => {
  assert.equal(classifyRemoteImageKind("xiaohongshu", false, true, "extract_only"), "temporary_image");
  assert.equal(classifyRemoteImageKind("douyin", true, true, "preserve_original"), "image");
  assert.equal(classifyRemoteImageKind("xiaohongshu", true, true, "extract_only"), null);
  assert.equal(classifyRemoteImageKind("generic", false, true), "image");
});

test("extract-only Markdown keeps image evidence as readable non-retention notices", () => {
  const payload = {
    title: "图文",
    titleSource: "platform",
    platformId: "note-1",
    contentType: "image_post",
    description: "正文",
    images: ["https://ci.xiaohongshu.com/1.jpg", "https://ci.xiaohongshu.com/2.jpg"],
    hashtags: [],
  };
  const markdown = renderPlatformMarkdown(
    "xiaohongshu",
    payload,
    "https://www.xiaohongshu.com/explore/note-1",
    [],
    null,
    "extract_only",
  );
  assert.match(markdown, /## 图片\n\n1\. （原图未保留）\n2\. （原图未保留）/u);
  assert.match(markdown, /## 原始正文\n\n正文/u);
});

test("extract-only image evidence remains previewable when the note has no description", () => {
  const markdown = renderPlatformMarkdown(
    "xiaohongshu",
    {
      title: "仅图片",
      titleSource: "platform",
      platformId: "note-2",
      contentType: "image_post",
      description: "",
      images: ["https://ci.xiaohongshu.com/1.jpg"],
      hashtags: [],
    },
    "https://www.xiaohongshu.com/explore/note-2",
    [],
    null,
    "extract_only",
  );
  assert.match(markdown, /## 图片\n\n1\. （原图未保留）/u);
  assert.doesNotMatch(markdown, /asset:\/\//u);
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

test("does not treat an XHS user profile as a note URL", () => {
  const html = `<script>{"user":{"id":"author-1","title":"作者主页","desc":"个人简介"},"feed":[{"noteId":"recommended","title":"推荐内容","desc":"不应导入"}]}</script>`;
  assert.equal(
    extractPlatformPayload("xiaohongshu", html, "https://www.xiaohongshu.com/user/profile/author-1"),
    null,
  );
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

test("extracts realistic XHS initial state with stable image order and deduplication", () => {
  const result = extractPlatformPayload(
    "xiaohongshu",
    fixture("image-note.html"),
    "https://www.xiaohongshu.com/explore/67f00abc1234?xsec_token=target-secret",
  );
  assert.equal(result.platformId, "67f00abc1234");
  assert.equal(result.titleSource, "platform");
  assert.equal(result.contentType, "image_post");
  assert.equal(result.author, "作者甲");
  assert.deepEqual(result.hashtags, ["#读书", "#知识库", "#结构化话题"]);
  assert.match(result.publishedAt, /^\d{4}-\d{2}-\d{2}T/);
  assert.equal(result.images.length, 2);
  assert.match(result.images[0], /001\.jpg/);
  assert.match(result.images[1], /002\.jpg/);
});

test("keeps declared XHS video type when the playable stream shape changes", () => {
  const html = `<script>{"note":{"noteId":"video1","type":"video","title":"视频笔记","desc":"正文","imageList":[{"urlDefault":"https://sns-img-qc.xhscdn.com/cover.jpg"}],"video":{"changedStreamShape":true}}}</script>`;
  const result = extractPlatformPayload("xiaohongshu", html, "https://www.xiaohongshu.com/explore/video1");
  assert.equal(result.contentType, "video");
  assert.equal(result.mediaUrl, null);
});

test("marks a title derived from the first body line", () => {
  const result = extractPlatformPayload(
    "xiaohongshu",
    fixture("inferred-title.html"),
    "https://www.xiaohongshu.com/explore/67f00infer1234",
  );
  assert.equal(result.title, "没有独立标题的第一行正文");
  assert.equal(result.titleSource, "inferred");
});

test("classifies XHS captcha login and removal separately", () => {
  assert.equal(classifyPlatformPage("xiaohongshu", fixture("captcha.html")), "IMPORT_WEB_CAPTCHA_REQUIRED");
  assert.equal(classifyPlatformPage("xiaohongshu", fixture("login-required.html")), "IMPORT_WEB_LOGIN_REQUIRED");
  assert.equal(classifyPlatformPage("xiaohongshu", fixture("removed.html")), "IMPORT_WEB_CONTENT_REMOVED");
});

test("renders the shared XHS source Markdown contract", () => {
  const payload = extractPlatformPayload(
    "xiaohongshu",
    fixture("image-note.html"),
    "https://www.xiaohongshu.com/explore/67f00abc1234",
  );
  const markdown = renderPlatformMarkdown(
    "xiaohongshu",
    payload,
    "https://www.xiaohongshu.com/explore/67f00abc1234",
    ["asset://webasset-0", "asset://webasset-1"],
  );
  for (const expected of [
    "type: source",
    "source_platform: \"xiaohongshu\"",
    "engine_version: \"0.1.0\"",
    "source_id: \"67f00abc1234\"",
    "## 原始正文",
    "## 话题",
    "## 图片",
    "asset://webasset-1",
  ]) assert.ok(markdown.includes(expected), `missing ${expected}`);
  assert.ok(!markdown.includes("## 字幕 / 转写"));
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

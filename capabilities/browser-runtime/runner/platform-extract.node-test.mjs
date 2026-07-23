import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { URL } from "node:url";
import { bilibiliMediaPolicy, classifyPlatformPage, classifyRemoteImageKind, extractBilibiliPlayerEvidence, extractBilibiliPlayerEvidenceFromHtml, extractPlatformPayload, extractRelevantBilibiliPlayerEvidence, isBilibiliPlayerApiUrl, mergeBilibiliPlayerEvidence, renderPlatformMarkdown, resolveSubtitleReference, selectRelevantApiEvidence } from "./platform-extract.mjs";

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
  assert.equal(result.asrMediaUrl, "https://upos-sz-mirrorali.bilivideo.com/video.mp4");
  assert.equal(result.subtitles.length, 1);
  assert.equal(result.subtitles[0], "https://aisubtitle.hdslb.com/subtitle.vtt");
});

test("merges a target-bound Bilibili player response into separate page metadata", () => {
  const page = extractPlatformPayload(
    "bilibili",
    `<script>{"data":{"bvid":"BV1abc","title":"Video","owner":{"name":"UP"}}}</script>`,
    "https://www.bilibili.com/video/BV1abc",
  );
  const player = {
    url: "https://api.bilibili.com/x/player/wbi/v2?bvid=BV1abc&cid=123",
    value: {
      data: {
        subtitle: {
          subtitles: [
            {
              lan_doc: "中文（自动生成）",
              subtitle_url: "//aisubtitle.hdslb.com/bfs/ai_subtitle/subtitle.json?token=one",
            },
          ],
        },
        dash: {
          audio: [
            {
              baseUrl: "https://upos-sz-mirrorali.bilivideo.com/audio.m4s?deadline=one",
            },
          ],
        },
      },
    },
  };
  const evidence = extractRelevantBilibiliPlayerEvidence(
    player,
    "https://www.bilibili.com/video/BV1abc",
  );
  const merged = mergeBilibiliPlayerEvidence(page, [evidence]);

  assert.equal(merged.title, "Video");
  assert.equal(merged.contentType, "video");
  assert.equal(merged.mediaUrl, null);
  assert.equal(
    merged.asrMediaUrl,
    "https://upos-sz-mirrorali.bilivideo.com/audio.m4s?deadline=one",
  );
  assert.deepEqual(merged.subtitles, [
    "https://aisubtitle.hdslb.com/bfs/ai_subtitle/subtitle.json?token=one",
  ]);
  assert.deepEqual(
    selectRelevantApiEvidence(
      "bilibili",
      [player],
      "https://www.bilibili.com/video/BV1abc",
    ),
    [player],
  );
});

test("rejects Bilibili player evidence for a different recommended video", () => {
  const candidate = {
    url: "https://api.bilibili.com/x/player/wbi/v2?bvid=BV1wrong&cid=999",
    value: {
      data: {
        subtitle: {
          subtitles: [
            {
              subtitle_url: "https://aisubtitle.hdslb.com/wrong.json",
            },
          ],
        },
      },
    },
  };
  const evidence = extractRelevantBilibiliPlayerEvidence(
    candidate,
    "https://www.bilibili.com/video/BV1abc",
  );
  assert.deepEqual(evidence, {
    mediaUrl: null,
    asrMediaUrl: null,
    subtitleCandidates: [],
    subtitles: [],
  });
  assert.deepEqual(
    selectRelevantApiEvidence(
      "bilibili",
      [candidate],
      "https://www.bilibili.com/video/BV1abc",
    ),
    [],
  );
});

test("keeps human Chinese Bilibili subtitles ahead of automatic alternatives", () => {
  const evidence = extractBilibiliPlayerEvidence(
    {
      subtitle: {
        subtitles: [
          {
            lan: "en",
            lan_doc: "English (AI)",
            subtitle_url: "https://aisubtitle.hdslb.com/en.json",
            ai_status: 1,
          },
          {
            lan: "zh-CN",
            lan_doc: "中文",
            subtitle_url: "https://aisubtitle.hdslb.com/zh.json",
            ai_status: 0,
          },
        ],
      },
    },
    "https://www.bilibili.com/video/BV1abc",
  );
  assert.deepEqual(
    evidence.subtitleCandidates.map((subtitle) => [
      subtitle.language,
      subtitle.automatic,
      subtitle.url,
    ]),
    [
      ["zh-CN", false, "https://aisubtitle.hdslb.com/zh.json"],
      ["en", true, "https://aisubtitle.hdslb.com/en.json"],
    ],
  );
});

test("does not merge recommendation player data from unrelated HTML scripts", () => {
  const html = `
    <script>{"target":{"bvid":"BV1abc","title":"Target"},"recommendation":{"bvid":"BV1wrong","title":"Wrong","durl":[{"url":"https://upos-sz-mirrorali.bilivideo.com/wrong.mp4"}],"subtitle":{"subtitles":[{"subtitle_url":"https://aisubtitle.hdslb.com/wrong.json"}]}}}</script>
  `;
  const evidence = extractBilibiliPlayerEvidenceFromHtml(
    html,
    "https://www.bilibili.com/video/BV1abc",
    { bvid: "BV1abc" },
  );
  assert.equal(evidence.mediaUrl, null);
  assert.deepEqual(evidence.subtitles, []);
});

test("scopes mixed API payload evidence to the target object instead of siblings", () => {
  const candidate = {
    url: "https://api.bilibili.com/x/web-interface/view/detail",
    value: {
      target: {
        bvid: "BV1abc",
        title: "Target",
        subtitle: {
          subtitles: [
            {
              subtitle_url: "https://aisubtitle.hdslb.com/right.json",
            },
          ],
        },
      },
      recommendation: {
        bvid: "BV1wrong",
        title: "Wrong",
        durl: [
          {
            url: "https://upos-sz-mirrorali.bilivideo.com/wrong.mp4",
          },
        ],
      },
    },
  };
  const evidence = extractRelevantBilibiliPlayerEvidence(
    candidate,
    "https://www.bilibili.com/video/BV1abc",
  );
  assert.equal(evidence.mediaUrl, null);
  assert.deepEqual(evidence.subtitles, [
    "https://aisubtitle.hdslb.com/right.json",
  ]);
});

test("accepts aid-only player requests after BV metadata establishes the alias", () => {
  const candidate = {
    url: "https://api.bilibili.com/x/player/wbi/v2?aid=123&cid=456",
    value: {
      data: {
        dash: {
          audio: [
            {
              baseUrl: "https://upos-sz-mirrorali.bilivideo.com/right.m4s",
            },
          ],
        },
      },
    },
  };
  const evidence = extractRelevantBilibiliPlayerEvidence(
    candidate,
    "https://www.bilibili.com/video/BV1abc",
    { bvid: "BV1abc", aid: "123", cid: "456" },
  );
  assert.equal(
    evidence.asrMediaUrl,
    "https://upos-sz-mirrorali.bilivideo.com/right.m4s",
  );
  assert.deepEqual(
    selectRelevantApiEvidence(
      "bilibili",
      [candidate],
      "https://www.bilibili.com/video/BV1abc",
      3,
      { bvid: "BV1abc", aid: "123", cid: "456" },
    ),
    [candidate],
  );
});

test("keeps DASH audio ASR-only and fails closed for unsupported policy combinations", () => {
  const payload = {
    mediaUrl: null,
    asrMediaUrl: "https://upos-sz-mirrorali.bilivideo.com/audio.m4s",
  };
  assert.deepEqual(
    bilibiliMediaPolicy(payload, {
      mediaSaveMode: "extract_only",
      hasSubtitle: false,
      localAsrAuthorized: false,
      allowMissingTranscript: false,
    }),
    { errorCode: "IMPORT_WEB_SUBTITLE_UNAVAILABLE", asrMediaUrl: null },
  );
  assert.deepEqual(
    bilibiliMediaPolicy(payload, {
      mediaSaveMode: "preserve_original",
      hasSubtitle: false,
      localAsrAuthorized: true,
      allowMissingTranscript: false,
    }),
    { errorCode: "IMPORT_WEB_MEDIA_UNAVAILABLE", asrMediaUrl: null },
  );
  assert.deepEqual(
    bilibiliMediaPolicy(payload, {
      mediaSaveMode: "extract_only",
      hasSubtitle: false,
      localAsrAuthorized: true,
      allowMissingTranscript: true,
    }),
    { errorCode: null, asrMediaUrl: null },
  );
  assert.deepEqual(
    bilibiliMediaPolicy(payload, {
      mediaSaveMode: "extract_only",
      hasSubtitle: false,
      localAsrAuthorized: true,
      allowMissingTranscript: false,
    }),
    {
      errorCode: null,
      asrMediaUrl: "https://upos-sz-mirrorali.bilivideo.com/audio.m4s",
    },
  );
});

test("recognizes bounded player API candidates and rejects text subtitle labels", () => {
  assert.equal(isBilibiliPlayerApiUrl(
    "https://api.bilibili.com/x/player/wbi/v2?bvid=BV1abc&cid=123",
  ), true);
  assert.equal(isBilibiliPlayerApiUrl(
    "https://api.bilibili.com/x/web-interface/popular?bvid=BV1abc",
  ), false);
  assert.equal(
    resolveSubtitleReference(
      "Related story",
      "https://example.com/article",
    ),
    null,
  );
  assert.equal(
    resolveSubtitleReference(
      "/captions/main.vtt",
      "https://example.com/article",
    )?.href,
    "https://example.com/captions/main.vtt",
  );
});

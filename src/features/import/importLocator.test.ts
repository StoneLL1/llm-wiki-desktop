import { describe, expect, it } from "vitest";

import type { ImportItem } from "../../types/importV2";
import {
  displayHostForImportLocator,
  importPlatformForLocator,
  isMediaCandidateUrl,
  isSupportedMediaPlatformUrl,
  isUnsupportedImportUrl,
  isValidPublicHttpImportUrl,
  routeForImportItem,
} from "./importLocator";

function item(locator: string, route?: string): ImportItem {
  return {
    itemId: "item-1",
    input: { kind: "url", displayName: locator, locator, normalizedLocator: null },
    status: "queued",
    selected: false,
    taskId: null,
    progress: null,
    attempts: route ? [{ route, engineId: "engine", engineVersion: "1", stage: "route", startedAt: "2026-07-22T00:00:00Z", completedAt: null, outcome: "succeeded", warnings: [] }] : [],
    preview: null,
    issue: null,
  };
}

describe("import locator policy", () => {
  it.each([
    ["https://mp.weixin.qq.com/s/a", "wechat"],
    ["https://www.zhihu.com/question/1", "zhihu"],
    ["https://b23.tv/abc", "bilibili"],
    ["https://xhslink.com/a/abc", "xiaohongshu"],
    ["https://v.douyin.com/abc", "douyin"],
    ["https://twitter.com/openai", "x"],
    ["https://example.com/post", "connector"],
  ] as const)("classifies %s as %s", (locator, platform) => {
    expect(importPlatformForLocator(locator)).toBe(platform);
  });

  it("uses one media policy for platforms and direct media URLs", () => {
    expect(isSupportedMediaPlatformUrl("https://www.bilibili.com/video/BV1xx")).toBe(true);
    expect(isMediaCandidateUrl("https://example.com/media/video.mp4?download=1")).toBe(true);
    expect(isMediaCandidateUrl("https://example.com/article")).toBe(false);
  });

  it.each(["file:///tmp/a.md", "data:text/plain,a", "javascript:alert(1)", "http://localhost:3000/a", "http://127.0.0.1/a", "http://[::1]/a"])(
    "rejects unsupported local URL %s",
    (locator) => {
      expect(isUnsupportedImportUrl(locator)).toBe(true);
      expect(isValidPublicHttpImportUrl(locator)).toBe(false);
    },
  );

  it("keeps host display and route fallback separate", () => {
    expect(displayHostForImportLocator("https://example.com:8443/post")).toBe("example.com:8443");
    expect(routeForImportItem(item("https://xhslink.com/a/abc"))).toBe("xiaohongshu");
    expect(routeForImportItem(item("https://example.com/post", "authenticated_http"))).toBe("authenticated_http");
  });
});

import { describe, expect, it } from "vitest";

import type { ImportItem } from "../../types/importV2";
import {
  displayHostForImportLocator,
  extractImportUrl,
  importPlatformForLocator,
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
  it("extracts a single Xiaohongshu share URL and preserves its access parameters", () => {
    expect(extractImportUrl("12 周末读书记录 http://xhslink.cn/o/abc 复制后打开【小红书】查看笔记！"))
      .toBe("http://xhslink.cn/o/abc");
    expect(extractImportUrl("分享：https://www.xiaohongshu.com/explore/note?xsec_token=signed%2Bvalue%3D&xsec_source=pc_share。"))
      .toBe("https://www.xiaohongshu.com/explore/note?xsec_token=signed%2Bvalue%3D&xsec_source=pc_share");
    expect(extractImportUrl("https://xhslink.com/a/one https://xhslink.com/a/two")).toBeNull();
    expect(extractImportUrl("text https://xiaohongshu.com.evil.example/note")).toBeNull();
    expect(extractImportUrl("text http://localhost/note")).toBeNull();
  });
  it.each([
    ["https://mp.weixin.qq.com/s/a", "wechat"],
    ["https://www.zhihu.com/question/1", "zhihu"],
    ["https://b23.tv/abc", "bilibili"],
    ["https://xhslink.com/a/abc", "xiaohongshu"],
    ["http://xhslink.cn/o/abc", "xiaohongshu"],
    ["https://v.douyin.com/abc", "douyin"],
    ["https://twitter.com/openai", "x"],
    ["https://example.com/post", "connector"],
  ] as const)("classifies %s as %s", (locator, platform) => {
    expect(importPlatformForLocator(locator)).toBe(platform);
  });

  it("recognizes supported media platforms without adding a pre-queue policy step", () => {
    expect(isSupportedMediaPlatformUrl("https://www.bilibili.com/video/BV1xx")).toBe(true);
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
    expect(routeForImportItem(item("http://xhslink.cn/o/abc"))).toBe("xiaohongshu");
    expect(routeForImportItem(item("https://example.com/post", "authenticated_http"))).toBe("authenticated_http");
  });
});

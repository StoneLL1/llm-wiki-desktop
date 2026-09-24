import { URL } from "node:url";
import assert from "node:assert/strict";
import test from "node:test";
import { JSDOM } from "jsdom";
import { extractArticle } from "./article-extract.mjs";
import { isPageResource, fetchPageResource } from "./page-resources.mjs";
import { classifyPlatformPage, classifyRemoteImageKind } from "./platform-extract.mjs";
import { isTrustedPlatformAssetHost, restoreCookieBackup } from "./policy.mjs";

for (const body of [
  '<p>短文。</p>',
  '<p>This article discusses captcha challenges, login required, and 安全验证 as ordinary source prose.</p>',
  '<p>正文带配图。<img data-src="https://mmbiz.qpic.cn/image.png"></p>',
]) test('full browser accepts WeChat article content: ' + body, () => {
  const dom = new JSDOM(`<title>网页标题</title><h1 id="activity-name">文章标题</h1><span id="js_name">作者</span><nav>登录后查看</nav><div id="js_content">${body}</div>`);
  const article = extractArticle(dom.window.document, "wechat");
  assert.equal(article.title, "文章标题");
  assert.equal(article.byline, "作者");
  assert.equal(article.content, body);
  assert.ok(article.textContent);
  assert.equal(classifyRemoteImageKind("wechat", false, false, "extract_only"), "image");
});

test("platform article extraction rejects page shells and keeps real challenge classification", () => {
  for (const platform of ["wechat", "generic"]) {
    const document = new JSDOM('<title>Title</title><main><nav>Home Login</nav></main><footer>Copyright</footer>').window.document;
    assert.equal(extractArticle(document, platform), null);
  }
  for (const body of ["<h1>Title only</h1>", "<article><h1>Title only</h1></article>"]) {
    assert.equal(extractArticle(new JSDOM(body).window.document, "generic"), null);
  }
  const challenge = new JSDOM('<form id="challenge-form">请完成验证 captcha</form>').window.document;
  assert.equal(extractArticle(challenge, "wechat"), null);
  assert.equal(classifyPlatformPage("wechat", challenge.body.textContent), "IMPORT_WEB_CAPTCHA_REQUIRED");
});

test("trusted CDNs serve only necessary browser subresources, never profile navigation or media", async () => {
  for (const [platform, target, resource] of [
    ["wechat", "https://mp.weixin.qq.com/s/article", "https://res.wx.qq.com/script.js"],
    ["xiaohongshu", "https://www.xiaohongshu.com/explore/id", "https://fe-static.xhscdn.com/app.js"],
  ]) {
    for (const type of ["script", "stylesheet", "font"]) assert.ok(isPageResource(platform, new URL(target), new URL(resource), type));
    for (const type of ["document", "image", "media", "xhr", "fetch"]) assert.equal(isPageResource(platform, new URL(target), new URL(resource), type), false);
    for (const value of [resource.replace("https:", "http:"), resource.replace(".com/", ".com.evil.test/"), resource.replace(".com/", ".com:8443/")]) {
      assert.equal(isPageResource(platform, new URL(target), new URL(value), "script"), false);
    }
    // The checked answer is required before the HTTPS request is started.
    await assert.rejects(fetchPageResource(platform, new URL(target), new URL(resource), "script", async () => { throw new Error("blocked DNS answer"); }), /blocked DNS answer/);
  }
  assert.ok(isTrustedPlatformAssetHost("wechat", "mp.weixin.qq.com", "mmbiz.qpic.cn"));
  assert.equal(isTrustedPlatformAssetHost("wechat", "mp.weixin.qq.com", "mmbiz.qpic.cn.evil.test"), false);
});


test("session cookie restore preserves newer profile credentials and ignores foreign cookies", async () => {
  const restored = [];
  const context = {
    async cookies() { return [{ name: "wxuin", value: "rotated", domain: "mp.weixin.qq.com", path: "/" }]; },
    async addCookies(cookies) { restored.push(...cookies); },
  };
  await restoreCookieBackup(context, "wechat", [
    { name: "wxuin", value: "old", domain: "mp.weixin.qq.com", path: "/", expires: -1 },
    { name: "key_ticket", value: "session-only", domain: "mp.weixin.qq.com", path: "/", expires: -1 },
    { name: "key_ticket", value: "foreign", domain: "evil.test", path: "/", expires: -1 },
  ]);
  assert.deepEqual(restored.map(cookie => [cookie.name, cookie.value]), [["key_ticket", "session-only"]]);
});

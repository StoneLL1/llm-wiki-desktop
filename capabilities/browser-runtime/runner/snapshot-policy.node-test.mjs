import assert from "node:assert/strict";
import test from "node:test";
import { isLoginChallengeState, redactJsonValue, redactSensitiveText, sanitizePublicUrl } from "./snapshot-policy.mjs";

test("redacts camel-case JSON and HTML meta credentials", () => {
  const html = redactSensitiveText('<script>{"accessToken":"secret-a","csrfToken":"secret-b"}</script><meta name="csrf-token" content="secret-c">');
  assert.doesNotMatch(html, /secret-[abc]/);
  assert.match(html, /REDACTED/);
  assert.deepEqual(redactJsonValue({ accessToken: "a", nested: { csrf_token: "b", title: "keep" } }), {
    accessToken: "REDACTED", nested: { csrf_token: "REDACTED", title: "keep" },
  });
});

test("keeps the resolved public URL while dropping signed query material", () => {
  assert.equal(
    sanitizePublicUrl("https://www.xiaohongshu.com/explore/n1?id=42&xsec_token=secret&source=share#frag"),
    "https://www.xiaohongshu.com/explore/n1?id=42",
  );
  const future = redactSensitiveText("https://cdn.example/video.mp4?id=42&Policy=secret&Key-Pair-Id=K123&future-sig=unknown");
  assert.equal(future, "https://cdn.example/video.mp4?id=42");
});

test("recognizes stale-cookie login and challenge pages", () => {
  assert.equal(isLoginChallengeState("https://passport.example/login", ""), true);
  assert.equal(isLoginChallengeState("https://www.bilibili.com/video/BV1", "扫码登录后继续"), true);
  assert.equal(isLoginChallengeState("https://www.bilibili.com/video/BV1", "ordinary content"), false);
});

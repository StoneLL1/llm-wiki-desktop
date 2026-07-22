import assert from "node:assert/strict";
import test from "node:test";
import { isLoginChallengeState, redactJsonValue, redactSensitiveText } from "./snapshot-policy.mjs";

test("redacts camel-case JSON and HTML meta credentials", () => {
  const html = redactSensitiveText('<script>{"accessToken":"secret-a","csrfToken":"secret-b"}</script><meta name="csrf-token" content="secret-c">');
  assert.doesNotMatch(html, /secret-[abc]/);
  assert.match(html, /REDACTED/);
  assert.deepEqual(redactJsonValue({ accessToken: "a", nested: { csrf_token: "b", title: "keep" } }), {
    accessToken: "REDACTED", nested: { csrf_token: "REDACTED", title: "keep" },
  });
});

test("recognizes stale-cookie login and challenge pages", () => {
  assert.equal(isLoginChallengeState("https://passport.example/login", ""), true);
  assert.equal(isLoginChallengeState("https://www.bilibili.com/video/BV1", "扫码登录后继续"), true);
  assert.equal(isLoginChallengeState("https://www.bilibili.com/video/BV1", "ordinary content"), false);
});

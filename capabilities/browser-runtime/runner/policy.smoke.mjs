import assert from "node:assert/strict";

import { allowedHostFor, isBlockedAddress } from "./policy.mjs";

for (const address of [
  "127.0.0.1",
  "10.0.0.1",
  "169.254.169.254",
  "192.168.1.1",
  "224.0.0.1",
  "::1",
  "fc00::1",
  "fe80::1",
]) {
  assert.equal(isBlockedAddress(address), true, address);
}
assert.equal(isBlockedAddress("93.184.216.34"), false);
assert.equal(allowedHostFor("wechat", "mp.weixin.qq.com", "mmbiz.qpic.cn"), true);
assert.equal(allowedHostFor("wechat", "mp.weixin.qq.com", "qpic.cn.evil.example"), false);
assert.equal(allowedHostFor("generic", "example.com", "cdn.example.net"), false);

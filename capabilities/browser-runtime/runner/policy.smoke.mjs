import assert from "node:assert/strict";

import { hasPlatformAuthentication, isBlockedAddress, isPinnedTargetHost, isPlatformTargetHost, resolvePinnedAddress } from "./policy.mjs";

for (const address of ["127.0.0.1", "10.0.0.1", "169.254.169.254", "192.168.1.1", "224.0.0.1", "::1", "fc00::1", "fe80::1"]) {
  assert.equal(isBlockedAddress(address), true, address);
}
assert.equal(isBlockedAddress("93.184.216.34"), false);

assert.equal(isPinnedTargetHost("mp.weixin.qq.com", "mp.weixin.qq.com"), true);
assert.equal(isPinnedTargetHost("mp.weixin.qq.com", "mmbiz.qpic.cn"), false);
assert.equal(isPinnedTargetHost("example.com", "example.com.evil.test"), false);
assert.equal(isPlatformTargetHost("wechat", "mp.weixin.qq.com"), true);
assert.equal(isPlatformTargetHost("wechat", "mp.weixin.qq.com.evil.test"), false);
assert.equal(isPlatformTargetHost("x", "mobile.twitter.com"), true);
assert.equal(isPlatformTargetHost("x", "example.com"), false);

const lookup = async () => [{ address: "93.184.216.34" }, { address: "93.184.216.35" }];
assert.equal(await resolvePinnedAddress("example.com", lookup), "93.184.216.34");
await assert.rejects(() => resolvePinnedAddress("example.com", async () => [{ address: "93.184.216.34" }, { address: "127.0.0.1" }]));

assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [{ name: "SESSDATA", domain: ".bilibili.com" }]), true);
assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [{ name: "SESSDATA", domain: ".evil.test" }]), false);
assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [], []), false);
assert.equal(hasPlatformAuthentication("x", "x.com", [], ["public-page-selector"]), false);

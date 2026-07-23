import assert from "node:assert/strict";

import { hasPlatformAuthentication, isBlockedAddress, isPinnedTargetHost, isPlatformCookieDomain, isPlatformNavigationHost, isPlatformTargetHost, isSecureAssetProtocol, isTrustedPlatformAssetHost, resolvePinnedAddress, sanitizeCookieBackup } from "./policy.mjs";

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

assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [{ name: "SESSDATA", value: "session", domain: ".bilibili.com" }]), true);
assert.equal(isPlatformCookieDomain("bilibili", ".bilibili.com"), true);
assert.equal(isPlatformCookieDomain("bilibili", ".evil.test"), false);
assert.equal(isPlatformCookieDomain("bilibili", ".evil.bilibili.com"), false);
assert.equal(isPlatformCookieDomain("bilibili", ".com"), false);
assert.equal(isPlatformTargetHost("douyin", "www.douyin.com"), true);
assert.equal(isPlatformTargetHost("xiaohongshu", "xhslink.com"), true);
assert.equal(isPlatformNavigationHost("xiaohongshu", "xhslink.com"), true);
assert.equal(isPlatformNavigationHost("bilibili", "evil.bilibili.com"), false);
assert.equal(isTrustedPlatformAssetHost("bilibili", "www.bilibili.com", "upos-sz-mirrorali.bilivideo.com"), true);
assert.equal(isTrustedPlatformAssetHost("bilibili", "www.bilibili.com", "809al93l.edge.mountaintoys.cn"), true);
assert.equal(isTrustedPlatformAssetHost("bilibili", "www.bilibili.com", "edge.mountaintoys.cn.evil.example"), false);
assert.equal(isSecureAssetProtocol("https:"), true);
assert.equal(isSecureAssetProtocol("http:"), false);
assert.equal(isTrustedPlatformAssetHost("douyin", "www.douyin.com", "p3-sign.douyinvod.com"), true);
assert.equal(isTrustedPlatformAssetHost("xiaohongshu", "www.xiaohongshu.com", "sns-img-qc.xhscdn.com"), true);
assert.equal(isTrustedPlatformAssetHost("bilibili", "www.bilibili.com", "cdn.example.test"), false);
assert.equal(hasPlatformAuthentication("douyin", "www.douyin.com", [{ name: "sessionid", value: "session", domain: ".douyin.com" }]), true);
assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [{ name: "SESSDATA", domain: ".evil.test" }]), false);
assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [], []), false);
assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [{ name: "SESSDATA", value: "", domain: ".bilibili.com" }]), false);
assert.equal(hasPlatformAuthentication("bilibili", "www.bilibili.com", [{ name: "SESSDATA", value: "expired", domain: ".bilibili.com", expires: 1 }]), false);
assert.deepEqual(
  sanitizeCookieBackup("bilibili", [
    { name: "SESSDATA", value: "kept", domain: ".bilibili.com", path: "/", httpOnly: true },
    { name: "SESSDATA", value: "dropped", domain: ".evil.test" },
    { name: "unrelated", value: "dropped", domain: ".bilibili.com" },
  ]),
  [{ name: "SESSDATA", value: "kept", domain: ".bilibili.com", path: "/", expires: -1, httpOnly: true, secure: false, sameSite: undefined }],
);
assert.equal(hasPlatformAuthentication("x", "x.com", [], ["public-page-selector"]), false);

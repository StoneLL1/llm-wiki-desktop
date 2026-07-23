import dns from "node:dns/promises";
import net from "node:net";

const LOGIN_COOKIE_NAMES = Object.freeze({
  wechat: new Set(["wxuin", "key_ticket"]),
  zhihu: new Set(["z_c0"]),
  bilibili: new Set(["SESSDATA"]),
  xiaohongshu: new Set(["web_session"]),
  douyin: new Set(["sessionid", "sid_guard", "passport_csrf_token"]),
  x: new Set(["auth_token"]),
});

const PLATFORM_HOSTS = Object.freeze({
  wechat: ["mp.weixin.qq.com"],
  zhihu: ["zhihu.com"],
  bilibili: ["bilibili.com", "b23.tv"],
  xiaohongshu: ["xiaohongshu.com", "xhslink.com"],
  douyin: ["douyin.com", "iesdouyin.com"],
  x: ["x.com", "twitter.com"],
});

// Navigation is intentionally narrower than platform classification. A
// shared persistent profile may contain domain cookies, so wildcard
// subdomains must never become implicit cookie destinations.
const PLATFORM_NAVIGATION_HOSTS = Object.freeze({
  wechat: ["mp.weixin.qq.com"],
  zhihu: ["zhihu.com", "www.zhihu.com"],
  bilibili: ["b23.tv", "bilibili.com", "www.bilibili.com", "m.bilibili.com", "space.bilibili.com", "api.bilibili.com"],
  xiaohongshu: ["xiaohongshu.com", "www.xiaohongshu.com", "xhslink.com", "www.xhslink.com", "edith.xiaohongshu.com"],
  douyin: ["douyin.com", "www.douyin.com", "m.douyin.com", "v.douyin.com", "iesdouyin.com", "www.iesdouyin.com"],
  x: ["x.com", "www.x.com", "twitter.com", "www.twitter.com"],
});

const PLATFORM_ASSET_HOSTS = Object.freeze({
  bilibili: ["bilivideo.com", "bilivideo.cn", "hdslb.com", "biliimg.com", "edge.mountaintoys.cn"],
  xiaohongshu: ["xhscdn.com", "xhscdn.net", "xhslink.com"],
  douyin: ["douyinvod.com", "douyincdn.com", "douyinpic.com", "amemv.com", "byteimg.com", "ibytedtos.com", "bytecdn.cn", "zjcdn.com"],
});

export function isBlockedAddress(address) {
  if (!net.isIP(address)) return true;
  if (address.includes(":")) {
    const value = address.toLowerCase();
    return value === "::" || value === "::1" || value.startsWith("fc") || value.startsWith("fd") || /^fe[89ab]/.test(value) || value.startsWith("ff") || value.startsWith("::ffff:127.") || value.startsWith("::ffff:10.") || value.startsWith("::ffff:192.168.");
  }
  const octets = address.split(".").map(Number);
  const [a, b] = octets;
  return a === 0 || a === 10 || a === 127 || a >= 224 || (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127) || (a === 198 && (b === 18 || b === 19));
}

export async function resolvePinnedAddress(host, lookup = dns.lookup) {
  if (host === "localhost" || host.endsWith(".localhost")) throw new Error("blocked host");
  const answers = await lookup(host, { all: true, verbatim: true });
  if (!answers.length || answers.some(({ address }) => isBlockedAddress(address))) throw new Error("blocked DNS answer");
  return answers[0].address;
}

export function isPinnedTargetHost(targetHost, candidateHost) {
  return candidateHost.toLowerCase() === targetHost.toLowerCase();
}

export function isSecureAssetProtocol(protocol) {
  return protocol === "https:";
}

export function isPlatformTargetHost(platform, targetHost) {
  const host = targetHost.toLowerCase();
  return (PLATFORM_HOSTS[platform] || []).some((suffix) => host === suffix || host.endsWith(`.${suffix}`));
}

export function isPlatformNavigationHost(platform, targetHost) {
  return (PLATFORM_NAVIGATION_HOSTS[platform] || []).includes(targetHost.toLowerCase());
}

export function platformNavigationHosts(platform) {
  return PLATFORM_NAVIGATION_HOSTS[platform] || [];
}

export function isTrustedPlatformAssetHost(platform, targetHost, candidateHost) {
  const host = candidateHost.toLowerCase();
  if (isPlatformNavigationHost(platform, host) && isPlatformNavigationHost(platform, targetHost)) return true;
  return (PLATFORM_ASSET_HOSTS[platform] || []).some((suffix) => host === suffix || host.endsWith(`.${suffix}`));
}

export function hasPlatformAuthentication(platform, targetHost, cookies) {
  const allowedNames = LOGIN_COOKIE_NAMES[platform];
  if (!allowedNames) return false;
  const cookieProof = cookies.some((cookie) => {
    const domain = String(cookie.domain || "").replace(/^\./, "").toLowerCase();
    const expires = Number(cookie.expires ?? -1);
    const unexpired = !Number.isFinite(expires) || expires <= 0 || expires > Date.now() / 1000;
    return allowedNames.has(cookie.name)
      && String(cookie.value || "").length > 0
      && unexpired
      && (domain === targetHost || targetHost.endsWith(`.${domain}`));
  });
  return cookieProof;
}

export function isPlatformCookieDomain(platform, cookieDomain) {
  const domain = String(cookieDomain || "").replace(/^\./, "").toLowerCase();
  if (!domain || !domain.includes(".") || domain.includes("..")) return false;
  return platformNavigationHosts(platform).some((host) => {
    const normalizedHost = host.toLowerCase();
    return domain === normalizedHost || normalizedHost.endsWith(`.${domain}`);
  });
}

export function sanitizeCookieBackup(platform, cookies) {
  if (!Array.isArray(cookies)) return [];
  return cookies
    .filter((cookie) => {
      if (!cookie || typeof cookie !== "object") return false;
      const domain = String(cookie.domain || "").replace(/^\./, "").toLowerCase();
      return isPlatformCookieDomain(platform, domain)
        && hasPlatformAuthentication(platform, domain, [cookie]);
    })
    .slice(0, 64)
    .map((cookie) => ({
      name: String(cookie.name), value: String(cookie.value || ""), domain: String(cookie.domain),
      path: String(cookie.path || "/"), expires: Number(cookie.expires || -1),
      httpOnly: Boolean(cookie.httpOnly), secure: Boolean(cookie.secure), sameSite: cookie.sameSite,
    }));
}

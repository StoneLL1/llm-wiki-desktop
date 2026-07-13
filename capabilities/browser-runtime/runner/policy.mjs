import dns from "node:dns/promises";
import net from "node:net";

const LOGIN_COOKIE_NAMES = Object.freeze({
  wechat: new Set(["wxuin", "key_ticket"]),
  zhihu: new Set(["z_c0"]),
  bilibili: new Set(["SESSDATA"]),
  xiaohongshu: new Set(["web_session"]),
  x: new Set(["auth_token"]),
});

const LOGIN_SENTINELS = Object.freeze({
  wechat: ["#js_profile_qrcode", ".account_nickname"],
  zhihu: [".AppHeader-profile", "[data-za-detail-view-element_name='Avatar']"],
  bilibili: [".header-avatar-wrap", ".bili-avatar"],
  xiaohongshu: [".user.side-bar-component", "[data-v-user-avatar]"],
  x: ["[data-testid='SideNav_AccountSwitcher_Button']"],
});

const PLATFORM_HOSTS = Object.freeze({
  wechat: ["mp.weixin.qq.com"],
  zhihu: ["zhihu.com"],
  bilibili: ["bilibili.com", "b23.tv"],
  xiaohongshu: ["xiaohongshu.com"],
  x: ["x.com", "twitter.com"],
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

export function isPlatformTargetHost(platform, targetHost) {
  const host = targetHost.toLowerCase();
  return (PLATFORM_HOSTS[platform] || []).some((suffix) => host === suffix || host.endsWith(`.${suffix}`));
}

export function loginSentinels(platform) {
  return [...(LOGIN_SENTINELS[platform] || [])];
}

export function hasPlatformAuthentication(platform, targetHost, cookies, visibleSentinels = []) {
  const allowedNames = LOGIN_COOKIE_NAMES[platform];
  if (!allowedNames) return false;
  const cookieProof = cookies.some((cookie) => {
    const domain = String(cookie.domain || "").replace(/^\./, "").toLowerCase();
    return allowedNames.has(cookie.name) && (domain === targetHost || targetHost.endsWith(`.${domain}`));
  });
  const allowedSentinels = new Set(LOGIN_SENTINELS[platform] || []);
  return cookieProof || visibleSentinels.some((selector) => allowedSentinels.has(selector));
}

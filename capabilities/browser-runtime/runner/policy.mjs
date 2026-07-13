import dns from "node:dns/promises";
import net from "node:net";

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

export async function resolvePublicHost(host) {
  if (host === "localhost" || host.endsWith(".localhost")) throw new Error("blocked host");
  const answers = await dns.lookup(host, { all: true, verbatim: true });
  if (!answers.length || answers.some(({ address }) => isBlockedAddress(address))) throw new Error("blocked DNS answer");
  return [...new Set(answers.map(({ address }) => address))];
}

export function allowedHostFor(platform, targetHost, candidateHost) {
  if (candidateHost === targetHost || candidateHost.endsWith(`.${targetHost}`)) return true;
  const suffixes = {
    wechat: ["qpic.cn", "qq.com"], zhihu: ["zhimg.com", "zhihu.com"], bilibili: ["hdslb.com", "bilibili.com", "biliapi.net"], xiaohongshu: ["xhscdn.com", "xiaohongshu.com"], x: ["twimg.com", "x.com", "twitter.com"], generic: [],
  }[platform] || [];
  return suffixes.some((suffix) => candidateHost === suffix || candidateHost.endsWith(`.${suffix}`));
}

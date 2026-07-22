import type { ImportItem } from "../../types/importV2";

export type ImportPlatformId =
  | "wechat"
  | "zhihu"
  | "bilibili"
  | "xiaohongshu"
  | "douyin"
  | "x"
  | "connector";

const MEDIA_PLATFORM_IDS = new Set<ImportPlatformId>([
  "bilibili",
  "xiaohongshu",
  "douyin",
]);

function isHostOrSubdomain(host: string, root: string): boolean {
  return host === root || host.endsWith(`.${root}`);
}

function normalizedHostname(value: string): string | null {
  try {
    return new URL(value).hostname.toLowerCase().replace(/\.$/, "");
  } catch {
    return null;
  }
}

export function displayHostForImportLocator(locator: string): string {
  try {
    return new URL(locator).host || locator;
  } catch {
    return locator;
  }
}

export function importPlatformForHost(host: string): ImportPlatformId {
  const normalized = host.toLowerCase().replace(/\.$/, "");
  if (normalized === "mp.weixin.qq.com") return "wechat";
  if (isHostOrSubdomain(normalized, "zhihu.com")) return "zhihu";
  if (isHostOrSubdomain(normalized, "bilibili.com") || normalized === "b23.tv") return "bilibili";
  if (isHostOrSubdomain(normalized, "xiaohongshu.com") || isHostOrSubdomain(normalized, "xhslink.com")) return "xiaohongshu";
  if (isHostOrSubdomain(normalized, "douyin.com") || isHostOrSubdomain(normalized, "iesdouyin.com")) return "douyin";
  if (isHostOrSubdomain(normalized, "x.com") || isHostOrSubdomain(normalized, "twitter.com")) return "x";
  return "connector";
}

export function importPlatformForLocator(locator: string): ImportPlatformId {
  const host = normalizedHostname(locator);
  return host ? importPlatformForHost(host) : "connector";
}

export function isSupportedMediaPlatformUrl(locator: string): boolean {
  return MEDIA_PLATFORM_IDS.has(importPlatformForLocator(locator));
}

export function isMediaCandidateUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return isSupportedMediaPlatformUrl(value)
      || /\.(?:mp4|webm|mov|m4v|m3u8|mp3|wav|jpg|jpeg|png|webp)$/i.test(parsed.pathname);
  } catch {
    return false;
  }
}

export function isUnsupportedImportUrl(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  if (normalized.startsWith("file:") || normalized.startsWith("data:") || normalized.startsWith("javascript:")) {
    return true;
  }
  try {
    const parsed = new URL(normalized);
    const host = parsed.hostname.toLowerCase().replace(/\.$/, "");
    return host === "localhost"
      || host === "0.0.0.0"
      || host === "::1"
      || host === "[::1]"
      || /^127(?:\.\d{1,3}){3}$/.test(host);
  } catch {
    return false;
  }
}

export function isValidPublicHttpImportUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return (parsed.protocol === "http:" || parsed.protocol === "https:") && !isUnsupportedImportUrl(value);
  } catch {
    return false;
  }
}

export function routeForImportItem(item: ImportItem): string {
  const attemptedRoute = item.attempts.at(-1)?.route;
  if (attemptedRoute) return attemptedRoute;
  if (item.input.kind !== "url") return "local_file";
  const platform = importPlatformForLocator(item.input.normalizedLocator ?? item.input.locator);
  return platform === "connector" ? "generic_http" : platform;
}

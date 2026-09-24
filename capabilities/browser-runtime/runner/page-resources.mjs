import { setTimeout, clearTimeout } from "node:timers";
import { URL } from "node:url";
import https from "node:https";
import net from "node:net";
import { Buffer } from "node:buffer";
import { isTrustedPlatformAssetHost, resolvePinnedAddress } from "./policy.mjs";

// CDNs also serve the scripts/styles needed to render the source and login.
// Images/media remain host-fetched evidence; CDN documents never navigate a
// persistent profile. Proxy these public resources with a pinned DNS answer so
// Chromium cannot resolve a checked CDN a second time or send profile cookies.
export function isPageResource(platform, target, url, resourceType) {
  return url.protocol === "https:"
    && !url.username && !url.password && (!url.port || url.port === "443")
    && ["script", "stylesheet", "font"].includes(resourceType)
    && isTrustedPlatformAssetHost(platform, target.hostname, url.hostname);
}

export async function fetchPageResource(platform, target, url, resourceType, resolve = resolvePinnedAddress, redirects = 0) {
  if (!isPageResource(platform, target, url, resourceType)) throw new Error("blocked page resource");
  const address = await resolve(url.hostname, undefined, { allowBenchmarkFakeIp: true });
  return new Promise((resolveResponse, reject) => {
    const request = https.get(url, {
      lookup: (_hostname, options, callback) => options?.all
        ? callback(null, [{ address, family: net.isIP(address) }])
        : callback(null, address, net.isIP(address)),
      headers: { "accept-encoding": "identity" },
    }, (response) => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode) && response.headers.location && redirects < 3) {
        response.resume();
        // Each hop revalidates both the trusted host and its pinned address.
        let next;
        try { next = new URL(response.headers.location, url); }
        catch { reject(new Error("invalid page resource redirect")); return; }
        fetchPageResource(platform, target, next, resourceType, resolve, redirects + 1).then(resolveResponse, reject);
        return;
      }
      if (response.statusCode < 200 || response.statusCode >= 300) {
        response.resume();
        reject(new Error("page resource response rejected"));
        return;
      }
      const chunks = [];
      let size = 0;
      response.on("data", (chunk) => {
        size += chunk.length;
        if (size > 8 * 1024 * 1024) request.destroy(new Error("page resource too large"));
        else chunks.push(chunk);
      });
      response.on("error", reject);
      response.on("end", () => resolveResponse({
        status: response.statusCode,
        headers: Object.fromEntries(["content-type", "content-encoding", "access-control-allow-origin", "cross-origin-resource-policy"]
          .filter((key) => typeof response.headers[key] === "string")
          .map((key) => [key, response.headers[key]])),
        body: Buffer.concat(chunks),
      }));
    });
    const timeout = setTimeout(() => request.destroy(new Error("page resource timed out")), 15_000);
    request.on("close", () => clearTimeout(timeout));
    request.on("error", reject);
  });
}

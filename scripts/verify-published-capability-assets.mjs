import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { catalogUrlErrors } from "./verify-capability-catalog.mjs";

// Verify the bytes an anonymous user actually receives. This works for object
// storage, CDN and GitHub equally; provider metadata is not download evidence.
export async function verifyPublishedCapabilityAssets({ catalog, fetchImpl = fetch, availabilityOnly = false }) {
  if (!Array.isArray(catalog?.entries) || catalog.entries.length === 0) {
    throw new Error("published capability check requires a non-empty install catalog");
  }
  const resources = new Map();
  const urlIdentities = new Map();
  for (const entry of catalog.entries) {
    const files = [{ urls: [entry.url], bytes: entry.compressedBytes, sha256: entry.archiveSha256, archive: true }, ...(entry.modelFiles ?? []).map((model) => ({ ...model, archive: false }))];
    for (const file of files) {
      if (!Array.isArray(file.urls) || !file.urls.length || !Number.isSafeInteger(file.bytes) || file.bytes <= 0
        || !/^[a-f0-9]{64}$/u.test(file.sha256 ?? "") || /^0+$/u.test(file.sha256)) {
        throw new Error("invalid catalog resource size, URLs or SHA-256");
      }
      for (const url of file.urls) {
        if (catalogUrlErrors({ url }).length) throw new Error(`invalid public capability URL: ${url}`);
        const previous = urlIdentities.get(url);
        if (previous && (previous.bytes !== file.bytes || previous.sha256 !== file.sha256)) {
          throw new Error(`conflicting catalog identities for ${url}`);
        }
        urlIdentities.set(url, file);
      }
      const key = file.sha256 + ":" + file.bytes + ":" + file.urls.join("\0");
      // The same resource used as an archive must meet the stricter ZIP prefix check.
      if (resources.get(key)?.archive) file.archive = true;
      resources.set(key, file);
    }
  }
  for (const file of resources.values()) {
    const errors = [];
    let available = false;
    for (const url of file.urls) {
      try {
        if (availabilityOnly) await verifyAvailability(url, file, fetchImpl);
        else await verifyDownload(url, file, fetchImpl);
        available = true;
        break;
      } catch (error) { errors.push(error.message); }
    }
    if (!available) throw new Error(errors.join("; "));
  }
  return { assetCount: resources.size };
}

async function verifyDownload(url, file, fetchImpl) {
    const response = await fetchImpl(url, {
      headers: { "Accept-Encoding": "identity", "User-Agent": "llm-wiki-desktop-release-verifier" },
      signal: AbortSignal.timeout(20 * 60_000),
    });
    if (response.status !== 200 || !response.body) {
      await response.body?.cancel();
      throw new Error(`capability download is not publicly available (HTTP ${response.status}): ${url}`);
    }
    if (response.url && catalogUrlErrors({ url: response.url.split("?")[0] }).length) {
      await response.body.cancel();
      throw new Error(`capability download redirected away from public HTTPS: ${url}`);
    }
    let bytes = 0;
    const hash = createHash("sha256");
    for await (const chunk of response.body) {
      bytes += chunk.byteLength;
      if (bytes > file.bytes) throw new Error(`public capability asset size mismatch: ${url}`);
      hash.update(chunk);
    }
    if (bytes !== file.bytes) throw new Error(`public capability asset size mismatch: ${url}`);
    if (hash.digest("hex") !== file.sha256) throw new Error(`public capability asset SHA-256 mismatch: ${url}`);
}

const PREFIX_BYTES = 4096;

async function verifyAvailability(url, file, fetchImpl) {
  const controller = new AbortController();
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      controller.abort();
      reject(new Error(`public capability availability check timed out: ${url}`));
    }, 25_000);
  });
  try {
    await Promise.race([inspectPrefix(url, file, fetchImpl, controller.signal), timeout]);
  } finally {
    clearTimeout(timer);
    controller.abort();
  }
}

async function inspectPrefix(url, file, fetchImpl, signal) {
  const prefixLength = Math.min(PREFIX_BYTES, file.bytes);
  let requestUrl = url;
  let response;
  for (let redirects = 0; ; redirects += 1) {
    response = await fetchImpl(requestUrl, {
      headers: { "Accept-Encoding": "identity", "User-Agent": "llm-wiki-desktop-release-verifier", Range: `bytes=0-${prefixLength - 1}` },
      redirect: "manual",
      signal,
    });
    if (![301, 302, 303, 307, 308].includes(response.status)) break;
    await response.body?.cancel();
    const location = response.headers.get("location");
    if (!location || redirects >= 5) throw new Error(`public capability redirect is missing or excessive: ${url}`);
    const next = new URL(location, requestUrl);
    // Signed object-storage query parameters are allowed; every transport hop
    // must stay public HTTPS and must never carry URL credentials.
    if (catalogUrlErrors({ url: next.href.split("?")[0] }).length) {
      throw new Error(`capability download redirected away from public HTTPS: ${url}`);
    }
    requestUrl = next.href;
  }
  let reader;
  try {
    if (![200, 206].includes(response.status) || !response.body) {
      throw new Error(`capability download is not publicly available (HTTP ${response.status}): ${url}`);
    }
    if (response.url && catalogUrlErrors({ url: response.url.split("?")[0] }).length) {
      throw new Error(`capability download redirected away from public HTTPS: ${url}`);
    }
    const range = response.headers.get("content-range");
    if (response.status === 206) {
      const match = /^bytes 0-(\d+)\/(\d+)$/u.exec(range ?? "");
      if (!match || Number(match[1]) !== prefixLength - 1 || Number(match[2]) !== file.bytes) {
        throw new Error(`public capability asset Content-Range mismatch: ${url}`);
      }
    } else if (range != null) {
      // A full response with Content-Range must still describe the complete file.
      if (range !== `bytes 0-${file.bytes - 1}/${file.bytes}`) throw new Error(`public capability asset Content-Range mismatch: ${url}`);
    }
    const length = response.headers.get("content-length");
    const expectedResponseBytes = response.status === 206 ? prefixLength : file.bytes;
    if (length != null && (!/^\d+$/u.test(length) || Number(length) !== expectedResponseBytes)) {
      throw new Error(`public capability asset size mismatch: ${url}`);
    }
    if (/^(?:text\/html|application\/xhtml\+xml)(?:;|$)/iu.test(response.headers.get("content-type") ?? "")) {
      throw new Error(`public capability asset returned HTML: ${url}`);
    }
    reader = response.body.getReader();
    const prefix = Buffer.alloc(prefixLength);
    let received = 0;
    // A 206 response (or a small complete file) must contain exactly its stated
    // bytes. An ignored Range on a large file is cancelled after the prefix.
    const completeResponse = response.status === 206 || file.bytes <= PREFIX_BYTES;
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      const keep = Math.min(value.byteLength, prefixLength - received);
      if (keep > 0) prefix.set(value.subarray(0, keep), received);
      received += value.byteLength;
      if (completeResponse && received > expectedResponseBytes) throw new Error(`public capability asset size mismatch: ${url}`);
      if (!completeResponse && received >= prefixLength) break;
    }
    if (received < prefixLength) throw new Error(`public capability asset size mismatch: ${url}`);
    const text = prefix.toString("utf8").replace(/^\uFEFF/u, "").trimStart();
    if (/^(?:<!doctype\s+html|<html(?:\s|>)|<head(?:\s|>)|<body(?:\s|>))/iu.test(text)) {
      throw new Error(`public capability asset returned HTML: ${url}`);
    }
    if (file.archive && !(prefix.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 3, 4]))
      || prefix.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 5, 6])))) {
      throw new Error(`public capability archive is not a ZIP download: ${url}`);
    }
    if (file.bytes <= PREFIX_BYTES && createHash("sha256").update(prefix).digest("hex") !== file.sha256) {
      throw new Error(`public capability asset SHA-256 mismatch: ${url}`);
    }
  } finally {
    if (reader) await reader.cancel().catch(() => {});
    else await response.body?.cancel().catch(() => {});
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    const availabilityOnly = args.includes("--availability-only");
    const options = args.filter((argument) => argument !== "--availability-only");
    if (options.length !== 2 || options[0] !== "--catalog" || args.length !== 2 + Number(availabilityOnly)) throw new Error("expected --catalog FILE [--availability-only]");
    const catalog = JSON.parse(fs.readFileSync(options[1], "utf8"));
    const result = await verifyPublishedCapabilityAssets({ catalog, availabilityOnly });
    process.stdout.write(availabilityOnly
      ? `[published-capabilities] checked availability of ${result.assetCount} public downloads (prefix only; full content not verified)\n`
      : `[published-capabilities] verified ${result.assetCount} public downloads and SHA-256 digests\n`);
  } catch (error) {
    process.stderr.write(`[published-capabilities] ${error.message}\n`);
    process.exitCode = 1;
  }
}

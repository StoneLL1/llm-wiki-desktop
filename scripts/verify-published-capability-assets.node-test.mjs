import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { verifyPublishedCapabilityAssets } from "./verify-published-capability-assets.mjs";

const bytes = Buffer.from("real archive bytes");
const sha = createHash("sha256").update(bytes).digest("hex");
const entry = (url = "https://cdn.llmwiki.cn/engines/v1/browser.zip") => ({ url, compressedBytes: bytes.length, archiveSha256: sha });
const verify = (entries, fetchImpl) => verifyPublishedCapabilityAssets({ catalog: { entries }, fetchImpl });

test("hashes actual anonymous downloads from independent hosts and deduplicates URLs", async () => {
  const calls = [];
  const entries = [entry(), entry("https://github.com/another/engine/releases/download/v1/browser.zip"), entry()];
  assert.deepEqual(await verify(entries, async (url, options) => {
    calls.push(url);
    assert.equal(options.headers.Authorization, undefined);
    return new Response(bytes);
  }), { assetCount: 2 });
  assert.equal(calls.length, 2);
});

test("also checks independent models and falls back when their primary mirror fails", async () => {
  const value = entry();
  value.modelFiles = [{ path: "models/model.bin", bytes: bytes.length, sha256: sha, urls: ["https://cdn.llmwiki.cn/model.bin", "https://mirror.llmwiki.cn/model.bin"] }];
  assert.deepEqual(await verify([value], async (url) => new Response(bytes, { status: url === "https://cdn.llmwiki.cn/model.bin" ? 404 : 200 })), { assetCount: 2 });
});

for (const status of [403, 404, 500, 206]) test(`rejects HTTP ${status}`, async () => {
  await assert.rejects(verify([entry()], async () => new Response(bytes, { status })), /not publicly available/);
});

test("detects truncated downloads, wrong content and HTML returned with 200", async () => {
  for (const body of [bytes.subarray(1), Buffer.alloc(bytes.length, 1), "<html>login</html>", Buffer.alloc(bytes.length + 1)]) {
    await assert.rejects(verify([entry()], async () => new Response(body)), /size mismatch|SHA-256 mismatch/);
  }
});

test("rejects malformed entries before requesting any network bytes", async () => {
  for (const entries of [[], [entry("http://cdn.llmwiki.cn/a.zip")], [entry("https://user:secret@cdn.llmwiki.cn/a.zip")], [{ ...entry(), archiveSha256: "0".repeat(64) }], [{ ...entry(), compressedBytes: 0 }]]) {
    await assert.rejects(verify(entries, () => { throw new Error("unexpected fetch"); }), /requires|invalid/);
  }
});

test("rejects one URL claiming conflicting identities", async () => {
  await assert.rejects(verify([entry(), { ...entry(), compressedBytes: 1 }], () => { throw new Error("unexpected fetch"); }), /conflicting/);
});

const zipPrefix = Buffer.concat([Buffer.from([0x50, 0x4b, 3, 4]), Buffer.alloc(4092, 1)]);
const largeEntry = () => ({ ...entry(), compressedBytes: 1024 ** 3 });
const checkAvailability = (entries, fetchImpl) => verifyPublishedCapabilityAssets({ catalog: { entries }, fetchImpl, availabilityOnly: true });

function streamedResponse(chunks, options = {}) {
  let reads = 0;
  let cancelled = false;
  const response = new Response(new ReadableStream({
    pull(controller) {
      reads += 1;
      if (chunks.length) controller.enqueue(chunks.shift());
      else controller.close();
    },
    cancel() { cancelled = true; },
  }, { highWaterMark: 0 }), options);
  return { response, reads: () => reads, cancelled: () => cancelled };
}

test("availability ignores a large 200 body after the requested prefix and cancels it", async () => {
  const value = largeEntry();
  const stream = streamedResponse([zipPrefix, Buffer.alloc(4096), Buffer.alloc(4096)], { headers: { "Content-Length": String(value.compressedBytes) } });
  let requestedSignal;
  await checkAvailability([value, value], async (_url, options) => {
    assert.equal(options.headers.Range, "bytes=0-4095");
    assert.equal(options.headers.Authorization, undefined);
    requestedSignal = options.signal;
    return stream.response;
  });
  assert.equal(stream.reads(), 1, "the rest of the ignored Range body must not be consumed");
  assert.equal(stream.cancelled(), true);
  assert.equal(requestedSignal.aborted, true, "abort also closes transport resources");
});

test("availability accepts exact 206 ranges and rejects malformed range or byte lengths", async () => {
  const value = largeEntry();
  const headers = { "Content-Range": `bytes 0-4095/${value.compressedBytes}`, "Content-Length": "4096" };
  await checkAvailability([value], async () => new Response(zipPrefix, { status: 206, headers }));
  for (const change of [
    { "Content-Range": "bytes 1-4096/1073741824" },
    { "Content-Range": "bytes 0-4095/10000" },
    { "Content-Range": "bytes 0-4095/*" },
    { "Content-Length": "1073741824" },
  ]) {
    await assert.rejects(checkAvailability([value], async () => new Response(zipPrefix, { status: 206, headers: { ...headers, ...change } })), /Content-Range mismatch|size mismatch/u);
  }
  await assert.rejects(checkAvailability([value], async () => new Response(zipPrefix, { status: 206 })), /Content-Range mismatch/u);
  for (const prefix of [zipPrefix.subarray(1), Buffer.concat([zipPrefix, Buffer.from("extra")])]) {
    await assert.rejects(checkAvailability([value], async () => new Response(prefix, { status: 206, headers })), /size mismatch/u);
  }
});

for (const status of [403, 404]) test(`availability rejects HTTP ${status}`, async () => {
  await assert.rejects(checkAvailability([largeEntry()], async () => new Response("no access", { status })), /not publicly available/u);
});

test("availability rejects HTML login pages, non-ZIP program bodies and empty responses", async () => {
  for (const body of [Buffer.from("<!DOCTYPE html><html>login</html>".padEnd(4096)), Buffer.alloc(4096, 1), new Uint8Array()]) {
    await assert.rejects(checkAvailability([largeEntry()], async () => new Response(body)), /HTML|not a ZIP|size mismatch/u);
  }
  await assert.rejects(checkAvailability([largeEntry()], async () => new Response(zipPrefix, { headers: { "Content-Type": "text/html; charset=utf-8" } })), /HTML/u);
});

test("availability verifies the entire digest for files smaller than the prefix", async () => {
  const small = zipPrefix.subarray(0, 128);
  const value = { ...entry(), compressedBytes: small.length, archiveSha256: createHash("sha256").update(small).digest("hex") };
  await checkAvailability([value], async (_url, options) => {
    assert.equal(options.headers.Range, "bytes=0-127");
    return new Response(small, { status: 206, headers: { "Content-Range": "bytes 0-127/128" } });
  });
  await assert.rejects(checkAvailability([{ ...value, archiveSha256: "a".repeat(64) }], async () => new Response(small)), /SHA-256 mismatch/u);
});

test("availability models use mirror fallback and do not require ZIP magic", async () => {
  const value = largeEntry();
  value.modelFiles = [{ path: "models/model.bin", bytes: value.compressedBytes, sha256: sha, urls: ["https://cdn.llmwiki.cn/model.bin", "https://mirror.llmwiki.cn/model.bin"] }];
  const calls = [];
  assert.deepEqual(await checkAvailability([value, value], async (url) => {
    calls.push(url);
    const body = url.endsWith(".zip") ? zipPrefix : Buffer.alloc(4096, 2);
    return new Response(body, { status: url === value.modelFiles[0].urls[0] ? 404 : 200 });
  }), { assetCount: 2 });
  assert.equal(calls.length, 3);
  value.modelFiles[0].urls = [value.modelFiles[0].urls[0]];
  await assert.rejects(checkAvailability([value], async (url) => new Response(url.endsWith(".zip") ? zipPrefix : "<html>login</html>".padEnd(4096))), /HTML/u);
});

test("availability rejects insecure redirects and mismatched total size", async () => {
  const response = new Response(zipPrefix);
  Object.defineProperty(response, "url", { value: "http://cdn.llmwiki.cn/redirected.zip" });
  await assert.rejects(checkAvailability([largeEntry()], async () => response), /public HTTPS/u);
  await assert.rejects(checkAvailability([largeEntry()], async () => new Response(zipPrefix, { headers: { "Content-Length": "4096" } })), /size mismatch/u);
});

test("availability follows HTTPS redirects manually and blocks HTTP before requesting it", async () => {
  const calls = [];
  await checkAvailability([largeEntry()], async (url, options) => {
    assert.equal(options.redirect, "manual");
    calls.push(url);
    if (calls.length === 1) return new Response(null, { status: 302, headers: { Location: "https://storage.llmwiki.cn/archive.zip?signature=temporary" } });
    return new Response(zipPrefix);
  });
  assert.equal(calls.length, 2);
  let insecureCalls = 0;
  await assert.rejects(checkAvailability([largeEntry()], async () => {
    insecureCalls += 1;
    return new Response(null, { status: 302, headers: { Location: "http://storage.llmwiki.cn/archive.zip" } });
  }), /public HTTPS/u);
  assert.equal(insecureCalls, 1);
});

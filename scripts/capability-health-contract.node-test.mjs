import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { restrictedEnvironment as mediaRuntimeEnvironment } from "../capabilities/media-runtime/runner/core.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const runnerByPack = {
  "asr-sensevoice-small": "capabilities/asr-sensevoice-small/runner/index.mjs",
  "asr-whisper": "capabilities/asr-whisper/runner/index.mjs",
  "browser-runtime": "capabilities/browser-runtime/runner/index.mjs",
  "browser-runtime-lite": "capabilities/browser-runtime-lite/runner/index.mjs",
  "document-layout": "capabilities/document-layout/runner/docling_pack.py",
  "document-standard": "capabilities/document-standard/runner/markitdown_pack.py",
  "media-metadata": "capabilities/media-metadata/runner/index.mjs",
  "media-runtime": "capabilities/media-runtime/runner/index.mjs",
  "ocr-basic": "capabilities/ocr-cjk-accurate/runner/index.py",
  "ocr-cjk-accurate": "capabilities/ocr-cjk-accurate/runner/index.py",
  "office-legacy": "capabilities/office-legacy/runner/office_legacy_pack.py",
};
const product = JSON.parse(fs.readFileSync(path.join(root, "capabilities/product-manifest.json"), "utf8"));
const published = product.definitions.filter((definition) => definition.distributionTier === "published");

test("every Batch 3A published capability implements the Batch 3B health protocol", () => {
  for (const { capabilityId, routes } of published) {
    const relativePath = runnerByPack[capabilityId];
    assert.ok(relativePath, `${capabilityId} has no formal runner mapping`);
    const source = fs.readFileSync(path.join(root, relativePath), "utf8");
    assert.match(source, /capability\.health/u, `${capabilityId} has no health method`);
    assert.ok(source.includes(capabilityId), `${capabilityId} does not bind its health identity`);
    for (const route of routes) assert.ok(source.includes(route), `${capabilityId} does not bind ${route}`);
    assert.match(source, /healthy/u, `${capabilityId} does not return a readiness result`);
    assert.match(source, /protocolVersion/u, `${capabilityId} does not bind protocol v2`);
  }
});

test("qualification runners preserve numeric JSON-RPC request IDs", () => {
  for (const relativePath of [
    "capabilities/document-standard/runner/markitdown_pack.py",
    "capabilities/document-layout/runner/docling_pack.py",
    "capabilities/office-legacy/runner/office_legacy_pack.py",
  ]) {
    const source = fs.readFileSync(path.join(root, relativePath), "utf8");
    assert.match(source, /request_id\s*=\s*request\.get\("id"\)/u, `${relativePath} does not preserve the request ID`);
    assert.doesNotMatch(source, /request_id\s*=\s*str\(/u, `${relativePath} coerces the request ID to text`);
  }
  const mediaRunner = fs.readFileSync(path.join(root, "capabilities/media-runtime/runner/index.mjs"), "utf8");
  assert.match(mediaRunner, /rpc\s*=\s*await readRpc\(\)/u);
  assert.doesNotMatch(mediaRunner, /readFile\(0,/u);
});

test("document-standard forces UTF-8 JSON-RPC streams for Windows CJK paths", () => {
  const source = fs.readFileSync(path.join(root, "capabilities/document-standard/runner/markitdown_pack.py"), "utf8");
  assert.match(source, /sys\.stdin\.reconfigure\(encoding="utf-8", errors="strict"\)/u);
  assert.match(source, /sys\.stdout\.reconfigure\(encoding="utf-8", errors="strict"\)/u);
});

test("media-runtime resolves the payload FFmpeg shared libraries", () => {
  const packRoot = path.join("payload", "media-runtime");
  assert.equal(
    mediaRuntimeEnvironment(packRoot, "linux", {}).LD_LIBRARY_PATH,
    path.join(packRoot, "runtime", "ffmpeg", "lib"),
  );
  assert.equal(
    mediaRuntimeEnvironment(packRoot, "darwin", {}).DYLD_LIBRARY_PATH,
    path.join(packRoot, "runtime", "ffmpeg", "lib"),
  );
  assert.match(mediaRuntimeEnvironment(packRoot, "win32", { PATH: "system" }).PATH, /ffmpeg.*system/u);
});

test("release producer and installer keep the same bounded file-count budget", () => {
  const releaseSource = fs.readFileSync(path.join(root, "src-tauri/src/bin/capability_release.rs"), "utf8");
  const installerSource = fs.readFileSync(path.join(root, "src-tauri/src/services/import_v2/capability_installer.rs"), "utf8");
  const packSource = fs.readFileSync(path.join(root, "src-tauri/src/services/import_v2/capability_pack.rs"), "utf8");
  const limit = (source) => Number(source.match(/const MAX_ARCHIVE_FILES: usize = ([\d_]+);/u)?.[1].replaceAll("_", ""));
  const runtimeLimit = Number(packSource.match(/const MAX_RUNTIME_FILES: usize = ([\d_]+);/u)?.[1].replaceAll("_", ""));
  assert.equal(limit(releaseSource), 50_000);
  assert.equal(limit(installerSource), limit(releaseSource));
  assert.equal(runtimeLimit, limit(releaseSource));
});

test("Windows whisper backport maps large media instead of buffering it in memory", () => {
  const patchSource = fs.readFileSync(
    path.join(root, "capabilities/asr-whisper/patches/whisper-v1.8.3-ffmpeg-windows.patch"),
    "utf8",
  );
  assert.match(patchSource, /CreateFileMappingW/u);
  assert.match(patchSource, /^\+\s*size_t size; \/\* size left in the buffer \*\//mu);
  assert.doesNotMatch(patchSource, /^\+.*std::vector<u8> input_data/mu);
  assert.doesNotMatch(patchSource, /numeric_limits<int>/u);
});

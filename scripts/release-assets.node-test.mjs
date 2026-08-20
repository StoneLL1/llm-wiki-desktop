import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { verifyChecksums, writeChecksums } from "./generate-release-checksums.mjs";
import { nodeSbom, rustSbom } from "./generate-release-sbom.mjs";
import {
  exactReleaseAssetUrl,
  RELEASE_PLATFORMS,
} from "./release-assets-contract.mjs";
import { stageDesktopRelease } from "./stage-desktop-release.mjs";
import { verifyReleaseAssets } from "./verify-release-assets.mjs";
import { generateLatestJson, validateLatestJson } from "./verify-latest-json.mjs";
import { publishedUpdaterPairs } from "./verify-updater-signatures.mjs";
import { CAPABILITY_PACKS, CAPABILITY_TARGETS } from "./verify-capability-catalog.mjs";

const TAG = "app-v0.1.0";
const VERSION = "0.1.0";
const COMMIT = "a".repeat(40);
const RUN_ID = "1234567890";
const MINISIGN_SIGNATURE = Buffer.from(
  "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==\n",
).toString("base64");
const SIGNING_KINDS = {
  "windows-x86_64": "windows-authenticode",
  "darwin-aarch64": "apple-developer-id-notarized",
  "darwin-x86_64": "apple-developer-id-notarized",
  "linux-x86_64": "linux-updater-signature",
};

const writeJson = (file, value) => {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
};

const releaseEntry = (capabilityId, targetTriple) => ({
  capabilityId,
  targetTriple,
  version: "1.2.3",
  url: exactReleaseAssetUrl(TAG, `${capabilityId}-1.2.3-${targetTriple}.zip`),
  archiveSha256: crypto.createHash("sha256").update("zip").digest("hex"),
  manifestSha256: "c".repeat(64),
  compressedBytes: 3,
  installedBytes: 2345,
  modelBytes: ["asr-sensevoice-small", "ocr-cjk-accurate"].includes(capabilityId) ? 640 : null,
  license: "MIT",
});

function createReleaseBundle(root) {
  const descriptors = [];
  for (const [platform, targetTriple] of Object.entries(RELEASE_PLATFORMS)) {
    const directory = path.join(root, "desktop", platform);
    fs.mkdirSync(directory, { recursive: true });
    const installer = `${platform}-installer.bin`;
    const updater = `${platform}-updater.tar.gz`;
    const signatureFile = `${updater}.sig`;
    fs.writeFileSync(path.join(directory, installer), `installer-${platform}`);
    fs.writeFileSync(path.join(directory, updater), "test");
    fs.writeFileSync(path.join(directory, signatureFile), MINISIGN_SIGNATURE);
    const descriptor = {
      schemaVersion: 1,
      releaseTag: TAG,
      version: VERSION,
      commitSha: COMMIT,
      platform,
      targetTriple,
      installer: { file: installer },
      updater: { file: updater, signatureFile, signature: MINISIGN_SIGNATURE },
      osSigning: { kind: SIGNING_KINDS[platform], verified: true },
    };
    writeJson(path.join(directory, "release-entry.json"), descriptor);
    descriptors.push(descriptor);
  }

  const entries = CAPABILITY_TARGETS.flatMap((target) => CAPABILITY_PACKS.map((pack) => releaseEntry(pack, target)));
  const capabilityRoot = path.join(root, "capabilities");
  fs.mkdirSync(capabilityRoot, { recursive: true });
  for (const entry of entries) fs.writeFileSync(path.join(capabilityRoot, path.basename(new URL(entry.url).pathname)), "zip");
  writeJson(path.join(capabilityRoot, "install-catalog.json"), { schemaVersion: 1, entries });
  writeJson(path.join(capabilityRoot, "trusted-keys.json"), { release: "d".repeat(64) });
  writeJson(path.join(capabilityRoot, "catalog-provenance.json"), {
    schemaVersion: 1, releaseTag: TAG, commitSha: COMMIT, workflowRunId: RUN_ID,
  });

  const latest = generateLatestJson({
    descriptors,
    tag: TAG,
    version: VERSION,
    notes: "Release notes",
    pubDate: "2026-08-21T00:00:00.000Z",
  });
  writeJson(path.join(root, "latest.json"), latest);
  writeJson(path.join(root, "provenance", "release-provenance.json"), {
    schemaVersion: 1,
    repository: "StoneLL1/llm-wiki-desktop",
    releaseTag: TAG,
    commitSha: COMMIT,
    workflowRunId: RUN_ID,
    capabilityWorkflowRunId: RUN_ID,
  });
  fs.writeFileSync(path.join(root, "provenance", "github-attestation.jsonl"), "{\"fixture\":true}\n");
  writeJson(path.join(root, "smoke", "packaged-smoke-summary.json"), {
    schemaVersion: 1,
    fixtureEndpointProductionUntouched: true,
    platforms: Object.keys(RELEASE_PLATFORMS).map((platform) => ({
      platform,
      status: "passed",
      journeys: ["install-launch", "packaged-process-alive", "updater-fixture-manifest-verified"],
    })),
  });
  writeJson(path.join(root, "sbom", "node.cdx.json"), { bomFormat: "CycloneDX", specVersion: "1.5" });
  writeJson(path.join(root, "sbom", "rust.cdx.json"), { bomFormat: "CycloneDX", specVersion: "1.5" });
  fs.writeFileSync(path.join(root, "release-notes.md"), "# Release notes 0.1.0\n");
  fs.writeFileSync(path.join(root, "known-limitations.md"), "# Known limitations 0.1.0\n");
  writeChecksums(root, path.join(root, "CHECKSUMS.sha256"));
}

const fixture = (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-release-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  createReleaseBundle(root);
  return root;
};

test("the complete four-platform release transaction passes the local rehearsal", (context) => {
  const root = fixture(context);
  assert.deepEqual(verifyReleaseAssets({
    root, tag: TAG, version: VERSION, commitSha: COMMIT, workflowRunId: RUN_ID,
  }).errors, []);
});

test("release verification rejects coordinate drift, incomplete smoke, and tampering", (context) => {
  const root = fixture(context);
  const provenancePath = path.join(root, "provenance", "release-provenance.json");
  writeJson(provenancePath, { ...JSON.parse(fs.readFileSync(provenancePath)), workflowRunId: "999" });
  const smokePath = path.join(root, "smoke", "packaged-smoke-summary.json");
  const smoke = JSON.parse(fs.readFileSync(smokePath));
  smoke.platforms[0].journeys = ["install-launch"];
  writeJson(smokePath, smoke);
  fs.writeFileSync(path.join(root, "desktop", "linux-x86_64", "linux-x86_64-updater.tar.gz"), "tampered");
  const errors = verifyReleaseAssets({
    root, tag: TAG, version: VERSION, commitSha: COMMIT, workflowRunId: RUN_ID,
  }).errors;
  assert.equal(errors.some((error) => error.includes("same workflow run")), true);
  assert.equal(errors.some((error) => error.includes("smoke is incomplete")), true);
  assert.equal(errors.some((error) => error.includes("CHECKSUMS")), true);
});

test("latest.json rejects mutable, cross-tag, incomplete, and missing asset entries", () => {
  const descriptors = Object.entries(RELEASE_PLATFORMS).map(([platform, targetTriple]) => ({
    platform,
    targetTriple,
    releaseTag: TAG,
    version: VERSION,
    updater: { file: `${platform}.tar.gz`, signature: "s".repeat(64) },
  }));
  const manifest = generateLatestJson({
    descriptors, tag: TAG, version: VERSION, notes: "notes", pubDate: "2026-08-21T00:00:00Z",
  });
  assert.deepEqual(validateLatestJson({
    manifest,
    tag: TAG,
    version: VERSION,
    assetFiles: new Set(descriptors.map(({ updater }) => updater.file)),
  }).errors, []);
  const changed = structuredClone(manifest);
  changed.platforms["linux-x86_64"].url = "https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/linux.tar.gz";
  delete changed.platforms["darwin-x86_64"];
  const errors = validateLatestJson({ manifest: changed, tag: TAG, version: VERSION, assetFiles: new Set() }).errors;
  assert.equal(errors.some((error) => error.includes("exactly the four")), true);
  assert.equal(errors.some((error) => error.includes("exact tag")), true);
  assert.equal(errors.some((error) => error.includes("missing from the release bundle")), true);
});

test("checksums use the exact flat publish contract and reject duplicate basenames", (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-checksums-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, "nested"));
  fs.writeFileSync(path.join(root, "z.txt"), "z");
  fs.writeFileSync(path.join(root, "nested", "a.txt"), "a");
  const output = path.join(root, "CHECKSUMS.sha256");
  const first = writeChecksums(root, output);
  const second = writeChecksums(root, output);
  assert.equal(first, second);
  assert.equal(first.includes("CHECKSUMS.sha256"), false);
  assert.match(first, / {2}a\.txt/);
  assert.equal(first.includes("nested/a.txt"), false);
  assert.equal(verifyChecksums(root, output), first);
  fs.writeFileSync(path.join(root, "a.txt"), "duplicate");
  assert.throws(() => writeChecksums(root, output), /duplicate public release asset name/);
});

test("desktop staging selects one canonical updater and requires OS-signing evidence", (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-stage-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const source = path.join(root, "source");
  const output = path.join(root, "output");
  fs.mkdirSync(source);
  fs.writeFileSync(path.join(source, "app-setup.exe"), "installer");
  fs.writeFileSync(path.join(source, "app-setup.exe.sig"), MINISIGN_SIGNATURE);
  const evidence = path.join(root, "evidence.json");
  writeJson(evidence, { kind: "windows-authenticode", verified: true, subject: "CN=Fixture" });
  const descriptor = stageDesktopRelease({
    source, output, platform: "windows-x86_64", releaseTag: TAG, version: VERSION, commitSha: COMMIT, signingEvidence: evidence,
  });
  assert.equal(descriptor.updater.file.startsWith("windows-x86_64-"), true);
  assert.equal(descriptor.installer.file, descriptor.updater.file);
  writeJson(evidence, { kind: "windows-authenticode", verified: false });
  assert.throws(() => stageDesktopRelease({
    source, output, platform: "windows-x86_64", releaseTag: TAG, version: VERSION, commitSha: COMMIT, signingEvidence: evidence,
  }), /verified structured record/);
});

test("flat reverse-download verification resolves all four latest.json updater pairs", (context) => {
  const root = fixture(context);
  const published = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-published-"));
  context.after(() => fs.rmSync(published, { recursive: true, force: true }));
  const manifestPath = path.join(root, "latest.json");
  for (const platform of Object.keys(RELEASE_PLATFORMS)) {
    const directory = path.join(root, "desktop", platform);
    const descriptor = JSON.parse(fs.readFileSync(path.join(directory, "release-entry.json"), "utf8"));
    for (const name of [descriptor.updater.file, descriptor.updater.signatureFile]) {
      fs.copyFileSync(path.join(directory, name), path.join(published, name));
    }
  }
  const pairs = publishedUpdaterPairs(published, manifestPath);
  assert.equal(pairs.length, 8);
  assert.equal(pairs.every((file) => fs.existsSync(file)), true);
});

test("SBOM generation is deterministic and derives components only from locked inputs", (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-sbom-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const packageLock = path.join(root, "package-lock.json");
  const cargoLock = path.join(root, "Cargo.lock");
  writeJson(packageLock, {
    lockfileVersion: 3,
    packages: { "": { name: "app", version: "1.0.0" }, "node_modules/z": { version: "2.0.0" } },
  });
  fs.writeFileSync(cargoLock, "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n");
  assert.deepEqual(nodeSbom(packageLock).components.map(({ name }) => name), ["z"]);
  assert.deepEqual(rustSbom(cargoLock).components.map(({ name }) => name), ["serde"]);
  assert.equal(JSON.stringify(nodeSbom(packageLock)), JSON.stringify(nodeSbom(packageLock)));
});

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { writeChecksums } from "./generate-release-checksums.mjs";
import { OS_IDENTITY_EVIDENCE, RELEASE_PLATFORMS } from "./release-assets-contract.mjs";
import { PRERELEASE_PLATFORMS, verifyPrereleaseAssets } from "./verify-prerelease-assets.mjs";

const TAG = "app-v0.1.0-rc.1";
const VERSION = "0.1.0-rc.1";
const COMMIT = "a".repeat(40);
const RUN_ID = "123456789";
const SIGNATURE = "s".repeat(80);

const writeJson = (file, value) => {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
};

function createFixture(context) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-prerelease-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const platform of PRERELEASE_PLATFORMS) {
    const directory = path.join(root, "desktop", platform);
    fs.mkdirSync(directory, { recursive: true });
    const installer = `${platform}-installer.bin`;
    const updater = `${platform}-updater.tar.gz`;
    const signatureFile = `${updater}.sig`;
    fs.writeFileSync(path.join(directory, installer), "installer");
    fs.writeFileSync(path.join(directory, updater), "updater");
    fs.writeFileSync(path.join(directory, signatureFile), SIGNATURE);
    writeJson(path.join(directory, "release-entry.json"), {
      schemaVersion: 1,
      releaseTag: TAG,
      version: VERSION,
      commitSha: COMMIT,
      platform,
      targetTriple: RELEASE_PLATFORMS[platform],
      installer: { file: installer },
      updater: { file: updater, signatureFile, signature: SIGNATURE },
      osSigning: OS_IDENTITY_EVIDENCE[platform],
    });
  }
  writeJson(path.join(root, "provenance", "release-provenance.json"), {
    schemaVersion: 1,
    repository: "StoneLL1/llm-wiki-desktop",
    releaseTag: TAG,
    version: VERSION,
    commitSha: COMMIT,
    workflowRunId: RUN_ID,
    channel: "prerelease",
    platforms: PRERELEASE_PLATFORMS,
  });
  fs.writeFileSync(path.join(root, "provenance", "github-attestation.jsonl"), "{}\n");
  writeJson(path.join(root, "smoke", "packaged-smoke-summary.json"), {
    schemaVersion: 1,
    fixtureEndpointProductionUntouched: true,
    platforms: PRERELEASE_PLATFORMS.map((platform) => ({
      platform,
      status: "passed",
      journeys: ["install-launch", "packaged-process-alive"],
      productionEndpointAccessed: false,
    })),
  });
  writeJson(path.join(root, "sbom", "node.cdx.json"), { bomFormat: "CycloneDX" });
  writeJson(path.join(root, "sbom", "rust.cdx.json"), { bomFormat: "CycloneDX" });
  fs.writeFileSync(path.join(root, "release-notes.md"), `# LLM Wiki Desktop ${VERSION}\n`);
  fs.writeFileSync(path.join(root, "known-limitations.md"), `# Known limitations for ${VERSION}\n`);
  writeChecksums(root, path.join(root, "CHECKSUMS.sha256"));
  return root;
}

const verify = (root) => verifyPrereleaseAssets({
  root,
  tag: TAG,
  version: VERSION,
  commitSha: COMMIT,
  workflowRunId: RUN_ID,
}).errors;

test("Windows and both macOS prerelease artifacts pass without Linux or latest.json", (context) => {
  const root = createFixture(context);
  assert.deepEqual(verify(root), []);
});

test("prerelease verification rejects stable-channel and Linux leakage", (context) => {
  const root = createFixture(context);
  fs.writeFileSync(path.join(root, "latest.json"), "{}\n");
  fs.mkdirSync(path.join(root, "desktop", "linux-x86_64"));
  const errors = verify(root);
  assert.equal(errors.some((error) => error.includes("latest.json")), true);
  assert.equal(errors.some((error) => error.includes("Linux desktop")), true);
  assert.equal(errors.some((error) => error.includes("exactly")), true);
});

test("prerelease verification rejects coordinate and smoke drift", (context) => {
  const root = createFixture(context);
  const descriptorPath = path.join(root, "desktop", "windows-x86_64", "release-entry.json");
  writeJson(descriptorPath, { ...JSON.parse(fs.readFileSync(descriptorPath, "utf8")), version: "0.1.0" });
  const smokePath = path.join(root, "smoke", "packaged-smoke-summary.json");
  const smoke = JSON.parse(fs.readFileSync(smokePath, "utf8"));
  smoke.platforms[0].status = "failed";
  writeJson(smokePath, smoke);
  const errors = verify(root);
  assert.equal(errors.some((error) => error.includes("coordinates")), true);
  assert.equal(errors.some((error) => error.includes("smoke")), true);
  assert.equal(errors.some((error) => error.includes("CHECKSUMS")), true);
});

test("prerelease verification rejects stale release-note versions", (context) => {
  const root = createFixture(context);
  fs.writeFileSync(path.join(root, "release-notes.md"), "# LLM Wiki Desktop 0.1.0-rc.0\n");
  writeChecksums(root, path.join(root, "CHECKSUMS.sha256"));
  assert.equal(verify(root).some((error) => error.includes("heading")), true);
});

import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  evaluateFinalFourRedlines,
  expectedRedlineStates,
  repositoryRoot,
} from "./check-final-four-redlines.mjs";

test("each final-four release blocker has a deterministic owner and expected state", () => {
  const declared = expectedRedlineStates(repositoryRoot);
  assert.equal(declared.length, 7);
  assert.equal(new Set(declared.map(({ id }) => id)).size, declared.length);
  for (const item of declared) {
    assert.match(item.id, /^[a-z0-9-]+$/);
    assert.match(item.ownerBatch, /^(?:1|2A|2B|3A|4A|4B|5)$/);
    assert.match(item.expected, /^(?:red|green)$/);
  }
});

test("quarantined redlines cannot disappear or turn green without updating their owner declaration", () => {
  const actual = evaluateFinalFourRedlines(repositoryRoot).map(({ id, state, ownerBatch }) => ({ id, state, ownerBatch }));
  const declared = expectedRedlineStates(repositoryRoot).map(({ id, expected: state, ownerBatch }) => ({ id, state, ownerBatch }));
  assert.deepEqual(actual, declared);
});

test("Batch 0 starts all seven strict contracts red without using test skip", () => {
  const actual = evaluateFinalFourRedlines(repositoryRoot);
  assert.deepEqual(actual.map(({ state }) => state), Array(7).fill("red"));
  assert.equal(actual.every(({ detail }) => detail.length > 30), true);
});

test("the strict checker can turn every owned contract green", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-final-four-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  const targets = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
  ];
  const packs = [
    "asr-sensevoice-small",
    "browser-runtime",
    "browser-runtime-lite",
    "media-metadata",
    "ocr-cjk-accurate",
  ];
  await write("capabilities/install-catalog.json", JSON.stringify({
    schemaVersion: 1,
    entries: targets.flatMap((targetTriple) => packs.map((capabilityId) => ({
      targetTriple,
      capabilityId,
      version: "1.0.0",
      url: `https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v1.0.0/${capabilityId}-${targetTriple}.zip`,
      archiveSha256: "a".repeat(64),
      manifestSha256: "b".repeat(64),
      compressedBytes: 1,
      installedBytes: 2,
      license: "MIT",
    }))),
  }));
  await write("capabilities/trusted-keys.json", JSON.stringify({ release: "c".repeat(64) }));
  await write("src-tauri/Cargo.toml", "tauri-plugin-updater = \"2\"\n");
  await write("src-tauri/tauri.conf.json", JSON.stringify({
    bundle: { createUpdaterArtifacts: true },
    plugins: {
      updater: {
        pubkey: "d".repeat(64),
        endpoints: ["https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json"],
      },
    },
  }));
  await write("src-tauri/src/lib.rs", "tauri_plugin_updater::Builder::new();\n");
  await write("src/features/settings/UpdateSettings.tsx", "invoke(\"check_app_update\");\n");
  await write("src/stores/updateStore.ts", "export const latestVersion = 'available';\n");
  await write("src/features/update/useUpdateController.ts", "invoke('check_app_update');\n");
  await write("src/features/update/UpdateController.test.tsx", [
    "test('no project still checks globally', () => {});",
    "test('available offer is visible', () => {});",
    "test('downloading state is visible', () => {});",
    "test('cancel returns to available', () => {});",
    "test('ready to install is visible', () => {});",
    "test('installing state is visible', () => {});",
  ].join("\n"));
  await write("src-tauri/tests/updater_contracts.rs", [
    "#[test] fn rejects_bad_signature() {}",
    "#[test] fn rejects_bad_manifest() {}",
    "#[test] fn handles_timeout() {}",
    "#[test] fn handles_cancel() {}",
    "#[test] fn ignores_same_version() {}",
    "#[test] fn rejects_downgrade() {}",
  ].join("\n"));
  await write("src/lib/backendError.ts", "export interface NormalizedBackendError {}\nexport function normalizeBackendError() {}\n");
  await write("src/test/backend-error-presentation.test.tsx", [
    "test('serialized BackendError is normalized', () => {});",
    "test('unknown object never renders Object object', () => {});",
    "test('circular input is safe', () => {});",
    "test('redacts Authorization api key and cookie', () => {});",
    "test('presents a zh-CN message', () => {});",
    "test('presents an English message', () => {});",
  ].join("\n"));
  await write("src-tauri/src/services/llm_service.rs", [
    "struct ProviderCredentialBinding { canonical_origin: String, credential_account_id: String }",
    "redirect(Policy::none())",
  ].join("\n"));
  await write("src-tauri/tests/provider_secret_origin_contracts.rs", [
    "#[test] fn attacker_request_count_stays_zero() {}",
    "#[test] fn redirect_never_carries_secret() {}",
    "#[test] fn rejects_0_0_0_0() {}",
    "#[test] fn rejects_169_254_169_254() {}",
    "#[test] fn legacy_credentials_are_untrusted() {}",
  ].join("\n"));
  await write("release/command-authority-inventory.json", JSON.stringify({
    commands: [{
      name: "save",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
    }],
  }));
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit;\n");
  await write("src-tauri/src/commands/file_commands.rs", "#[tauri::command]\npub fn save() {}\n");
  await write(".github/workflows/desktop-release.yml", [
    "jobs:",
    "  preflight:",
    "  capability-build:",
    "  desktop-build:",
    "  manifest-and-provenance:",
    "    steps: [latest.json]",
    "  packaged-smoke:",
    "  publish-stable:",
    "    needs: [capability-build, desktop-build, manifest-and-provenance, packaged-smoke]",
    "    environment: desktop-release",
    "    permissions:",
    "      contents: write",
  ].join("\n"));
  await write(".github/workflows/capability-release.yml", "permissions:\n  contents: read\n");
  await write("scripts/verify-release-assets.mjs", "export const verifyReleaseAssets = true;\n");
  await write("scripts/verify-latest-json.mjs", "export const verifyLatestJson = true;\n");

  assert.deepEqual(
    evaluateFinalFourRedlines(root).map(({ state }) => state),
    Array(7).fill("green"),
  );
});

test("strict structural contracts reject representative near misses", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-final-four-near-miss-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };

  const duplicatedEntry = {
    targetTriple: "x86_64-pc-windows-msvc",
    capabilityId: "browser-runtime",
    url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v1.0.0/browser.zip",
    archiveSha256: "a".repeat(64),
    manifestSha256: "b".repeat(64),
    compressedBytes: 1,
    installedBytes: 1,
    license: "MIT",
  };
  await write("capabilities/install-catalog.json", JSON.stringify({
    schemaVersion: 1,
    entries: Array(20).fill(duplicatedEntry),
  }));
  await write("capabilities/trusted-keys.json", JSON.stringify({ release: "c".repeat(64) }));
  await write("src-tauri/src/commands/file_commands.rs", [
    "#[tauri::command]",
    "pub fn save() {}",
    "#[tauri::command]",
    "pub fn remove() {}",
  ].join("\n"));
  await write("release/command-authority-inventory.json", JSON.stringify({
    commands: [{
      name: "save",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
    }],
  }));
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit;\n");
  await write(".github/workflows/desktop-release.yml", [
    "jobs:",
    "  preflight:",
    "  capability-build:",
    "  desktop-build:",
    "  manifest-and-provenance:",
    "    steps: [latest.json]",
    "  packaged-smoke:",
    "  publish-stable:",
    "    needs: [capability-build, desktop-build, manifest-and-provenance, packaged-smoke]",
    "    environment: desktop-release",
    "    permissions:",
    "      contents: write",
    "  second-writer:",
    "    permissions:",
    "      contents: write",
  ].join("\n"));
  await write("scripts/verify-release-assets.mjs", "export {};\n");
  await write("scripts/verify-latest-json.mjs", "export {};\n");

  const states = new Map(evaluateFinalFourRedlines(root).map(({ id, state }) => [id, state]));
  assert.equal(states.get("capability-release-catalog"), "red");
  assert.equal(states.get("mutation-write-authority-inventory"), "red");
  assert.equal(states.get("atomic-stable-release-workflow"), "red");
});

test("comments and token lists cannot satisfy behavioral redlines", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-final-four-comments-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };

  await write("src-tauri/Cargo.toml", "tauri-plugin-updater = '2'\n");
  await write("src-tauri/tauri.conf.json", JSON.stringify({
    bundle: { createUpdaterArtifacts: true },
    plugins: {
      updater: {
        pubkey: "d".repeat(64),
        endpoints: ["https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json"],
      },
    },
  }));
  await write("src-tauri/src/lib.rs", "tauri_plugin_updater::Builder::new();\n");
  await write("src-tauri/tests/updater_contracts.rs", "// bad_signature bad_manifest timeout cancel same_version downgrade\n");
  await write("src/features/settings/UpdateSettings.tsx", "check_app_update\n");
  await write("src/stores/updateStore.ts", "latestVersion available\n");
  await write("src/features/update/useUpdateController.ts", "check_app_update\n");
  await write("src/features/update/UpdateController.test.tsx", "// no project available downloading cancel ready to install installing\n");
  await write("src/lib/backendError.ts", "NormalizedBackendError normalizeBackendError\n");
  await write("src/test/backend-error-presentation.test.tsx", "// serialized BackendError Object object circular Authorization api key cookie zh-CN English\n");
  await write("src-tauri/src/services/llm_service.rs", [
    "ProviderCredentialBinding canonical_origin credential_account_id",
    "redirect(Policy::none())",
  ].join("\n"));
  await write("src-tauri/tests/provider_secret_origin_contracts.rs", "// attacker request count redirect secret 0_0_0_0 169_254_169_254 legacy untrusted\n");

  const states = new Map(evaluateFinalFourRedlines(root).map(({ id, state }) => [id, state]));
  assert.equal(states.get("signed-updater-foundation"), "red");
  assert.equal(states.get("real-update-offer"), "red");
  assert.equal(states.get("structured-backend-error-presentation"), "red");
  assert.equal(states.get("provider-secret-origin-binding"), "red");
});

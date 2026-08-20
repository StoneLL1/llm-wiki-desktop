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

test("completed owner contracts turn green while later batches remain red", () => {
  const actual = evaluateFinalFourRedlines(repositoryRoot);
  const completed = new Set([
    "structured-backend-error-presentation",
    "provider-secret-origin-binding",
    "mutation-write-authority-inventory",
    "signed-updater-foundation",
    "real-update-offer",
    "atomic-stable-release-workflow",
  ]);
  assert.equal(
    actual.every(({ id, state }) => state === (completed.has(id) ? "green" : "red")),
    true,
  );
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
  await write("src-tauri/build.rs", [
    "const CATALOG_MODE_ENV: &str = \"LLM_WIKI_CAPABILITY_CATALOG_MODE\";",
    "const STAGING_DIR_ENV: &str = \"LLM_WIKI_CAPABILITY_STAGING_DIR\";",
  ].join("\n"));
  await write(
    "src-tauri/src/services/import_v2/capability_embed.rs",
    "// release builds cannot embed an empty capability catalog\n",
  );
  await write("scripts/verify-capability-catalog.mjs", "export function verifyCapabilityCatalog() {}\n");
  await write("scripts/verify-embedded-capability-catalog.mjs", "export function verifyEmbeddedCapabilityCatalog() {}\n");
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
  await write("src/features/settings/UpdateSettings.tsx", "ActionableErrorNotice useUpdateStore;\n");
  await write("src/services/updateApi.ts", "invoke<AppUpdateState>(\"check_app_update\");\n");
  await write("src/stores/updateStore.ts", "checkAppUpdate latestVersion available\n");
  await write("src/features/update/useUpdateController.ts", "useUpdateStore checkNow\n");
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
  await write(
    "src/lib/backendError.ts",
    await fs.readFile(path.join(repositoryRoot, "src/lib/backendError.ts"), "utf8"),
  );
  await write(
    "src/components/app/ActionableErrorNotice.tsx",
    await fs.readFile(path.join(repositoryRoot, "src/components/app/ActionableErrorNotice.tsx"), "utf8"),
  );
  await write(
    "src/components/app/LazyActionableErrorNotice.tsx",
    await fs.readFile(path.join(repositoryRoot, "src/components/app/LazyActionableErrorNotice.tsx"), "utf8"),
  );
  await write(
    "src/test/backend-error-presentation.test.tsx",
    await fs.readFile(path.join(repositoryRoot, "src/test/backend-error-presentation.test.tsx"), "utf8"),
  );
  for (const relativePath of [
    "src/features/project/NoProjectWorkspace.tsx",
    "src/stores/projectStore.ts",
    "src/features/import/ImportCapabilityDialog.tsx",
    "src/features/settings/useProviderWorkflow.ts",
    "src/features/chat/ChatView.tsx",
    "src/features/chat/PageChatPanel.tsx",
    "src/components/app/TaskLogDrawer.tsx",
    "src/hooks/useTaskLauncher.ts",
    "src/hooks/useTaskEvents.ts",
    "src/features/workflows/useWorkflowsController.ts",
    "src/features/workflows/WorkflowsRightPanel.tsx",
    "src/features/workflows/WorkflowTaskDetail.tsx",
  ]) {
    await write(relativePath, await fs.readFile(path.join(repositoryRoot, relativePath), "utf8"));
  }
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
      source: "src-tauri/src/commands/file_commands.rs",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
      authorityPaths: [{
        function: "save",
        authority: "ProjectWritePermit",
        requiredCalls: ["save_authorized(&permit"],
      }],
    }],
    serviceAuthorityContracts: [{
      source: "src-tauri/src/services/file_service.rs",
      function: "save_authorized",
      capability: "ProjectWritePermit",
      requiredCalls: ["permit.context()"],
    }],
  }));
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit;\n");
  await write(
    "src-tauri/src/services/file_service.rs",
    "fn save_authorized(permit: &ProjectWritePermit) { save_unchecked(permit.context()); }\n",
  );
  await write("src-tauri/src/commands/file_commands.rs", [
    "#[tauri::command]",
    "pub fn save() { state.with_current_project_write_access(project_id, root, |permit, _context| service.save_authorized(&permit)); }",
  ].join("\n"));
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
  await write(".github/workflows/capability-release.yml", "on:\n  workflow_call:\npermissions:\n  contents: read\n");
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

test("mutation inventory requires a real ProjectWritePermit command path", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-write-authority-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  await write("release/command-authority-inventory.json", JSON.stringify({
    commands: [{
      name: "save",
      source: "src-tauri/src/commands/file_commands.rs",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
      authorityPaths: [{
        function: "save",
        authority: "ProjectWritePermit",
        requiredCalls: ["save_authorized(&permit"],
      }],
    }],
    serviceAuthorityContracts: [{
      source: "src-tauri/src/services/file_service.rs",
      function: "save_authorized",
      capability: "ProjectWritePermit",
      requiredCalls: ["permit.context()"],
    }],
  }));
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit;\n");
  await write(
    "src-tauri/src/services/file_service.rs",
    "fn save_authorized(permit: &ProjectWritePermit) { save_unchecked(permit.context()); }\n",
  );
  const authorityState = () => evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "mutation-write-authority-inventory")?.state;

  await write("src-tauri/src/commands/file_commands.rs", [
    "#[tauri::command]",
    "pub fn save() { /* state.with_current_project_write_access(...) */ }",
  ].join("\n"));
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/commands/file_commands.rs", [
    "#[tauri::command]",
    "pub fn save() { state.with_current_project_write_access(project_id, root, |_permit, _context| Ok(())); }",
  ].join("\n"));
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/commands/file_commands.rs", [
    "#[tauri::command]",
    "pub fn save() { state.with_current_project_write_access(project_id, root, |permit, _context| service.save_authorized(&permit)); }",
  ].join("\n"));
  assert.equal(authorityState(), "green");
});

test("mutation inventory rejects a permitted launcher with a naked background worker", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-worker-authority-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  await write("release/command-authority-inventory.json", JSON.stringify({
    commands: [{
      name: "start_import",
      source: "src-tauri/src/commands/import.rs",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
      authorityPaths: [
        { function: "start_import", authority: "ProjectWritePermit" },
        {
          function: "run_worker",
          authority: "ProjectExecutionLease",
          requiredCalls: ["run_authorized"],
          forbiddenCalls: ["run_naked"],
        },
      ],
    }],
    serviceAuthorityContracts: [{
      source: "src-tauri/src/services/import.rs",
      function: "run_authorized",
      capability: "ProjectExecutionLease",
      requiredCalls: ["task_context"],
    }],
  }));
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit; struct ProjectExecutionLease;\n");
  await write("src-tauri/src/commands/import.rs", [
    "#[tauri::command]",
    "pub fn start_import() { state.with_current_project_write_access(id, root, |_permit, _context| Ok(())); }",
    "fn run_worker() { let lease = state.begin_project_external_task(context, task_id); service.run_naked(context); }",
  ].join("\n"));
  await write("src-tauri/src/services/import.rs", "fn run_authorized(context: &ProjectContext) { run(context); }\n");
  const authorityState = () => evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "mutation-write-authority-inventory")?.state;
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/commands/import.rs", [
    "#[tauri::command]",
    "pub fn start_import() { state.with_current_project_write_access(id, root, |_permit, _context| Ok(())); }",
    "fn run_worker() { let lease = state.begin_project_external_task(context, task_id); service.run_authorized(&lease); }",
  ].join("\n"));
  await write(
    "src-tauri/src/services/import.rs",
    "fn run_authorized(lease: &ProjectExecutionLease) { run(lease.task_context(task_id)); }\n",
  );
  assert.equal(authorityState(), "green");
});

test("service authority contracts reject release-visible naked write siblings", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-service-authority-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  await write("release/command-authority-inventory.json", JSON.stringify({
    commands: [{
      name: "save",
      source: "src-tauri/src/commands/save.rs",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
    }],
    serviceAuthorityContracts: [{
      source: "src-tauri/src/services/save.rs",
      function: "save_authorized",
      capability: "ProjectWritePermit",
      requiredCalls: ["permit.context()"],
      debugOnlyNakedFunctions: ["save"],
    }],
  }));
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit;\n");
  await write(
    "src-tauri/src/commands/save.rs",
    "#[tauri::command]\npub fn save() { state.with_current_project_write_access(id, root, |_permit, _context| Ok(())); }\n",
  );
  const authorityState = () => evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "mutation-write-authority-inventory")?.state;

  await write("src-tauri/src/services/save.rs", [
    "fn save_authorized(permit: &ProjectWritePermit) { save_unchecked(permit.context()); }",
    "pub fn save(context: &ProjectContext) { save_unchecked(context); }",
  ].join("\n"));
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/services/save.rs", [
    "fn save_authorized(permit: &ProjectWritePermit) { save_unchecked(permit.context()); }",
    "#[cfg(debug_assertions)]",
    "pub fn save(context: &ProjectContext) { save_unchecked(context); }",
  ].join("\n"));
  assert.equal(authorityState(), "green");
});

test("service authority contracts validate every capability wrapper and reject crate-visible unchecked APIs", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-service-capability-set-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit;\n");
  await write(
    "src-tauri/src/commands/import.rs",
    "#[tauri::command]\npub fn mutate() { state.with_current_project_write_access(id, root, |_permit, _context| Ok(())); }\n",
  );
  const inventory = {
    commands: [{
      name: "mutate",
      source: "src-tauri/src/commands/import.rs",
      classifications: ["mutation"],
      projectScoped: true,
      writeAuthority: "ProjectWritePermit",
    }],
    serviceAuthorityContracts: [
      {
        source: "src-tauri/src/services/import.rs",
        function: "create_authorized",
        capability: "ProjectWritePermit",
        capabilityFunctions: ["finish_authorized"],
        requiredCalls: ["permit.context()"],
      },
      {
        source: "src-tauri/src/services/import.rs",
        function: "create_unchecked",
        visibility: "module-internal",
        internalOnlyFunctions: ["finish_unchecked"],
      },
    ],
  };
  await write("release/command-authority-inventory.json", JSON.stringify(inventory));
  const authorityState = () => evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "mutation-write-authority-inventory")?.state;

  await write("src-tauri/src/services/import.rs", [
    "fn create_authorized(permit: &ProjectWritePermit) { create_unchecked(permit.context()); }",
    "fn finish_authorized(_permit: &ProjectWritePermit) { finish_unchecked(context); }",
    "pub(crate) fn create_unchecked(context: &ProjectContext) { write(context); }",
    "fn finish_unchecked(context: &ProjectContext) { write(context); }",
  ].join("\n"));
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/services/import.rs", [
    "fn create_authorized(permit: &ProjectWritePermit) { create_unchecked(permit.context()); }",
    "fn finish_authorized(permit: &ProjectWritePermit) { finish_unchecked(permit.context()); }",
    "pub(super) fn create_unchecked(context: &ProjectContext) { write(context); }",
    "fn finish_unchecked(context: &ProjectContext) { write(context); }",
  ].join("\n"));
  assert.equal(authorityState(), "green");
});

test("execution authority accepts only the two backend epoch helpers", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-execution-authority-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  const inventory = (helper) => ({
    commands: [{
      name: "probe",
      source: "src-tauri/src/commands/probe.rs",
      classifications: ["read", "network"],
      projectScoped: true,
      authorityPaths: [{
        function: "probe",
        authority: "ProjectExecutionLease",
        helper,
        orderedCalls: ["begin_project_external_execution", "publish_result"],
      }],
    }],
  });
  await write("src-tauri/src/app_state.rs", "struct ProjectWritePermit; struct ProjectExecutionLease;\n");
  await write("src-tauri/src/commands/probe.rs", [
    "#[tauri::command]",
    "pub fn probe() { state.totally_fake(context, id); }",
  ].join("\n"));
  await write("release/command-authority-inventory.json", JSON.stringify(inventory("totally_fake")));
  const authorityState = () => evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "mutation-write-authority-inventory")?.state;
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/commands/probe.rs", [
    "#[tauri::command]",
    "pub fn probe() { publish_result(); state.begin_project_external_execution(context, id); }",
  ].join("\n"));
  await write(
    "release/command-authority-inventory.json",
    JSON.stringify(inventory("begin_project_external_execution")),
  );
  assert.equal(authorityState(), "red");

  await write("src-tauri/src/commands/probe.rs", [
    "#[tauri::command]",
    "pub fn probe() { state.begin_project_external_execution(context, id); publish_result(); }",
  ].join("\n"));
  assert.equal(authorityState(), "green");
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

test("empty named tests and a stub backend adapter cannot turn Batch 1 green", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-final-four-backend-stub-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };

  await write(
    "src/lib/backendError.ts",
    "export interface NormalizedBackendError {}\nexport function normalizeBackendError() {}\n",
  );
  await write("src/components/app/ActionableErrorNotice.tsx", "export function ActionableErrorNotice() {}\n");
  await write("src/features/settings/UpdateSettings.tsx", "ActionableErrorNotice normalizeBackendError\n");
  await write("src/test/backend-error-presentation.test.tsx", [
    "test('serialized BackendError is normalized', () => {});",
    "test('unknown object never renders Object object', () => {});",
    "test('plain Error string null and array stay safe', () => {});",
    "test('circular input is safe', () => {});",
    "test('redacts Authorization api key and cookie', () => {});",
    "test('presents a zh-CN message', () => {});",
    "test('presents an English message', () => {});",
    "test('retry failure twice restores the action', () => {});",
    "test('updater uses the shared error', () => {});",
    "test('provider uses the shared error', () => {});",
  ].join("\n"));

  const backendState = evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "structured-backend-error-presentation")?.state;
  assert.equal(backendState, "red");
});

test("a missing priority migration keeps the Batch 1 redline red", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-final-four-backend-migration-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  const copy = async (relativePath) => write(
    relativePath,
    await fs.readFile(path.join(repositoryRoot, relativePath), "utf8"),
  );

  await copy("src/lib/backendError.ts");
  await copy("src/components/app/ActionableErrorNotice.tsx");
  await copy("src/components/app/LazyActionableErrorNotice.tsx");
  await copy("src/test/backend-error-presentation.test.tsx");
  for (const relativePath of [
    "src/features/project/NoProjectWorkspace.tsx",
    "src/stores/projectStore.ts",
    "src/features/import/ImportCapabilityDialog.tsx",
    "src/features/settings/UpdateSettings.tsx",
    "src/features/settings/useProviderWorkflow.ts",
    "src/features/chat/ChatView.tsx",
    // PageChatPanel is deliberately absent.
    "src/components/app/TaskLogDrawer.tsx",
    "src/hooks/useTaskLauncher.ts",
    "src/hooks/useTaskEvents.ts",
    "src/features/workflows/useWorkflowsController.ts",
    "src/features/workflows/WorkflowsRightPanel.tsx",
    "src/features/workflows/WorkflowTaskDetail.tsx",
  ]) {
    await copy(relativePath);
  }

  const backendState = evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "structured-backend-error-presentation")?.state;
  assert.equal(backendState, "red");
});

test("a stub lazy notice keeps the Batch 1 redline red", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-final-four-backend-lazy-stub-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relativePath, contents) => {
    const target = path.join(root, relativePath);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, contents);
  };
  const copy = async (relativePath) => write(
    relativePath,
    await fs.readFile(path.join(repositoryRoot, relativePath), "utf8"),
  );

  await copy("src/lib/backendError.ts");
  await copy("src/components/app/ActionableErrorNotice.tsx");
  await write(
    "src/components/app/LazyActionableErrorNotice.tsx",
    [
      "export function LazyActionableErrorNotice() { return null; }",
      "const noop = 0; // const ActionableErrorNotice = lazy(async () => { const module = await import(\"./ActionableErrorNotice\"); return { default: module.ActionableErrorNotice }; });",
      "const noop2 = 0; // export function LazyActionableErrorNotice(props: ActionableErrorNoticeProps) { return (<ViewErrorBoundary errorRole={props.role}><Suspense fallback={<ErrorNoticeLoading />}><ActionableErrorNotice {...props} /></Suspense></ViewErrorBoundary>); }",
    ].join("\n"),
  );
  await copy("src/test/backend-error-presentation.test.tsx");
  for (const relativePath of [
    "src/features/project/NoProjectWorkspace.tsx",
    "src/stores/projectStore.ts",
    "src/features/import/ImportCapabilityDialog.tsx",
    "src/features/settings/UpdateSettings.tsx",
    "src/features/settings/useProviderWorkflow.ts",
    "src/features/chat/ChatView.tsx",
    "src/features/chat/PageChatPanel.tsx",
    "src/components/app/TaskLogDrawer.tsx",
    "src/hooks/useTaskLauncher.ts",
    "src/hooks/useTaskEvents.ts",
    "src/features/workflows/useWorkflowsController.ts",
    "src/features/workflows/WorkflowsRightPanel.tsx",
    "src/features/workflows/WorkflowTaskDetail.tsx",
  ]) {
    await copy(relativePath);
  }

  const backendState = evaluateFinalFourRedlines(root)
    .find(({ id }) => id === "structured-backend-error-presentation")?.state;
  assert.equal(backendState, "red");
});

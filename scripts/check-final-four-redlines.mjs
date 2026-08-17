import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.resolve(scriptDirectory, "..");

const readText = (root, relativePath) => {
  try {
    return fs.readFileSync(path.join(root, relativePath), "utf8");
  } catch (error) {
    if (error.code === "ENOENT") return "";
    throw error;
  }
};

const readJson = (root, relativePath, fallback = null) => {
  const text = readText(root, relativePath);
  return text ? JSON.parse(text) : fallback;
};

const collectFiles = (directory, extension, files = []) => {
  if (!fs.existsSync(directory)) return files;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) collectFiles(target, extension, files);
    else if (entry.isFile() && target.endsWith(extension)) files.push(target);
  }
  return files;
};

const readTree = (root, relativeDirectory, extension) => collectFiles(
  path.join(root, relativeDirectory),
  extension,
).map((file) => fs.readFileSync(file, "utf8")).join("\n");

const javascriptTestNames = (text) => [...text.matchAll(/\b(?:it|test)\s*\(\s*["'`]([^"'`]+)["'`]/g)]
  .map((match) => match[1]);

const rustTestNames = (text) => [...text.matchAll(/#\[(?:tokio::)?test\][\s\S]*?\bfn\s+([a-zA-Z0-9_]+)/g)]
  .map((match) => match[1]);

const testNamesCover = (names, requiredPatterns) => requiredPatterns.every((pattern) => (
  names.some((name) => pattern.test(name))
));

const tauriCommandNames = (root) => {
  const commands = readTree(root, "src-tauri/src/commands", ".rs");
  return [...commands.matchAll(/#\[tauri::command\]\s*pub\s+(?:async\s+)?fn\s+([a-zA-Z0-9_]+)/g)]
    .map((match) => match[1])
    .sort();
};

const result = (id, passed, ownerBatch, detail) => ({
  id,
  state: passed ? "green" : "red",
  ownerBatch,
  detail,
});

export function evaluateFinalFourRedlines(root) {
  const catalog = readJson(root, "capabilities/install-catalog.json", { entries: [] });
  const trustedKeys = readJson(root, "capabilities/trusted-keys.json", {});
  const cargo = readText(root, "src-tauri/Cargo.toml");
  const tauri = readJson(root, "src-tauri/tauri.conf.json", {});
  const lib = readText(root, "src-tauri/src/lib.rs");
  const updateSettings = readText(root, "src/features/settings/UpdateSettings.tsx");
  const updateStore = readText(root, "src/stores/updateStore.ts");
  const updateController = readText(root, "src/features/update/useUpdateController.ts")
    + readText(root, "src/components/app/UpdateController.tsx");
  const updaterContractTests = readText(root, "src-tauri/tests/updater_contracts.rs");
  const updaterTestNames = rustTestNames(updaterContractTests);
  const updateOfferTests = readText(root, "src/features/update/UpdateController.test.tsx");
  const updateOfferTestNames = javascriptTestNames(updateOfferTests);
  const backendErrorAdapter = readText(root, "src/lib/backendError.ts");
  const backendErrorNotice = readText(root, "src/components/app/ActionableErrorNotice.tsx");
  const backendErrorLazyNotice = readText(root, "src/components/app/LazyActionableErrorNotice.tsx");
  const backendErrorLazyNoticeCode = backendErrorLazyNotice
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/.*$/gm, "");
  const backendErrorLazyImportReady = /const\s+ActionableErrorNotice\s*=\s*lazy\s*\(\s*async\s*\(\)\s*=>\s*\{\s*const\s+module\s*=\s*await\s+import\(["']\.\/ActionableErrorNotice["']\);\s*return\s*\{\s*default:\s*module\.ActionableErrorNotice\s*\};?\s*\}\s*\);/.test(backendErrorLazyNoticeCode);
  const backendErrorLazyRenderReady = /export function LazyActionableErrorNotice\s*\(props:\s*ActionableErrorNoticeProps\)\s*\{\s*return\s*\(\s*<ViewErrorBoundary\s+errorRole=\{props\.role\}>[\s\S]*?<Suspense\s+fallback=\{<ErrorNoticeLoading\s*\/>\}>[\s\S]*?<ActionableErrorNotice\s+\{\.\.\.props\}\s*\/>[\s\S]*?<\/Suspense>[\s\S]*?<\/ViewErrorBoundary>\s*\);\s*\}/.test(backendErrorLazyNoticeCode);
  const backendErrorTest = readText(root, "src/test/backend-error-presentation.test.tsx");
  const backendErrorTestNames = javascriptTestNames(backendErrorTest);
  const backendErrorPriorityContracts = [
    ["src/features/project/NoProjectWorkspace.tsx", ["ActionableErrorNotice", "normalizeBackendError"]],
    ["src/stores/projectStore.ts", ["normalizeBackendError"]],
    ["src/features/import/ImportCapabilityDialog.tsx", ["ActionableErrorNotice", "normalizeBackendError"]],
    ["src/features/settings/UpdateSettings.tsx", ["ActionableErrorNotice", "normalizeBackendError"]],
    ["src/features/settings/useProviderWorkflow.ts", ["normalizeBackendError"]],
    ["src/features/chat/ChatView.tsx", ["ActionableErrorNotice", "normalizeBackendError"]],
    ["src/features/chat/PageChatPanel.tsx", ["ActionableErrorNotice", "normalizeBackendError"]],
    ["src/components/app/TaskLogDrawer.tsx", ["ActionableErrorNotice"]],
    ["src/hooks/useTaskLauncher.ts", ["translateBackendError"]],
    ["src/hooks/useTaskEvents.ts", ["translateBackendError"]],
    ["src/features/workflows/useWorkflowsController.ts", ["normalizeBackendError", "backendErrorCode"]],
    ["src/features/workflows/WorkflowsRightPanel.tsx", ["normalizeBackendError"]],
    ["src/features/workflows/WorkflowTaskDetail.tsx", ["backendErrorCode"]],
  ];
  const backendErrorPrioritySources = backendErrorPriorityContracts.map(([file, markers]) => ({
    file,
    markers,
    source: readText(root, file),
  }));
  const backendErrorPriorityUi = backendErrorPrioritySources.map(({ source }) => source).join("\n");
  const backendErrorMigrationsReady = backendErrorPrioritySources.every(({ markers, source }) => (
    source.length > 0 && markers.every((marker) => source.includes(marker))
  ));
  const rustServices = readTree(root, "src-tauri/src", ".rs");
  const providerContractTests = readText(root, "src-tauri/tests/provider_secret_origin_contracts.rs");
  const providerTestNames = rustTestNames(providerContractTests);
  const authorityInventory = readJson(root, "release/command-authority-inventory.json", null);
  const releaseWorkflow = readText(root, ".github/workflows/desktop-release.yml")
    || readText(root, ".github/workflows/release.yml");
  const capabilityWorkflow = readText(root, ".github/workflows/capability-release.yml");

  const catalogEntries = Array.isArray(catalog?.entries) ? catalog.entries : [];
  const expectedTargets = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
  ];
  const expectedPacks = [
    "asr-sensevoice-small",
    "browser-runtime",
    "browser-runtime-lite",
    "media-metadata",
    "ocr-cjk-accurate",
  ];
  const catalogTargets = new Set(catalogEntries.map((entry) => entry.targetTriple));
  const catalogPacks = new Set(catalogEntries.map((entry) => entry.capabilityId));
  const catalogPairs = new Set(catalogEntries.map((entry) => `${entry.capabilityId}\0${entry.targetTriple}`));
  const catalogTags = new Set();
  const catalogEntriesValid = catalogEntries.every((entry) => {
    const match = /^https:\/\/github\.com\/StoneLL1\/llm-wiki-desktop\/releases\/download\/(app-v[^/]+)\/[^?#]+$/.exec(entry.url ?? "");
    if (match) catalogTags.add(match[1]);
    return Boolean(match)
      && /^[0-9a-f]{64}$/.test(entry.archiveSha256 ?? "")
      && !/^0+$/.test(entry.archiveSha256)
      && /^[0-9a-f]{64}$/.test(entry.manifestSha256 ?? "")
      && !/^0+$/.test(entry.manifestSha256)
      && Number.isSafeInteger(entry.compressedBytes) && entry.compressedBytes > 0
      && Number.isSafeInteger(entry.installedBytes) && entry.installedBytes > 0
      && typeof entry.license === "string" && entry.license.trim().length > 0;
  });
  const trustedKeyValues = Object.values(trustedKeys ?? {});
  const catalogReady = catalog?.schemaVersion === 1
    && catalogEntries.length === 20
    && catalogPairs.size === 20
    && JSON.stringify([...catalogTargets].sort()) === JSON.stringify(expectedTargets)
    && JSON.stringify([...catalogPacks].sort()) === JSON.stringify(expectedPacks)
    && catalogTags.size === 1
    && catalogEntriesValid
    && trustedKeyValues.length > 0
    && trustedKeyValues.every((key) => /^[0-9a-f]{64}$/.test(key) && !/^0+$/.test(key));

  const updaterConfig = tauri?.plugins?.updater ?? {};
  const updaterReady = cargo.includes("tauri-plugin-updater")
    && lib.includes("tauri_plugin_updater::Builder")
    && tauri?.bundle?.createUpdaterArtifacts === true
    && typeof updaterConfig.pubkey === "string"
    && updaterConfig.pubkey.trim().length >= 32
    && Array.isArray(updaterConfig.endpoints)
    && updaterConfig.endpoints.includes("https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json")
    && testNamesCover(updaterTestNames, [
      /bad_signature/i,
      /bad_manifest/i,
      /timeout/i,
      /cancel/i,
      /same_version/i,
      /downgrade/i,
    ]);

  const updateOfferReady = updateSettings.includes("check_app_update")
    && !updateSettings.includes("get_app_summary")
    && !updateSettings.includes("window.confirm")
    && /latestVersion|available/i.test(updateStore)
    && /check_app_update/.test(updateController)
    && testNamesCover(updateOfferTestNames, [
      /no.project/i,
      /available/i,
      /downloading/i,
      /cancel/i,
      /ready.to.install/i,
      /installing/i,
    ]);

  const backendErrorReady = /export function normalizeBackendError\s*\(/.test(backendErrorAdapter)
    && backendErrorAdapter.includes("NormalizedBackendError")
    && backendErrorAdapter.includes("redactBackendErrorDetails")
    && backendErrorAdapter.includes("serializeTechnicalDetails")
    && backendErrorAdapter.includes("safeProperty")
    && backendErrorAdapter.includes("actionKindOverride")
    && backendErrorAdapter.includes("MAX_DETAILS_LENGTH")
    && backendErrorAdapter.includes("REDACTED_PATH")
    && /export function ActionableErrorNotice\s*\(/.test(backendErrorNotice)
    && backendErrorNotice.includes("normalizeBackendError")
    && backendErrorNotice.includes("navigator.clipboard.writeText")
    && backendErrorNotice.includes("onAction")
    && backendErrorLazyImportReady
    && backendErrorLazyRenderReady
    && testNamesCover(backendErrorTestNames, [
      /serialized.backenderror/i,
      /object.object/i,
      /plain.error.*string.*null.*array/i,
      /circular/i,
      /authorization.*api.key.*cookie/i,
      /zh-cn/i,
      /english/i,
      /retry.failure.twice/i,
      /updater/i,
      /provider/i,
    ])
    && (backendErrorTest.match(/\bexpect\s*\(/g) ?? []).length >= 20
    && backendErrorTest.includes("render(<ActionableErrorNotice")
    && backendErrorTest.includes("new Proxy")
    && backendErrorTest.includes("navigator, \"clipboard\"")
    && backendErrorTest.includes('changeLanguage("zh-CN")')
    && backendErrorMigrationsReady
    && !backendErrorPriorityUi.includes("String(error)");

  const providerBindingReady = rustServices.includes("ProviderCredentialBinding")
    && rustServices.includes("canonical_origin")
    && rustServices.includes("credential_account_id")
    && /redirect[^\n]*(?:Policy::none|none\(\))/i.test(rustServices)
    && testNamesCover(providerTestNames, [
      /attacker.*request.count/i,
      /redirect.*secret/i,
      /0_0_0_0/i,
      /169_254_169_254/i,
      /legacy.*untrusted/i,
    ]);

  const inventoryEntries = Array.isArray(authorityInventory?.commands) ? authorityInventory.commands : [];
  const actualCommands = tauriCommandNames(root);
  const inventoriedCommands = inventoryEntries.map((entry) => entry.name).sort();
  const allowedClasses = new Set(["read", "mutation", "network", "external-process", "secret"]);
  const mutationInventoryReady = actualCommands.length > 0
    && JSON.stringify(actualCommands) === JSON.stringify(inventoriedCommands)
    && inventoryEntries.every((entry) => Array.isArray(entry.classifications)
      && entry.classifications.length > 0
      && entry.classifications.every((kind) => allowedClasses.has(kind))
      && (!entry.classifications.includes("mutation") || (typeof entry.writeAuthority === "string" && entry.writeAuthority.length > 0))
      && (!entry.projectScoped || !entry.classifications.includes("mutation") || entry.writeAuthority === "ProjectWritePermit"))
    && readText(root, "src-tauri/src/app_state.rs").includes("ProjectWritePermit");

  const publishStable = /^ {2}publish-stable:\s*$([\s\S]*?)(?=^ {2}[A-Za-z0-9_-]+:\s*$|(?![\s\S]))/m.exec(releaseWorkflow)?.[1] ?? "";
  const requiredReleaseJobs = ["preflight", "capability-build", "desktop-build", "manifest-and-provenance", "packaged-smoke", "publish-stable"];
  const atomicReleaseReady = releaseWorkflow.length > 0
    && requiredReleaseJobs.every((job) => new RegExp(`^ {2}${job}:`, "m").test(releaseWorkflow))
    && ["capability-build", "desktop-build", "manifest-and-provenance", "packaged-smoke"]
      .every((job) => publishStable.includes(job))
    && /latest\.json/i.test(releaseWorkflow)
    && /^ {4}environment:\s+desktop-release\s*$/m.test(publishStable)
    && /^ {6}contents:\s+write\s*$/m.test(publishStable)
    && (releaseWorkflow.match(/contents:\s+write/gi) ?? []).length === 1
    && !/gh release (?:create|upload)/i.test(capabilityWorkflow)
    && readText(root, "scripts/verify-release-assets.mjs").length > 0
    && readText(root, "scripts/verify-latest-json.mjs").length > 0;

  return [
    result("capability-release-catalog", catalogReady, "3A", "release mode requires 4 targets × 5 signed capability packs"),
    result("signed-updater-foundation", updaterReady, "4A", "Tauri updater dependency, plugin, artifacts, public key, and canonical endpoint must exist"),
    result("real-update-offer", updateOfferReady, "4B", "UpdateSettings must consume the global updater offer instead of get_app_summary placeholder state"),
    result("structured-backend-error-presentation", backendErrorReady, "1", "shared normalization must cover serialized, circular, and object-shaped failures without [object Object]"),
    result("provider-secret-origin-binding", providerBindingReady, "2A", "provider credentials must bind to canonical origin and redirects must not carry secrets"),
    result("mutation-write-authority-inventory", mutationInventoryReady, "2B", "every mutation command must be inventoried and require ProjectWritePermit"),
    result("atomic-stable-release-workflow", atomicReleaseReady, "5", "only one final publisher may release complete desktop, capability, and manifest artifacts"),
  ];
}

export function expectedRedlineStates(root) {
  const declaration = readJson(root, "release/final-four-redlines.json");
  if (declaration?.schemaVersion !== 1 || !Array.isArray(declaration.redlines)) {
    throw new Error("release/final-four-redlines.json must declare schemaVersion 1 and redlines[]");
  }
  return declaration.redlines;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const redlines = evaluateFinalFourRedlines(repositoryRoot);
  for (const item of redlines) {
    const marker = item.state === "green" ? "GREEN" : "RED";
    process.stdout.write(`[final-four:${marker}] ${item.id} (Batch ${item.ownerBatch}) — ${item.detail}\n`);
  }
  const failures = redlines.filter(({ state }) => state === "red");
  if (failures.length > 0) {
    process.stderr.write(`[final-four] ${failures.length} release-blocking contract(s) remain red\n`);
    process.exitCode = 1;
  }
}

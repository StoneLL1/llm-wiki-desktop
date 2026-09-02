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

const rustFunctionBody = (source, functionName) => {
  const declaration = new RegExp(`\\b(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?fn\\s+${functionName}(?:<[^>{}]*>)?\\s*\\(`)
    .exec(source);
  if (!declaration) return "";
  const start = source.indexOf("{", declaration.index);
  if (start < 0) return "";
  let depth = 0;
  let quote = null;
  let escaped = false;
  for (let index = start; index < source.length; index += 1) {
    const character = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"') {
      quote = character;
      continue;
    }
    if (character === "{") depth += 1;
    else if (character === "}" && (depth -= 1) === 0) return source.slice(start, index + 1);
  }
  return "";
};

const rustFunctionSignature = (source, functionName) => {
  const declaration = new RegExp(`\\b(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?fn\\s+${functionName}(?:<[^>{}]*>)?\\s*\\(`)
    .exec(source);
  if (!declaration) return "";
  const start = source.indexOf("{", declaration.index);
  return start < 0 ? "" : source.slice(declaration.index, start);
};

const stripRustComments = (source) => source
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/\/\/.*$/gm, "");

const callsAppearInOrder = (source, calls = []) => {
  let cursor = -1;
  return calls.every((call) => {
    cursor = source.indexOf(call, cursor + 1);
    return cursor >= 0;
  });
};

const publicFunctionIsDebugOnly = (source, functionName) => {
  const declarations = [...source.matchAll(
    new RegExp(`\\bpub\\s+fn\\s+${functionName}(?:<[^>{}]*>)?\\s*\\(`, "g"),
  )];
  return declarations.every((declaration) => source
    .slice(Math.max(0, declaration.index - 160), declaration.index)
    .includes("#[cfg(debug_assertions)]"));
};

const projectWriteAuthorityIsImplemented = (root, entry) => {
  if (!entry.projectScoped) return true;
  const hasExplicitAuthorityPaths = Array.isArray(entry.authorityPaths)
    && entry.authorityPaths.length > 0;
  if (!entry.classifications.includes("mutation") && !hasExplicitAuthorityPaths) return true;
  if (typeof entry.source !== "string" || entry.source.length === 0) return false;
  const source = readText(root, entry.source);
  const commandBody = stripRustComments(rustFunctionBody(source, entry.name));
  if (!commandBody) return false;
  const helpersForAuthority = {
    ProjectWritePermit: ["with_current_project_write_access"],
    ProjectAuthorityMutationPermit: ["with_current_project_authority_mutation"],
    ProjectTaskMutationPermit: ["with_current_project_task_access"],
    ProjectExecutionLease: ["begin_project_external_task", "begin_project_external_execution"],
  };
  const paths = hasExplicitAuthorityPaths
    ? entry.authorityPaths
    : [{
      function: entry.authorityFunction ?? entry.name,
      authority: entry.writeAuthority,
      commandDelegate: entry.authorityFunction,
    }];
  return paths.every((authorityPath) => {
    const authority = authorityPath.authority ?? entry.writeAuthority;
    const allowedHelpers = helpersForAuthority[authority];
    const helper = authorityPath.helper ?? allowedHelpers?.[0];
    if (!allowedHelpers?.includes(helper) || typeof authorityPath.function !== "string") return false;
    if (authorityPath.commandDelegate
        && authorityPath.function !== entry.name
        && !new RegExp(`\\b${authorityPath.commandDelegate}\\s*\\(`).test(commandBody)) return false;
    const pathSource = readText(root, authorityPath.source ?? entry.source);
    const authorityBody = stripRustComments(rustFunctionBody(pathSource, authorityPath.function));
    if (!new RegExp(`\\.\\s*${helper}\\s*\\(`).test(authorityBody)) return false;
    if ((authorityPath.requiredCalls ?? []).some((call) => !authorityBody.includes(call))) return false;
    if (!callsAppearInOrder(authorityBody, authorityPath.orderedCalls)) return false;
    return !(authorityPath.forbiddenCalls ?? []).some((call) => authorityBody.includes(call));
  });
};

const serviceAuthorityContractIsImplemented = (root, contract) => {
  if (typeof contract?.source !== "string" || typeof contract?.function !== "string") return false;
  const source = readText(root, contract.source);
  if (contract.visibility === "module-internal") {
    const functions = [contract.function, ...(contract.internalOnlyFunctions ?? [])];
    return functions.every((functionName) => {
      const signature = rustFunctionSignature(source, functionName);
      const visibilityIsModuleInternal = !signature.startsWith("pub ")
        && (!signature.startsWith("pub(") || /^pub\((?:self|super)\)\s+fn\b/.test(signature));
      return signature.length > 0 && visibilityIsModuleInternal;
    }) && (contract.debugOnlyNakedFunctions ?? [])
      .every((functionName) => publicFunctionIsDebugOnly(source, functionName));
  }
  if (typeof contract?.capability !== "string") return false;
  const capabilityFunctions = [contract.function, ...(contract.capabilityFunctions ?? [])];
  const capabilityFunctionsValid = capabilityFunctions.every((functionName) => {
    const signature = rustFunctionSignature(source, functionName);
    const body = stripRustComments(rustFunctionBody(source, functionName));
    return signature.includes(contract.capability)
      && (contract.requiredCalls ?? []).every((call) => body.includes(call))
      && callsAppearInOrder(body, contract.orderedCalls)
      && !(contract.forbiddenCalls ?? []).some((call) => body.includes(call));
  });
  return capabilityFunctionsValid
    && (contract.debugOnlyNakedFunctions ?? [])
      .every((functionName) => publicFunctionIsDebugOnly(source, functionName));
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
  const updateApi = readText(root, "src/services/updateApi.ts");
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
    ["src/features/settings/UpdateSettings.tsx", ["ActionableErrorNotice", "useUpdateStore"]],
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
  const productManifest = readJson(root, "capabilities/product-manifest.json", null);

  const catalogEntries = Array.isArray(catalog?.entries) ? catalog.entries : [];
  const expectedTargets = Array.isArray(productManifest?.supportedTargets)
    ? [...productManifest.supportedTargets].sort()
    : [];
  const expectedPacks = Array.isArray(productManifest?.definitions)
    ? productManifest.definitions
      .filter((definition) => definition?.distributionTier === "published")
      .map((definition) => definition.capabilityId)
      .sort()
    : [];
  // Each published provider ships its declared target subset; expected size is
  // the sum over definitions, not a cartesian product with the product targets.
  const expectedCatalogEntries = Array.isArray(productManifest?.definitions)
    ? productManifest.definitions
      .filter((definition) => definition?.distributionTier === "published")
      .reduce((total, definition) => total + (Array.isArray(definition.supportedTargets) ? definition.supportedTargets.length : 0), 0)
    : 0;
  const catalogTargets = new Set(catalogEntries.map((entry) => entry.targetTriple));
  const catalogPacks = new Set(catalogEntries.map((entry) => entry.capabilityId));
  const catalogPairs = new Set(catalogEntries.map((entry) => `${entry.capabilityId}\0${entry.targetTriple}`));
  const catalogTags = new Set();
  const capabilityBuildScript = readText(root, "src-tauri/build.rs");
  const capabilityEmbedModule = readText(root, "src-tauri/src/services/import_v2/capability_embed.rs");
  const catalogVerifier = readText(root, "scripts/verify-capability-catalog.mjs");
  const embeddedCatalogVerifier = readText(root, "scripts/verify-embedded-capability-catalog.mjs");
  const catalogEntriesValid = catalogEntries.every((entry) => {
    const match = /^https:\/\/github\.com\/StoneLL1\/llm-wiki-desktop\/releases\/download\/(app-v[^/]+)\/[^?#]+$/.exec(entry.url ?? "");
    if (match) catalogTags.add(match[1]);
    return Boolean(match)
      && /^[0-9a-f]{64}$/.test(entry.archiveSha256 ?? "")
      && !/^0+$/.test(entry.archiveSha256)
      && /^[0-9a-f]{64}$/.test(entry.manifestSha256 ?? "")
      && !/^0+$/.test(entry.manifestSha256)
      && typeof entry.signingKeyId === "string" && entry.signingKeyId.length > 0
      && Number.isSafeInteger(entry.compressedBytes) && entry.compressedBytes > 0
      && Number.isSafeInteger(entry.installedBytes) && entry.installedBytes > 0
      && typeof entry.license === "string" && entry.license.trim().length > 0;
  });
  const trustedKeyValues = Object.values(trustedKeys ?? {});
  const catalogPipelineReady = capabilityBuildScript.includes("LLM_WIKI_CAPABILITY_CATALOG_MODE")
    && capabilityBuildScript.includes("LLM_WIKI_CAPABILITY_STAGING_DIR")
    && (capabilityBuildScript + capabilityEmbedModule)
      .includes("release builds cannot embed an empty capability catalog")
    && catalogVerifier.includes("export function verifyCapabilityCatalog")
    && embeddedCatalogVerifier.includes("export function verifyEmbeddedCapabilityCatalog")
    && /^ {2}workflow_call:\s*$/m.test(capabilityWorkflow)
    && !/gh release (?:create|upload)/i.test(capabilityWorkflow);
  const catalogReady = catalog?.schemaVersion === 1
    && expectedCatalogEntries > 0
    && catalogEntries.length === expectedCatalogEntries
    && catalogPairs.size === expectedCatalogEntries
    && JSON.stringify([...catalogTargets].sort()) === JSON.stringify(expectedTargets)
    && JSON.stringify([...catalogPacks].sort()) === JSON.stringify(expectedPacks)
    && catalogTags.size === 1
    && catalogEntriesValid
    && trustedKeyValues.length > 0
    && trustedKeyValues.every((key) => /^[0-9a-f]{64}$/.test(key) && !/^0+$/.test(key))
    && catalogPipelineReady;

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

  const updateOfferReady = updateSettings.includes("useUpdateStore")
    && !updateSettings.includes("get_app_summary")
    && !updateSettings.includes("window.confirm")
    && updateApi.includes('invoke<AppUpdateState>("check_app_update")')
    && /checkAppUpdate/.test(updateStore)
    && /latestVersion|available/i.test(updateStore)
    && /useUpdateStore/.test(updateController)
    && /checkNow/.test(updateController)
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
  const serviceAuthorityContracts = Array.isArray(authorityInventory?.serviceAuthorityContracts)
    ? authorityInventory.serviceAuthorityContracts
    : [];
  const actualCommands = tauriCommandNames(root);
  const inventoriedCommands = inventoryEntries.map((entry) => entry.name).sort();
  const allowedClasses = new Set(["read", "mutation", "network", "external-process", "secret"]);
  const invalidAuthorityEntries = inventoryEntries.filter((entry) => !projectWriteAuthorityIsImplemented(root, entry));
  const invalidServiceAuthorityContracts = serviceAuthorityContracts
    .filter((contract) => !serviceAuthorityContractIsImplemented(root, contract));
  if (process.env.FINAL_FOUR_DEBUG === "1") {
    process.stderr.write(`[final-four:debug] invalid command authority: ${invalidAuthorityEntries.map((entry) => entry.name).join(", ")}\n`);
    process.stderr.write(`[final-four:debug] invalid service authority: ${invalidServiceAuthorityContracts.map((contract) => contract.function || JSON.stringify(contract)).join(", ")}\n`);
  }
  const mutationInventoryReady = actualCommands.length > 0
    && JSON.stringify(actualCommands) === JSON.stringify(inventoriedCommands)
    && inventoryEntries.every((entry) => Array.isArray(entry.classifications)
      && entry.classifications.length > 0
      && entry.classifications.every((kind) => allowedClasses.has(kind))
      && (!entry.classifications.includes("mutation") || (typeof entry.writeAuthority === "string" && entry.writeAuthority.length > 0))
      && !invalidAuthorityEntries.includes(entry))
    && invalidServiceAuthorityContracts.length === 0
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
    result("capability-release-catalog", catalogReady, "3A", "release mode requires the product-manifest-derived exact signed capability matrix"),
    result("signed-updater-foundation", updaterReady, "4A", "Tauri updater dependency, plugin, artifacts, public key, and canonical endpoint must exist"),
    result("real-update-offer", updateOfferReady, "4B", "UpdateSettings must consume the global updater offer instead of get_app_summary placeholder state"),
    result("structured-backend-error-presentation", backendErrorReady, "1", "shared normalization must cover serialized, circular, and object-shaped failures without [object Object]"),
    result("provider-secret-origin-binding", providerBindingReady, "2A", "provider credentials must bind to canonical origin and redirects must not carry secrets"),
    result("mutation-write-authority-inventory", mutationInventoryReady, "2B", "every mutation path must be inventoried and carry an unforgeable project authority capability"),
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

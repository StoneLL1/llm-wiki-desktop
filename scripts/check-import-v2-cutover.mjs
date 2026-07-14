import { readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const jsonOutput = process.argv.includes("--json");
const blockers = [];
const warnings = [];

const requiredFiles = [
  "src-tauri/src/models/import_v2_migration.rs",
  "src-tauri/src/services/import_v2/migration/scanner.rs",
  "src-tauri/src/services/import_v2/migration/planner.rs",
  "src-tauri/src/services/import_v2/migration/apply.rs",
  "src-tauri/src/services/import_v2/migration/legacy_history.rs",
  "src-tauri/src/services/import_v2/migration/verifier.rs",
  "src-tauri/src/models/import_backend_activation.rs",
  "src-tauri/src/services/import_v2/activation.rs",
  "src-tauri/src/commands/import_v2_migration.rs",
  "src-tauri/src/commands/import_v2_activation.rs",
  "src/types/importV2Migration.ts",
  "src/types/importV2Activation.ts",
  "src/services/importV2MigrationApi.ts",
  "src/services/importV2ActivationApi.ts",
  "docs/import-v2-cutover-checklist.md",
  "docs/import-v2-cutover-evidence.json",
];

for (const relative of requiredFiles) {
  try {
    await readFile(path.join(root, relative));
  } catch {
    blockers.push(`Missing cutover evidence or implementation file: ${relative}`);
  }
}

let evidence;
try {
  evidence = JSON.parse(
    await readFile(path.join(root, "docs/import-v2-cutover-evidence.json"), "utf8"),
  );
} catch {
  evidence = null;
}

if (evidence) {
  const expectedPackages = new Set(["core", "file", "web", "agent"]);
  const gates = new Map((evidence.packageGates ?? []).map((gate) => [gate.package, gate]));
  for (const packageName of expectedPackages) {
    const gate = gates.get(packageName);
    if (!gate || gate.contractVersion !== "import-v2-core-v2" || gate.releaseGatePassed !== true) {
      blockers.push(`Package release gate is missing or failed: ${packageName}`);
    }
  }

  for (const platform of ["windows", "macos", "linux"]) {
    const result = (evidence.platformMatrix ?? []).find((item) => item.platform === platform);
    if (!result || result.passed !== true) {
      blockers.push(`Platform acceptance is not passed: ${platform}`);
    }
  }

  for (const flag of [
    "coreRecoveryPassed",
    "fixtureMatrixPassed",
    "idempotencePassed",
    "legacyImmutabilityPassed",
    "longTaskRecoveryPassed",
    "schemaRegeneratedAndReviewed",
  ]) {
    if (evidence[flag] !== true) blockers.push(`Readiness evidence is incomplete: ${flag}`);
  }

  const licenses = evidence.licenseEvidence ?? [];
  if (licenses.length === 0) {
    blockers.push("External tool license/provenance evidence is missing.");
  }
  for (const tool of licenses) {
    const license = String(tool.license ?? "").toLowerCase();
    if (license.includes("gpl") || license.includes("agpl") || license.includes("non-commercial")) {
      blockers.push(`Disallowed external tool license: ${tool.name ?? "unnamed"}`);
    }
    if (
      !tool.name || !tool.version || !tool.platform || !tool.hashOrSignature ||
      !Number.isInteger(tool.sizeBytes) || tool.sizeBytes <= 0 || !tool.fallback
    ) {
      blockers.push(`Incomplete external tool provenance: ${tool.name ?? "unnamed"}`);
    }
  }

  if (evidence.legacyMutationRetirement?.approved !== true) {
    warnings.push("Legacy mutation code remains until the separately approved soak window.");
  }
}

const report = {
  passed: blockers.length === 0,
  blockers,
  warnings,
  readOnly: true,
};

if (jsonOutput) {
  console.log(JSON.stringify(report, null, 2));
} else if (report.passed) {
  console.log("Import V2 cutover readiness gate passed.");
} else {
  console.error("Import V2 cutover readiness gate blocked:");
  for (const blocker of blockers) console.error(`- ${blocker}`);
  for (const warning of warnings) console.error(`Warning: ${warning}`);
}

if (!report.passed) process.exitCode = 1;

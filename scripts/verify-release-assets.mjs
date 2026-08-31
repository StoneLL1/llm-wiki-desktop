import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { checksumDocument } from "./generate-release-checksums.mjs";
import {
  collectRegularFiles,
  COMMIT_PATTERN,
  isPlainObject,
  osIdentityEvidenceErrors,
  parseNamedArguments,
  RELEASE_PLATFORMS,
  RELEASE_REPOSITORY,
  safeAssetName,
  STABLE_TAG_PATTERN,
} from "./release-assets-contract.mjs";
import { CAPABILITY_PACKS, CAPABILITY_TARGETS, verifyCapabilityCatalog } from "./verify-capability-catalog.mjs";
import { validateLatestJson } from "./verify-latest-json.mjs";

const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
const existsFile = (file) => fs.existsSync(file) && fs.statSync(file).isFile();

function descriptorErrors({ root, descriptor, platform, tag, version, commitSha }) {
  const errors = [];
  const label = `desktop/${platform}/release-entry.json`;
  if (!isPlainObject(descriptor) || descriptor.schemaVersion !== 1) return [`${label} must use schemaVersion 1`];
  if (descriptor.platform !== platform || descriptor.targetTriple !== RELEASE_PLATFORMS[platform]) {
    errors.push(`${label} platform identity is invalid`);
  }
  if (descriptor.releaseTag !== tag || descriptor.version !== version || descriptor.commitSha !== commitSha) {
    errors.push(`${label} release coordinate does not match the transaction`);
  }
  for (const [kind, asset] of [["installer", descriptor.installer], ["updater", descriptor.updater]]) {
    if (!isPlainObject(asset) || !safeAssetName(asset.file)) {
      errors.push(`${label} ${kind} file is invalid`);
    } else if (!existsFile(path.join(root, "desktop", platform, asset.file))) {
      errors.push(`${label} ${kind} file is missing: ${asset.file}`);
    }
  }
  const updater = descriptor.updater;
  if (!isPlainObject(updater) || !safeAssetName(updater.signatureFile)) {
    errors.push(`${label} updater signature file is invalid`);
  } else {
    const signaturePath = path.join(root, "desktop", platform, updater.signatureFile);
    if (!existsFile(signaturePath)) errors.push(`${label} updater signature file is missing`);
    else if (fs.readFileSync(signaturePath, "utf8").trim() !== updater.signature?.trim()) {
      errors.push(`${label} updater signature does not match its .sig asset`);
    }
  }
  for (const error of osIdentityEvidenceErrors(descriptor.osSigning, platform)) {
    errors.push(`${label} ${error}`);
  }
  return errors;
}

function provenanceErrors(provenance, { tag, commitSha, workflowRunId }) {
  if (!isPlainObject(provenance)) return ["release provenance must be an object"];
  const errors = [];
  if (provenance.schemaVersion !== 1) errors.push("release provenance schemaVersion must be 1");
  if (provenance.repository !== RELEASE_REPOSITORY) errors.push("release provenance repository is invalid");
  if (provenance.releaseTag !== tag || provenance.commitSha !== commitSha) {
    errors.push("release provenance tag/commit does not match the transaction");
  }
  if (String(provenance.workflowRunId) !== String(workflowRunId)
    || String(provenance.capabilityWorkflowRunId) !== String(workflowRunId)) {
    errors.push("desktop and capability provenance must come from the same workflow run");
  }
  return errors;
}

function smokeErrors(smoke) {
  if (!isPlainObject(smoke) || smoke.schemaVersion !== 1) return ["packaged smoke summary must use schemaVersion 1"];
  const errors = [];
  if (smoke.fixtureEndpointProductionUntouched !== true) {
    errors.push("packaged updater smoke must prove the production endpoint was untouched");
  }
  const results = Array.isArray(smoke.platforms) ? smoke.platforms : [];
  const keys = results.map((result) => result?.platform).sort();
  if (JSON.stringify(keys) !== JSON.stringify(Object.keys(RELEASE_PLATFORMS).sort())) {
    errors.push("packaged smoke must cover exactly the four desktop platforms");
  }
  const requiredJourneys = ["install-launch", "packaged-process-alive", "updater-fixture-manifest-verified"];
  for (const result of results) {
    if (result?.status !== "passed" || !requiredJourneys.every((journey) => result.journeys?.includes(journey))) {
      errors.push(`${result?.platform ?? "unknown platform"} packaged smoke is incomplete`);
    }
  }
  return errors;
}

function sbomErrors(root) {
  const directory = path.join(root, "sbom");
  if (!fs.existsSync(directory)) return ["release bundle is missing the sbom directory"];
  const files = collectRegularFiles(directory).filter((file) => file.endsWith(".cdx.json"));
  if (files.length < 2) return ["release bundle must contain Node and Rust CycloneDX SBOMs"];
  const errors = [];
  for (const file of files) {
    try {
      if (readJson(file).bomFormat !== "CycloneDX") errors.push(`${path.basename(file)} is not a CycloneDX SBOM`);
    } catch {
      errors.push(`${path.basename(file)} is not valid JSON`);
    }
  }
  return errors;
}

export function verifyReleaseAssets({ root, tag, version, commitSha, workflowRunId, requireChecksums = true }) {
  const absoluteRoot = path.resolve(root);
  const errors = [];
  if (!STABLE_TAG_PATTERN.test(tag ?? "")) errors.push("release bundle requires a stable app-v tag");
  if (!COMMIT_PATTERN.test(commitSha ?? "")) errors.push("release bundle requires a 40-character commit SHA");
  if (!/^\d+$/.test(String(workflowRunId ?? ""))) errors.push("release bundle requires a numeric workflow run ID");
  try {
    collectRegularFiles(absoluteRoot);
  } catch (error) {
    return { errors: [...errors, error.message] };
  }

  const descriptors = [];
  for (const platform of Object.keys(RELEASE_PLATFORMS)) {
    const descriptorPath = path.join(absoluteRoot, "desktop", platform, "release-entry.json");
    if (!existsFile(descriptorPath)) {
      errors.push(`missing desktop descriptor for ${platform}`);
      continue;
    }
    try {
      const descriptor = readJson(descriptorPath);
      descriptors.push(descriptor);
      errors.push(...descriptorErrors({ root: absoluteRoot, descriptor, platform, tag, version, commitSha }));
    } catch {
      errors.push(`desktop descriptor for ${platform} is not valid JSON`);
    }
  }

  const capabilityRoot = path.join(absoluteRoot, "capabilities");
  const catalogPath = path.join(capabilityRoot, "install-catalog.json");
  const trustedKeysPath = path.join(capabilityRoot, "trusted-keys.json");
  const catalogProvenancePath = path.join(capabilityRoot, "catalog-provenance.json");
  if (![catalogPath, trustedKeysPath, catalogProvenancePath].every(existsFile)) {
    errors.push("capability catalog, trusted keys, and catalog provenance are required");
  } else {
    try {
      const catalog = readJson(catalogPath);
      const catalogProvenance = readJson(catalogProvenancePath);
      errors.push(...verifyCapabilityCatalog({
        catalog,
        trustedKeys: readJson(trustedKeysPath),
        mode: "release",
        expectedTag: tag,
        provenance: catalogProvenance,
        expectedCommit: commitSha,
        expectedRunId: String(workflowRunId),
      }).errors);
      const archives = collectRegularFiles(capabilityRoot).filter((file) => file.endsWith(".zip"));
      const expectedArchiveCount = CAPABILITY_TARGETS.length * CAPABILITY_PACKS.length;
      if (archives.length !== expectedArchiveCount) {
        errors.push(`release bundle must contain the manifest-derived exact matrix of ${expectedArchiveCount} capability archives, found ${archives.length}`);
      }
      const archivesByName = new Map(archives.map((file) => [path.basename(file), file]));
      if (archivesByName.size !== archives.length) errors.push("capability archive names must be globally unique");
      for (const entry of catalog.entries ?? []) {
        const name = path.posix.basename(new URL(entry.url).pathname);
        const archive = archivesByName.get(name);
        if (!archive) {
          errors.push(`catalog archive is missing from release bundle: ${name}`);
          continue;
        }
        const actualHash = crypto.createHash("sha256").update(fs.readFileSync(archive)).digest("hex");
        if (actualHash !== entry.archiveSha256) errors.push(`catalog archive hash does not match: ${name}`);
        if (fs.statSync(archive).size !== entry.compressedBytes) errors.push(`catalog compressedBytes does not match: ${name}`);
      }
    } catch (error) {
      errors.push(`capability release assets are invalid: ${error.message}`);
    }
  }

  const latestPath = path.join(absoluteRoot, "latest.json");
  if (!existsFile(latestPath)) errors.push("release bundle is missing latest.json");
  else {
    try {
      const updaterFiles = new Set(descriptors.map((descriptor) => descriptor.updater?.file).filter(Boolean));
      errors.push(...validateLatestJson({ manifest: readJson(latestPath), tag, version, assetFiles: updaterFiles }).errors);
    } catch {
      errors.push("latest.json is not valid JSON");
    }
  }

  const provenancePath = path.join(absoluteRoot, "provenance", "release-provenance.json");
  if (!existsFile(provenancePath)) errors.push("release bundle is missing release provenance");
  else {
    try { errors.push(...provenanceErrors(readJson(provenancePath), { tag, commitSha, workflowRunId })); }
    catch { errors.push("release provenance is not valid JSON"); }
  }
  const attestationPath = path.join(absoluteRoot, "provenance", "github-attestation.jsonl");
  if (!existsFile(attestationPath) || fs.statSync(attestationPath).size === 0) {
    errors.push("release bundle is missing the exported GitHub attestation bundle");
  }
  const smokePath = path.join(absoluteRoot, "smoke", "packaged-smoke-summary.json");
  if (!existsFile(smokePath)) errors.push("release bundle is missing packaged smoke evidence");
  else {
    try { errors.push(...smokeErrors(readJson(smokePath))); }
    catch { errors.push("packaged smoke summary is not valid JSON"); }
  }
  errors.push(...sbomErrors(absoluteRoot));
  for (const required of ["release-notes.md", "known-limitations.md"]) {
    const file = path.join(absoluteRoot, required);
    if (!existsFile(file) || fs.readFileSync(file, "utf8").trim().length === 0) {
      errors.push(`release bundle is missing non-empty ${required}`);
    } else {
      const document = fs.readFileSync(file, "utf8");
      if (!document.includes(version)) errors.push(`${required} must name release version ${version}`);
      if (/\b(?:TODO|TBD)\b/i.test(document)) errors.push(`${required} contains an unresolved placeholder`);
    }
  }

  if (requireChecksums) {
    const checksumsPath = path.join(absoluteRoot, "CHECKSUMS.sha256");
    if (!existsFile(checksumsPath)) errors.push("release bundle is missing CHECKSUMS.sha256");
    else if (fs.readFileSync(checksumsPath, "utf8") !== checksumDocument(absoluteRoot, checksumsPath)) {
      errors.push("CHECKSUMS.sha256 does not match the published release asset set");
    }
  }
  return { errors };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    for (const required of ["root", "tag", "version", "commit", "runId"]) {
      if (!options[required]) throw new Error(`--${required} is required`);
    }
    const { errors } = verifyReleaseAssets({
      root: options.root,
      tag: options.tag,
      version: options.version,
      commitSha: options.commit,
      workflowRunId: options.runId,
      requireChecksums: options.requireChecksums !== "false",
    });
    if (errors.length > 0) throw new Error(errors.join("; "));
    process.stdout.write("[release-assets] verified atomic four-platform release bundle\n");
  } catch (error) {
    process.stderr.write(`[release-assets] ${error.message}\n`);
    process.exitCode = 1;
  }
}

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { verifyChecksums } from "./generate-release-checksums.mjs";
import {
  COMMIT_PATTERN,
  PRERELEASE_TAG_PATTERN,
  RELEASE_PLATFORMS,
  isPlainObject,
  osIdentityEvidenceErrors,
  parseNamedArguments,
  safeAssetName,
} from "./release-assets-contract.mjs";

export const PRERELEASE_PLATFORMS = Object.freeze([
  "windows-x86_64",
  "darwin-aarch64",
  "darwin-x86_64",
]);

const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
const regularFile = (file) => fs.existsSync(file) && fs.statSync(file).isFile();

const exactSorted = (actual, expected) => (
  JSON.stringify([...actual].sort()) === JSON.stringify([...expected].sort())
);

const tagVersion = (tag) => {
  const match = PRERELEASE_TAG_PATTERN.exec(tag ?? "");
  return match ? `${match[1]}.${match[2]}.${match[3]}-rc.${match[4]}` : null;
};

export function verifyPrereleaseAssets({ root, tag, version, commitSha, workflowRunId }) {
  const absoluteRoot = path.resolve(root);
  const errors = [];
  if (tagVersion(tag) !== version) errors.push("prerelease tag and configured version must match exactly");
  if (!COMMIT_PATTERN.test(commitSha ?? "")) errors.push("commit SHA must be 40 lowercase hex characters");
  if (!/^[1-9]\d*$/.test(workflowRunId ?? "")) errors.push("workflow run ID must be a positive integer");
  if (fs.existsSync(path.join(absoluteRoot, "latest.json"))) {
    errors.push("prerelease bundles must not contain stable-channel latest.json");
  }
  if (fs.existsSync(path.join(absoluteRoot, "desktop", "linux-x86_64"))) {
    errors.push("Windows/macOS prerelease bundles must not contain a Linux desktop artifact");
  }

  const desktopRoot = path.join(absoluteRoot, "desktop");
  const actualPlatforms = fs.existsSync(desktopRoot)
    ? fs.readdirSync(desktopRoot, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
    : [];
  if (!exactSorted(actualPlatforms, PRERELEASE_PLATFORMS)) {
    errors.push(`prerelease desktop platforms must be exactly ${PRERELEASE_PLATFORMS.join(", ")}`);
  }

  for (const platform of PRERELEASE_PLATFORMS) {
    const directory = path.join(desktopRoot, platform);
    const descriptorPath = path.join(directory, "release-entry.json");
    if (!regularFile(descriptorPath)) {
      errors.push(`${platform} release-entry.json is missing`);
      continue;
    }
    let descriptor;
    try {
      descriptor = readJson(descriptorPath);
    } catch {
      errors.push(`${platform} release-entry.json is invalid JSON`);
      continue;
    }
    if (descriptor.releaseTag !== tag || descriptor.version !== version || descriptor.commitSha !== commitSha) {
      errors.push(`${platform} release coordinates do not match the prerelease`);
    }
    if (descriptor.platform !== platform || descriptor.targetTriple !== RELEASE_PLATFORMS[platform]) {
      errors.push(`${platform} target identity is invalid`);
    }
    errors.push(...osIdentityEvidenceErrors(descriptor.osSigning, platform));
    for (const [kind, fileName] of [
      ["installer", descriptor.installer?.file],
      ["updater", descriptor.updater?.file],
      ["updater signature", descriptor.updater?.signatureFile],
    ]) {
      if (!safeAssetName(fileName) || !regularFile(path.join(directory, fileName ?? ""))) {
        errors.push(`${platform} ${kind} is missing or unsafe`);
      }
    }
    if (typeof descriptor.updater?.signature !== "string"
      || descriptor.updater.signature.trim().length < 32
      || !regularFile(path.join(directory, descriptor.updater?.signatureFile ?? ""))
      || (regularFile(path.join(directory, descriptor.updater?.signatureFile ?? ""))
        && fs.readFileSync(path.join(directory, descriptor.updater.signatureFile), "utf8").trim()
          !== descriptor.updater.signature.trim())) {
      errors.push(`${platform} updater signature descriptor is invalid`);
    }
  }

  const provenancePath = path.join(absoluteRoot, "provenance", "release-provenance.json");
  if (!regularFile(provenancePath)) {
    errors.push("prerelease provenance is missing");
  } else {
    const provenance = readJson(provenancePath);
    if (provenance.repository !== "StoneLL1/llm-wiki-desktop"
      || provenance.releaseTag !== tag
      || provenance.version !== version
      || provenance.commitSha !== commitSha
      || String(provenance.workflowRunId) !== workflowRunId
      || provenance.channel !== "prerelease"
      || !exactSorted(provenance.platforms ?? [], PRERELEASE_PLATFORMS)) {
      errors.push("prerelease provenance coordinates or channel are invalid");
    }
  }

  const smokePath = path.join(absoluteRoot, "smoke", "packaged-smoke-summary.json");
  if (!regularFile(smokePath)) {
    errors.push("packaged smoke summary is missing");
  } else {
    const smoke = readJson(smokePath);
    const platforms = Array.isArray(smoke.platforms) ? smoke.platforms : [];
    if (smoke.fixtureEndpointProductionUntouched !== true
      || !exactSorted(platforms.map((entry) => entry.platform), PRERELEASE_PLATFORMS)
      || platforms.some((entry) => entry.status !== "passed"
        || entry.productionEndpointAccessed !== false
        || !["install-launch", "packaged-process-alive"].every((journey) => entry.journeys?.includes(journey)))) {
      errors.push("packaged smoke evidence is incomplete for the prerelease platforms");
    }
  }

  for (const relative of [
    "sbom/node.cdx.json",
    "sbom/rust.cdx.json",
    "release-notes.md",
    "known-limitations.md",
    "provenance/github-attestation.jsonl",
    "CHECKSUMS.sha256",
  ]) {
    if (!regularFile(path.join(absoluteRoot, relative))) errors.push(`${relative} is missing`);
  }
  for (const [relative, heading] of [
    ["release-notes.md", `# LLM Wiki Desktop ${version}`],
    ["known-limitations.md", `# Known limitations for ${version}`],
  ]) {
    const file = path.join(absoluteRoot, relative);
    if (regularFile(file) && fs.readFileSync(file, "utf8").split(/\r?\n/, 1)[0] !== heading) {
      errors.push(`${relative} heading does not match prerelease version ${version}`);
    }
  }
  for (const relative of ["sbom/node.cdx.json", "sbom/rust.cdx.json"]) {
    const file = path.join(absoluteRoot, relative);
    if (regularFile(file)) {
      const sbom = readJson(file);
      if (!isPlainObject(sbom) || sbom.bomFormat !== "CycloneDX") errors.push(`${relative} is not CycloneDX`);
    }
  }
  if (regularFile(path.join(absoluteRoot, "CHECKSUMS.sha256"))) {
    try {
      verifyChecksums(absoluteRoot, path.join(absoluteRoot, "CHECKSUMS.sha256"));
    } catch (error) {
      errors.push(error.message);
    }
  }
  return { errors, platforms: PRERELEASE_PLATFORMS };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    for (const required of ["root", "tag", "version", "commit", "runId"]) {
      if (!options[required]) throw new Error(`--${required} is required`);
    }
    const result = verifyPrereleaseAssets({
      root: options.root,
      tag: options.tag,
      version: options.version,
      commitSha: options.commit,
      workflowRunId: options.runId,
    });
    if (result.errors.length > 0) throw new Error(result.errors.join("; "));
    process.stdout.write(`[prerelease-assets] verified ${result.platforms.length} desktop platforms without latest.json\n`);
  } catch (error) {
    process.stderr.write(`[prerelease-assets] ${error.message}\n`);
    process.exitCode = 1;
  }
}

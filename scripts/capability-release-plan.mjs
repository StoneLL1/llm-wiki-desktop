import fs from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// These are route-union formats, not inputs that the named pack can consume
// without the preceding capability in the product pipeline. Keep this allowlist
// in executable release policy so corpus data alone cannot weaken qualification.
const approvedIndirectExtensions = Object.freeze({
  "document-standard": ["doc", "ppt", "xls"],
  "media-runtime": ["wma"],
});

async function readJson(relativePath, root = repositoryRoot) {
  return JSON.parse(await fs.readFile(path.join(root, relativePath), "utf8"));
}

async function exists(relativePath, root) {
  return Boolean(await fs.stat(path.join(root, relativePath)).catch(() => null));
}

function lookup(source, dottedPath) {
  return dottedPath.split(".").reduce((value, key) => value?.[key], source);
}

function licenseTerms(expression) {
  return [...expression.matchAll(/[A-Za-z][A-Za-z0-9.+-]*/gu)]
    .map((match) => match[0])
    .filter((term) => !["AND", "OR", "WITH"].includes(term));
}

// Declared digests are recorded over the canonical LF bytes that Git stores.
// A Windows checkout with `core.autocrlf=true` materializes CRLF lock files,
// so hash the line-ending-normalized content to keep verification identical
// across contributor machines and CI runners.
function lockDigest(bytes) {
  return createHash("sha256").update(bytes.toString("utf8").replace(/\r\n/gu, "\n")).digest("hex");
}

async function validateSourceLock(name, source, root, targets) {
  const errors = [];
  const localLock = source.lock || (source.lockSha256 && typeof source.source === "string" && !source.source.startsWith("https://") ? source.source : null);
  if (localLock) {
    const bytes = await fs.readFile(path.join(root, localLock)).catch(() => null);
    const digest = bytes && lockDigest(bytes);
    if (!bytes || digest !== source.lockSha256) errors.push(`${name} dependency lock digest is not exact`);
  }
  if (source.distributions) {
    for (const target of targets) {
      const artifact = source.distributions[target];
      if (!artifact || !/^[0-9a-f]{64}$/u.test(artifact.sha256) ||
          !Number.isSafeInteger(artifact.bytes) || artifact.bytes <= 0) {
        errors.push(`${name} has no SHA-256 and byte lock for ${target}`);
      }
    }
  }
  if (source.models?.repository && !/^[0-9a-f]{40}$/u.test(source.models.revision || "")) {
    errors.push(`${name} model repository revision is not an exact commit`);
  }
  if (source.models?.repository && (typeof source.models.layoutRepository !== "string" ||
      !/^[0-9a-f]{40}$/u.test(source.models.layoutRevision || ""))) {
    errors.push(`${name} layout model repository revision is not an exact commit`);
  }
  for (const [lockField, digestField] of [["dependencyLock", "dependencyLockSha256"]]) {
    if (source[lockField]) {
      const bytes = await fs.readFile(path.join(root, source[lockField])).catch(() => null);
      const digest = bytes && lockDigest(bytes);
      if (!bytes || digest !== source[digestField]) errors.push(`${name} ${lockField} digest is not exact`);
    }
  }
  function inspect(value, label) {
    if (!value || typeof value !== "object") return;
    if (Object.hasOwn(value, "sha256")) {
      if (!/^[0-9a-f]{64}$/u.test(value.sha256) || /^0+$/u.test(value.sha256)) errors.push(`${label} has no exact SHA-256`);
      if (!Number.isSafeInteger(value.bytes) || value.bytes <= 0) errors.push(`${label} has no exact byte count`);
      if (typeof value.source === "string" && value.source.includes("://") && !value.source.startsWith("https://")) {
        errors.push(`${label} source is not public HTTPS`);
      }
    }
    for (const [key, child] of Object.entries(value)) inspect(child, `${label}.${key}`);
  }
  inspect(source, name);
  return errors;
}

const runnerForTarget = (target) => ({
  "x86_64-pc-windows-msvc": "windows-2025",
  "aarch64-apple-darwin": "macos-15",
  "x86_64-apple-darwin": "macos-15-intel",
  "x86_64-unknown-linux-gnu": "ubuntu-24.04",
})[target];

export async function buildCapabilityReleasePlan(root = repositoryRoot) {
  const [manifest, recipes, sources, corpus] = await Promise.all([
    readJson("capabilities/product-manifest.json", root),
    readJson("capabilities/release-recipes.json", root),
    readJson("capabilities/release-sources.json", root),
    readJson("capabilities/qualification-corpus.json", root),
  ]);
  const errors = [];
  const indirectCapabilityIds = new Set([
    ...Object.keys(approvedIndirectExtensions),
    ...Object.keys(corpus.indirectExtensionsByCapability ?? {}),
  ]);
  for (const capabilityId of indirectCapabilityIds) {
    const declared = [...(corpus.indirectExtensionsByCapability?.[capabilityId] ?? [])].sort();
    const approved = [...(approvedIndirectExtensions[capabilityId] ?? [])].sort();
    if (JSON.stringify(declared) !== JSON.stringify(approved)) {
      errors.push(`${capabilityId} indirect qualification extensions are not approved`);
    }
  }
  const published = manifest.definitions
    .filter((definition) => definition.distributionTier === "published")
    .sort((left, right) => left.capabilityId.localeCompare(right.capabilityId));
  const entries = [];
  const checkedSources = new Set();
  for (const definition of published) {
    const recipe = recipes.recipes?.[definition.capabilityId];
    if (!recipe) errors.push(`${definition.capabilityId} has no release recipe`);
    if (definition.release.stagingStatus !== "implemented") errors.push(`${definition.capabilityId} staging is not implemented`);
    if (definition.qualification.status !== "implemented") errors.push(`${definition.capabilityId} qualification is not implemented`);
    for (const entrypoint of [definition.release.stagingScript, definition.qualification.entrypoint]) {
      if (!entrypoint || !(await exists(entrypoint, root))) errors.push(`${definition.capabilityId} is missing ${entrypoint || "an entrypoint"}`);
    }
    for (const sourceName of recipe?.sources || []) {
      const source = lookup(sources, sourceName);
      if (!source || typeof source.version !== "string" || typeof source.license !== "string") {
        errors.push(`${definition.capabilityId} source ${sourceName} is not version/license locked`);
      }
      if (source?.license && licenseTerms(source.license).some((term) => !definition.licensePolicy.expression.includes(term))) {
        errors.push(`${definition.capabilityId} product license omits ${sourceName}: ${source.license}`);
      }
      if (source && !checkedSources.has(sourceName)) {
        checkedSources.add(sourceName);
        errors.push(...await validateSourceLock(sourceName, source, root, manifest.supportedTargets));
      }
    }
    if (!definition.licensePolicy.thirdPartyNotices.length) {
      errors.push(`${definition.capabilityId} has no published third-party notice policy`);
    }
    if (recipe?.modelSource && !lookup(sources, recipe.modelSource)) errors.push(`${definition.capabilityId} model source is not locked`);
    for (const extension of definition.formats.extensions) {
      const capabilityFixture = corpus.fixtureByCapability?.[definition.capabilityId]?.[extension];
      const fixture = capabilityFixture ?? corpus.fixtureByExtension?.[extension];
      const fixtureRoot = capabilityFixture ? corpus.capabilityFixtureRoot : corpus.root;
      if (!fixtureRoot || !fixture || !(await exists(path.join(fixtureRoot, fixture), root))) {
        errors.push(`${definition.capabilityId} format ${extension} has no real qualification fixture`);
      }
    }
    for (const extension of corpus.indirectExtensionsByCapability?.[definition.capabilityId] ?? []) {
      if (!definition.formats.extensions.includes(extension)) {
        errors.push(`${definition.capabilityId} marks undeclared format ${extension} as indirect`);
      }
    }
    // Each published provider ships exactly its declared target subset; the
    // product target set is the union surface, not a per-capability obligation.
    for (const targetTriple of definition.supportedTargets) {
      if (!manifest.supportedTargets.includes(targetTriple)) errors.push(`${definition.capabilityId} declares unknown target ${targetTriple}`);
      entries.push({
        capabilityId: definition.capabilityId,
        targetTriple,
        os: runnerForTarget(targetTriple),
        family: recipe?.family,
        stagingScript: definition.release.stagingScript,
        qualificationEntrypoint: definition.qualification.entrypoint,
        routes: definition.routes,
        extensions: definition.formats.extensions,
        platformContentTypes: definition.formats.platformContentTypes,
      });
    }
  }
  const unique = new Set(entries.map((entry) => `${entry.capabilityId}\0${entry.targetTriple}`));
  const expectedEntryCount = published.reduce((total, definition) => total + definition.supportedTargets.length, 0);
  if (unique.size !== entries.length) errors.push("release plan contains duplicate pack × target entries");
  if (entries.length !== expectedEntryCount) errors.push("release plan is not the exact sum of per-capability target sets");
  if (!Array.isArray(corpus.generatedCases) || corpus.generatedCases.length !== 5) errors.push("qualification corpus must define the five required generated cases");
  return { errors, manifest, entries, expectedEntryCount };
}

async function main() {
  const { errors, entries, expectedEntryCount } = await buildCapabilityReleasePlan();
  if (errors.length) {
    for (const error of errors) process.stderr.write(`[capability-release-plan] ${error}\n`);
    process.exitCode = 1;
    return;
  }
  process.stdout.write(`${JSON.stringify({ include: entries, expectedEntryCount })}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`[capability-release-plan] ${error.message}\n`);
    process.exitCode = 2;
  });
}

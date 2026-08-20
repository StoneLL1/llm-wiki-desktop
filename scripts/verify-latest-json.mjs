import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  exactReleaseAssetUrl,
  isPlainObject,
  parseNamedArguments,
  RELEASE_PLATFORMS,
  safeAssetName,
  STABLE_TAG_PATTERN,
} from "./release-assets-contract.mjs";

const SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function validateLatestJson({ manifest, tag, version, assetFiles = null }) {
  const errors = [];
  if (!STABLE_TAG_PATTERN.test(tag ?? "")) errors.push("latest.json requires an exact stable app-v tag");
  if (!SEMVER_PATTERN.test(version ?? "")) errors.push("latest.json requires a stable SemVer version");
  if (!isPlainObject(manifest)) return { errors: [...errors, "latest.json must be an object"] };
  if (manifest.version !== version) errors.push(`latest.json version must equal ${version}`);
  if (typeof manifest.notes !== "string" || manifest.notes.trim().length === 0) {
    errors.push("latest.json notes must be non-empty");
  }
  if (typeof manifest.pub_date !== "string" || !Number.isFinite(Date.parse(manifest.pub_date))) {
    errors.push("latest.json pub_date must be an ISO date-time");
  }
  const platforms = isPlainObject(manifest.platforms) ? manifest.platforms : {};
  const actualKeys = Object.keys(platforms).sort();
  const expectedKeys = Object.keys(RELEASE_PLATFORMS).sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    errors.push("latest.json must contain exactly the four supported updater platforms");
  }
  for (const platform of expectedKeys) {
    const entry = platforms[platform];
    if (!isPlainObject(entry)) {
      errors.push(`${platform} entry must be an object`);
      continue;
    }
    let fileName = null;
    try {
      const url = new URL(entry.url);
      fileName = decodeURIComponent(path.posix.basename(url.pathname));
      if (!safeAssetName(fileName) || entry.url !== exactReleaseAssetUrl(tag, fileName)) {
        errors.push(`${platform} url must use the canonical repository, exact tag, and encoded asset name`);
      }
    } catch {
      errors.push(`${platform} url must be a valid HTTPS URL`);
    }
    if (typeof entry.signature !== "string" || entry.signature.trim().length < 32 || entry.signature.length > 16_384) {
      errors.push(`${platform} signature must contain a bounded updater signature`);
    }
    if (assetFiles && fileName && !assetFiles.has(fileName)) {
      errors.push(`${platform} url asset is missing from the release bundle: ${fileName}`);
    }
  }
  return { errors };
}
export function generateLatestJson({ descriptors, tag, version, notes, pubDate }) {
  const platforms = {};
  for (const descriptor of descriptors) {
    const platform = descriptor.platform;
    const updater = descriptor.updater;
    if (!Object.hasOwn(RELEASE_PLATFORMS, platform)) throw new Error(`unsupported platform: ${platform}`);
    if (platforms[platform]) throw new Error(`duplicate platform descriptor: ${platform}`);
    if (descriptor.releaseTag !== tag || descriptor.version !== version) {
      throw new Error(`${platform} descriptor release coordinate does not match ${tag} / ${version}`);
    }
    if (!safeAssetName(updater?.file) || typeof updater?.signature !== "string") {
      throw new Error(`${platform} descriptor has an invalid updater asset`);
    }
    platforms[platform] = {
      signature: updater.signature.trim(),
      url: exactReleaseAssetUrl(tag, updater.file),
    };
  }
  const manifest = { version, notes, pub_date: pubDate, platforms };
  const { errors } = validateLatestJson({ manifest, tag, version });
  if (errors.length > 0) throw new Error(errors.join("; "));
  return manifest;
}

const descriptorFiles = (root) => Object.keys(RELEASE_PLATFORMS).map((platform) =>
  path.join(path.resolve(root), "desktop", platform, "release-entry.json"));

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    if (!options.manifest || !options.tag || !options.version) {
      throw new Error("--manifest, --tag, and --version are required");
    }
    if (options.generate === "true") {
      if (!options.root || !options.notes || !options.pubDate) {
        throw new Error("generation requires --root, --notes, and --pubDate");
      }
      const notes = fs.readFileSync(path.resolve(options.notes), "utf8").trim();
      const descriptors = descriptorFiles(options.root).map((file) => JSON.parse(fs.readFileSync(file, "utf8")));
      const manifest = generateLatestJson({ descriptors, tag: options.tag, version: options.version, notes, pubDate: options.pubDate });
      fs.mkdirSync(path.dirname(path.resolve(options.manifest)), { recursive: true });
      fs.writeFileSync(path.resolve(options.manifest), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
    }
    const manifest = JSON.parse(fs.readFileSync(path.resolve(options.manifest), "utf8"));
    const assetFiles = options.root
      ? new Set(descriptorFiles(options.root).map((file) => JSON.parse(fs.readFileSync(file, "utf8")).updater.file))
      : null;
    const { errors } = validateLatestJson({ manifest, tag: options.tag, version: options.version, assetFiles });
    if (errors.length > 0) throw new Error(errors.join("; "));
    process.stdout.write("[latest-json] verified four exact-tag updater entries\n");
  } catch (error) {
    process.stderr.write(`[latest-json] ${error.message}\n`);
    process.exitCode = 1;
  }
}

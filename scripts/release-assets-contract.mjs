import fs from "node:fs";
import path from "node:path";

export const RELEASE_REPOSITORY = "StoneLL1/llm-wiki-desktop";
export const RELEASE_PLATFORMS = Object.freeze({
  "windows-x86_64": "x86_64-pc-windows-msvc",
  "darwin-aarch64": "aarch64-apple-darwin",
  "darwin-x86_64": "x86_64-apple-darwin",
  "linux-x86_64": "x86_64-unknown-linux-gnu",
});

export const OS_IDENTITY_EVIDENCE = Object.freeze({
  "windows-x86_64": Object.freeze({
    schemaVersion: 1,
    kind: "windows-authenticode-not-required",
    platform: "windows-x86_64",
    required: false,
    verified: true,
    updaterSignatureRequired: true,
    userNotice: "smartscreen-or-unknown-publisher-warning-expected",
  }),
  "darwin-aarch64": Object.freeze({
    schemaVersion: 1,
    kind: "apple-developer-id-not-required",
    platform: "darwin-aarch64",
    required: false,
    verified: true,
    updaterSignatureRequired: true,
    userNotice: "gatekeeper-manual-override-may-be-required",
  }),
  "darwin-x86_64": Object.freeze({
    schemaVersion: 1,
    kind: "apple-developer-id-not-required",
    platform: "darwin-x86_64",
    required: false,
    verified: true,
    updaterSignatureRequired: true,
    userNotice: "gatekeeper-manual-override-may-be-required",
  }),
  "linux-x86_64": Object.freeze({
    schemaVersion: 1,
    kind: "linux-os-code-signing-not-applicable",
    platform: "linux-x86_64",
    required: false,
    verified: true,
    updaterSignatureRequired: true,
    userNotice: "linux-distribution-integration-varies",
  }),
});

export const STABLE_TAG_PATTERN = /^app-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
export const COMMIT_PATTERN = /^[0-9a-f]{40}$/;

export function isPlainObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function osIdentityEvidenceErrors(evidence, platform) {
  const expected = OS_IDENTITY_EVIDENCE[platform];
  if (!expected) return [`unsupported desktop platform: ${platform}`];
  if (!isPlainObject(evidence)) return [`${platform} OS identity evidence must be an object`];
  const errors = [];
  const actualKeys = Object.keys(evidence).sort();
  const expectedKeys = Object.keys(expected).sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    errors.push(`${platform} OS identity evidence must contain exactly the policy fields`);
  }
  for (const [key, value] of Object.entries(expected)) {
    if (evidence[key] !== value) errors.push(`${platform} OS identity evidence has invalid ${key}`);
  }
  return errors;
}

export function safeAssetName(value) {
  return typeof value === "string"
    && value.length > 0
    && value.length <= 240
    && path.basename(value) === value
    && !/[\\/:*?"<>|]/.test(value)
    && [...value].every((character) => character.codePointAt(0) >= 32);
}

export function githubReleaseAssetName(value) {
  if (!safeAssetName(value)) throw new Error(`invalid public release asset name: ${value}`);
  const normalized = value.replace(/\s+/gu, ".");
  if (!safeAssetName(normalized) || /\s/u.test(normalized)) {
    throw new Error(`cannot normalize public release asset name: ${value}`);
  }
  return normalized;
}

export function exactReleaseAssetUrl(tag, fileName) {
  return `https://github.com/${RELEASE_REPOSITORY}/releases/download/${tag}/${encodeURIComponent(fileName)}`;
}

export function collectRegularFiles(root) {
  const absoluteRoot = path.resolve(root);
  const pending = [absoluteRoot];
  const files = [];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const target = path.join(current, entry.name);
      const stats = fs.lstatSync(target);
      if (stats.isSymbolicLink()) throw new Error(`release trees cannot contain symlinks: ${target}`);
      if (stats.isDirectory()) pending.push(target);
      else if (stats.isFile()) files.push(target);
      else throw new Error(`release trees can contain only regular files: ${target}`);
    }
  }
  return files.sort((left, right) => left.localeCompare(right, "en"));
}

export function publishedReleaseFiles(root) {
  const files = collectRegularFiles(root).filter((file) => {
    const name = path.basename(file);
    return name !== "release-entry.json" && name !== "CHECKSUMS.sha256";
  });
  const names = new Set();
  for (const file of files) {
    const name = path.basename(file);
    if (!safeAssetName(name)) throw new Error(`invalid public release asset name: ${name}`);
    if (names.has(name)) throw new Error(`duplicate public release asset name: ${name}`);
    names.add(name);
  }
  return files.sort((left, right) => path.basename(left).localeCompare(path.basename(right), "en"));
}

export function parseNamedArguments(arguments_) {
  const result = {};
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index];
    const value = arguments_[index + 1];
    if (!name?.startsWith("--") || value == null) throw new Error(`unknown or incomplete argument: ${name ?? ""}`);
    result[name.slice(2)] = value;
  }
  return result;
}

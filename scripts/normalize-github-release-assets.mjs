import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  githubReleaseAssetName,
  parseNamedArguments,
  RELEASE_PLATFORMS,
  safeAssetName,
} from "./release-assets-contract.mjs";

const descriptorReferences = (descriptor) => [
  [descriptor.installer, "file"],
  [descriptor.updater, "file"],
  [descriptor.updater, "signatureFile"],
];

export function normalizeGitHubReleaseAssets(root) {
  const absoluteRoot = path.resolve(root);
  const descriptors = [];
  const moves = new Map();

  for (const platform of Object.keys(RELEASE_PLATFORMS)) {
    const directory = path.join(absoluteRoot, "desktop", platform);
    const descriptorPath = path.join(directory, "release-entry.json");
    const descriptor = JSON.parse(fs.readFileSync(descriptorPath, "utf8"));
    if (descriptor.platform !== platform) throw new Error(`desktop descriptor platform mismatch: ${platform}`);

    for (const [owner, field] of descriptorReferences(descriptor)) {
      const currentName = owner?.[field];
      if (!safeAssetName(currentName)) throw new Error(`invalid ${platform} ${field} asset name`);
      const normalizedName = githubReleaseAssetName(currentName);
      const source = path.join(directory, currentName);
      const target = path.join(directory, normalizedName);
      if (!fs.existsSync(source) || !fs.lstatSync(source).isFile()) {
        throw new Error(`missing ${platform} release asset: ${currentName}`);
      }
      if (source !== target) {
        if (fs.existsSync(target)) throw new Error(`normalized release asset already exists: ${normalizedName}`);
        moves.set(source, target);
      }
      owner[field] = normalizedName;
    }
    descriptors.push({ descriptorPath, descriptor });
  }

  for (const [source, target] of moves) fs.renameSync(source, target);
  for (const { descriptorPath, descriptor } of descriptors) {
    fs.writeFileSync(descriptorPath, `${JSON.stringify(descriptor, null, 2)}\n`, "utf8");
  }
  return { renamedAssets: moves.size, updatedDescriptors: descriptors.length };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    if (!options.root) throw new Error("--root is required");
    const result = normalizeGitHubReleaseAssets(options.root);
    process.stdout.write(`[github-release-assets] normalized ${result.renamedAssets} assets across ${result.updatedDescriptors} descriptors\n`);
  } catch (error) {
    process.stderr.write(`[github-release-assets] ${error.message}\n`);
    process.exitCode = 1;
  }
}

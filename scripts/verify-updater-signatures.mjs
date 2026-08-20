import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import {
  parseNamedArguments,
  RELEASE_PLATFORMS,
  safeAssetName,
} from "./release-assets-contract.mjs";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const cargoManifest = path.join(repositoryRoot, "src-tauri", "Cargo.toml");

const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"));

function updaterPairFromDescriptor(descriptorPath) {
  const directory = path.dirname(path.resolve(descriptorPath));
  const descriptor = readJson(descriptorPath);
  if (!safeAssetName(descriptor.updater?.file) || !safeAssetName(descriptor.updater?.signatureFile)) {
    throw new Error(`invalid updater descriptor: ${descriptorPath}`);
  }
  return [path.join(directory, descriptor.updater.file), path.join(directory, descriptor.updater.signatureFile)];
}

function updaterPairsFromCandidate(root) {
  return Object.keys(RELEASE_PLATFORMS).flatMap((platform) => {
    const descriptorPath = path.join(path.resolve(root), "desktop", platform, "release-entry.json");
    return updaterPairFromDescriptor(descriptorPath);
  });
}

export function publishedUpdaterPairs(root, manifestPath) {
  const directory = path.resolve(root);
  const manifest = readJson(manifestPath);
  return Object.keys(RELEASE_PLATFORMS).flatMap((platform) => {
    const entry = manifest.platforms?.[platform];
    if (typeof entry?.url !== "string" || typeof entry?.signature !== "string") {
      throw new Error(`latest.json is missing updater data for ${platform}`);
    }
    const assetName = decodeURIComponent(new URL(entry.url).pathname.split("/").pop());
    const signatureName = `${assetName}.sig`;
    if (!safeAssetName(assetName) || !safeAssetName(signatureName)) throw new Error(`invalid updater asset name for ${platform}`);
    const signaturePath = path.join(directory, signatureName);
    if (fs.readFileSync(signaturePath, "utf8").trim() !== entry.signature.trim()) {
      throw new Error(`published updater signature does not match latest.json for ${platform}`);
    }
    return [path.join(directory, assetName), signaturePath];
  });
}

export function verifyUpdaterSignatures({ root, updaterPublicKey, manifestPath, descriptorPath }) {
  if (typeof updaterPublicKey !== "string" || updaterPublicKey.trim().length === 0) {
    throw new Error("the committed updater public key is required");
  }
  const pairs = descriptorPath
    ? updaterPairFromDescriptor(descriptorPath)
    : manifestPath
      ? publishedUpdaterPairs(root, manifestPath)
      : updaterPairsFromCandidate(root);
  const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
  const result = spawnSync(cargo, [
    "run", "--quiet", "--locked", "--no-default-features",
    "--manifest-path", cargoManifest,
    "--bin", "verify_update_signature", "--",
    updaterPublicKey.trim(), ...pairs,
  ], { cwd: repositoryRoot, encoding: "utf8", windowsHide: true });
  if (result.error) throw new Error(`cannot execute updater signature verifier: ${result.error.message}`);
  if (result.status !== 0) {
    throw new Error((result.stderr || result.stdout || "updater signature verification failed").trim());
  }
  return pairs.length / 2;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    if ((!options.root && !options.descriptor) || !options.updaterConfig) {
      throw new Error("--updaterConfig and either --root or --descriptor are required");
    }
    const updaterPublicKey = readJson(path.resolve(options.updaterConfig)).plugins?.updater?.pubkey;
    const count = verifyUpdaterSignatures({
      root: options.root,
      updaterPublicKey,
      manifestPath: options.manifest ? path.resolve(options.manifest) : undefined,
      descriptorPath: options.descriptor ? path.resolve(options.descriptor) : undefined,
    });
    process.stdout.write(`[updater-signatures] cryptographically verified ${count} updater artifacts\n`);
  } catch (error) {
    process.stderr.write(`[updater-signatures] ${error.message}\n`);
    process.exitCode = 1;
  }
}

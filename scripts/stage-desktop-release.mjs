import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  collectRegularFiles,
  COMMIT_PATTERN,
  isPlainObject,
  parseNamedArguments,
  RELEASE_PLATFORMS,
  STABLE_TAG_PATTERN,
} from "./release-assets-contract.mjs";

const BUNDLE_PATTERNS = {
  "windows-x86_64": { installer: /-setup\.exe$/i, updater: /-setup\.exe$/i },
  "darwin-aarch64": { installer: /\.dmg$/i, updater: /\.app\.tar\.gz$/i },
  "darwin-x86_64": { installer: /\.dmg$/i, updater: /\.app\.tar\.gz$/i },
  "linux-x86_64": { installer: /\.AppImage$/i, updater: /\.AppImage$/i },
};

const exactlyOne = (files, pattern, label) => {
  const matches = files.filter((file) => pattern.test(file));
  if (matches.length !== 1) throw new Error(`expected exactly one ${label}, found ${matches.length}`);
  return matches[0];
};

export function stageDesktopRelease({ source, output, platform, releaseTag, version, commitSha, signingEvidence }) {
  const patterns = BUNDLE_PATTERNS[platform];
  if (!patterns) throw new Error(`unsupported desktop platform: ${platform}`);
  if (!STABLE_TAG_PATTERN.test(releaseTag ?? "")) throw new Error("release tag must be a stable app-v tag");
  if (!COMMIT_PATTERN.test(commitSha ?? "")) throw new Error("commit SHA must be 40 lowercase hex characters");
  const evidence = JSON.parse(fs.readFileSync(path.resolve(signingEvidence), "utf8"));
  if (!isPlainObject(evidence) || evidence.verified !== true || typeof evidence.kind !== "string") {
    throw new Error("OS signing evidence must be a verified structured record");
  }
  const files = collectRegularFiles(source);
  const installer = exactlyOne(files, patterns.installer, `${platform} installer`);
  const updater = exactlyOne(files, patterns.updater, `${platform} updater`);
  const signature = `${updater}.sig`;
  if (!fs.existsSync(signature) || !fs.statSync(signature).isFile()) throw new Error(`missing updater signature: ${signature}`);
  const signatureText = fs.readFileSync(signature, "utf8").trim();
  if (signatureText.length < 32 || signatureText.length > 16_384) throw new Error("updater signature has an invalid length");

  const absoluteOutput = path.resolve(output);
  fs.mkdirSync(absoluteOutput, { recursive: true });
  const stagedName = (file) => `${platform}-${path.basename(file)}`;
  const installerName = stagedName(installer);
  const updaterName = stagedName(updater);
  const signatureName = `${updaterName}.sig`;
  fs.copyFileSync(installer, path.join(absoluteOutput, installerName));
  if (path.resolve(updater) !== path.resolve(installer)) {
    fs.copyFileSync(updater, path.join(absoluteOutput, updaterName));
  }
  fs.copyFileSync(signature, path.join(absoluteOutput, signatureName));
  const descriptor = {
    schemaVersion: 1,
    releaseTag,
    version,
    commitSha,
    platform,
    targetTriple: RELEASE_PLATFORMS[platform],
    installer: { file: installerName },
    updater: { file: updaterName, signatureFile: signatureName, signature: signatureText },
    osSigning: evidence,
  };
  fs.writeFileSync(path.join(absoluteOutput, "release-entry.json"), `${JSON.stringify(descriptor, null, 2)}\n`, "utf8");
  return descriptor;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    for (const required of ["source", "output", "platform", "tag", "version", "commit", "signingEvidence"]) {
      if (!options[required]) throw new Error(`--${required} is required`);
    }
    const descriptor = stageDesktopRelease({
      source: options.source,
      output: options.output,
      platform: options.platform,
      releaseTag: options.tag,
      version: options.version,
      commitSha: options.commit,
      signingEvidence: options.signingEvidence,
    });
    process.stdout.write(`[desktop-release] staged ${descriptor.platform}\n`);
  } catch (error) {
    process.stderr.write(`[desktop-release] ${error.message}\n`);
    process.exitCode = 1;
  }
}

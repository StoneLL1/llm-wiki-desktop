import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  COMMIT_PATTERN,
  DESKTOP_TAG_PATTERN,
  githubReleaseAssetName,
  osIdentityEvidenceErrors,
  parseNamedArguments,
  RELEASE_PLATFORMS,
  OS_IDENTITY_EVIDENCE,
  safeAssetName,
} from "./release-assets-contract.mjs";
import { generateLatestJson } from "./verify-latest-json.mjs";
import { writeChecksums } from "./generate-release-checksums.mjs";

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

const isTransientBundleDirectory = (name) => name.endsWith(".app") || name.endsWith(".AppDir");

function collectBundleArtifacts(root) {
  const absoluteRoot = path.resolve(root);
  const pending = [absoluteRoot];
  const files = [];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const target = path.join(current, entry.name);
      const stats = fs.lstatSync(target);
      if (stats.isSymbolicLink()) continue;
      if (stats.isDirectory()) {
        if (!isTransientBundleDirectory(entry.name)) pending.push(target);
      } else if (stats.isFile()) {
        files.push(target);
      }
    }
  }
  return files.sort((left, right) => left.localeCompare(right, "en"));
}

export function stageDesktopRelease({ source, output, platform, releaseTag, version, commitSha, signingEvidence }) {
  const patterns = BUNDLE_PATTERNS[platform];
  if (!patterns) throw new Error(`unsupported desktop platform: ${platform}`);
  if (!DESKTOP_TAG_PATTERN.test(releaseTag ?? "") || releaseTag !== `app-v${version}`) throw new Error("release tag and desktop version must agree");
  if (!COMMIT_PATTERN.test(commitSha ?? "")) throw new Error("commit SHA must be 40 lowercase hex characters");
  const evidence = signingEvidence
    ? JSON.parse(fs.readFileSync(path.resolve(signingEvidence), "utf8"))
    : OS_IDENTITY_EVIDENCE[platform];
  const evidenceErrors = osIdentityEvidenceErrors(evidence, platform);
  if (evidenceErrors.length > 0) throw new Error(evidenceErrors.join("; "));
  const files = collectBundleArtifacts(source);
  const installer = exactlyOne(files, patterns.installer, `${platform} installer`);
  const updater = exactlyOne(files, patterns.updater, `${platform} updater`);
  const signature = `${updater}.sig`;
  if (!fs.existsSync(signature) || !fs.lstatSync(signature).isFile()) throw new Error(`missing updater signature: ${signature}`);
  const signatureText = fs.readFileSync(signature, "utf8").trim();
  if (signatureText.length < 32 || signatureText.length > 16_384) throw new Error("updater signature has an invalid length");

  const absoluteOutput = path.resolve(output);
  fs.mkdirSync(absoluteOutput, { recursive: true });
  const stagedName = (file) => githubReleaseAssetName(`${platform}-${path.basename(file)}`);
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

export function assembleDesktopDownloads({ candidate, output, releaseTag, version, notes, pubDate }) {
  if (!DESKTOP_TAG_PATTERN.test(releaseTag ?? "") || releaseTag !== `app-v${version}`) {
    throw new Error("release tag and desktop version must agree");
  }
  const prerelease = version.includes("-rc.");
  const descriptors = [];
  const files = new Map();
  let commit;
  for (const platform of Object.keys(RELEASE_PLATFORMS)) {
    const directory = path.resolve(candidate, "desktop", platform);
    const descriptor = JSON.parse(fs.readFileSync(path.join(directory, "release-entry.json"), "utf8"));
    if (descriptor.platform !== platform || descriptor.releaseTag !== releaseTag || descriptor.version !== version
      || !COMMIT_PATTERN.test(descriptor.commitSha) || (commit && descriptor.commitSha !== commit)) {
      throw new Error(`${platform} build does not match the selected release`);
    }
    commit = descriptor.commitSha;
    descriptors.push(descriptor);
    const names = [descriptor.installer?.file, descriptor.updater?.file];
    if (prerelease) names.push(descriptor.updater?.signatureFile);
    for (const name of new Set(names)) {
      if (!safeAssetName(name) || files.has(name)) throw new Error(`invalid or duplicate desktop asset: ${name}`);
      const file = path.join(directory, name);
      const stat = fs.lstatSync(file);
      if (!stat.isFile() || stat.size === 0) throw new Error(`empty or non-regular desktop asset: ${name}`);
      files.set(name, file);
    }
  }
  const latest = prerelease ? null : generateLatestJson({ descriptors, tag: releaseTag, version, notes, pubDate });
  // Validate the complete candidate before writing its public download directory.
  if (fs.existsSync(output) && fs.readdirSync(output).length) throw new Error("public output directory must be empty");
  fs.mkdirSync(output, { recursive: true });
  for (const [name, file] of files) fs.copyFileSync(file, path.join(output, name));
  if (latest) fs.writeFileSync(path.join(output, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
  writeChecksums(output, path.join(output, "CHECKSUMS.sha256"));
  return { channel: prerelease ? "prerelease" : "stable", platformCount: descriptors.length };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    if (options.candidate) {
      for (const required of ["output", "tag", "version", "notes", "pubDate"]) {
        if (!options[required]) throw new Error(`--${required} is required`);
      }
      const result = assembleDesktopDownloads({
        candidate: options.candidate, output: options.output, releaseTag: options.tag,
        version: options.version, notes: fs.readFileSync(options.notes, "utf8").trim(), pubDate: options.pubDate,
      });
      process.stdout.write(`[desktop-release] assembled ${result.platformCount} ${result.channel} platforms\n`);
    } else {
      for (const required of ["source", "output", "platform", "tag", "version", "commit"]) {
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
    }
  } catch (error) {
    process.stderr.write(`[desktop-release] ${error.message}\n`);
    process.exitCode = 1;
  }
}

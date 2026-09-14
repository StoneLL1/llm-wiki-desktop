import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { parseNamedArguments, publishedReleaseFiles, RELEASE_REPOSITORY, STABLE_TAG_PATTERN } from "./release-assets-contract.mjs";

const RC_TAG_PATTERN = /^app-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-rc\.[1-9]\d*$/u;
const CAPABILITY_TAG_PATTERN = /^capabilities-[A-Za-z0-9][A-Za-z0-9._+-]*$/u;
const REPOSITORY_PATTERN = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;

export function capabilityReleaseLocation(baseUrl, expectedRepository) {
  const url = new URL(baseUrl);
  const parts = url.pathname.replace(/\/$/u, "").split("/");
  const repository = parts.slice(1, 3).join("/");
  const tag = parts[5];
  if (url.protocol !== "https:" || url.hostname !== "github.com" || url.port || url.username || url.password
    || url.search || url.hash || parts.length !== 6 || parts[3] !== "releases" || parts[4] !== "download"
    || !REPOSITORY_PATTERN.test(repository) || !CAPABILITY_TAG_PATTERN.test(tag ?? "")
    || (expectedRepository && repository.toLowerCase() !== expectedRepository.toLowerCase())) {
    throw new Error("GitHub resource publishing requires this repository's HTTPS releases/download/capabilities-TAG directory");
  }
  return { repository, tag };
}

function runGh(args) {
  return execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

async function digestFile(file) {
  const hash = crypto.createHash("sha256");
  for await (const chunk of fs.createReadStream(file)) hash.update(chunk);
  return `sha256:${hash.digest("hex")}`;
}

function releaseNotFound(error) {
  const failure = [error.message, error.stderr, error.stdout].filter(Boolean).join("\n");
  const statuses = [...failure.matchAll(/HTTP\s+(\d{3})/giu)].map((match) => Number(match[1]));
  // A permission or transport failure is not evidence that a release is absent.
  if (statuses.length) return statuses.every((status) => status === 404);
  return /^\s*release not found\s*$/imu.test(failure);
}

export async function releaseUploads(root) {
  const files = [...publishedReleaseFiles(root), path.resolve(root, "CHECKSUMS.sha256")];
  return Promise.all(files.map(async (file) => ({
    file, name: path.basename(file), size: fs.statSync(file).size, digest: await digestFile(file),
  })));
}

export function uploadedAssetErrors(uploads, assets) {
  const errors = [];
  for (const upload of uploads) {
    const matches = assets.filter((asset) => asset.name === upload.name);
    if (!matches.length) errors.push(`missing uploaded asset: ${upload.name}`);
    else if (matches.length !== 1) errors.push(`duplicate remote asset name: ${upload.name}`);
    else if (matches[0].state !== "uploaded" || matches[0].size !== upload.size || matches[0].digest !== upload.digest) {
      errors.push(`uploaded bytes do not match: ${upload.name}`);
    }
  }
  // Other attachments may be maintainer notes or assets from another workflow.
  // Leave them in place; publication only needs to establish our required files.
  return errors;
}

function newerStableTag(candidate, current) {
  const previous = STABLE_TAG_PATTERN.exec(current ?? "");
  if (!previous) return false;
  const next = STABLE_TAG_PATTERN.exec(candidate);
  for (let index = 1; index <= 3; index++) {
    if (BigInt(next[index]) !== BigInt(previous[index])) return BigInt(next[index]) > BigInt(previous[index]);
  }
  return false;
}

// Build/signature checks happen before this step. A retry resumes the same draft
// and never changes already public assets, including when GitHub omits digests.
export async function publishDesktopRelease({ root, tag, notesFile = path.resolve(root, "release-notes.md"), channel = "desktop", repository = RELEASE_REPOSITORY, target, gh = runGh }) {
  const capability = channel === "capabilities";
  const rc = RC_TAG_PATTERN.test(tag ?? "");
  if (!["desktop", "capabilities"].includes(channel)
    || !(capability ? CAPABILITY_TAG_PATTERN.test(tag ?? "") : STABLE_TAG_PATTERN.test(tag ?? "") || rc)) {
    throw new Error("a valid release tag for the selected channel is required");
  }
  if (!REPOSITORY_PATTERN.test(repository)) throw new Error("repository must be OWNER/REPO");
  if (target != null && (!capability || !/^[a-f0-9]{40}$/u.test(target))) throw new Error("--target is only supported for resource releases and must be a full commit SHA");
  const prerelease = capability || rc;
  const uploads = await releaseUploads(root);
  const endpoint = `repos/${repository}/releases`;
  const releaseForTag = async () => {
    try {
      const value = JSON.parse(await gh([
        "release", "view", tag, "--repo", repository, "--json", "databaseId,isDraft",
      ]));
      return { id: value.databaseId, draft: value.isDraft };
    } catch (error) {
      if (releaseNotFound(error)) return null;
      throw error;
    }
  };
  let release = await releaseForTag();
  if (!release) {
    await gh(["release", "create", tag, "--repo", repository, ...(target ? ["--target", target] : ["--verify-tag"]), "--draft", "--latest=false",
      "--title", capability ? `Optional capability resources ${tag.slice("capabilities-".length)}` : `LLM Wiki Desktop ${tag.slice(4)}`,
      "--notes-file", path.resolve(notesFile), ...(prerelease ? ["--prerelease"] : [])]);
    release = await releaseForTag();
    if (!release) throw new Error(`could not resolve the draft created for tag: ${tag}`);
  }
  const downloaded = new Map();
  const remoteAssets = async () => {
    const assets = JSON.parse(await gh(["api", `${endpoint}/${release.id}/assets?per_page=100`, "--paginate", "--slurp"])).flat();
    for (const upload of uploads) {
      const matches = assets.filter((asset) => asset.name === upload.name);
      if (matches.length !== 1) continue;
      const asset = matches[0];
      // A present digest is authoritative, including a mismatch. Only missing
      // metadata needs a download; starter/truncated assets need replacing.
      if (asset.state !== "uploaded" || asset.size !== upload.size || (asset.digest != null && asset.digest !== "")) continue;
      const identity = JSON.stringify([asset.id, asset.updated_at, asset.name, asset.size]);
      if (!downloaded.has(identity)) {
        const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "wiki-release-asset-"));
        try {
          const file = path.join(temporary, "asset");
          await gh(["release", "download", tag, "--repo", repository, "--pattern", upload.name, "--output", file]);
          downloaded.set(identity, fs.statSync(file).size === upload.size ? await digestFile(file) : "size-mismatch");
        } finally {
          fs.rmSync(temporary, { recursive: true, force: true });
        }
      }
      asset.digest = downloaded.get(identity);
    }
    return assets;
  };
  if (!release.draft) {
    const errors = uploadedAssetErrors(uploads, await remoteAssets());
    if (errors.length) throw new Error(`release is already public; refusing to change its assets: ${errors.join("; ")}`);
    return;
  }
  const existing = await remoteAssets();
  for (const upload of uploads) {
    if (!uploadedAssetErrors([upload], existing).length) continue;
    await gh(["release", "upload", tag, upload.file, "--repo", repository, "--clobber"]);
  }
  const errors = uploadedAssetErrors(uploads, await remoteAssets());
  if (errors.length) throw new Error(errors.join("; "));
  let latest = false;
  if (!prerelease) {
    // Query latest only at publication. A matching public retry returned above,
    // and publishing an older stable version must not roll back the update feed.
    try {
      const current = JSON.parse(await gh(["release", "view", "--repo", repository, "--json", "tagName"]));
      latest = newerStableTag(tag, current.tagName);
    } catch (error) {
      if (!releaseNotFound(error)) throw error;
      latest = true;
    }
  }
  await gh(["release", "edit", tag, "--repo", repository, "--draft=false", `--prerelease=${prerelease}`, latest ? "--latest" : "--latest=false"]);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { root, tag, notes, channel, repository, target } = parseNamedArguments(process.argv.slice(2));
    if (!root) throw new Error("--root is required");
    await publishDesktopRelease({ root, tag, notesFile: notes, channel, repository, target });
    process.stdout.write(`[release] ${tag} published with verified upload bytes\n`);
  } catch (error) {
    process.stderr.write(`[release] ${error.message}\n`);
    process.exitCode = 1;
  }
}

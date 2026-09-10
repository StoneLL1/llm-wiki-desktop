import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { parseNamedArguments, publishedReleaseFiles, RELEASE_REPOSITORY, STABLE_TAG_PATTERN } from "./release-assets-contract.mjs";

function runGh(args) {
  return execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

export async function releaseUploads(root) {
  const files = [...publishedReleaseFiles(root), path.resolve(root, "CHECKSUMS.sha256")];
  const uploads = [];
  for (const file of files) {
    const hash = crypto.createHash("sha256");
    for await (const chunk of fs.createReadStream(file)) hash.update(chunk);
    uploads.push({ file, name: path.basename(file), size: fs.statSync(file).size, digest: `sha256:${hash.digest("hex")}` });
  }
  return uploads;
}

export function uploadedAssetErrors(uploads, assets) {
  const errors = [];
  const byName = new Map(assets.map((asset) => [asset.name, asset]));
  const expected = new Set(uploads.map((asset) => asset.name));
  if (byName.size !== assets.length) errors.push("duplicate remote asset names");
  for (const upload of uploads) {
    const actual = byName.get(upload.name);
    if (!actual) errors.push(`missing uploaded asset: ${upload.name}`);
    else if (actual.state !== "uploaded" || actual.size !== upload.size || actual.digest !== upload.digest) {
      errors.push(`uploaded bytes do not match: ${upload.name}`);
    }
  }
  for (const asset of assets) {
    if (!expected.has(asset.name)) errors.push(`unexpected draft asset: ${asset.name}`);
  }
  return errors;
}

// Local signatures and the complete candidate are verified by the preceding
// workflow step. Compare GitHub's upload digests without downloading gigabytes
// again. Failures leave the draft available for the next run.
export async function publishDesktopRelease({ root, tag, notesFile = path.resolve(root, "release-notes.md"), channel = "desktop", gh = runGh }) {
  const capability = channel === "capabilities";
  const appTag = capability ? tag?.replace(/^capabilities-v/, "app-v") : tag;
  if (!["desktop", "capabilities"].includes(channel) || !STABLE_TAG_PATTERN.test(appTag ?? "")
    || (capability && !tag.startsWith("capabilities-v"))) throw new Error("a stable tag for the selected channel is required");
  const uploads = await releaseUploads(root);
  const endpoint = `repos/${RELEASE_REPOSITORY}/releases`;
  const releasesForTag = () => JSON.parse(
    gh(["api", `${endpoint}?per_page=100`, "--paginate", "--slurp"]),
  ).flat().filter((candidate) => candidate.tag_name === tag);
  let matches = releasesForTag();
  if (matches.length > 1) throw new Error(`duplicate releases exist for tag: ${tag}`);
  let release = matches[0];
  if (!release) {
    gh(["release", "create", tag, "--repo", RELEASE_REPOSITORY, "--verify-tag", "--draft",
      "--title", capability ? `Optional capability packs ${appTag.slice(4)}` : `LLM Wiki Desktop ${tag.slice(4)}`,
      "--notes-file", path.resolve(notesFile), ...(capability ? ["--prerelease"] : [])]);
    matches = releasesForTag();
    if (matches.length !== 1) throw new Error(`could not resolve the draft created for tag: ${tag}`);
    release = matches[0];
  }
  const remoteAssets = () => JSON.parse(gh(["api", `${endpoint}/${release.id}/assets?per_page=100`, "--paginate", "--slurp"])).flat();
  if (!release.draft) {
    const errors = uploadedAssetErrors(uploads, remoteAssets());
    if (errors.length) throw new Error(`release is already public; refusing to change its assets: ${errors.join("; ")}`);
    return;
  }
  // --clobber is limited to this draft. A partial upload can be retried without
  // rebuilding every installer and capability pack.
  const existing = new Map(remoteAssets().map((asset) => [asset.name, asset]));
  for (const upload of uploads) {
    const asset = existing.get(upload.name);
    if (asset?.state === "uploaded" && asset.size === upload.size && asset.digest === upload.digest) continue;
    gh(["release", "upload", tag, upload.file, "--repo", RELEASE_REPOSITORY, "--clobber"]);
  }
  const errors = uploadedAssetErrors(uploads, remoteAssets());
  if (errors.length) throw new Error(errors.join("; "));
  gh(["release", "edit", tag, "--repo", RELEASE_REPOSITORY, "--draft=false", capability ? "--latest=false" : "--latest"]);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { root, tag, notes, channel } = parseNamedArguments(process.argv.slice(2));
    if (!root) throw new Error("--root is required");
    await publishDesktopRelease({ root, tag, notesFile: notes, channel });
    process.stdout.write(`[release] ${tag} published with verified upload digests\n`);
  } catch (error) {
    process.stderr.write(`[release] ${error.message}\n`);
    process.exitCode = 1;
  }
}

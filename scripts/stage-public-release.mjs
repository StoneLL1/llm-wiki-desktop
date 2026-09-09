import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { collectRegularFiles, parseNamedArguments, RELEASE_PLATFORMS, safeAssetName } from "./release-assets-contract.mjs";
import { writeChecksums } from "./generate-release-checksums.mjs";

// The complete candidate stays in Actions. Public downloads are an explicit
// projection, so new diagnostics can never accidentally become release assets.
export function stagePublicRelease({ root, output }) {
  const source = path.resolve(root);
  const destination = path.resolve(output);
  if (destination === source || destination.startsWith(source + path.sep)) throw new Error("public output must be outside the candidate");
  collectRegularFiles(source); // Reject symlinks before copying any payload.
  if (fs.existsSync(destination)) throw new Error("public output already exists");
  const desktop = [];
  for (const platform of Object.keys(RELEASE_PLATFORMS)) {
    const directory = path.join(source, "desktop", platform);
    const descriptor = JSON.parse(fs.readFileSync(path.join(directory, "release-entry.json"), "utf8"));
    for (const asset of [descriptor.installer, descriptor.updater]) {
      if (!safeAssetName(asset?.file)) throw new Error(`invalid asset for ${platform}`);
      desktop.push(path.join(directory, asset.file));
    }
  }
  desktop.push(path.join(source, "latest.json"));
  const capabilityRoot = path.join(source, "capabilities");
  const catalog = JSON.parse(fs.readFileSync(path.join(capabilityRoot, "install-catalog.json"), "utf8"));
  const capabilities = catalog.entries.map((entry) => {
    const name = path.posix.basename(new URL(entry.url).pathname);
    if (!safeAssetName(name)) throw new Error("invalid capability archive name");
    return path.join(capabilityRoot, name);
  });
  for (const name of ["install-catalog.json", "trusted-keys.json", "catalog-provenance.json"]) capabilities.push(path.join(capabilityRoot, name));
  for (const [channel, files] of [["desktop", desktop], ["capabilities", capabilities]]) {
    const directory = path.join(destination, channel);
    fs.mkdirSync(directory, { recursive: true });
    const names = new Set();
    for (const file of new Set(files)) {
      const name = path.basename(file);
      if (names.has(name)) throw new Error(`duplicate public asset: ${name}`);
      names.add(name);
      fs.copyFileSync(file, path.join(directory, name));
    }
    writeChecksums(directory, path.join(directory, "CHECKSUMS.sha256"));
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = parseNamedArguments(process.argv.slice(2));
  if (!options.root || !options.output) throw new Error("--root and --output are required");
  stagePublicRelease(options);
}

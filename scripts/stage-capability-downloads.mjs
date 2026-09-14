import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { githubReleaseAssetName, parseNamedArguments, safeAssetName } from "./release-assets-contract.mjs";
import { verifyCapabilityCatalog } from "./verify-capability-catalog.mjs";

const GITHUB_ASSET_LIMIT = 2 * 1024 ** 3;

async function sha256(file) {
  const hash = createHash("sha256");
  for await (const bytes of fs.createReadStream(file)) hash.update(bytes);
  return hash.digest("hex");
}

function releaseBase(value) {
  const url = new URL(value);
  if (url.protocol !== "https:" || url.hostname !== "github.com" || url.port || url.username || url.password
    || url.search || url.hash || !/^\/[^/]+\/[^/]+\/releases\/download\/[A-Za-z0-9._+-]+\/?$/u.test(url.pathname)) {
    throw new Error("--base-url must be an exact public GitHub releases/download/TAG HTTPS directory");
  }
  return url.href.replace(/\/$/u, "") + "/";
}

// GitHub releases have flat attachments. Keep model.path unchanged for runners,
// while a separate models.zip restores the directory used by offline installation.
export async function stageCapabilityDownloads({ input, output, baseUrl }) {
  const sourceRoot = fs.realpathSync(input);
  const destination = path.resolve(output);
  if (fs.existsSync(destination)) throw new Error("output must be a new directory; existing files will not be overwritten");
  const base = releaseBase(baseUrl);
  const catalog = JSON.parse(fs.readFileSync(path.join(sourceRoot, "install-catalog.json"), "utf8"));
  const { errors } = verifyCapabilityCatalog({ catalog, trustedKeys: {}, mode: "release" });
  if (errors.length) throw new Error(errors.join("; "));
  const assets = new Map();
  const models = new Map();
  const checked = new Map();
  async function add(source, name, bytes, digest) {
    if (!safeAssetName(name) || ["models.zip", "install-catalog.json", "CHECKSUMS.sha256"].includes(name)) {
      throw new Error(`reserved or invalid attachment name: ${name}`);
    }
    if (bytes >= GITHUB_ASSET_LIMIT) throw new Error(`GitHub attachment must be smaller than 2 GiB: ${name}`);
    const metadata = fs.lstatSync(source);
    const relative = path.relative(sourceRoot, fs.realpathSync(source));
    if (!metadata.isFile() || metadata.size !== bytes || relative === ".." || relative.startsWith(".." + path.sep) || path.isAbsolute(relative)) {
      throw new Error(`resource size or path does not match catalog: ${source}`);
    }
    if (!checked.has(source)) checked.set(source, await sha256(source));
    if (checked.get(source) !== digest) throw new Error(`resource SHA-256 does not match catalog: ${source}`);
    const previous = assets.get(name);
    if (previous && (previous.digest !== digest || previous.bytes !== bytes)) throw new Error(`conflicting attachment name: ${name}`);
    assets.set(name, { source, bytes, digest });
    return base + encodeURIComponent(name);
  }
  for (const entry of catalog.entries) {
    const original = decodeURIComponent(path.posix.basename(new URL(entry.url).pathname));
    if (!safeAssetName(original) || !original.endsWith(".zip")) throw new Error("catalog program URL must name a ZIP file");
    entry.url = await add(path.join(sourceRoot, original), githubReleaseAssetName(original), entry.compressedBytes, entry.archiveSha256);
    for (const model of entry.modelFiles ?? []) {
      const originalName = path.posix.basename(model.path);
      const relative = `models/${model.sha256}/${originalName}`;
      const name = githubReleaseAssetName(`${model.sha256}-${originalName}`);
      model.urls = [await add(path.join(sourceRoot, ...relative.split("/")), name, model.bytes, model.sha256)];
      models.set(relative, { name, bytes: model.bytes });
    }
  }
  // Stored ZIP adds a local header and a central-directory entry per model.
  const modelZipBytes = 22 + [...models].reduce((total, [name, model]) => total + model.bytes + 76 + 2 * Buffer.byteLength(name), 0);
  if (models.size && modelZipBytes >= GITHUB_ASSET_LIMIT) throw new Error("shared models.zip would exceed GitHub's 2 GiB attachment limit; use directory hosting instead");
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.mkdirSync(destination);
  try {
    for (const [name, asset] of assets) {
      const target = path.join(destination, name);
      try { fs.linkSync(asset.source, target); }
      catch { fs.copyFileSync(asset.source, target, fs.constants.COPYFILE_EXCL); }
    }
    if (models.size) {
      const archive = path.join(destination, "models.zip");
      execFileSync(process.platform === "win32" ? "python" : "python3", ["-c", `
import json,shutil,sys,zipfile
with zipfile.ZipFile(sys.argv[1], 'x', compression=zipfile.ZIP_STORED) as archive:
    for item in json.load(sys.stdin):
        info = zipfile.ZipInfo(item['entry'], date_time=(1980,1,1,0,0,0))
        info.create_system = 3
        info.external_attr = 0o100644 << 16
        with open(item['source'], 'rb') as source, archive.open(info, 'w') as target:
            shutil.copyfileobj(source, target, length=1024*1024)
`, archive], { input: JSON.stringify([...models].map(([entry, model]) => ({ entry, source: path.join(destination, model.name) }))), stdio: ["pipe", "pipe", "pipe"] });
      if (fs.statSync(archive).size >= GITHUB_ASSET_LIMIT) throw new Error("shared models.zip exceeds GitHub's 2 GiB attachment limit");
      assets.set("models.zip", { digest: await sha256(archive) });
    }
    const catalogPath = path.join(destination, "install-catalog.json");
    fs.writeFileSync(catalogPath, JSON.stringify(catalog, null, 2) + "\n", { flag: "wx" });
    assets.set("install-catalog.json", { digest: await sha256(catalogPath) });
    fs.writeFileSync(path.join(destination, "CHECKSUMS.sha256"), [...assets]
      .sort(([left], [right]) => left.localeCompare(right, "en"))
      .map(([name, asset]) => `${asset.digest}  ${name}\n`).join(""), { flag: "wx" });
  } catch (error) {
    fs.rmSync(destination, { recursive: true, force: true });
    throw error;
  }
  return { entryCount: catalog.entries.length, modelCount: models.size, attachmentCount: assets.size + 1 };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    for (const key of ["input", "output", "base-url"]) if (!options[key]) throw new Error(`--${key} is required`);
    const result = await stageCapabilityDownloads({ input: options.input, output: options.output, baseUrl: options["base-url"] });
    process.stdout.write(`[capability-downloads] staged ${result.entryCount} programs, ${result.modelCount} shared models and ${result.attachmentCount} attachments\n`);
  } catch (error) {
    process.stderr.write(`[capability-downloads] ${error.message}\n`);
    process.exitCode = 1;
  }
}

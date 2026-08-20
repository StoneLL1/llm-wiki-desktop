import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { parseNamedArguments } from "./release-assets-contract.mjs";

const digest = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");

const component = (type, name, version) => ({
  type,
  name,
  version,
});

export function nodeSbom(packageLockPath) {
  const lock = JSON.parse(fs.readFileSync(packageLockPath, "utf8"));
  const components = [];
  for (const [packagePath, metadata] of Object.entries(lock.packages ?? {})) {
    if (!packagePath || !metadata?.version) continue;
    const name = metadata.name ?? packagePath.slice(packagePath.lastIndexOf("node_modules/") + 13);
    components.push(component("library", name, metadata.version));
  }
  components.sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`, "en"));
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    version: 1,
    metadata: { properties: [{ name: "llm-wiki:lockfile-sha256", value: digest(packageLockPath) }] },
    components,
  };
}

export function rustSbom(cargoLockPath) {
  const text = fs.readFileSync(cargoLockPath, "utf8");
  const components = [...text.matchAll(/\[\[package\]\]\s*\r?\nname = "([^"]+)"\s*\r?\nversion = "([^"]+)"/g)]
    .map((match) => component("library", match[1], match[2]))
    .sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`, "en"));
  if (components.length === 0) throw new Error("Cargo.lock did not contain any package records");
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    version: 1,
    metadata: { properties: [{ name: "llm-wiki:lockfile-sha256", value: digest(cargoLockPath) }] },
    components,
  };
}

export function writeReleaseSboms({ packageLock, cargoLock, output }) {
  const root = path.resolve(output);
  fs.mkdirSync(root, { recursive: true });
  fs.writeFileSync(path.join(root, "node.cdx.json"), `${JSON.stringify(nodeSbom(packageLock), null, 2)}\n`, "utf8");
  fs.writeFileSync(path.join(root, "rust.cdx.json"), `${JSON.stringify(rustSbom(cargoLock), null, 2)}\n`, "utf8");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    if (!options.packageLock || !options.cargoLock || !options.output) {
      throw new Error("--packageLock, --cargoLock, and --output are required");
    }
    writeReleaseSboms({ packageLock: options.packageLock, cargoLock: options.cargoLock, output: options.output });
    process.stdout.write("[release-sbom] wrote locked Node and Rust CycloneDX inventories\n");
  } catch (error) {
    process.stderr.write(`[release-sbom] ${error.message}\n`);
    process.exitCode = 1;
  }
}

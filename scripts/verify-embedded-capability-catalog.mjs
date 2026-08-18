import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { CAPABILITY_PACKS, CAPABILITY_TARGETS } from "./verify-capability-catalog.mjs";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.resolve(scriptDirectory, "..");

export function verifyEmbeddedCapabilityCatalog({ binary, catalogText, mode = "source" }) {
  if (mode !== "source" && mode !== "release") {
    throw new Error("unknown embed mode " + mode + "; expected source or release");
  }
  const errors = [];
  const binaryBuffer = Buffer.isBuffer(binary) ? binary : Buffer.from(binary ?? "");
  let catalog = null;
  try {
    catalog = JSON.parse(catalogText);
  } catch {
    errors.push("catalog text must be valid JSON");
  }
  if (catalog) {
    if (catalog.schemaVersion !== 1) {
      errors.push("embedded catalog schemaVersion must be 1");
    }
    if (!Array.isArray(catalog.entries)) {
      errors.push("embedded catalog entries must be an array");
    }
  }
  const entries = Array.isArray(catalog?.entries) ? catalog.entries : [];
  if (mode === "release") {
    if (entries.length !== 20) {
      errors.push("release binaries must embed exactly 20 catalog entries, found " + entries.length);
    }
    const pairs = new Set(entries
      .filter((entry) => typeof entry === "object" && entry !== null)
      .map((entry) => entry.capabilityId + "\u0000" + entry.targetTriple));
    if (pairs.size !== entries.length) {
      errors.push("embedded release catalog entries must be unique capability and target pairs");
    }
    const targets = new Set(entries.map((entry) => entry?.targetTriple));
    const packs = new Set(entries.map((entry) => entry?.capabilityId));
    if (JSON.stringify([...targets].sort()) !== JSON.stringify([...CAPABILITY_TARGETS].sort())) {
      errors.push("embedded release catalog must cover exactly the four supported desktop targets");
    }
    if (JSON.stringify([...packs].sort()) !== JSON.stringify([...CAPABILITY_PACKS].sort())) {
      errors.push("embedded release catalog must cover exactly the five signed capability packs");
    }
  }
  const exactEmbed = binaryBuffer.length > 0
    && binaryBuffer.includes(Buffer.from(String(catalogText ?? ""), "utf8"));
  if (!exactEmbed) {
    errors.push("binary does not embed the exact staged catalog bytes");
  }
  return { errors };
};

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arguments_ = process.argv.slice(2);
  const options = {};
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index];
    if (index + 1 >= arguments_.length) {
      process.stderr.write("[embedded-catalog] incomplete argument: " + name + "\n");
      process.exit(2);
    }
    options[name] = arguments_[index + 1];
  }
  if (!options["--binary"] || !options["--catalog"]) {
    process.stderr.write("[embedded-catalog] --binary and --catalog are required\n");
    process.exit(2);
  }
  const { errors } = verifyEmbeddedCapabilityCatalog({
    binary: fs.readFileSync(path.resolve(repositoryRoot, options["--binary"])),
    catalogText: fs.readFileSync(path.resolve(repositoryRoot, options["--catalog"]), "utf8"),
    mode: options["--mode"] ?? "source",
  });
  if (errors.length > 0) {
    for (const error of errors) process.stderr.write("[embedded-catalog] " + error + "\n");
    process.exitCode = 1;
  } else {
    process.stdout.write("[embedded-catalog] verified " + (options["--mode"] ?? "source") + " embed\n");
  }
}

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { parseNamedArguments, publishedReleaseFiles } from "./release-assets-contract.mjs";

export function checksumDocument(root, outputPath) {
  const absoluteRoot = path.resolve(root);
  const absoluteOutput = path.resolve(outputPath);
  const relativeOutput = path.relative(absoluteRoot, absoluteOutput);
  if (relativeOutput.startsWith("..") || path.isAbsolute(relativeOutput)) {
    throw new Error("checksum output must stay inside the release root");
  }
  const lines = publishedReleaseFiles(absoluteRoot)
    .map((file) => {
      const name = path.basename(file);
      const digest = crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
      return `${digest}  ${name}`;
    });
  if (lines.length === 0) throw new Error("release root contains no assets to checksum");
  return `${lines.join("\n")}\n`;
}

export function writeChecksums(root, outputPath) {
  const document = checksumDocument(root, outputPath);
  fs.mkdirSync(path.dirname(path.resolve(outputPath)), { recursive: true });
  fs.writeFileSync(path.resolve(outputPath), document, "utf8");
  return document;
}

export function verifyChecksums(root, checksumsPath) {
  const absoluteChecksums = path.resolve(checksumsPath);
  if (!fs.existsSync(absoluteChecksums) || !fs.statSync(absoluteChecksums).isFile()) {
    throw new Error("CHECKSUMS.sha256 is missing");
  }
  const actual = fs.readFileSync(absoluteChecksums, "utf8");
  const expected = checksumDocument(root, absoluteChecksums);
  if (actual !== expected) throw new Error("CHECKSUMS.sha256 does not match the published release assets");
  return actual;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseNamedArguments(process.argv.slice(2));
    if (!options.root) throw new Error("--root is required");
    if (options.verify) {
      const document = verifyChecksums(options.root, options.verify);
      process.stdout.write(`[release-checksums] verified ${document.trimEnd().split("\n").length} published assets\n`);
    } else {
      if (!options.output) throw new Error("--output is required");
      const document = writeChecksums(options.root, options.output);
      process.stdout.write(`[release-checksums] wrote ${document.trimEnd().split("\n").length} SHA-256 entries\n`);
    }
  } catch (error) {
    process.stderr.write(`[release-checksums] ${error.message}\n`);
    process.exitCode = 1;
  }
}

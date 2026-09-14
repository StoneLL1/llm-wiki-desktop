import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  CAPABILITY_PACKS,
  CAPABILITY_TARGETS,
  MODEL_CAPABILITY_PACKS,
  PRODUCT_MANIFEST,
  expectedReleaseMatrix,
} from "./verify-product-capabilities.mjs";

export { CAPABILITY_PACKS, CAPABILITY_TARGETS, MODEL_CAPABILITY_PACKS, expectedReleaseMatrix };

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.resolve(scriptDirectory, "..");

const MODEL_PACK_SET = new Set(MODEL_CAPABILITY_PACKS);
const PRODUCT_DEFINITIONS = new Map(PRODUCT_MANIFEST.definitions.map((definition) => [definition.capabilityId, definition]));
const CAPABILITY_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9_-]*$/;
const SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const RELEASE_TAG_PATTERN = /^app-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-rc\.[1-9]\d*)?$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

const trustedKeyErrors = (trustedKeys, { requireCommittedKey }) => {
  const errors = [];
  if (!isObject(trustedKeys)) {
    return ["trusted keys must be a JSON object"];
  }
  for (const [keyId, value] of Object.entries(trustedKeys)) {
    if (!CAPABILITY_ID_PATTERN.test(keyId)) {
      errors.push("trusted key id " + keyId + " is not a stable identifier");
    }
    if (typeof value !== "string" || !SHA256_PATTERN.test(value)) {
      errors.push("trusted key " + keyId + " must be 64 lowercase hex characters");
    } else if (/^0+$/.test(value)) {
      errors.push("trusted key " + keyId + " is all zeros");
    }
  }
  if (requireCommittedKey && Object.keys(trustedKeys).length === 0) {
    errors.push("release mode requires at least one committed trusted capability key");
  }
  return errors;
};

export const catalogUrlErrors = (entry, label = "entry") => {
  try {
    const url = new URL(entry.url);
    if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash
      || !url.hostname || url.hostname === "localhost" || url.hostname === "[::1]"
      || /^127\./u.test(url.hostname) || /(?:^|\.)(?:test|invalid|localhost)$/u.test(url.hostname)
      || /^example\.(?:com|org|net)$/u.test(url.hostname)) {
      return [label + " url must be a public HTTPS URL without credentials, query or fragment"];
    }
    return [];
  } catch {
    return [label + " url must be a public HTTPS URL"];
  }
};

const entryErrors = (entry, index, expectedTag) => {
  const errors = [];
  const label = "entries[" + index + "]";
  if (!isObject(entry)) {
    return [label + " must be an object"];
  }
  if (typeof entry.capabilityId !== "string" || !CAPABILITY_ID_PATTERN.test(entry.capabilityId)) {
    errors.push(label + " capabilityId must be a stable identifier");
  }
  if (typeof entry.version !== "string" || !SEMVER_PATTERN.test(entry.version)) {
    errors.push(label + " version must be valid SemVer");
  }
  if (typeof entry.targetTriple !== "string" || !CAPABILITY_TARGETS.includes(entry.targetTriple)) {
    errors.push(label + " targetTriple must be one of the four supported desktop triples");
  }
  if (typeof entry.license !== "string" || entry.license.trim().length === 0) {
    errors.push(label + " license must be a non-empty expression");
  }
  // Archive identity is pinned in the catalog embedded in the trusted app.
  // Legacy signature metadata is optional and is no longer an install gate.
  for (const field of ["archiveSha256", "manifestSha256"]) {
    if (field === "manifestSha256" && (entry[field] == null || entry[field] === "")) continue;
    if (typeof entry[field] !== "string" || !SHA256_PATTERN.test(entry[field]) || /^0+$/.test(entry[field])) {
      errors.push(label + " " + field + " must be a non-zero SHA-256 in lowercase hex");
    }
  }
  for (const field of ["compressedBytes", "installedBytes"]) {
    if (!Number.isSafeInteger(entry[field]) || entry[field] <= 0) {
      errors.push(label + " " + field + " must be a positive integer");
    }
  }
  const definition = PRODUCT_DEFINITIONS.get(entry.capabilityId);
  if (!definition || definition.distributionTier !== "published") {
    errors.push(label + " capabilityId is not a published product capability");
  } else {
    if (!definition.supportedTargets.includes(entry.targetTriple)) {
      errors.push(label + " targetTriple is not supported by the product capability");
    }
    if (entry.license !== definition.licensePolicy.expression) {
      errors.push(label + " license must match the product capability license policy");
    }
  }
  const requiresModelBytes = typeof entry.capabilityId === "string"
    && MODEL_PACK_SET.has(entry.capabilityId);
  if (requiresModelBytes && (!Number.isSafeInteger(entry.modelBytes) || entry.modelBytes <= 0)) {
    errors.push(label + " modelBytes is required for model capability packs");
  }
  if (!requiresModelBytes && entry.modelBytes != null
    && (!Number.isSafeInteger(entry.modelBytes) || entry.modelBytes <= 0)) {
    errors.push(label + " modelBytes must be a positive integer when present");
  }
  if (entry.modelFiles != null) {
    if (!Array.isArray(entry.modelFiles)) errors.push(label + " modelFiles must be an array");
    else {
      const paths = new Set();
      if (entry.modelFiles.length > 1024) errors.push(label + " modelFiles exceeds 1024 files");
      for (const file of entry.modelFiles) {
        if (!isObject(file)) { errors.push(label + " modelFiles entry must be an object"); continue; }
        if (typeof file.path !== "string" || !file.path.startsWith("models/") || file.path.includes("\\")
          || file.path.includes(":") || file.path.split("/").some((part) => !part || part === "." || part === "..")
          || !/\.(?:onnx|bin|txt|json|safetensors|md)$/u.test(file.path)
          || paths.has(file.path.toLowerCase())) errors.push(label + " model file path must be unique data under models/");
        if (typeof file.path === "string") paths.add(file.path.toLowerCase());
        if (!SHA256_PATTERN.test(file.sha256 ?? "") || /^0+$/.test(file.sha256)
          || !Number.isSafeInteger(file.bytes) || file.bytes <= 0 || file.bytes > 8 * 1024 ** 3) errors.push(label + " model file needs SHA-256 and positive bytes");
        if (!Array.isArray(file.urls) || !file.urls.length || file.urls.length > 8) errors.push(label + " model file needs download URLs");
        else for (const url of file.urls) errors.push(...catalogUrlErrors({ url }, label + " model file"));
      }
      if (entry.modelFiles.reduce((sum, file) => sum + (file?.bytes ?? 0), 0) > 16 * 1024 ** 3) errors.push(label + " models exceed 16 GiB");
      if (entry.modelFiles.length && entry.modelFiles.reduce((sum, file) => sum + (file?.bytes ?? 0), 0) !== entry.modelBytes) {
        errors.push(label + " modelBytes must equal the independent model file sizes");
      }
    }
  }
  errors.push(...catalogUrlErrors(entry, label, expectedTag));
  return errors;
};

const provenanceErrors = (provenance, { expectedTag, expectedCommit, expectedRunId }) => {
  if (!isObject(provenance)) {
    return ["catalog provenance must be a JSON object"];
  }
  const errors = [];
  if (provenance.schemaVersion !== 1) {
    errors.push("catalog provenance schemaVersion must be 1");
  }
  if (typeof provenance.releaseTag !== "string" || !RELEASE_TAG_PATTERN.test(provenance.releaseTag)) {
    errors.push("catalog provenance releaseTag must match the frozen app-v grammar");
  } else if (expectedTag && provenance.releaseTag !== expectedTag) {
    errors.push("catalog provenance releaseTag must equal " + expectedTag);
  }
  if (typeof provenance.commitSha !== "string" || !/^[0-9a-f]{40}$/.test(provenance.commitSha)) {
    errors.push("catalog provenance commitSha must be 40 lowercase hex characters");
  } else if (expectedCommit && provenance.commitSha !== expectedCommit) {
    errors.push("catalog provenance commitSha must equal " + expectedCommit);
  }
  if (typeof provenance.workflowRunId !== "string" || !/^\d+$/.test(provenance.workflowRunId)) {
    errors.push("catalog provenance workflowRunId must be numeric");
  } else if (expectedRunId && provenance.workflowRunId !== expectedRunId) {
    errors.push("catalog provenance workflowRunId must equal " + expectedRunId);
  }
  return errors;
};

export function verifyCapabilityCatalog({
  catalog,
  trustedKeys,
  mode = "source",
  expectedTag = null,
  provenance = null,
  expectedCommit = null,
  expectedRunId = null,
}) {
  if (mode !== "source" && mode !== "release") {
    throw new Error("unknown catalog mode " + mode + "; expected source or release");
  }
  const releaseMode = mode === "release";
  const errors = [];
  if (expectedTag !== null && !RELEASE_TAG_PATTERN.test(expectedTag)) {
    errors.push("expected release tag must match the frozen app-v grammar");
  }
  if (!isObject(catalog)) {
    return {
      errors: [
        ...errors,
        "install catalog must be a JSON object",
        ...trustedKeyErrors(trustedKeys, { requireCommittedKey: false }),
      ],
    };
  }
  if (catalog.schemaVersion !== 1) {
    errors.push("install catalog schemaVersion must be 1");
  }
  const entries = Array.isArray(catalog.entries) ? catalog.entries : null;
  if (!entries) {
    errors.push("install catalog entries must be an array");
  } else {
    entries.forEach((entry, index) => {
      errors.push(...entryErrors(entry, index, expectedTag));
    });
    if (releaseMode) {
      const expectedMatrix = expectedReleaseMatrix(PRODUCT_MANIFEST);
      if (entries.length !== expectedMatrix.length) {
        errors.push("release catalog must contain the manifest-derived exact matrix of "
          + expectedMatrix.length + " entries, found " + entries.length);
      }
      if (entries.length === 0) {
        errors.push("release builds cannot embed an empty capability catalog");
      }
      const pairs = new Set(
        entries.filter(isObject).map((entry) => entry.capabilityId + "\u0000" + entry.targetTriple),
      );
      if (pairs.size !== entries.length) {
        errors.push("release catalog entries must be unique capability and target pairs");
      }
      const targets = new Set(entries.filter(isObject).map((entry) => entry.targetTriple));
      const packs = new Set(entries.filter(isObject).map((entry) => entry.capabilityId));
      if (JSON.stringify([...targets].sort()) !== JSON.stringify([...CAPABILITY_TARGETS].sort())) {
        errors.push("release catalog must cover exactly the four supported desktop targets");
      }
      if (JSON.stringify([...packs].sort()) !== JSON.stringify([...CAPABILITY_PACKS].sort())) {
        errors.push("release catalog must cover exactly the product manifest's published capability packs");
      }
      const expectedPairs = new Set(expectedMatrix.map((entry) => entry.capabilityId + "\u0000" + entry.targetTriple));
      for (const pair of expectedPairs) {
        if (!pairs.has(pair)) errors.push("release catalog is missing product matrix entry " + pair.replace("\u0000", " / "));
      }
    }
  }
  errors.push(...trustedKeyErrors(trustedKeys, { requireCommittedKey: false }));
  if (provenance) {
    errors.push(...provenanceErrors(provenance, { expectedTag, expectedCommit, expectedRunId }));
  }
  return { errors };
};

export function emitCatalogProvenance({ outputPath, releaseTag, commitSha, workflowRunId }) {
  const document = {
    schemaVersion: 1,
    releaseTag,
    commitSha,
    workflowRunId,
  };
  const errors = provenanceErrors(document, { expectedTag: null, expectedCommit: null, expectedRunId: null });
  if (errors.length > 0) {
    throw new Error("catalog provenance is invalid: " + errors.join("; "));
  }
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, JSON.stringify(document, null, 2) + "\n", "utf8");
}

const parseArguments = (arguments_) => {
  const names = {
    "--catalog": "catalog",
    "--trusted-keys": "trustedKeys",
    "--mode": "mode",
    "--tag": "tag",
    "--provenance": "provenance",
    "--expected-commit": "expectedCommit",
    "--expected-run-id": "expectedRunId",
    "--emit-provenance": "emitProvenance",
    "--commit-sha": "commitSha",
    "--run-id": "runId",
  };
  const options = {};
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index];
    const field = names[name];
    if (!field || index + 1 >= arguments_.length) {
      throw new Error("unknown or incomplete argument: " + name);
    }
    options[field] = arguments_[index + 1];
  }
  return options;
};

const readJsonFile = (targetPath) => JSON.parse(
  fs.readFileSync(path.resolve(repositoryRoot, targetPath), "utf8"),
);

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
    if (!options.catalog || !options.trustedKeys) {
      throw new Error("--catalog and --trusted-keys are required");
    }
  } catch (error) {
    process.stderr.write("[capability-catalog] " + error.message + "\n");
    process.exit(2);
  }
  const provenance = options.provenance ? readJsonFile(options.provenance) : null;
  const { errors } = verifyCapabilityCatalog({
    catalog: readJsonFile(options.catalog),
    trustedKeys: readJsonFile(options.trustedKeys),
    mode: options.mode ?? "source",
    expectedTag: options.tag ?? null,
    provenance,
    expectedCommit: options.expectedCommit ?? null,
    expectedRunId: options.expectedRunId ?? null,
  });
  if (errors.length > 0) {
    for (const error of errors) process.stderr.write("[capability-catalog] " + error + "\n");
    process.exitCode = 1;
  } else if (options.emitProvenance) {
    emitCatalogProvenance({
      outputPath: path.resolve(repositoryRoot, options.emitProvenance),
      releaseTag: options.tag,
      commitSha: options.commitSha,
      workflowRunId: options.runId,
    });
    process.stdout.write("[capability-catalog] verified " + (options.mode ?? "source") + " catalog and recorded provenance\n");
  } else {
    process.stdout.write("[capability-catalog] verified " + (options.mode ?? "source") + " catalog\n");
  }
}

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.resolve(scriptDirectory, "..");
export const productManifestPath = path.join(repositoryRoot, "capabilities", "product-manifest.json");
export const productSchemaPath = path.join(repositoryRoot, "capabilities", "product-manifest.schema.json");

const STABLE_ID = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const DISTRIBUTION_TIERS = new Set(["built_in", "published", "experimental", "unsupported"]);
const RELEASE_STATUSES = new Set(["implemented", "planned_batch_8", "not_applicable"]);
const REQUIRED_TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc",
  "x86_64-unknown-linux-gnu",
];
const PRODUCT_SURFACES = ["routes", "formats", "platformContentTypes", "recoveryActions", "asrProfiles", "ocrProfiles"];

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
const sorted = (values) => [...values].sort();
const sameSet = (left, right) => JSON.stringify(sorted(new Set(left))) === JSON.stringify(sorted(new Set(right)));
const readJson = (target) => JSON.parse(fs.readFileSync(target, "utf8"));
export const PRODUCT_SCHEMA = readJson(productSchemaPath);

const resolveSchema = (schema, root) => {
  if (typeof schema?.$ref !== "string") return schema;
  if (!schema.$ref.startsWith("#/")) throw new Error(`unsupported product schema reference ${schema.$ref}`);
  return schema.$ref.slice(2).split("/").reduce((value, segment) => value?.[segment], root);
};

const matchesSchemaType = (value, type) => {
  if (type === "null") return value === null;
  if (type === "array") return Array.isArray(value);
  if (type === "object") return isObject(value);
  if (type === "integer") return Number.isInteger(value);
  return typeof value === type;
};

const schemaErrors = (value, inputSchema, root = PRODUCT_SCHEMA, label = "product manifest") => {
  const schema = resolveSchema(inputSchema, root);
  if (!schema) return [`${label} refers to a missing schema definition`];
  const errors = [];
  const types = Array.isArray(schema.type) ? schema.type : schema.type ? [schema.type] : [];
  if (types.length > 0 && !types.some((type) => matchesSchemaType(value, type))) {
    return [`${label} must match schema type ${types.join(" or ")}`];
  }
  if (Object.hasOwn(schema, "const") && value !== schema.const) errors.push(`${label} must equal ${JSON.stringify(schema.const)}`);
  if (Array.isArray(schema.enum) && !schema.enum.includes(value)) errors.push(`${label} is outside the schema enum`);
  if (typeof value === "string") {
    if (schema.minLength && value.length < schema.minLength) errors.push(`${label} is shorter than the schema minimum`);
    if (schema.pattern && !new RegExp(schema.pattern).test(value)) errors.push(`${label} does not match the schema pattern`);
  }
  if (Array.isArray(value)) {
    if (schema.minItems && value.length < schema.minItems) errors.push(`${label} has too few items`);
    if (schema.uniqueItems && new Set(value.map((item) => JSON.stringify(item))).size !== value.length) {
      errors.push(`${label} must contain unique items`);
    }
    if (schema.items) value.forEach((item, index) => errors.push(...schemaErrors(item, schema.items, root, `${label}[${index}]`)));
  }
  if (isObject(value)) {
    for (const field of schema.required ?? []) {
      if (!Object.hasOwn(value, field)) errors.push(`${label}.${field} is required by the schema`);
    }
    if (schema.additionalProperties === false) {
      for (const field of Object.keys(value)) {
        if (!Object.hasOwn(schema.properties ?? {}, field)) errors.push(`${label}.${field} is not allowed by the schema`);
      }
    }
    for (const [field, childSchema] of Object.entries(schema.properties ?? {})) {
      if (Object.hasOwn(value, field)) errors.push(...schemaErrors(value[field], childSchema, root, `${label}.${field}`));
    }
  }
  return errors;
};

export function loadFormatPipelineFixtures(root = repositoryRoot) {
  const target = path.join(root, "src-tauri", "tests", "import_v2_format_pipeline.rs");
  const source = fs.readFileSync(target, "utf8");
  const marker = "for (route, extensions) in [";
  const start = source.indexOf(marker);
  const end = source.indexOf("] {", start);
  if (start < 0 || end < 0) throw new Error(`cannot locate the capability fixture matrix in ${path.relative(root, target)}`);
  const block = source.slice(start + marker.length, end);
  const routes = [...block.matchAll(/\(\s*"([^"]+)"\s*,\s*vec!\[([\s\S]*?)\]\s*,?\s*\)/g)].map((match) => ({
    route: match[1],
    extensions: [...match[2].matchAll(/"([^"]+)"/g)].map((extension) => extension[1]),
  }));
  if (routes.length === 0) throw new Error(`the capability fixture matrix in ${path.relative(root, target)} is empty`);
  return [{ capabilityId: "batch9-runtime-fixture", routes }];
}

export const PRODUCT_MANIFEST = readJson(productManifestPath);
export const CAPABILITY_TARGETS = Object.freeze([...PRODUCT_MANIFEST.supportedTargets]);
export const CAPABILITY_PACKS = Object.freeze(PRODUCT_MANIFEST.definitions
  .filter((definition) => definition.distributionTier === "published")
  .map((definition) => definition.capabilityId)
  .sort());
export const MODEL_CAPABILITY_PACKS = Object.freeze(PRODUCT_MANIFEST.definitions
  .filter((definition) => definition.distributionTier === "published" && definition.sizeSources.modelBytes !== null)
  .map((definition) => definition.capabilityId)
  .sort());

export async function loadProductCapabilityManifest(target = productManifestPath) {
  return readJson(target);
}

export function expectedReleaseMatrix(manifest = PRODUCT_MANIFEST) {
  const published = manifest.definitions
    .filter((definition) => definition.distributionTier === "published")
    .sort((left, right) => left.capabilityId.localeCompare(right.capabilityId));
  return manifest.supportedTargets.flatMap((targetTriple) => published.map((definition) => ({
    capabilityId: definition.capabilityId,
    targetTriple,
    stagingStatus: definition.release.stagingStatus,
    stagingScript: definition.release.stagingScript,
    qualificationStatus: definition.qualification.status,
  })));
}

const listErrors = (value, label) => {
  if (!Array.isArray(value)) return [`${label} must be an array`];
  const errors = [];
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !STABLE_ID.test(item)) errors.push(`${label} contains invalid id ${JSON.stringify(item)}`);
    if (seen.has(item)) errors.push(`${label} contains duplicate ${item}`);
    seen.add(item);
  }
  return errors;
};

const definitionErrors = (definition, index, manifest, root) => {
  const label = `definitions[${index}]`;
  if (!isObject(definition)) return [`${label} must be an object`];
  const errors = [];
  if (typeof definition.capabilityId !== "string" || !STABLE_ID.test(definition.capabilityId)) {
    errors.push(`${label}.capabilityId must be stable`);
  }
  if (!DISTRIBUTION_TIERS.has(definition.distributionTier)) errors.push(`${label}.distributionTier is invalid`);
  if (definition.protocolVersion !== "2") errors.push(`${label}.protocolVersion must be 2`);
  for (const [field, value] of [
    ["routes", definition.routes],
    ["formats.extensions", definition.formats?.extensions],
    ["formats.platformContentTypes", definition.formats?.platformContentTypes],
    ["supportedTargets", definition.supportedTargets],
    ["recoveryActions", definition.recoveryActions],
    ["profiles.asr", definition.profiles?.asr],
    ["profiles.ocr", definition.profiles?.ocr],
    ["runtime.filesystem", definition.runtime?.filesystem],
  ]) errors.push(...listErrors(value, `${label}.${field}`));
  if (typeof definition.nameKey !== "string" || !definition.nameKey.startsWith("importV2.capabilityName.")) {
    errors.push(`${label}.nameKey must be an Import capability i18n key`);
  }
  if (typeof definition.purposeKey !== "string" || !definition.purposeKey.startsWith("importV2.capabilityPurpose.")) {
    errors.push(`${label}.purposeKey must be an Import capability-purpose i18n key`);
  }
  if (!isObject(definition.licensePolicy) || typeof definition.licensePolicy.expression !== "string"
    || definition.licensePolicy.expression.trim() === "") {
    errors.push(`${label}.licensePolicy.expression is required`);
  }
  if (!isObject(definition.sizeSources)) errors.push(`${label}.sizeSources is required`);
  if (!isObject(definition.installation) || typeof definition.installation.proactive !== "boolean"
    || typeof definition.installation.updates !== "boolean") {
    errors.push(`${label}.installation facts are required`);
  }
  if (!isObject(definition.runtime) || typeof definition.runtime.network !== "boolean"
    || typeof definition.runtime.subprocess !== "boolean") {
    errors.push(`${label}.runtime permission facts are required`);
  }
  if (!isObject(definition.qualification) || !RELEASE_STATUSES.has(definition.qualification.status)) {
    errors.push(`${label}.qualification.status is invalid`);
  }
  if (!isObject(definition.release) || !RELEASE_STATUSES.has(definition.release.stagingStatus)) {
    errors.push(`${label}.release.stagingStatus is invalid`);
  }
  if (definition.distributionTier === "published") {
    if (!sameSet(definition.supportedTargets ?? [], manifest.supportedTargets ?? [])) {
      errors.push(`${label} published provider must support every product target`);
    }
    if (!definition.installation?.proactive || !definition.installation?.updates) {
      errors.push(`${label} published provider must allow proactive install and updates`);
    }
    for (const [field, value] of [
      ["qualification.entrypoint", definition.qualification?.entrypoint],
      ["release.stagingScript", definition.release?.stagingScript],
      ["release.owner", definition.release?.owner],
    ]) {
      if (typeof value !== "string" || value.trim() === "") errors.push(`${label}.${field} is required for published providers`);
    }
    const packManifestPath = path.join(root, "capabilities", definition.capabilityId, "manifest.json");
    if (!fs.existsSync(packManifestPath)) {
      errors.push(`${label} published provider is missing ${path.relative(root, packManifestPath)}`);
    } else {
      const pack = readJson(packManifestPath);
      if (pack.packId !== definition.capabilityId) errors.push(`${label} pack manifest id does not match`);
      if (pack.protocolVersion !== definition.protocolVersion) errors.push(`${label} pack manifest protocol does not match`);
      if (pack.licenseExpression !== definition.licensePolicy.expression) errors.push(`${label} pack manifest license does not match`);
    }
  }
  if (definition.distributionTier === "built_in" && definition.installation?.proactive) {
    errors.push(`${label} built-in provider cannot expose install`);
  }
  for (const [status, entrypoint, field] of [
    [definition.qualification?.status, definition.qualification?.entrypoint, "qualification.entrypoint"],
    [definition.release?.stagingStatus, definition.release?.stagingScript, "release.stagingScript"],
  ]) {
    if (status === "implemented" && (typeof entrypoint !== "string" || !fs.existsSync(path.join(root, entrypoint)))) {
      errors.push(`${label}.${field} marked implemented but file does not exist`);
    }
  }
  return errors;
};

const coverageErrors = (manifest) => {
  const definitions = manifest.definitions.filter((definition) => ["built_in", "published"].includes(definition.distributionTier));
  const provided = {
    routes: definitions.flatMap((definition) => definition.routes),
    formats: definitions.flatMap((definition) => definition.formats.extensions),
    platformContentTypes: definitions.flatMap((definition) => definition.formats.platformContentTypes),
    recoveryActions: definitions.flatMap((definition) => definition.recoveryActions),
    asrProfiles: definitions.flatMap((definition) => definition.profiles.asr),
    ocrProfiles: definitions.flatMap((definition) => definition.profiles.ocr),
  };
  const errors = [];
  for (const surface of PRODUCT_SURFACES) {
    for (const item of manifest.surface[surface]) {
      if (!provided[surface].includes(item)) errors.push(`user-visible ${surface} ${item} has no built-in or published provider`);
    }
  }
  return errors;
};

const i18nErrors = (manifest, root) => {
  const errors = [];
  for (const locale of ["en", "zh-CN"]) {
    const messages = readJson(path.join(root, "src", "i18n", "locales", `${locale}.json`));
    for (const definition of manifest.definitions) {
      for (const key of [definition.nameKey, definition.purposeKey]) {
        if (typeof messages[key] !== "string" || messages[key].trim() === "") errors.push(`${locale} is missing ${key}`);
      }
    }
  }
  return errors;
};

const fixtureErrors = (manifest, fixtures) => {
  const errors = [];
  const definitions = manifest.definitions.filter((definition) => ["built_in", "published"].includes(definition.distributionTier));
  const productExtensions = new Set(definitions.flatMap((definition) => definition.formats.extensions));
  for (const fixture of fixtures) {
    for (const route of fixture.routes ?? []) {
      for (const extension of route.extensions ?? []) {
        if (!productExtensions.has(extension)) {
          errors.push(`fixture ${fixture.capabilityId} extension ${extension} is not declared by the product manifest`);
        }
      }
    }
  }
  return errors;
};

export async function verifyProductCapabilities({ manifest = PRODUCT_MANIFEST, repositoryRoot: root = repositoryRoot, fixtures = null } = {}) {
  const errors = [];
  if (!isObject(manifest)) return { errors: ["product manifest must be an object"] };
  errors.push(...schemaErrors(manifest, PRODUCT_SCHEMA));
  if (manifest.schemaVersion !== 1) errors.push("product manifest schemaVersion must be 1");
  errors.push(...listErrors(manifest.supportedTargets, "supportedTargets"));
  if (!sameSet(manifest.supportedTargets ?? [], REQUIRED_TARGETS)) errors.push("supportedTargets must be the exact four desktop targets");
  if (!isObject(manifest.surface)) errors.push("surface must be an object");
  else for (const surface of PRODUCT_SURFACES) errors.push(...listErrors(manifest.surface[surface], `surface.${surface}`));
  if (!Array.isArray(manifest.definitions)) errors.push("definitions must be an array");
  else {
    manifest.definitions.forEach((definition, index) => errors.push(...definitionErrors(definition, index, manifest, root)));
    const ids = manifest.definitions.map((definition) => definition.capabilityId);
    if (new Set(ids).size !== ids.length) errors.push("capabilityId values must be unique");
    if (isObject(manifest.surface)) errors.push(...coverageErrors(manifest));
    errors.push(...i18nErrors(manifest, root));
  }
  errors.push(...fixtureErrors(manifest, fixtures ?? loadFormatPipelineFixtures(root)));
  return { errors };
}

const parseArguments = (values) => {
  const options = {};
  for (let index = 0; index < values.length; index += 1) {
    const name = values[index];
    if (["--print-published", "--print-targets", "--print-matrix", "--require-release-ready"].includes(name)) options[name] = true;
    else if (name === "--manifest" && index + 1 < values.length) options[name] = values[++index];
    else throw new Error(`unknown or incomplete argument: ${name}`);
  }
  return options;
};

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = parseArguments(process.argv.slice(2));
    const manifest = options["--manifest"] ? readJson(path.resolve(repositoryRoot, options["--manifest"])) : PRODUCT_MANIFEST;
    const { errors } = await verifyProductCapabilities({ manifest, repositoryRoot });
    if (options["--require-release-ready"]) {
      for (const definition of manifest.definitions.filter((item) => item.distributionTier === "published")) {
        if (definition.release.stagingStatus !== "implemented") {
          errors.push(`${definition.capabilityId} release staging remains ${definition.release.stagingStatus}`);
        }
        if (definition.qualification.status !== "implemented") {
          errors.push(`${definition.capabilityId} qualification remains ${definition.qualification.status}`);
        }
      }
    }
    if (errors.length > 0) {
      for (const error of errors) process.stderr.write(`[product-capabilities] ${error}\n`);
      process.exitCode = 1;
    } else if (options["--print-published"]) {
      process.stdout.write(`${manifest.definitions.filter((definition) => definition.distributionTier === "published").map((definition) => definition.capabilityId).sort().join("\n")}\n`);
    } else if (options["--print-targets"]) {
      process.stdout.write(`${manifest.supportedTargets.join("\n")}\n`);
    } else if (options["--print-matrix"]) {
      process.stdout.write(`${JSON.stringify({ include: expectedReleaseMatrix(manifest) })}\n`);
    } else {
      process.stdout.write(`[product-capabilities] verified ${manifest.definitions.length} definitions and ${expectedReleaseMatrix(manifest).length} release entries\n`);
    }
  } catch (error) {
    process.stderr.write(`[product-capabilities] ${error.message}\n`);
    process.exitCode = 2;
  }
}

import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function parse(values) {
  if (values.length % 2) throw new Error("every option requires one value");
  const result = {};
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index].match(/^--([a-z-]+)$/u)?.[1];
    if (!key || result[key]) throw new Error("options must be unique --name value pairs");
    result[key] = values[index + 1];
  }
  for (const key of ["pack", "target", "prepared-root", "output", "entrypoint"]) {
    if (!result[key]?.trim()) throw new Error(`--${key} is required`);
  }
  return result;
}

async function assertNoLinks(root) {
  for (const entry of await fs.readdir(root, { withFileTypes: true })) {
    const candidate = path.join(root, entry.name);
    const status = await fs.lstat(candidate);
    if (status.isSymbolicLink()) throw new Error(`prepared payload contains a symbolic link: ${candidate}`);
    if (status.isDirectory()) await assertNoLinks(candidate);
  }
}

export async function stagePreparedCapability(options, root = repositoryRoot) {
  const [product, recipes, sources] = await Promise.all([
    fs.readFile(path.join(root, "capabilities", "product-manifest.json"), "utf8").then(JSON.parse),
    fs.readFile(path.join(root, "capabilities", "release-recipes.json"), "utf8").then(JSON.parse),
    fs.readFile(path.join(root, "capabilities", "release-sources.json"), "utf8").then(JSON.parse),
  ]);
  const definition = product.definitions.find((item) => item.capabilityId === options.pack && item.distributionTier === "published");
  if (!definition || !definition.supportedTargets.includes(options.target)) throw new Error("pack/target is not published by the product manifest");
  const recipe = recipes.recipes?.[options.pack];
  if (!recipe) throw new Error("release recipe is missing");
  const preparedRoot = path.resolve(options.preparedRoot);
  const output = path.resolve(options.output);
  if (!(await fs.stat(preparedRoot).catch(() => null))?.isDirectory()) throw new Error("prepared root is missing");
  if (await fs.stat(output).catch(() => null)) throw new Error("output must not already exist");
  for (const required of [options.entrypoint, "NOTICE.md", "SBOM.spdx.json", "BUILD-PROVENANCE.json"]) {
    if (!(await fs.stat(path.join(preparedRoot, ...required.split("/"))).catch(() => null))?.isFile()) {
      throw new Error(`prepared payload is missing ${required}`);
    }
  }
  const provenance = JSON.parse(await fs.readFile(path.join(preparedRoot, "BUILD-PROVENANCE.json"), "utf8"));
  if (provenance.target !== options.target || provenance.packId !== options.pack || provenance.runtimeNetwork !== definition.runtime.network) {
    throw new Error("prepared provenance does not match the product contract");
  }
  const sourceLocks = Object.fromEntries((recipe.sources || []).map((name) => {
    const source = name.split(".").reduce((value, key) => value?.[key], sources);
    if (!source?.version || !source?.license) throw new Error(`release source ${name} is not locked`);
    const locked = structuredClone(source);
    if (locked.distributions) locked.distributions = { [options.target]: locked.distributions[options.target] };
    return [name, locked];
  }));
  await assertNoLinks(preparedRoot);
  await fs.cp(preparedRoot, output, { recursive: true, dereference: true, errorOnExist: true, force: false, preserveTimestamps: true });
  const contract = {
    schemaVersion: 1,
    capabilityId: definition.capabilityId,
    targetTriple: options.target,
    protocolVersion: definition.protocolVersion,
    entrypoint: options.entrypoint,
    entrypointArgs: options.entrypointArgs || [],
    routes: definition.routes,
    formats: definition.formats,
    runtime: definition.runtime,
    licenseExpression: definition.licensePolicy.expression,
    sourceLocks,
  };
  await fs.writeFile(path.join(output, "CAPABILITY-CONTRACT.json"), `${JSON.stringify(contract, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  await assertNoLinks(output);
  return { entrypoint: options.entrypoint, entrypointArgs: contract.entrypointArgs, contract };
}

async function main() {
  const values = parse(process.argv.slice(2));
  const result = await stagePreparedCapability({
    pack: values.pack,
    target: values.target,
    preparedRoot: values["prepared-root"],
    output: values.output,
    entrypoint: values.entrypoint,
    entrypointArgs: values["entrypoint-arg"] ? [values["entrypoint-arg"]] : [],
  });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`stage-prepared-capability: ${error.message}\n`);
    process.exitCode = 1;
  });
}

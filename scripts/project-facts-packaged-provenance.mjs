import { createHash } from "node:crypto";
import { readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

function runGit(repository, args) {
  const result = spawnSync("git", ["-C", repository, ...args], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(result.error?.message ?? result.stderr.trim() ?? "git failed");
  }
  return result.stdout.trim();
}

async function fileEvidence(file) {
  const bytes = await readFile(file);
  return {
    name: path.basename(file),
    size: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

export async function createProjectFactsPackagedProvenance({
  repository,
  sourceCommit,
  installer,
  builtExecutable,
  expectedVersion,
  output,
}) {
  const resolvedCommit = runGit(repository, ["rev-parse", `${sourceCommit}^{commit}`]);
  if (resolvedCommit !== sourceCommit) {
    throw new Error("Source commit must be the exact full commit used for the build.");
  }
  const sourceTree = runGit(repository, ["rev-parse", `${sourceCommit}^{tree}`]);
  const provenance = {
    schemaVersion: 1,
    source: { commitSha: sourceCommit, treeSha: sourceTree },
    build: {
      expectedVersion,
      rustFeatures: ["performance-observers"],
      frontendObserver: "VITE_PROJECT_FACTS_PERF_OBSERVER=1",
      bundleTarget: "msi",
    },
    artifacts: {
      installer: await fileEvidence(installer),
      builtExecutable: await fileEvidence(builtExecutable),
    },
  };
  await writeFile(output, `${JSON.stringify(provenance, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  return provenance;
}

export async function verifyProjectFactsPackagedProvenance({
  provenance,
  repository,
  sourceCommit,
  installer,
  builtExecutable,
  installedExecutable,
  expectedVersion,
}) {
  if (provenance?.schemaVersion !== 1
      || provenance.source?.commitSha !== sourceCommit
      || provenance.build?.expectedVersion !== expectedVersion
      || provenance.build?.frontendObserver !== "VITE_PROJECT_FACTS_PERF_OBSERVER=1"
      || !provenance.build?.rustFeatures?.includes("performance-observers")
      || provenance.build?.bundleTarget !== "msi") {
    throw new Error("Packaged provenance does not describe this performance build.");
  }
  const resolvedCommit = runGit(repository, ["rev-parse", `${sourceCommit}^{commit}`]);
  const resolvedTree = runGit(repository, ["rev-parse", `${sourceCommit}^{tree}`]);
  if (resolvedCommit !== sourceCommit || resolvedTree !== provenance.source.treeSha) {
    throw new Error("Packaged provenance source identity does not match Git.");
  }
  const [installerEvidence, builtEvidence, installedEvidence] = await Promise.all([
    fileEvidence(installer),
    fileEvidence(builtExecutable),
    fileEvidence(installedExecutable),
  ]);
  if (JSON.stringify(installerEvidence) !== JSON.stringify(provenance.artifacts?.installer)
      || JSON.stringify(builtEvidence) !== JSON.stringify(provenance.artifacts?.builtExecutable)) {
    throw new Error("Packaged artifact hashes do not match build provenance.");
  }
  if (installedEvidence.sha256 !== builtEvidence.sha256
      || installedEvidence.size !== builtEvidence.size) {
    throw new Error("Installed executable does not match the executable built with the MSI.");
  }
  return { installerEvidence, builtEvidence, installedEvidence, sourceTree: resolvedTree };
}

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const required = (name) => {
    const value = argument(name);
    if (!value) throw new Error(`${name} is required.`);
    return value;
  };
  const output = path.resolve(required("--output"));
  try {
    await stat(output);
    throw new Error("--output must not already exist.");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  const provenance = await createProjectFactsPackagedProvenance({
    repository: path.resolve(required("--repository")),
    sourceCommit: required("--source-commit"),
    installer: path.resolve(required("--installer")),
    builtExecutable: path.resolve(required("--built-exe")),
    expectedVersion: required("--expected-version"),
    output,
  });
  process.stdout.write(`${JSON.stringify(provenance, null, 2)}\n`);
}

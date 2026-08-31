import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

function git(repository, args) {
  const result = spawnSync("git", ["-C", repository, ...args], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) throw new Error(result.error?.message ?? result.stderr.trim() ?? "git failed");
  return result.stdout.trim();
}

async function artifact(file) {
  const bytes = await readFile(file);
  return {
    name: path.basename(file),
    size: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

export async function createGraphTitlebarDragProvenance({
  repository,
  sourceCommit,
  installer,
  builtExecutable,
  output,
  buildCommand,
}) {
  const head = git(repository, ["rev-parse", "HEAD"]);
  const resolvedCommit = git(repository, ["rev-parse", `${sourceCommit}^{commit}`]);
  const sourceTree = git(repository, ["rev-parse", `${sourceCommit}^{tree}`]);
  const status = git(repository, ["status", "--porcelain"]);
  if (head !== sourceCommit || resolvedCommit !== sourceCommit || status !== "") {
    throw new Error("Build provenance requires a clean repository at the exact source commit.");
  }
  const packageVersion = JSON.parse(await readFile(path.join(repository, "package.json"), "utf8")).version;
  const provenance = {
    schemaVersion: 1,
    source: { commitSha: sourceCommit, treeSha: sourceTree, clean: true },
    build: { packageVersion, command: buildCommand },
    artifacts: {
      installer: await artifact(installer),
      builtExecutable: await artifact(builtExecutable),
    },
  };
  await writeFile(output, `${JSON.stringify(provenance, null, 2)}\n`, { encoding: "utf8", flag: "wx" });
  return provenance;
}

export async function verifyGraphTitlebarDragProvenance({
  provenance,
  repository,
  sourceCommit,
  installer,
  builtExecutable,
}) {
  const expectedInstaller = await artifact(installer);
  const expectedExecutable = await artifact(builtExecutable);
  const head = git(repository, ["rev-parse", "HEAD"]);
  const tree = git(repository, ["rev-parse", `${sourceCommit}^{tree}`]);
  const status = git(repository, ["status", "--porcelain"]);
  assertGraphTitlebarDragProvenanceRecord(provenance, {
    sourceCommit,
    sourceTree: tree,
    sourceHead: head,
    sourceClean: status === "",
    packageVersion: JSON.parse(await readFile(path.join(repository, "package.json"), "utf8")).version,
    installer: expectedInstaller,
    builtExecutable: expectedExecutable,
  });
  return provenance;
}

export function assertGraphTitlebarDragProvenanceRecord(provenance, expected) {
  if (provenance?.schemaVersion !== 1
      || provenance.source?.commitSha !== expected.sourceCommit
      || provenance.source?.treeSha !== expected.sourceTree
      || provenance.source?.clean !== true
      || provenance.build?.packageVersion !== expected.packageVersion
      || typeof provenance.build?.command !== "string"
      || !provenance.build.command.includes("tauri")
      || JSON.stringify(provenance.artifacts?.installer) !== JSON.stringify(expected.installer)
      || JSON.stringify(provenance.artifacts?.builtExecutable) !== JSON.stringify(expected.builtExecutable)
      || expected.sourceHead !== expected.sourceCommit
      || expected.sourceClean !== true) {
    throw new Error("Packaged build provenance does not match the exact clean source and artifacts.");
  }
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
  const provenance = await createGraphTitlebarDragProvenance({
    repository: path.resolve(required("--repository")),
    sourceCommit: required("--source-commit"),
    installer: path.resolve(required("--installer")),
    builtExecutable: path.resolve(required("--built-exe")),
    output: path.resolve(required("--output")),
    buildCommand: required("--build-command"),
  });
  process.stdout.write(`${JSON.stringify(provenance, null, 2)}\n`);
}

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { buildCapabilityReleasePlan } from "./capability-release-plan.mjs";
import { verifyCapabilityCatalog } from "./verify-capability-catalog.mjs";

export const repository = "StoneLL1/llm-wiki-desktop";
const jobPrefix = "Build signed capability matrix without publishing / ";
const sourceScripts = new Set([
  "capability-release-plan", "prepare-release-capability", "prepare-macos-runtime",
  "fetch-capability-runtime", "fetch-rapidocr-sources", "fetch-sensevoice-sources",
  "stage-node-capability", "stage-prepared-capability", "stage-rapidocr-capability",
  "stage-sensevoice-capability", "qualify-staged-capability", "qualify-release-corpus",
  "verify-capability-catalog", "verify-product-capabilities",
].map((name) => `scripts/${name}.mjs`));
const rustInputs = new Set([
  "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/build.rs",
  "src-tauri/src/bin/capability_release.rs",
  ...["capability_pack", "capability_installer", "capability_payload"].map((name) => `src-tauri/src/services/import_v2/${name}.rs`),
]);

export function isCapabilityInput(file) {
  return file.startsWith("capabilities/") || file.startsWith("tests/fixtures/import-v2/")
    || sourceScripts.has(file) || rustInputs.has(file);
}

export function validateCapabilityReuse({ entries, run, jobs, artifacts, changedFiles, tag, sourceRunId, now = Date.now() }) {
  if (!/^app-v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-rc\.[1-9]\d*)?$/u.test(tag)) throw new Error("invalid desktop release tag");
  if (!Number.isSafeInteger(run.id) || run.id !== Number(sourceRunId)
    || run.repository?.full_name !== repository || run.head_repository?.full_name !== repository
    || run.path !== ".github/workflows/desktop-release.yml" || run.head_branch !== tag
    || run.status !== "completed" || !["failure", "success"].includes(run.conclusion)
    || !/^[a-f0-9]{40}$/u.test(run.head_sha)) throw new Error("reuse requires the completed canonical desktop release run for the exact tag");
  if (changedFiles.length) throw new Error(`payload, dependency, or qualification inputs changed; rebuild capabilities: ${changedFiles.join(", ")}`);
  const successfulJob = (name) => {
    const matches = jobs.filter((job) => job.name === jobPrefix + name);
    if (matches.length !== 1 || matches[0].status !== "completed" || matches[0].conclusion !== "success"
      || matches[0].head_sha !== run.head_sha || matches[0].run_id !== run.id) throw new Error(`missing, failed, or unbound source job: ${name}`);
    return matches[0].id;
  };
  const artifactFor = (name) => {
    const matches = artifacts.filter((artifact) => artifact.name === name);
    if (matches.length !== 1) throw new Error(`missing or ambiguous source artifact: ${name}`);
    const artifact = matches[0];
    if (artifact.expired !== false || !Number.isFinite(Date.parse(artifact.expires_at)) || Date.parse(artifact.expires_at) <= now) throw new Error(`expired source artifact: ${name}`);
    if (!Number.isSafeInteger(artifact.id) || artifact.id <= 0 || artifact.size_in_bytes <= 0
      || artifact.workflow_run?.head_sha !== run.head_sha || artifact.workflow_run?.id !== run.id
      || artifact.workflow_run?.repository_id !== run.repository.id
      || artifact.workflow_run?.head_repository_id !== run.repository.id
      || !/^sha256:[a-f0-9]{64}$/u.test(artifact.digest)) throw new Error(`unbound source artifact: ${name}`);
    return { name, artifactId: artifact.id, digest: artifact.digest, sizeInBytes: artifact.size_in_bytes };
  };
  if (!entries.length || new Set(entries.map((entry) => `${entry.capabilityId}/${entry.targetTriple}`)).size !== entries.length) throw new Error("invalid expected capability matrix");
  const reused = entries.map(({ capabilityId, targetTriple }) => ({
    capabilityId, targetTriple,
    jobId: successfulJob(`Build and qualify ${capabilityId} (${targetTriple})`),
    ...artifactFor(`${capabilityId}-capabilities-${targetTriple}`),
  }));
  const mergeJobId = successfulJob("Merge and verify the manifest-derived install catalog");
  const catalog = artifactFor("capability-install-catalog");
  return {
    schemaVersion: 1, repository, tag, sourceRunId: run.id, sourceCommit: run.head_sha,
    expectedEntryCount: entries.length, catalogArtifactId: catalog.artifactId,
    catalogArtifactDigest: catalog.digest, mergeJobId, artifacts: reused,
    include: reused.map((entry) => ({ ...entry, os: "ubuntu-24.04", reuseArtifactId: String(entry.artifactId) })),
  };
}

export function compareCapabilityInputs(root, sourceCommit) {
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  const sourceFiles = git("ls-tree", "-r", "--name-only", "-z", sourceCommit).split("\0").filter(Boolean);
  const currentFiles = git("ls-files", "-z", "--cached", "--others", "--exclude-standard").split("\0").filter(Boolean);
  const inputFiles = new Set([...sourceFiles, ...currentFiles].filter(isCapabilityInput));
  // Include the transitive local JS dependencies from BOTH trees. A modified
  // import cannot silently remove an old input from the comparison set.
  for (const file of inputFiles) {
    if (!file.endsWith(".mjs")) continue;
    const contents = [];
    if (sourceFiles.includes(file)) contents.push(git("show", `${sourceCommit}:${file}`));
    if (fs.existsSync(path.join(root, file))) contents.push(fs.readFileSync(path.join(root, file), "utf8"));
    for (const content of contents) for (const match of content.matchAll(/(?:from\s*|import\s*\(\s*)["'](\.[^"']+)["']/gu)) {
      inputFiles.add(path.posix.normalize(path.posix.join(path.posix.dirname(file), match[1])));
    }
  }
  const changed = git("diff", "--name-only", "--no-renames", "-z", sourceCommit, "--").split("\0").filter(Boolean);
  const untracked = currentFiles.filter((file) => !sourceFiles.includes(file));
  return { checkedInputFiles: [...inputFiles].sort(), changedFiles: [...new Set([...changed, ...untracked])].filter((file) => inputFiles.has(file)).sort() };
}

async function main() {
  const args = process.argv.slice(2);
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    if (!["--source-run-id", "--tag", "--output"].includes(args[index]) || !args[index + 1]) throw new Error("expected --source-run-id ID --tag app-vX.Y.Z --output FILE");
    options[args[index]] = args[index + 1];
  }
  if (!/^[1-9]\d*$/u.test(options["--source-run-id"] ?? "") || !options["--tag"] || !options["--output"]) throw new Error("--source-run-id, --tag and --output are required");
  const root = path.resolve(import.meta.dirname, "..");
  const api = (...args_) => execFileSync("gh", ["api", ...args_], { maxBuffer: 32 * 1024 * 1024 });
  const sourceRunId = options["--source-run-id"];
  const base = `repos/${repository}/actions/runs/${sourceRunId}`;
  const run = JSON.parse(api(base));
  // Full source SHA is the identity; a later recovery branch need not contain it.
  if (!/^[a-f0-9]{40}$/u.test(run.head_sha)) throw new Error("invalid source commit");
  try { execFileSync("git", ["cat-file", "-e", `${run.head_sha}^{commit}`], { cwd: root, stdio: "pipe" }); }
  catch { execFileSync("git", ["fetch", "--no-tags", `https://github.com/${repository}.git`, run.head_sha], { cwd: root, stdio: "inherit" }); }
  const jobs = JSON.parse(api(`${base}/jobs?per_page=100&filter=latest`, "--paginate", "--slurp")).flatMap((page) => page.jobs);
  const artifacts = JSON.parse(api(`${base}/artifacts?per_page=100`, "--paginate", "--slurp")).flatMap((page) => page.artifacts);
  const comparison = compareCapabilityInputs(root, run.head_sha);
  const plan = await buildCapabilityReleasePlan(root);
  if (plan.errors.length) throw new Error(plan.errors.join("; "));
  const result = validateCapabilityReuse({ entries: plan.entries, run, jobs, artifacts, changedFiles: comparison.changedFiles, tag: options["--tag"], sourceRunId });
  const archive = api(`repos/${repository}/actions/artifacts/${result.catalogArtifactId}/zip`);
  if (`sha256:${createHash("sha256").update(archive).digest("hex")}` !== result.catalogArtifactDigest) throw new Error("catalog artifact archive digest mismatch");
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "wiki-capability-reuse-"));
  try {
    const zip = path.join(temporary, "catalog.zip");
    fs.writeFileSync(zip, archive);
    const read = (name) => JSON.parse(execFileSync("unzip", ["-p", zip, name], { maxBuffer: 16 * 1024 * 1024 }));
    const catalog = read("install-catalog.json");
    const provenance = read("catalog-provenance.json");
    const trustedKeys = read("trusted-keys.json");
    const localKeys = JSON.parse(fs.readFileSync(path.join(root, "capabilities/trusted-keys.json"), "utf8"));
    if (JSON.stringify(trustedKeys) !== JSON.stringify(localKeys)) throw new Error("source catalog trusted keys differ from checkout");
    const verification = verifyCapabilityCatalog({ catalog, trustedKeys, provenance, mode: "release", expectedTag: result.tag, expectedCommit: result.sourceCommit, expectedRunId: String(result.sourceRunId) });
    if (verification.errors.length) throw new Error(verification.errors.join("; "));
    result.catalogProvenance = provenance;
    result.checkedInputFiles = comparison.checkedInputFiles;
    result.targetCommit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
    fs.mkdirSync(path.dirname(path.resolve(options["--output"])), { recursive: true });
    fs.writeFileSync(options["--output"], JSON.stringify(result, null, 2) + "\n");
    process.stdout.write(JSON.stringify(result) + "\n");
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch((error) => {
  process.stderr.write(`[reuse-capability-release] ${error.message}\n`);
  process.exitCode = 1;
});

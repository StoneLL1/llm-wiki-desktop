import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repository = "StoneLL1/llm-wiki-desktop";
const repairFiles = new Set([
  ".github/workflows/capability-release.yml", ".github/workflows/desktop-release.yml",
  "scripts/recover-capability-plan.mjs", "scripts/recover-capability-plan.node-test.mjs",
  "scripts/check-release-version.mjs", "scripts/check-release-config.node-test.mjs",
  "scripts/prepare-macos-runtime.mjs", "scripts/prepare-macos-runtime.node-test.mjs",
]);

export function recoverCapabilityPlan(plan, { run, artifacts, changedFiles }) {
  if (run.path !== ".github/workflows/desktop-release.yml" || run.conclusion !== "failure"
    || run.repository?.full_name !== repository || !/^[a-f0-9]{40}$/.test(run.head_sha)) {
    throw new Error("recovery requires a failed canonical desktop release run");
  }
  if (changedFiles.some((file) => !repairFiles.has(file))) {
    throw new Error("payload, dependency, or qualification inputs changed; rebuild the capability matrix");
  }
  const reused = [];
  const include = plan.include.map((entry) => {
    const name = `${entry.capabilityId}-capabilities-${entry.targetTriple}`;
    const matches = artifacts.filter((artifact) => artifact.name === name && !artifact.expired);
    if (matches.length > 1) throw new Error(`ambiguous recovery artifact: ${name}`);
    const artifact = matches[0];
    if (!artifact) return { ...entry, reuseArtifactId: "" };
    if (!Number.isSafeInteger(artifact.id) || artifact.id <= 0 || artifact.workflow_run?.head_sha !== run.head_sha) {
      throw new Error(`unbound recovery artifact: ${name}`);
    }
    reused.push({ name, artifactId: artifact.id });
    return { ...entry, os: "ubuntu-24.04", reuseArtifactId: String(artifact.id) };
  });
  return { ...plan, include, recovery: { sourceRunId: run.id, sourceCommit: run.head_sha, reused } };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [planFile, runId] = process.argv.slice(2);
  const plan = JSON.parse(fs.readFileSync(planFile, "utf8"));
  if (!runId) {
    process.stdout.write(JSON.stringify({ ...plan, include: plan.include.map((entry) => ({ ...entry, reuseArtifactId: "" })) }));
  } else {
    if (!/^[1-9][0-9]*$/.test(runId)) throw new Error("invalid source run ID");
    const gh = (...args) => JSON.parse(execFileSync("gh", ["api", ...args], { encoding: "utf8" }));
    const run = gh(`repos/${repository}/actions/runs/${runId}`);
    // Recovery is limited to descendants of the source, with unchanged payload
    // and qualification inputs. Signatures and corpus evidence are checked again
    // by the existing merger and full candidate verifier before publication.
    execFileSync("git", ["merge-base", "--is-ancestor", run.head_sha, "HEAD"]);
    const changedFiles = execFileSync("git", ["diff", "--name-only", run.head_sha, "HEAD"], { encoding: "utf8" }).trim().split("\n").filter(Boolean);
    const pages = gh(`repos/${repository}/actions/runs/${runId}/artifacts?per_page=100`, "--paginate", "--slurp");
    const result = recoverCapabilityPlan(plan, { run, artifacts: pages.flatMap((page) => page.artifacts), changedFiles });
    fs.writeFileSync("capability-recovery.json", JSON.stringify(result.recovery, null, 2) + "\n");
    process.stdout.write(JSON.stringify(result));
  }
}

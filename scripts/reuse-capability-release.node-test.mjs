import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { compareCapabilityInputs, repository, validateCapabilityReuse } from "./reuse-capability-release.mjs";
import { expectedReleaseMatrix } from "./verify-product-capabilities.mjs";

function fixture() {
  const entries = expectedReleaseMatrix();
  const run = { id: 123, head_sha: "a".repeat(40), head_branch: "app-v0.2.1", path: ".github/workflows/desktop-release.yml", status: "completed", conclusion: "failure", repository: { full_name: repository, id: 7 }, head_repository: { full_name: repository } };
  const job = (name, id) => ({ name: `Build signed capability matrix without publishing / ${name}`, id, run_id: run.id, head_sha: run.head_sha, status: "completed", conclusion: "success" });
  const artifact = (name, id) => ({ name, id, expired: false, expires_at: "2099-01-01T00:00:00Z", size_in_bytes: 100, digest: `sha256:${"b".repeat(64)}`, workflow_run: { head_sha: run.head_sha, id: run.id, repository_id: 7, head_repository_id: 7 } });
  return { entries, run, tag: "app-v0.2.1", sourceRunId: "123", changedFiles: [],
    jobs: [...entries.map((entry, index) => job(`Build and qualify ${entry.capabilityId} (${entry.targetTriple})`, index + 100)), job("Merge and verify the manifest-derived install catalog", 200)],
    artifacts: [...entries.map((entry, index) => artifact(`${entry.capabilityId}-capabilities-${entry.targetTriple}`, index + 300)), artifact("capability-install-catalog", 400)],
  };
}

test("reuse records the complete 43-entry source matrix and catalog identity", () => {
  const result = validateCapabilityReuse(fixture());
  assert.equal(result.expectedEntryCount, 43);
  assert.equal(result.artifacts.length, 43);
  assert.equal(result.sourceCommit, "a".repeat(40));
  assert.equal(result.sourceRunId, 123);
  assert.equal(result.catalogArtifactId, 400);
  assert.equal(result.include[0].reuseArtifactId, "300");
});

test("reuse rejects wrong repository, source tag, job SHA, job status and missing merge", () => {
  for (const mutate of [
    (value) => { value.run.repository.full_name = "someone/fork"; },
    (value) => { value.run.head_repository.full_name = "someone/fork"; },
    (value) => { value.run.head_branch = "app-v0.2.0"; },
    (value) => { value.jobs[0].head_sha = "c".repeat(40); },
    (value) => { value.jobs[0].conclusion = "failure"; },
    (value) => { value.jobs[0].run_id = 321; },
    (value) => { value.jobs.pop(); },
    (value) => { value.jobs.push(value.jobs[0]); },
  ]) {
    const value = fixture(); mutate(value);
    assert.throws(() => validateCapabilityReuse(value), /canonical|source job/);
  }
});

test("reuse rejects missing, duplicate, expired and falsely bound artifact metadata", () => {
  for (const mutate of [
    (value) => { value.artifacts.shift(); },
    (value) => { value.artifacts.pop(); },
    (value) => { value.artifacts.push(value.artifacts[0]); },
    (value) => { value.artifacts[0].expired = true; },
    (value) => { value.artifacts[0].expires_at = "2000-01-01T00:00:00Z"; },
    (value) => { value.artifacts[0].workflow_run.head_sha = "c".repeat(40); },
    (value) => { value.artifacts[0].workflow_run.id = 321; },
    (value) => { value.artifacts[0].workflow_run.repository_id = 8; },
    (value) => { value.artifacts[0].digest = "sha256:invalid"; },
  ]) {
    const value = fixture(); mutate(value);
    assert.throws(() => validateCapabilityReuse(value), /source artifact/);
  }
});

test("reuse refuses changed capability inputs", () => {
  assert.throws(() => validateCapabilityReuse({ ...fixture(), changedFiles: ["capabilities/release-sources.json"] }), /inputs changed/);
});

test("content comparison allows app-only commits and unrelated ancestry, but catches local payload and transitive input edits", (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "wiki-reuse-inputs-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const write = (file, value) => { fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true }); fs.writeFileSync(path.join(root, file), value); };
  git("init"); git("config", "user.name", "Fixture"); git("config", "user.email", "fixture@example.invalid");
  write("capabilities/pack/runner.mjs", "export const value = 1;\n");
  write("scripts/prepare-release-capability.mjs", 'import { helper } from "./helper.mjs";\n');
  write("scripts/helper.mjs", "export const helper = 1;\n");
  write("src/App.tsx", "app version one\n");
  git("add", "."); git("commit", "-m", "source");
  const source = git("rev-parse", "HEAD");
  git("checkout", "--orphan", "new-app-history");
  write("src/App.tsx", "app version two\n");
  git("add", "."); git("commit", "-m", "app-only recovery");
  assert.deepEqual(compareCapabilityInputs(root, source).changedFiles, []);
  write("scripts/helper.mjs", "export const helper = 2;\n");
  assert.deepEqual(compareCapabilityInputs(root, source).changedFiles, ["scripts/helper.mjs"]);
  git("restore", "scripts/helper.mjs");
  write("capabilities/pack/runner.mjs", "export const value = 2;\n");
  assert.deepEqual(compareCapabilityInputs(root, source).changedFiles, ["capabilities/pack/runner.mjs"]);
  git("restore", "capabilities/pack/runner.mjs");
  write("capabilities/new-model.bin", "untracked input");
  assert.deepEqual(compareCapabilityInputs(root, source).changedFiles, ["capabilities/new-model.bin"]);
});

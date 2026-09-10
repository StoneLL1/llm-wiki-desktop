import assert from "node:assert/strict";
import test from "node:test";
import { recoverCapabilityPlan } from "./recover-capability-plan.mjs";

const sha = "a".repeat(40);
const plan = { expectedEntryCount: 2, include: [
  { capabilityId: "browser-runtime", targetTriple: "aarch64-apple-darwin", os: "macos-15" },
  { capabilityId: "ocr-basic", targetTriple: "aarch64-apple-darwin", os: "macos-15" },
] };
const input = () => ({
  run: { id: 123, head_sha: sha, conclusion: "failure", path: ".github/workflows/desktop-release.yml", repository: { full_name: "StoneLL1/llm-wiki-desktop" } },
  artifacts: [{ id: 456, name: "ocr-basic-capabilities-aarch64-apple-darwin", expired: false, workflow_run: { head_sha: sha } }],
  changedFiles: ["scripts/prepare-macos-runtime.mjs"],
});

test("reuses qualified artifacts on Linux while missing targets retain their native build runner", () => {
  const result = recoverCapabilityPlan(plan, input());
  assert.equal(result.expectedEntryCount, 2);
  assert.equal(result.include[0].reuseArtifactId, "");
  assert.equal(result.include[0].os, "macos-15");
  assert.equal(result.include[1].reuseArtifactId, "456");
  assert.equal(result.include[1].os, "ubuntu-24.04");
  assert.equal(result.recovery.sourceCommit, sha);
});
test("never reuses archives after payload, dependency, or qualification changes", () => {
  for (const file of ["capabilities/browser-runtime/runner/index.mjs", "capabilities/release-sources.json", "scripts/qualify-release-corpus.mjs", "src-tauri/src/bin/capability_release.rs"]) {
    assert.throws(() => recoverCapabilityPlan(plan, { ...input(), changedFiles: [file] }), /inputs changed/);
  }
});
test("rejects foreign runs, mismatched provenance, and duplicate archives", () => {
  const value = input();
  assert.throws(() => recoverCapabilityPlan(plan, { ...value, run: { ...value.run, repository: { full_name: "other/repo" } } }), /canonical/);
  assert.throws(() => recoverCapabilityPlan(plan, { ...value, artifacts: [{ ...value.artifacts[0], workflow_run: { head_sha: "b".repeat(40) } }] }), /unbound/);
  assert.throws(() => recoverCapabilityPlan(plan, { ...value, artifacts: [...value.artifacts, ...value.artifacts] }), /ambiguous/);
  assert.equal(recoverCapabilityPlan(plan, { ...value, artifacts: [{ ...value.artifacts[0], expired: true }] }).include[1].reuseArtifactId, "");
});

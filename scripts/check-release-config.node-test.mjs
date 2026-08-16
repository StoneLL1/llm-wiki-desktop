import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

import {
  parseReleaseTag,
  repositoryRoot,
  validateLocalGit,
  validateReleaseCommitTrace,
  validateReleaseState,
  validateWorkflowPermissions,
} from "./check-release-version.mjs";

const contract = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "release/release-contract.json"), "utf8"));
const packageJson = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "package.json"), "utf8"));
const cargoToml = fs.readFileSync(path.join(repositoryRoot, "src-tauri/Cargo.toml"), "utf8");
const tauriConfig = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "src-tauri/tauri.conf.json"), "utf8"));

const state = (overrides = {}) => validateReleaseState({
  contract,
  packageJson,
  cargoToml,
  tauriConfig,
  ...overrides,
});

test("repository versions and frozen application identity agree", () => {
  assert.deepEqual(state().errors, []);
});

test("version drift is a deterministic release failure", () => {
  const result = state({ packageJson: { ...packageJson, version: "0.1.1" } });
  assert.equal(result.errors.some((error) => error.includes("version mismatch")), true);
});

test("the first public version remains historical while later synchronized versions are valid", () => {
  const nextCargo = cargoToml.replace('version = "0.1.0"', 'version = "0.1.1"');
  const result = validateReleaseState({
    contract,
    packageJson: { ...packageJson, version: "0.1.1" },
    cargoToml: nextCargo,
    tauriConfig: { ...tauriConfig, version: "0.1.1" },
    tag: "app-v0.1.1",
  });
  assert.deepEqual(result.errors, []);

  const invalidPrerelease = "1.0.0-01";
  const invalidResult = validateReleaseState({
    contract,
    packageJson: { ...packageJson, version: invalidPrerelease },
    cargoToml: cargoToml.replace('version = "0.1.0"', `version = "${invalidPrerelease}"`),
    tauriConfig: { ...tauriConfig, version: invalidPrerelease },
  });
  assert.equal(invalidResult.errors.some((error) => error.includes("not valid SemVer")), true);
});

test("stable and prerelease tags use the frozen app-v SemVer grammar", () => {
  assert.deepEqual(parseReleaseTag("app-v0.1.0", contract), {
    channel: "stable",
    version: "0.1.0",
    baseVersion: "0.1.0",
    rc: null,
  });
  assert.deepEqual(parseReleaseTag("app-v0.1.0-rc.2", contract), {
    channel: "prerelease",
    version: "0.1.0-rc.2",
    baseVersion: "0.1.0",
    rc: 2,
  });
  assert.throws(() => parseReleaseTag("v0.1.0", contract), /frozen app-v SemVer policy/);
  assert.throws(() => parseReleaseTag("app-v0.1.0-rc.0", contract), /frozen app-v SemVer policy/);
  assert.throws(() => parseReleaseTag("app-v00.1.0", contract), /frozen app-v SemVer policy/);
  assert.equal(state({ tag: "app-v0.1.1" }).errors.some((error) => error.includes("does not match configured version")), true);

  const rcVersion = "0.1.0-rc.2";
  const rcResult = validateReleaseState({
    contract,
    packageJson: { ...packageJson, version: rcVersion },
    cargoToml: cargoToml.replace('version = "0.1.0"', `version = "${rcVersion}"`),
    tauriConfig: { ...tauriConfig, version: rcVersion },
    tag: "app-v0.1.0-rc.2",
  });
  assert.deepEqual(rcResult.errors, []);
});

test("canonical endpoints cannot drift to a different repository", () => {
  const changed = structuredClone(contract);
  changed.endpoints.stableUpdaterManifest = "https://github.com/example/fork/releases/latest/download/latest.json";
  changed.endpoints.capabilityAssetBaseTemplate = "https://github.com/example/fork/releases/download/<exact-tag>/";
  const result = validateReleaseState({ contract: changed, packageJson, cargoToml, tauriConfig });
  assert.equal(result.errors.filter((error) => error.includes("canonical repository")).length, 2);
});

test("unknown human signing and approval owners remain explicit pending states", () => {
  assert.equal(contract.publishing.approvalOwner, null);
  assert.equal(contract.publishing.approvalOwnerStatus, "pending-human-input");
  assert.equal(["updater", "capability", "windows", "apple"].every((kind) => {
    const { owner, ownerRole, status } = contract.signing[kind];
    return owner === null && ownerRole === "StoneLL1 repository owner" && status === "pending-human-input";
  }), true);

  const ambiguous = structuredClone(contract);
  delete ambiguous.signing.updater.ownerRole;
  assert.equal(state({ contract: ambiguous }).errors.some((error) => error.includes("updater signing ownership")), true);
});

test("workflow permissions stay read-only except for the final publisher", () => {
  const ciWorkflow = "permissions:\n  contents: read\n";
  const capabilityWorkflow = "permissions:\n  contents: read\n\njobs:\n  publish-catalog:\n    environment: capability-release\n    permissions:\n      contents: write\n    steps:\n      - run: gh release create app-v0.1.0\n";
  assert.deepEqual(validateWorkflowPermissions({ ciWorkflow, capabilityWorkflow }), []);
  assert.deepEqual(validateWorkflowPermissions({
    ciWorkflow: "jobs: {}\n",
    capabilityWorkflow: "permissions:\n  contents: write\n",
  }), [
    "CI must declare top-level contents: read",
    "capability workflow must default to contents: read",
    "non-publishing capability workflows cannot request contents: write",
  ]);

  assert.deepEqual(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace(
      "  publish-catalog:\n",
      "  build:\n    permissions:\n      contents: write\n  publish-catalog:\n",
    ),
  }), ["publishing capability workflow must contain exactly one contents: write grant"]);

  assert.equal(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace(
      "  publish-catalog:\n",
      "  build:\n    permissions:\n      id-token: write\n  publish-catalog:\n",
    ),
  }).some((error) => error.includes("exactly one contents: write")), true);

  assert.equal(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace(
      "  publish-catalog:\n",
      "  build:\n    permissions: { packages: write }\n  publish-catalog:\n",
    ),
  }).some((error) => error.includes("exactly one contents: write")), true);

  assert.deepEqual(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace("    environment: capability-release\n", ""),
  }), ["capability publisher must use the protected capability-release environment"]);

  assert.deepEqual(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: "permissions:\n  contents: read\n\njobs:\n  build:\n    steps: []\n",
  }), []);
});

test("local Git validation normalizes .git while rejecting the wrong origin or missing default branch", () => {
  const runGit = (_root, arguments_) => {
    if (arguments_[0] === "remote") return "https://github.com/StoneLL1/llm-wiki-desktop";
    if (arguments_[0] === "show-ref") return "ok";
    throw new Error(`unexpected git call: ${arguments_.join(" ")}`);
  };
  assert.deepEqual(validateLocalGit("fixture", contract, runGit), []);

  const wrongOrigin = (_root, arguments_) => {
    if (arguments_[0] === "remote") return "https://github.com/example/fork.git";
    throw new Error("missing ref");
  };
  assert.equal(validateLocalGit("fixture", contract, wrongOrigin).length, 2);
});

test("release tags must resolve to a commit reachable from the frozen default branch", () => {
  const calls = [];
  const success = (_root, arguments_) => {
    calls.push(arguments_);
    if (arguments_.at(-1).startsWith("refs/remotes/")) throw new Error("remote ref absent");
  };
  assert.deepEqual(validateReleaseCommitTrace("fixture", contract, "app-v0.1.0", success), []);
  assert.deepEqual(calls, [
    ["merge-base", "--is-ancestor", "app-v0.1.0^{commit}", "refs/remotes/origin/master"],
    ["merge-base", "--is-ancestor", "app-v0.1.0^{commit}", "refs/heads/master"],
  ]);

  const failure = () => { throw new Error("not an ancestor"); };
  assert.match(validateReleaseCommitTrace("fixture", contract, "app-v0.1.0", failure)[0], /not traceable/);
});

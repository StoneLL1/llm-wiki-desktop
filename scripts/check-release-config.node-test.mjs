import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  parseReleaseTag,
  repositoryRoot,
  validateStableReleaseAdvance,
  validateLocalGit,
  validateReleaseCommitTrace,
  validateReleaseState,
  validateDesktopReleaseWorkflow,
  validateWorkflowPermissions,
} from "./check-release-version.mjs";

const contract = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "release/release-contract.json"), "utf8"));
const packageJson = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "package.json"), "utf8"));
const cargoToml = fs.readFileSync(path.join(repositoryRoot, "src-tauri/Cargo.toml"), "utf8");
const tauriConfig = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "src-tauri/tauri.conf.json"), "utf8"));
const trustedKeys = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "capabilities/trusted-keys.json"), "utf8"));

const state = (overrides = {}) => validateReleaseState({
  contract,
  packageJson,
  cargoToml,
  tauriConfig,
  trustedKeys,
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
  const nextCargo = cargoToml.replace('version = "0.2.0"', 'version = "0.2.1"');
  const result = validateReleaseState({
    contract,
    packageJson: { ...packageJson, version: "0.2.1" },
    cargoToml: nextCargo,
    tauriConfig: { ...tauriConfig, version: "0.2.1" },
    trustedKeys,
    tag: "app-v0.2.1",
  });
  assert.deepEqual(result.errors, []);

  const invalidPrerelease = "1.0.0-01";
  const invalidResult = validateReleaseState({
    contract,
    packageJson: { ...packageJson, version: invalidPrerelease },
    cargoToml: cargoToml.replace('version = "0.2.0"', `version = "${invalidPrerelease}"`),
    tauriConfig: { ...tauriConfig, version: invalidPrerelease },
    trustedKeys,
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

  const rcVersion = "0.2.0-rc.2";
  const rcResult = validateReleaseState({
    contract,
    packageJson: { ...packageJson, version: rcVersion },
    cargoToml: cargoToml.replace('version = "0.2.0"', `version = "${rcVersion}"`),
    tauriConfig: { ...tauriConfig, version: rcVersion },
    trustedKeys,
    tag: "app-v0.2.0-rc.2",
  });
  assert.deepEqual(rcResult.errors, []);
});

test("stable publication advances monotonically across the global latest channel", () => {
  assert.deepEqual(validateStableReleaseAdvance("app-v0.2.0", "app-v0.1.9", contract), []);
  assert.match(
    validateStableReleaseAdvance("app-v0.1.9", "app-v0.1.9", contract)[0],
    /must be newer/,
  );
  assert.match(
    validateStableReleaseAdvance("app-v0.1.8", "app-v0.1.9", contract)[0],
    /must be newer/,
  );
  assert.match(
    validateStableReleaseAdvance("app-v0.2.0", "not-a-release-tag", contract)[0],
    /cannot compare/,
  );
});

test("canonical endpoints cannot drift to a different repository", () => {
  const changed = structuredClone(contract);
  changed.endpoints.stableUpdaterManifest = "https://github.com/example/fork/releases/latest/download/latest.json";
  changed.endpoints.capabilityAssetBaseTemplate = "https://github.com/example/fork/releases/download/<exact-tag>/";
  const result = validateReleaseState({ contract: changed, packageJson, cargoToml, tauriConfig, trustedKeys });
  assert.equal(result.errors.filter((error) => error.includes("canonical repository")).length, 2);
});

test("the sole maintainer owns cryptographic signing while backup and OS certificates are not required", () => {
  assert.equal(contract.publishing.approvalOwner, "StoneLL1");
  assert.equal(contract.publishing.approvalOwnerStatus, "confirmed");
  assert.equal(contract.publishing.environmentReviewer, "StoneLL1");
  assert.equal(contract.publishing.environmentPreventSelfReview, false);
  assert.equal(["updater", "capability"].every((kind) => {
    const { owner, ownerRole, status, backupCustodian, backupStatus } = contract.signing[kind];
    return owner === "StoneLL1"
      && ownerRole === "sole repository owner and maintainer"
      && status === "owner-confirmed"
      && backupCustodian === null
      && backupStatus === "not-required";
  }), true);
  assert.equal(contract.signing.windows.publisherSubject, null);
  assert.equal(contract.signing.windows.osIdentityPolicy, "not-required");
  assert.equal(contract.signing.apple.teamId, null);
  assert.equal(contract.signing.apple.osIdentityPolicy, "not-required");

  const ambiguous = structuredClone(contract);
  delete ambiguous.signing.updater.ownerRole;
  assert.equal(state({ contract: ambiguous }).errors.some((error) => error.includes("updater signing ownership")), true);

  const backupRequired = structuredClone(contract);
  backupRequired.signing.capability.backupStatus = "pending-human-input";
  assert.equal(state({ contract: backupRequired }).errors.some((error) => error.includes("backup-custodian policy")), true);

  const hiddenCertificateGate = structuredClone(contract);
  hiddenCertificateGate.signing.windows.publisherSubject = "CN=Fixture";
  assert.equal(state({ contract: hiddenCertificateGate }).errors.some((error) => error.includes("not required")), true);

  const wrongUpdaterKeyId = structuredClone(contract);
  wrongUpdaterKeyId.signing.updater.publicKeyId = "AAAAAAAAAAAAAAAA";
  assert.equal(state({ contract: wrongUpdaterKeyId }).errors.some((error) => error.includes("Tauri trust anchor")), true);

  const unconfirmedUpdaterPair = structuredClone(contract);
  unconfirmedUpdaterPair.signing.updater.publicKeyStatus = "supplied";
  assert.equal(state({ contract: unconfirmedUpdaterPair }).errors.some((error) => error.includes("key-pair selection")), true);
});

test("the committed capability key ID resolves to the reviewed public trust anchor", () => {
  assert.equal(contract.signing.capability.publicKeyId, "llm-wiki-capability-v1");
  assert.equal(contract.signing.capability.publicKeyStatus, "committed");
  assert.match(trustedKeys[contract.signing.capability.publicKeyId], /^[0-9a-f]{64}$/);
  assert.deepEqual(state().errors, []);

  const missingTrustAnchor = state({ trustedKeys: {} });
  assert.equal(missingTrustAnchor.errors.some((error) => error.includes("32-byte lowercase hex trust anchor")), true);

  const missingRecoveryEvidence = structuredClone(contract);
  delete missingRecoveryEvidence.signing.capability.recoveryCopyStatus;
  assert.equal(state({ contract: missingRecoveryEvidence }).errors.some((error) => error.includes("recovery copy")), true);
});

test("the 0.2.0 upgrade waiver is one-time and 0.2.1 restores the real upgrade gate", () => {
  assert.deepEqual(state().errors, []);

  const widenedWaiver = structuredClone(contract);
  widenedWaiver.acceptance.subsequentStable.firstRequiredVersion = "0.2.0";
  assert.equal(state({ contract: widenedWaiver }).errors.some((error) => error.includes("mandatory from 0.2.1")), true);

  const missingReplacementGate = structuredClone(contract);
  missingReplacementGate.acceptance.firstStable.replacementGate = "source-tests-only";
  assert.equal(state({ contract: missingReplacementGate }).errors.some((error) => error.includes("four-platform clean-install")), true);
});

test("capability workflow permissions stay read-only, reusable, and non-publishing", () => {
  const ciWorkflow = "permissions:\n  contents: read\n";
  const capabilityWorkflow = "on:\n  workflow_call:\npermissions:\n  contents: read\n\njobs:\n  merge-catalog:\n    steps: []\n";
  assert.deepEqual(validateWorkflowPermissions({ ciWorkflow, capabilityWorkflow }), []);
  assert.deepEqual(validateWorkflowPermissions({
    ciWorkflow: "jobs: {}\n",
    capabilityWorkflow: "permissions:\n  contents: write\n",
  }), [
    "CI must declare top-level contents: read",
    "capability workflow must default to contents: read",
    "the non-publishing capability workflow cannot request any write permission",
    "capability workflow must be reusable through workflow_call for the unified desktop release",
  ]);

  assert.deepEqual(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace("  merge-catalog:\n", "  build:\n    permissions:\n      contents: write\n  merge-catalog:\n"),
  }), ["the non-publishing capability workflow cannot request any write permission"]);

  assert.equal(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace("  merge-catalog:\n", "  build:\n    permissions:\n      id-token: write\n  merge-catalog:\n"),
  }).some((error) => error.includes("cannot request any write permission")), true);

  assert.equal(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace("    steps: []\n", "    steps:\n      - run: gh release upload app-v0.1.0\n"),
  }).some((error) => error.includes("must not publish releases")), true);

  assert.equal(validateWorkflowPermissions({
    ciWorkflow,
    capabilityWorkflow: capabilityWorkflow.replace("  workflow_call:\n", "  workflow_dispatch:\n"),
  }).some((error) => error.includes("workflow_call")), true);
});

test("the committed capability workflow is reusable and never publishes", () => {
  const ciWorkflow = fs.readFileSync(path.join(repositoryRoot, ".github/workflows/ci.yml"), "utf8");
  const capabilityWorkflow = fs.readFileSync(
    path.join(repositoryRoot, ".github/workflows/capability-release.yml"),
    "utf8",
  );
  assert.deepEqual(validateWorkflowPermissions({ ciWorkflow, capabilityWorkflow }), []);
});

test("the committed desktop workflow is the only atomic publisher and pins every action", () => {
  const desktopWorkflow = fs.readFileSync(path.join(repositoryRoot, ".github/workflows/desktop-release.yml"), "utf8");
  const capabilityWorkflow = fs.readFileSync(path.join(repositoryRoot, ".github/workflows/capability-release.yml"), "utf8");
  assert.deepEqual(validateDesktopReleaseWorkflow({ desktopWorkflow, capabilityWorkflow }), []);

  const unpinned = desktopWorkflow.replace(
    /actions\/checkout@[0-9a-f]{40}/,
    "actions/checkout@v4",
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: unpinned, capabilityWorkflow })
    .some((error) => error.includes("not pinned")), true);

  const earlyPublisher = desktopWorkflow.replace(
    /^ {2}manifest-and-provenance:/m,
    "  early-release:\n    steps:\n      - run: gh release create app-v0.1.0\n  manifest-and-provenance:",
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: earlyPublisher, capabilityWorkflow })
    .some((error) => error.includes("before publish-stable")), true);

  const earlyWrite = desktopWorkflow.replace(
    /^ {2}desktop-build:/m,
    "  desktop-build:\n    permissions:\n      contents: write",
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: earlyWrite, capabilityWorkflow })
    .some((error) => error.includes("only publish-stable")), true);

  const publicFirst = desktopWorkflow.replace(" --notes-file candidate/release-notes.md --draft", " --notes-file candidate/release-notes.md");
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: publicFirst, capabilityWorkflow })
    .some((error) => error.includes("as a draft")), true);

  const tagScopedConcurrency = desktopWorkflow.replace(
    "group: desktop-release-stable-channel",
    "group: desktop-release-${{ inputs.release_tag || github.ref_name }}",
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: tagScopedConcurrency, capabilityWorkflow })
    .some((error) => error.includes("globally serialize")), true);

  const noStableAdvanceCheck = desktopWorkflow.replace(
    '--current-stable-tag "$current_stable_tag"',
    '--tag "$current_stable_tag"',
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: noStableAdvanceCheck, capabilityWorkflow })
    .some((error) => error.includes("candidate advances")), true);

  const errOnlyRollback = desktopWorkflow
    .replace("trap 'rollback_if_unverified' EXIT", "trap 'rollback_if_unverified' ERR")
    .replace("trap 'exit 130' INT", ":")
    .replace("trap 'exit 143' TERM", ":");
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: errOnlyRollback, capabilityWorkflow })
    .some((error) => error.includes("cancellation, and termination")), true);

  const noImmutableReleaseFallback = desktopWorkflow.replace(
    'gh release delete "$RELEASE_TAG" --yes',
    ': # immutable release fallback removed',
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: noImmutableReleaseFallback, capabilityWorkflow })
    .some((error) => error.includes("delete an immutable release")), true);

  const unguardedPublication = desktopWorkflow.replace("published=1", "published=0");
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: unguardedPublication, capabilityWorkflow })
    .some((error) => error.includes("publish-through-anonymous-verification")), true);

  const prematurelyVerified = desktopWorkflow.replace(
    / {10}verified=1\r?\n {10}trap - EXIT INT TERM/,
    "          trap - EXIT INT TERM\n          verified=1",
  );
  assert.notEqual(prematurelyVerified, desktopWorkflow);
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: prematurelyVerified, capabilityWorkflow })
    .some((error) => error.includes("publish-through-anonymous-verification")), true);

  const noReverseDownload = desktopWorkflow.replaceAll("gh release download", "gh draft download");
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: noReverseDownload, capabilityWorkflow })
    .some((error) => error.includes("gh release download")), true);

  const noCryptoVerification = desktopWorkflow.replaceAll("verify-updater-signatures.mjs", "inspect-updater-signatures.mjs");
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: noCryptoVerification, capabilityWorkflow })
    .some((error) => error.includes("verify-updater-signatures.mjs")), true);

  const legacyCertificateGate = desktopWorkflow.replace(
    "CAPABILITY_KEY_ID: ${{ vars.CAPABILITY_SIGNING_KEY_ID }}",
    "CAPABILITY_KEY_ID: ${{ vars.CAPABILITY_SIGNING_KEY_ID }}\n          WINDOWS_CERTIFICATE: ${{ secrets.WINDOWS_CERTIFICATE }}",
  );
  assert.notEqual(legacyCertificateGate, desktopWorkflow);
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: legacyCertificateGate, capabilityWorkflow })
    .some((error) => error.includes("must not require OS vendor signing credentials")), true);

  const missingUnsignedPolicy = desktopWorkflow.replace("windows-authenticode-not-required", "windows-policy-missing");
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: missingUnsignedPolicy, capabilityWorkflow })
    .some((error) => error.includes("windows-authenticode-not-required")), true);

  const mutableSealingToolchain = desktopWorkflow.replaceAll(
    "actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
    `actions/setup-node@${"a".repeat(40)}`,
  );
  assert.equal(validateDesktopReleaseWorkflow({ desktopWorkflow: mutableSealingToolchain, capabilityWorkflow })
    .some((error) => error.includes("pinned Node and Rust toolchains")), true);
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

test("local Git validation follows a linked worktree commondir", (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-release-linked-worktree-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const checkout = path.join(root, "checkout");
  const commonGit = path.join(root, "common.git");
  const worktreeGit = path.join(commonGit, "worktrees", "fixture");
  fs.mkdirSync(checkout, { recursive: true });
  fs.mkdirSync(worktreeGit, { recursive: true });
  fs.mkdirSync(path.join(commonGit, "refs", "heads"), { recursive: true });
  fs.writeFileSync(path.join(checkout, ".git"), `gitdir: ${worktreeGit}\n`);
  fs.writeFileSync(path.join(worktreeGit, "commondir"), "../..\n");
  fs.writeFileSync(path.join(commonGit, "config"), [
    '[remote "origin"]',
    "  url = https://github.com/StoneLL1/llm-wiki-desktop.git",
    "",
  ].join("\n"));
  fs.writeFileSync(path.join(commonGit, "refs", "heads", "master"), `${"a".repeat(40)}\n`);

  assert.deepEqual(validateLocalGit(checkout, contract), []);
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

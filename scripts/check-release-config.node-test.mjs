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
} from "./check-release-version.mjs";

const contract = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "release/release-contract.json"), "utf8"));
const packageJson = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "package.json"), "utf8"));
const cargoToml = fs.readFileSync(path.join(repositoryRoot, "src-tauri/Cargo.toml"), "utf8");
const tauriConfig = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "src-tauri/tauri.conf.json"), "utf8"));
const trustedKeys = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "capabilities/trusted-keys.json"), "utf8"));
const releaseSources = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "capabilities/release-sources.json"), "utf8"));

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

test("LibreOffice release inputs use the immutable build-qualified archive", () => {
  assert.equal(
    releaseSources.libreOffice.source,
    "https://downloadarchive.documentfoundation.org/libreoffice/old/26.2.4.2/",
  );
  assert.deepEqual(
    Object.values(releaseSources.libreOffice.distributions).map(({ file }) => file).sort(),
    [
      "LibreOffice_26.2.4.2_Linux_x86-64_deb.tar.gz",
      "LibreOffice_26.2.4.2_MacOS_aarch64.dmg",
      "LibreOffice_26.2.4.2_MacOS_x86-64.dmg",
      "LibreOffice_26.2.4.2_Win_x86-64.msi",
    ],
  );
});

test("version drift is a deterministic release failure", () => {
  const result = state({ packageJson: { ...packageJson, version: "0.1.1" } });
  assert.equal(result.errors.some((error) => error.includes("version mismatch")), true);
});

test("the first public version remains historical while later synchronized versions are valid", () => {
  const nextCargo = cargoToml.replace(`version = "${packageJson.version}"`, 'version = "0.2.1"');
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
    cargoToml: cargoToml.replace(`version = "${packageJson.version}"`, `version = "${invalidPrerelease}"`),
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
    cargoToml: cargoToml.replace(`version = "${packageJson.version}"`, `version = "${rcVersion}"`),
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

test("signing keys match the committed trust anchors", () => {
  const wrongKey = structuredClone(contract);
  wrongKey.signing.updater.publicKeyId = "AAAAAAAAAAAAAAAA";
  assert.ok(state({ contract: wrongKey }).errors.some((error) => error.includes("Tauri trust anchor")));
  assert.ok(state({ trustedKeys: {} }).errors.some((error) => error.includes("32-byte lowercase hex trust anchor")));
  const privateKey = structuredClone(contract);
  privateKey.signing.updater.privateKey = "must-not-be-committed";
  assert.ok(state({ contract: privateKey }).errors.some((error) => error.includes("private key material")));
});

test("historical approval records do not block a new release", () => {
  const metadata = structuredClone(contract);
  delete metadata.acceptance;
  delete metadata.publishing.environmentReviewer;
  delete metadata.signing.capability.recoveryCopyStatus;
  assert.deepEqual(state({ contract: metadata }).errors, []);
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

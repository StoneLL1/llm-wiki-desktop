import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = path.resolve(scriptDirectory, "..");

const SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

const isValidSemver = (value) => {
  const match = SEMVER_PATTERN.exec(value);
  if (!match) return false;
  const prerelease = match[4];
  return !prerelease || prerelease.split(".").every((identifier) => (
    !/^\d+$/.test(identifier) || identifier === "0" || !identifier.startsWith("0")
  ));
};

const readJson = (root, relativePath) =>
  JSON.parse(fs.readFileSync(path.join(root, relativePath), "utf8"));

const readOptionalText = (root, relativePath) => {
  try {
    return fs.readFileSync(path.join(root, relativePath), "utf8");
  } catch (error) {
    if (error.code === "ENOENT") return "";
    throw error;
  }
};

export function cargoPackageVersion(cargoToml) {
  const packageSection = cargoToml.match(/^\[package\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m)?.[1] ?? "";
  const version = packageSection.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error("src-tauri/Cargo.toml is missing [package].version");
  return version;
}

export function parseReleaseTag(tag, contract) {
  const stable = new RegExp(contract.tags.stablePattern).exec(tag);
  if (stable && isValidSemver(stable[1])) {
    return { channel: "stable", version: stable[1], baseVersion: stable[1], rc: null };
  }
  const prerelease = new RegExp(contract.tags.prereleasePattern).exec(tag);
  if (prerelease && isValidSemver(prerelease[1])) {
    return {
      channel: "prerelease",
      version: `${prerelease[1]}-rc.${prerelease[2]}`,
      baseVersion: prerelease[1],
      rc: Number(prerelease[2]),
    };
  }
  throw new Error(`release tag ${JSON.stringify(tag)} does not match the frozen app-v SemVer policy`);
}

export function validateStableReleaseAdvance(candidateTag, currentStableTag, contract) {
  if (!currentStableTag) return [];
  try {
    const candidate = parseReleaseTag(candidateTag, contract);
    const current = parseReleaseTag(currentStableTag, contract);
    if (candidate.channel !== "stable" || current.channel !== "stable") {
      return ["the stable release channel can only advance between stable app-v tags"];
    }
    const candidateParts = candidate.baseVersion.split(".").map(Number);
    const currentParts = current.baseVersion.split(".").map(Number);
    const comparison = candidateParts.findIndex((part, index) => part !== currentParts[index]);
    if (comparison < 0 || candidateParts[comparison] < currentParts[comparison]) {
      return [`candidate stable tag ${candidateTag} must be newer than current stable tag ${currentStableTag}`];
    }
    return [];
  } catch (error) {
    return [`cannot compare the stable release channel: ${error.message}`];
  }
}

export function validateReleaseState({
  contract,
  packageJson,
  cargoToml,
  tauriConfig,
  tag = null,
  repository = null,
}) {
  const errors = [];
  const versions = {
    packageJson: packageJson.version,
    cargoToml: cargoPackageVersion(cargoToml),
    tauriConfig: tauriConfig.version,
  };
  const uniqueVersions = new Set(Object.values(versions));
  if (uniqueVersions.size !== 1) {
    errors.push(`version mismatch: ${JSON.stringify(versions)}`);
  }
  const version = versions.packageJson;
  if (!isValidSemver(version)) errors.push(`package version is not valid SemVer: ${version}`);
  if (!isValidSemver(contract.application.firstPublicVersion)) {
    errors.push(`frozen first public version is not valid SemVer: ${contract.application.firstPublicVersion}`);
  }
  if (tauriConfig.productName !== contract.application.productName) {
    errors.push(`productName drifted from ${contract.application.productName}`);
  }
  if (tauriConfig.identifier !== contract.application.identifier) {
    errors.push(`identifier drifted from ${contract.application.identifier}`);
  }
  if (contract.repository.slug !== "StoneLL1/llm-wiki-desktop") {
    errors.push("canonical repository must remain StoneLL1/llm-wiki-desktop");
  }
  if (contract.repository.originHttps !== "https://github.com/StoneLL1/llm-wiki-desktop.git") {
    errors.push("canonical origin must remain the approved StoneLL1 HTTPS Git URL");
  }
  if (contract.repository.defaultBranch !== "master") {
    errors.push("default branch must remain the frozen master branch");
  }
  if (contract.repository.visibility !== "public") {
    errors.push("release repository must remain public");
  }
  if (repository && repository !== contract.repository.slug) {
    errors.push(`workflow repository ${repository} does not match ${contract.repository.slug}`);
  }
  const expectedUpdater = `https://github.com/${contract.repository.slug}/releases/latest/download/latest.json`;
  if (contract.endpoints.stableUpdaterManifest !== expectedUpdater) {
    errors.push("stable updater endpoint does not use the canonical repository");
  }
  const expectedCapabilityBase = `https://github.com/${contract.repository.slug}/releases/download/<exact-tag>/`;
  if (contract.endpoints.capabilityAssetBaseTemplate !== expectedCapabilityBase) {
    errors.push("capability asset base does not use the canonical repository and exact tag");
  }
  if (contract.publishing.latestManifestChannel !== "stable-only") {
    errors.push("latest.json generation must remain stable-only");
  }
  const approvalOwnerConfirmed = contract.publishing.approvalOwner === "StoneLL1"
    && contract.publishing.approvalOwnerStatus === "confirmed"
    && typeof contract.publishing.approvalOwnerRole === "string"
    && contract.publishing.approvalOwnerRole.trim().length > 0;
  const environmentReviewerConfigured = contract.publishing.environmentReviewer === "StoneLL1"
    && contract.publishing.environmentReviewerStatus === "configured"
    && contract.publishing.environmentPreventSelfReview === false;
  if (contract.publishing.stableEnvironment !== "desktop-release"
    || contract.publishing.capabilityEnvironment !== "capability-release"
    || !approvalOwnerConfirmed
    || !environmentReviewerConfigured) {
    errors.push("protected release environments and the confirmed sole-maintainer approval policy must remain explicit");
  }
  if (contract.signing?.privateKeyPolicy !== "protected-environment-secrets-only"
    || contract.signing?.continuity?.backupCustodianRequired !== false
    || contract.signing?.continuity?.decisionOwner !== "StoneLL1"
    || contract.signing?.continuity?.riskStatus !== "single-maintainer-continuity-risk-accepted") {
    errors.push("single-maintainer key continuity policy must remain explicit");
  }
  for (const signingKind of ["updater", "capability"]) {
    const signingContract = contract.signing?.[signingKind];
    const ownerConfirmed = signingContract?.owner === "StoneLL1"
      && signingContract?.status === "owner-confirmed"
      && typeof signingContract?.ownerRole === "string"
      && signingContract.ownerRole.trim().length > 0;
    if (!ownerConfirmed) {
      errors.push(`${signingKind} signing ownership must be confirmed for StoneLL1`);
    }
    if (signingContract?.backupCustodian !== null || signingContract?.backupStatus !== "not-required") {
      errors.push(`${signingKind} signing backup-custodian policy must be explicitly not required`);
    }
    if (signingContract && Object.hasOwn(signingContract, "privateKey")) {
      errors.push(`${signingKind} private key material cannot appear in the release contract`);
    }
  }
  const updaterPublicKey = contract.signing?.updater?.publicKey;
  const updaterPublicKeyId = contract.signing?.updater?.publicKeyId;
  const updaterPublicKeyStatus = contract.signing?.updater?.publicKeyStatus;
  const updaterPublicKeyDocument = typeof updaterPublicKey === "string" && /^[A-Za-z0-9+/]+={0,2}$/.test(updaterPublicKey)
    ? Buffer.from(updaterPublicKey, "base64").toString("utf8")
    : "";
  if (updaterPublicKey !== tauriConfig.plugins?.updater?.pubkey
    || !/^[0-9A-F]{16}$/.test(updaterPublicKeyId ?? "")
    || !updaterPublicKeyDocument.includes(`minisign public key: ${updaterPublicKeyId}\n`)) {
    errors.push("updater public key and key ID must match the committed Tauri trust anchor");
  }
  if (updaterPublicKeyStatus !== "confirmed-existing-keypair") {
    errors.push("updater key-pair selection must be confirmed in the release contract");
  }
  const capabilityPublicKeyPending = contract.signing?.capability?.publicKeyId === null
    && contract.signing?.capability?.publicKeyStatus === "pending-human-input";
  const capabilityPublicKeyCommitted = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(
    contract.signing?.capability?.publicKeyId ?? "",
  ) && contract.signing?.capability?.publicKeyStatus === "committed";
  if (!capabilityPublicKeyPending && !capabilityPublicKeyCommitted) {
    errors.push("capability public key must be committed or explicitly pending human input");
  }
  const osIdentityPolicies = [
    ["windows", "publisherSubject", "smartscreen-or-unknown-publisher-warning-expected"],
    ["apple", "teamId", "gatekeeper-manual-override-may-be-required"],
  ];
  for (const [platform, identityField, userWarning] of osIdentityPolicies) {
    const osContract = contract.signing?.[platform];
    if (osContract?.owner !== "StoneLL1"
      || osContract?.status !== "not-required"
      || osContract?.osIdentityPolicy !== "not-required"
      || osContract?.[identityField] !== null
      || osContract?.userWarning !== userWarning) {
      errors.push(`${platform} OS vendor identity signing must remain explicitly not required`);
    }
    if (osContract && Object.hasOwn(osContract, "privateKey")) {
      errors.push(`${platform} private key material cannot appear in the release contract`);
    }
  }
  if (contract.application.identifier !== contract.signing.apple.bundleIdentifier) {
    errors.push("Apple bundle identifier must match the frozen Tauri identifier");
  }
  if (tag) {
    try {
      const parsedTag = parseReleaseTag(tag, contract);
      if (parsedTag.version !== version) {
        errors.push(`tag version ${parsedTag.version} does not match configured version ${version}`);
      }
      if (parsedTag.channel === "stable" && SEMVER_PATTERN.exec(parsedTag.version)?.[4]) {
        errors.push("stable tags cannot contain a prerelease version");
      }
      if (version === contract.application.firstPublicVersion && parsedTag.channel === "stable" && tag !== contract.tags.firstStable) {
        errors.push(`first stable release must use ${contract.tags.firstStable}`);
      }
    } catch (error) {
      errors.push(error.message);
    }
  }
  return { errors, version, versions };
}

const permissionWriteGrants = (workflow) => {
  const lines = workflow.match(/.*(?:\r?\n|$)/g) ?? [];
  const grants = [];
  let offset = 0;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const permission = /^(\s*)permissions:\s*(.*?)\s*(?:\r?\n)?$/.exec(line);
    if (!permission) {
      offset += line.length;
      continue;
    }
    const indent = permission[1].length;
    const inline = permission[2];
    if (/^write-all(?:\s+#.*)?$/i.test(inline)) {
      grants.push({ scope: "*", index: offset });
    }
    for (const match of inline.matchAll(/\b([a-z-]+)\s*:\s*write\b/gi)) {
      grants.push({ scope: match[1].toLowerCase(), index: offset + match.index });
    }
    if (!inline) {
      let nestedOffset = offset + line.length;
      for (let nestedIndex = index + 1; nestedIndex < lines.length; nestedIndex += 1) {
        const nestedLine = lines[nestedIndex];
        if (nestedLine.trim() && (nestedLine.match(/^\s*/)?.[0].length ?? 0) <= indent) break;
        const write = /^\s*([a-z-]+):\s*write\s*(?:#.*)?(?:\r?\n)?$/i.exec(nestedLine);
        if (write) grants.push({ scope: write[1].toLowerCase(), index: nestedOffset });
        nestedOffset += nestedLine.length;
      }
    }
    offset += line.length;
  }
  return grants;
};

export function validateWorkflowPermissions({ ciWorkflow, capabilityWorkflow }) {
  const errors = [];
  if (!/^permissions:\s*\r?\n\s+contents:\s+read\s*$/m.test(ciWorkflow)) {
    errors.push("CI must declare top-level contents: read");
  }
  if (permissionWriteGrants(ciWorkflow).length > 0) {
    errors.push("CI cannot request any write permission scope");
  }
  if (!/^permissions:\s*\r?\n\s+contents:\s+read\s*$/m.test(capabilityWorkflow)) {
    errors.push("capability workflow must default to contents: read");
  }
  const writeGrants = permissionWriteGrants(capabilityWorkflow);
  if (writeGrants.length !== 0) {
    errors.push("the non-publishing capability workflow cannot request any write permission");
  }
  if (/gh release (?:create|upload)/i.test(capabilityWorkflow)) {
    errors.push("capability workflow must not publish releases; the unified desktop release workflow owns publication");
  }
  if (!/^ {2}workflow_call:\s*$/m.test(capabilityWorkflow)) {
    errors.push("capability workflow must be reusable through workflow_call for the unified desktop release");
  }
  return errors;
}

export function validateDesktopReleaseWorkflow({ desktopWorkflow, capabilityWorkflow }) {
  const errors = [];
  if (!/^permissions:\s*\r?\n\s+contents:\s+read\s*$/m.test(desktopWorkflow)) {
    errors.push("desktop release workflow must default to contents: read");
  }
  const publishOffset = desktopWorkflow.search(/^ {2}publish-stable:\s*$/m);
  if (publishOffset < 0) {
    errors.push("desktop release workflow must have one final publish-stable job");
  } else {
    const beforePublish = desktopWorkflow.slice(0, publishOffset);
    const publisher = desktopWorkflow.slice(publishOffset);
    if (/\bcontents:\s*write\b/.test(beforePublish)) {
      errors.push("only publish-stable may request contents: write");
    }
    if ((publisher.match(/\bcontents:\s*write\b/g) ?? []).length !== 1) {
      errors.push("publish-stable must request contents: write exactly once");
    }
    if (!/^ {4}environment:\s*desktop-release\s*$/m.test(publisher)) {
      errors.push("publish-stable must use the protected desktop-release environment");
    }
    if (/\bgh release (?:create|upload|edit)\b/i.test(beforePublish)) {
      errors.push("no job before publish-stable may create, upload, or publish a GitHub release");
    }
  }
  const requiredJobs = [
    "preflight",
    "capability-build",
    "desktop-build",
    "manifest-and-provenance",
    "packaged-smoke",
    "assemble-release",
    "publish-stable",
  ];
  for (const job of requiredJobs) {
    if (!new RegExp(`^ {2}${job}:\\s*$`, "m").test(desktopWorkflow)) errors.push(`desktop release workflow is missing ${job}`);
  }
  for (const marker of [
    "npm ci",
    "npm run check",
    "cargo metadata --locked",
    "capability-release.yml",
    "draft-release-bundle",
    "actions/attest-build-provenance@",
    "gh attestation verify",
    "gh release download",
    "verify-updater-signatures.mjs",
    "generate-release-checksums.mjs --root remote-draft --verify",
    "generate-release-checksums.mjs --root published-assets --verify",
    "packaged-process-alive",
    "CAPABILITY_SIGNING_KEY_ID",
    "TAURI_SIGNING_PRIVATE_KEY",
    "windows-authenticode-not-required",
    "apple-developer-id-not-required",
    "linux-os-code-signing-not-applicable",
  ]) {
    if (!desktopWorkflow.includes(marker)) errors.push(`desktop release workflow is missing required marker: ${marker}`);
  }
  for (const forbiddenMarker of [
    "WINDOWS_CERTIFICATE",
    "WINDOWS_PUBLISHER_SUBJECT",
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_TEAM_ID",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "KEYCHAIN_PASSWORD",
    "Import-PfxCertificate",
    "xcrun stapler",
    "spctl --assess",
  ]) {
    if (desktopWorkflow.includes(forbiddenMarker)) {
      errors.push(`desktop release workflow must not require OS vendor signing credentials or verification: ${forbiddenMarker}`);
    }
  }
  if (!/for name in CAPABILITY_KEY_ID UPDATER_PRIVATE_KEY UPDATER_PRIVATE_KEY_PASSWORD; do/.test(desktopWorkflow)) {
    errors.push("desktop release preflight must require exactly the capability and updater signing inputs");
  }
  if (!/gh release create[^\r\n]*--draft(?:\s|$)/.test(desktopWorkflow)) {
    errors.push("publish-stable must create the GitHub Release as a draft");
  }
  if (!/rollback_if_unverified\(\)\s*\{[\s\S]*trap - EXIT INT TERM[\s\S]*"\$published" -eq 1[\s\S]*"\$verified" -ne 1[\s\S]*gh release edit "\$RELEASE_TAG" --draft=true/.test(desktopWorkflow)
    || !/trap\s+'rollback_if_unverified'\s+EXIT/.test(desktopWorkflow)
    || !/trap\s+'exit 130'\s+INT/.test(desktopWorkflow)
    || !/trap\s+'exit 143'\s+TERM/.test(desktopWorkflow)) {
    errors.push("publish-stable must restore draft visibility on error, cancellation, and termination until anonymous verification completes");
  }
  if (!/if ! gh release edit "\$RELEASE_TAG" --draft=true[\s\S]*gh release delete "\$RELEASE_TAG" --yes[\s\S]*CRITICAL: unverified stable release rollback failed/.test(desktopWorkflow)) {
    errors.push("publish-stable rollback must delete an immutable release when restoring draft visibility fails");
  }
  const stablePublishOffset = desktopWorkflow.indexOf('gh release edit "$RELEASE_TAG" --draft=false --latest');
  const publishedGuardOffset = desktopWorkflow.lastIndexOf("published=1", stablePublishOffset);
  const anonymousVerificationOffset = desktopWorkflow.indexOf("generate-release-checksums.mjs --root published-assets --verify", stablePublishOffset);
  const verifiedGuardOffset = desktopWorkflow.indexOf("verified=1", anonymousVerificationOffset);
  const disarmRollbackOffset = desktopWorkflow.indexOf("trap - EXIT INT TERM", verifiedGuardOffset);
  if (stablePublishOffset < 0) {
    errors.push("publish-stable must make the verified draft stable exactly once");
  } else if (publishedGuardOffset < 0
    || anonymousVerificationOffset < stablePublishOffset
    || verifiedGuardOffset < anonymousVerificationOffset
    || disarmRollbackOffset < verifiedGuardOffset) {
    errors.push("publish-stable must guard one publish-through-anonymous-verification critical section");
  }
  const assembleOffset = desktopWorkflow.search(/^ {2}assemble-release:\s*$/m);
  const sealedJobs = assembleOffset >= 0 && publishOffset > assembleOffset
    ? desktopWorkflow.slice(assembleOffset)
    : "";
  if ((sealedJobs.match(/actions\/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020/g) ?? []).length < 2
    || (sealedJobs.match(/rustup default "\$RUST_VERSION"/g) ?? []).length < 2) {
    errors.push("assemble-release and publish-stable must install the pinned Node and Rust toolchains");
  }
  for (const platform of ["windows-x86_64", "darwin-aarch64", "darwin-x86_64", "linux-x86_64"]) {
    if (!desktopWorkflow.includes(platform)) errors.push(`desktop release matrix is missing ${platform}`);
  }
  if (!/group:\s*desktop-release-stable-channel(?:\s|$)/.test(desktopWorkflow)
    || !/cancel-in-progress:\s*false/.test(desktopWorkflow)) {
    errors.push("desktop release concurrency must globally serialize the stable channel without cancellation");
  }
  if (!/releases\/latest/.test(desktopWorkflow)
    || !/--current-stable-tag\s+"\$current_stable_tag"/.test(desktopWorkflow)
    || !/^\s*404\)\s*$/m.test(desktopWorkflow)
    || !/cannot establish the current stable release/.test(desktopWorkflow)) {
    errors.push("desktop release preflight must fail closed unless the candidate advances the current stable tag or no release exists");
  }
  for (const match of desktopWorkflow.matchAll(/^\s*(?:-\s+)?uses:\s*([^\s#]+)(?:\s+#.*)?$/gm)) {
    const reference = match[1];
    if (reference.startsWith("./")) continue;
    if (!/@[0-9a-f]{40}$/.test(reference)) errors.push(`GitHub Action is not pinned to a full commit SHA: ${reference}`);
  }
  if (!/^ {4}uses:\s*\.\/\.github\/workflows\/capability-release\.yml\s*$/m.test(desktopWorkflow)) {
    errors.push("desktop release must call the reusable capability workflow");
  }
  if (/\bgh release (?:create|upload|edit)\b/i.test(capabilityWorkflow)) {
    errors.push("capability workflow cannot publish independently after desktop orchestration exists");
  }
  return errors;
}

const normalizeOrigin = (value) => value.trim().replace(/\/$/, "").replace(/\.git$/, "").toLowerCase();

const resolveGitDirectory = (root) => {
  const dotGit = path.join(root, ".git");
  if (fs.statSync(dotGit).isDirectory()) return dotGit;
  const pointer = fs.readFileSync(dotGit, "utf8").match(/^gitdir:\s*(.+)\s*$/i)?.[1];
  if (!pointer) throw new Error(".git does not contain a gitdir pointer");
  return path.resolve(root, pointer);
};

const readLocalGitState = (root, contract) => {
  const gitDirectory = resolveGitDirectory(root);
  const config = fs.readFileSync(path.join(gitDirectory, "config"), "utf8");
  const originSection = config.match(/^\[remote\s+"origin"\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m)?.[1] ?? "";
  const origin = originSection.match(/^\s*url\s*=\s*(.+?)\s*$/m)?.[1] ?? "";
  const branchRef = `refs/heads/${contract.repository.defaultBranch}`;
  const looseRefExists = fs.existsSync(path.join(gitDirectory, ...branchRef.split("/")));
  const packedRefs = readOptionalText(gitDirectory, "packed-refs");
  const packedRefExists = packedRefs.split(/\r?\n/).some((line) => line.endsWith(` ${branchRef}`));
  return { origin, branchRefExists: looseRefExists || packedRefExists };
};

export function validateLocalGit(root, contract, runGit = null) {
  const errors = [];
  let origin;
  let branchRefExists;
  if (runGit) {
    origin = runGit(root, ["remote", "get-url", "origin"]);
    try {
      runGit(root, ["show-ref", "--verify", `refs/heads/${contract.repository.defaultBranch}`]);
      branchRefExists = true;
    } catch {
      branchRefExists = false;
    }
  } else {
    ({ origin, branchRefExists } = readLocalGitState(root, contract));
  }
  if (normalizeOrigin(origin) !== normalizeOrigin(contract.repository.originHttps)) {
    errors.push(`origin ${origin} does not match ${contract.repository.originHttps}`);
  }
  const defaultBranchRef = `refs/heads/${contract.repository.defaultBranch}`;
  if (!branchRefExists) {
    errors.push(`local default branch ref is missing: ${defaultBranchRef}`);
  }
  return errors;
}

export function validateReleaseCommitTrace(root, contract, tag, runGit = defaultRunGit) {
  const branchRefs = [
    `refs/remotes/origin/${contract.repository.defaultBranch}`,
    `refs/heads/${contract.repository.defaultBranch}`,
  ];
  for (const branchRef of branchRefs) {
    try {
      runGit(root, ["merge-base", "--is-ancestor", `${tag}^{commit}`, branchRef]);
      return [];
    } catch {
      // Try the next explicit default-branch ref.
    }
  }
  return [`release tag ${tag} is not traceable to ${branchRefs.join(" or ")}`];
}

function defaultRunGit(root, arguments_) {
  return execFileSync("git", arguments_, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

function parseArguments(arguments_) {
  const result = { checkGit: false, tag: null, currentStableTag: null };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--check-git") result.checkGit = true;
    else if (argument === "--tag") result.tag = arguments_[index += 1] ?? null;
    else if (argument === "--current-stable-tag") result.currentStableTag = arguments_[index += 1] ?? null;
    else throw new Error(`unknown argument: ${argument}`);
  }
  return result;
}

export function checkRepository(root, options = {}) {
  const contract = readJson(root, "release/release-contract.json");
  const tag = options.tag
    ?? (process.env.GITHUB_REF_TYPE === "tag" ? process.env.GITHUB_REF_NAME : null);
  const state = validateReleaseState({
    contract,
    packageJson: readJson(root, "package.json"),
    cargoToml: fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8"),
    tauriConfig: readJson(root, "src-tauri/tauri.conf.json"),
    tag,
    repository: process.env.GITHUB_REPOSITORY ?? null,
  });
  const errors = [
    ...state.errors,
    ...validateWorkflowPermissions({
      ciWorkflow: fs.readFileSync(path.join(root, ".github/workflows/ci.yml"), "utf8"),
      capabilityWorkflow: fs.readFileSync(path.join(root, ".github/workflows/capability-release.yml"), "utf8"),
    }),
    ...validateDesktopReleaseWorkflow({
      desktopWorkflow: fs.readFileSync(path.join(root, ".github/workflows/desktop-release.yml"), "utf8"),
      capabilityWorkflow: fs.readFileSync(path.join(root, ".github/workflows/capability-release.yml"), "utf8"),
    }),
  ];
  if (options.checkGit) errors.push(...validateLocalGit(root, contract));
  if (tag) errors.push(...validateReleaseCommitTrace(root, contract, tag));
  if (tag && options.currentStableTag) {
    errors.push(...validateStableReleaseAdvance(tag, options.currentStableTag, contract));
  }
  return { ...state, errors, tag };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`[release-config] ${error.message}\n`);
    process.exit(2);
  }
  const result = checkRepository(repositoryRoot, options);
  if (result.errors.length > 0) {
    for (const error of result.errors) process.stderr.write(`[release-config] ${error}\n`);
    process.exitCode = 1;
  } else {
    process.stdout.write(`[release-config] ${result.version}${result.tag ? ` / ${result.tag}` : ""} matches the frozen release contract\n`);
  }
}

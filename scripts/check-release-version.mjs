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
  const approvalOwnerConfirmed = typeof contract.publishing.approvalOwner === "string"
    && contract.publishing.approvalOwner.trim().length > 0;
  const approvalOwnerPending = contract.publishing.approvalOwner === null
    && contract.publishing.approvalOwnerStatus === "pending-human-input"
    && typeof contract.publishing.approvalOwnerRole === "string"
    && contract.publishing.approvalOwnerRole.trim().length > 0;
  if (contract.publishing.stableEnvironment !== "desktop-release"
    || contract.publishing.capabilityEnvironment !== "capability-release"
    || (!approvalOwnerConfirmed && !approvalOwnerPending)) {
    errors.push("protected release environments and confirmed-or-pending approval ownership must remain explicit");
  }
  for (const signingKind of ["updater", "capability", "windows", "apple"]) {
    const signingContract = contract.signing?.[signingKind];
    const ownerConfirmed = typeof signingContract?.owner === "string"
      && signingContract.owner.trim().length > 0;
    const ownerPending = signingContract?.owner === null
      && signingContract?.status === "pending-human-input"
      && typeof signingContract?.ownerRole === "string"
      && signingContract.ownerRole.trim().length > 0;
    if (!ownerConfirmed && !ownerPending) {
      errors.push(`${signingKind} signing ownership must be confirmed or explicitly pending`);
    }
    if (signingContract && Object.hasOwn(signingContract, "privateKey")) {
      errors.push(`${signingKind} private key material cannot appear in the release contract`);
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
  const publisherMatch = /^ {2}publish-catalog:\s*$([\s\S]*?)(?=^ {2}[A-Za-z0-9_-]+:\s*$|(?![\s\S]))/m.exec(capabilityWorkflow);
  const publisher = publisherMatch?.[1] ?? "";
  const publisherStart = publisherMatch?.index ?? -1;
  const publisherEnd = publisherStart < 0 ? -1 : publisherStart + publisherMatch[0].length;
  const writeGrants = permissionWriteGrants(capabilityWorkflow);
  const publishesRelease = /gh release (?:create|upload)/i.test(capabilityWorkflow);
  if (publishesRelease) {
    if (!/^ {4}permissions:\s*\r?\n\s+contents:\s+write\s*$/m.test(publisher)) {
      errors.push("only the capability publisher job may elevate to contents: write");
    }
    const onlyApprovedWriter = writeGrants.length === 1
      && writeGrants[0].scope === "contents"
      && writeGrants[0].index >= publisherStart
      && writeGrants[0].index < publisherEnd;
    if (!onlyApprovedWriter) {
      errors.push("publishing capability workflow must contain exactly one contents: write grant");
    }
    if (!/^ {4}environment:\s+capability-release\s*$/m.test(publisher)) {
      errors.push("capability publisher must use the protected capability-release environment");
    }
  } else if (writeGrants.length !== 0) {
    errors.push("non-publishing capability workflows cannot request contents: write");
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
  const result = { checkGit: false, tag: null };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--check-git") result.checkGit = true;
    else if (argument === "--tag") result.tag = arguments_[index += 1] ?? null;
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
  ];
  if (options.checkGit) errors.push(...validateLocalGit(root, contract));
  if (tag) errors.push(...validateReleaseCommitTrace(root, contract, tag));
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

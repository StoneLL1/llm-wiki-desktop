# Atomic desktop release runbook

Status: Batch 6 local automated acceptance is complete and the cross-platform CI remediation is merged. Public repository access and authorized GitHub access were restored and reverified on 2026-08-25. The Public beta remains No-Go until the repository owner supplies the production signing identities and custody evidence, commits the capability trust key, configures protected-environment reviewers/secrets/variables and `master` branch protection, identifies a prior signed upgrade baseline, and provides the four real platform hosts required below. See [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md) and [`first-release-candidate-checklist.md`](first-release-candidate-checklist.md).

## Transaction and authority

`.github/workflows/desktop-release.yml` is the only stable publisher. A stable release is one transaction bound to an existing `app-vX.Y.Z` tag, the tag's exact 40-character commit SHA, and one GitHub workflow run ID.

The transaction is ordered as follows:

1. `preflight` checks the canonical repository/tag/commit/version/identifier, the pinned Node and Rust toolchains, lockfiles, required protected inputs, and the complete `npm run check` gate.
2. `capability-build` calls the reusable non-publishing capability workflow and produces 20 signed packs plus the exact catalog/trust/provenance inputs.
3. `desktop-build` builds Windows x64, macOS arm64, macOS x64, and Linux x64 from that same catalog. It verifies catalog embedding and records updater-signature evidence separately from Windows Authenticode or Apple Developer ID/notarization/stapling evidence.
4. `manifest-and-provenance` creates exact-tag `latest.json`, deterministic CycloneDX inventories from `package-lock.json` and `Cargo.lock`, and same-run provenance.
5. `packaged-smoke` installs and launches each packaged target and verifies the candidate fixture manifest without contacting the production endpoint. The real old-version-to-draft update path and the broader upgrade, recovery, security, and user-journey matrix remain Batch 6 acceptance work; Batch 5 evidence must not be described as that later acceptance.
6. `assemble-release` merges smoke evidence, creates GitHub artifact attestations, generates flat checksums for the exact public asset names, cryptographically verifies every updater against the committed Tauri public key, runs the full local release-bundle rehearsal, and uploads the sealed `draft-release-bundle` workflow artifact.
7. `publish-stable` is the only job with `contents: write`. A required reviewer in the `desktop-release` environment approves it. The job reverifies assets and attestations, creates one GitHub draft, uploads the complete bundle, reverse-downloads the draft into a clean directory, verifies names, bytes, checksums, and updater signatures, then enters one publish-through-anonymous-verification shell critical section. `EXIT`, cancellation, and termination first restore draft visibility until every anonymous exact-tag checksum/signature check completes. If GitHub release immutability prevents that edit, rollback deletes the unverified release and the tag is retired; if both actions fail, the job emits a critical incident marker. An unrecoverable runner or GitHub control-plane loss can still defeat process-local rollback, so the release approver must monitor the protected job and apply the incident rollback immediately; this residual risk is not described as an absolute transaction guarantee.

No capability job, matrix build, manifest job, smoke job, or attestation job may create or upload to a GitHub Release. One global stable-channel concurrency group serializes every tag without cancellation. Before work fans out, preflight reads GitHub's current latest release and fails closed unless the candidate stable SemVer is strictly newer; only a real 404 is accepted as the first-release case.

## Required repository configuration

The repository owner must complete all items before the first remote rehearsal:

- make `StoneLL1/llm-wiki-desktop` publicly reachable and confirm `master` as the protected default branch;
- configure `capability-release` and `desktop-release` GitHub Environments with required reviewers; only the default branch may approve a stable tag;
- set repository variables `CAPABILITY_SIGNING_KEY_ID`, `WINDOWS_PUBLISHER_SUBJECT`, and `APPLE_TEAM_ID`;
- place `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX` only in the protected `capability-release` environment;
- place `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`, `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, and `KEYCHAIN_PASSWORD` only in the protected `desktop-release` environment used by preflight, desktop signing, and the final publisher;
- verify primary and backup custody for the updater and capability keys without recording secret material in Git, logs, notes, artifacts, or support evidence;
- update `docs/release/release-notes.md` and `docs/release/known-limitations.md` for the exact version before creating its tag.

The preflight checks only whether protected values are present. It never prints their contents. Production credentials must not be used for pull-request workflows or local fixture rehearsals.

## Local rehearsal

Run from the repository root:

```powershell
npm run test:release-assets
npm run check:release-config
npm run check
```

The fixture rehearsal covers four desktop descriptors, exact tag/commit/run identity, 20 unique capability archives, exact-tag `latest.json`, distinct OS/updater signing evidence, Node and Rust SBOMs, packaged-smoke evidence, provenance, exported attestation evidence, and deterministic flat checksums. Negative cases cover mutable URLs, missing platforms/assets, run drift, incomplete smoke, invalid signing evidence, duplicate public basenames, changed updater bytes, wrong updater keys, and tampering.

A real remote rehearsal must use a disposable stable-format tag at the current candidate commit, keep the GitHub Release as a draft until the protected publisher step, install every workflow artifact, and save the workflow URL plus artifact digests. Do not describe local fixture tests as a successful platform signing, notarization, installation, upgrade, or anonymous-network rehearsal.

## Batch 6 release decision

The 2026-08-21 local acceptance gates are recorded in [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md). The canonical repository and Releases page were not anonymously reachable, the stable `latest.json` endpoint returned 404, local GitHub authorization was invalid, and named production signing/custody/reviewer inputs remain unavailable. Therefore no draft tag or workflow run was created and no platform was marked installed, upgraded, signed, notarized, uninstalled, or anonymously reverse-downloaded.

The 2026-08-25 resumption audit confirmed that the repository is now public, anonymous `HEAD` resolves to `master`, the Releases page returns HTTP 200, and authenticated Actions access works. It also confirmed that `master` is not protected; both required Environments have no reviewer or branch policy; their secret-name lists and the repository variable-name list are empty; the capability trust-key file is empty; and no tag, Release, prior signed upgrade baseline, or four-platform packaged evidence exists. The first-release `latest.json` endpoint therefore still returns the expected pre-release HTTP 404.

This remains a release No-Go, not permission to weaken the workflow. Resume only after the remaining owner, custodian, reviewer, branch-protection, capability-trust, signing-input, signed-baseline, and hardware requirements in [`first-release-candidate-checklist.md`](first-release-candidate-checklist.md) are satisfied without copying secret material into the workspace. Run through the sealed `draft-release-bundle` candidate first; do not approve `publish-stable` or expose the stable updater channel until the full four-platform Batch 6 matrix passes and the user gives a separate explicit approval.

## Approval checklist

The `desktop-release` reviewer confirms all of the following before approving `publish-stable`:

- tag, commit SHA, workflow run, version, repository, and bundle identifier agree;
- 4 desktop installers/updaters and their `.sig` files exist;
- Windows Authenticode publisher subject and Apple Developer ID Team ID match the protected variables;
- both macOS builds pass codesign, Gatekeeper, notarization, and staple validation;
- the catalog has exactly 20 unique entries and all URLs use the same exact tag;
- `latest.json` has exactly the four supported Tauri platform keys and contains no mutable internal URL;
- SBOMs, GitHub attestation, checksums, release notes, known limitations, and packaged-smoke summary are present;
- Batch 6 acceptance evidence is attached when the release is intended for public stable users;
- no active incident, expired certificate, key-custody gap, or production-endpoint anomaly is open.

## Rollback and incidents

### Bad manifest or artifact

Owner: release approver. Immediately mark the bad release as a draft. If release immutability rejects that transition, delete the whole release, permanently retire that tag, and record the incident; never try to reuse the immutable tag name. Stop further approvals, preserve the workflow evidence, and identify whether the manifest, signature, or asset changed. Never point `latest.json` to an older SemVer as a forced downgrade. Publish a higher, fully signed hotfix after the complete transaction passes.

### Updater key loss or compromise

Owner: updater key primary custodian with the independent backup custodian. Stop publication. Attempt recovery of the exact existing key through the documented offline backup. If recovery is impossible, follow the bridge/manual-reinstall constraints in `release-identity-and-access.md`; never disable verification, reuse an OS certificate as the updater key, or publish an unsigned updater.

### Windows or Apple certificate expiry/revocation

Owner: platform-signing custodian. Stop the affected platform matrix. Renew or replace the certificate through the issuer, update the reviewed publisher/Team contract if it changes, run a separate signing-policy review, and repeat notarization/Authenticode plus packaged installation. Do not ship a Tauri updater signature as substitute OS-signing evidence.

### GitHub outage, 403/404/429/5xx, or unavailable anonymous endpoint

Owner: release approver. Keep or return the Release to draft, make no client authentication workaround, and wait for public anonymous reads to recover. Local Wiki, Search, and Edit continue to work. Retry the entire publisher verification after recovery; do not upload a partial release to an alternate unapproved origin.

### Emergency hotfix

Owner: release approver and affected subsystem owner. Create a higher SemVer tag from an approved default-branch commit, update release notes and limitations, and execute the complete workflow. The hotfix may narrow code changes, but it may not skip catalog, signing, SBOM, provenance, checksums, packaged smoke, protected approval, or anonymous post-publish verification.

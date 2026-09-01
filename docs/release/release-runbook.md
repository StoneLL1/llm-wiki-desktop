# Atomic desktop release runbook

Status: Batch 6 local automated acceptance is complete and the cross-platform CI remediation is merged. Public access, `master` protection, both solo-maintainer protected Environments, updater secrets, and the capability signing secret/Key ID are configured. The owner selected the existing updater pair and generated capability key `llm-wiki-capability-v1` on 2026-08-26; PR [#5](https://github.com/StoneLL1/llm-wiki-desktop/pull/5) committed the capability public trust anchor and machine-checked first-release policy, and its merge SHA `41dee7207778222c8b4e44c5cf7da25e87cc6ec9` passed Windows, Ubuntu, and macOS in [Actions run 32933847144](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32933847144). Windows Authenticode and Apple Developer ID/notarization are explicitly not required; updater and capability cryptographic signing remain mandatory. On 2026-09-01 the owner completed the Windows x64 real-machine acceptance of `app-v0.2.0-rc.1` with no blocker, then explicitly approved creating the `app-v0.2.0` tag and publishing the stable channel before the remaining real-machine rows execute; macOS arm64, macOS x64, and Ubuntu 24.04 x64 acceptance is tracked post-publication in [#34](https://github.com/StoneLL1/llm-wiki-desktop/issues/34). See [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md) and [`first-release-candidate-checklist.md`](first-release-candidate-checklist.md).

## Transaction and authority

`.github/workflows/desktop-release.yml` is the only stable publisher. A stable release is one transaction bound to an existing `app-vX.Y.Z` tag, the tag's exact 40-character commit SHA, and one GitHub workflow run ID.

The transaction is ordered as follows:

1. `preflight` checks the canonical repository/tag/commit/version/identifier, the pinned Node and Rust toolchains, lockfiles, required protected inputs, and the complete `npm run check` gate.
2. `capability-build` calls the reusable non-publishing capability workflow and produces exactly one signed pack for every current `published definition × supported target`, plus the exact catalog/trust/provenance inputs. The count is derived from the product manifest (currently 44), never from the historical five-pack matrix.
3. `desktop-build` builds Windows x64, macOS arm64, macOS x64, and Linux x64 from that same catalog. It verifies catalog embedding and every updater signature, and records exact machine-readable evidence that Windows Authenticode and Apple Developer ID/notarization are not required by the release policy.
4. `manifest-and-provenance` creates exact-tag `latest.json`, deterministic CycloneDX inventories from `package-lock.json` and `Cargo.lock`, and same-run provenance.
5. `packaged-smoke` installs and launches each packaged target and verifies the candidate fixture manifest without contacting the production endpoint. For `0.2.0`, the owner-approved one-time bootstrap policy replaces only the nonexistent prior-version upgrade with four-platform clean-install acceptance. From `0.2.1`, the real installed production-version-to-candidate update path is mandatory again. The broader recovery, security, and user-journey matrix remains Batch 6 acceptance work; Batch 5 evidence must not be described as that later acceptance.
6. `assemble-release` merges smoke evidence, creates GitHub artifact attestations, generates flat checksums for the exact public asset names, cryptographically verifies every updater against the committed Tauri public key, runs the full local release-bundle rehearsal, and uploads the sealed `draft-release-bundle` workflow artifact.
7. `publish-stable` is the only job with `contents: write`. A required reviewer in the `desktop-release` environment approves it. The job reverifies assets and attestations, creates one GitHub draft, uploads the complete bundle, reverse-downloads the draft into a clean directory, verifies names, bytes, checksums, and updater signatures, then enters one publish-through-anonymous-verification shell critical section. `EXIT`, cancellation, and termination first restore draft visibility until every anonymous exact-tag checksum/signature check completes. If GitHub release immutability prevents that edit, rollback deletes the unverified release and the tag is retired; if both actions fail, the job emits a critical incident marker. An unrecoverable runner or GitHub control-plane loss can still defeat process-local rollback, so the release approver must monitor the protected job and apply the incident rollback immediately; this residual risk is not described as an absolute transaction guarantee.

No capability job, matrix build, manifest job, smoke job, or attestation job may create or upload to a GitHub Release. One global stable-channel concurrency group serializes every tag without cancellation. Before work fans out, preflight reads GitHub's current latest release and fails closed unless the candidate stable SemVer is strictly newer; only a real 404 is accepted as the first-release case.

## Required repository configuration

The repository owner must retain all of the following before the first remote rehearsal:

- keep `StoneLL1/llm-wiki-desktop` public and retain the required three-platform `master` protection checks with force-push/deletion disabled;
- keep `capability-release` and `desktop-release` restricted to `master` and `app-v*`, with required reviewer `StoneLL1` and sole-maintainer self-review allowed;
- set only the repository variable `CAPABILITY_SIGNING_KEY_ID` after its matching public key is committed;
- place `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX` only in the protected `capability-release` environment;
- place only `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in the protected `desktop-release` environment used by preflight, updater signing, and the final publisher;
- keep `StoneLL1` as the named owner of both cryptographic signing keys; a separate backup custodian is not required, while an encrypted recovery copy remains recommended;
- keep the `0.2.0` upgrade waiver limited to the first stable tag and require four-platform clean-install acceptance instead; require a real prior-production-to-candidate upgrade from `0.2.1` onward;
- update `docs/release/release-notes.md` and `docs/release/known-limitations.md` for the exact version before creating its tag.

The preflight checks only whether the capability key ID and updater protected values are present. It never prints their contents. Capability signing fails closed inside the protected reusable workflow if its private-key secret is missing or mismatched. Production credentials must not be used for pull-request workflows or local fixture rehearsals. Do not configure Windows certificate, Windows publisher, Apple certificate, Apple account, Team ID, notarization, or keychain inputs under the current policy.

## Local rehearsal

Run from the repository root:

```powershell
npm run test:release-assets
npm run check:release-config
npm run check
```

The fixture rehearsal covers four desktop descriptors, exact tag/commit/run identity, the manifest-derived exact capability archive matrix, exact-tag `latest.json`, exact OS-identity policy evidence, mandatory updater signatures, Node and Rust SBOMs, packaged-smoke evidence, provenance, exported attestation evidence, and deterministic flat checksums. Negative cases cover mutable URLs, missing platforms/assets, run drift, incomplete smoke, obsolete or altered OS-certificate evidence, duplicate public basenames, changed updater bytes, wrong updater keys, and tampering.

A real remote rehearsal must use a disposable stable-format tag at the current candidate commit, keep the GitHub Release as a draft until the protected publisher step, install every workflow artifact, and save the workflow URL plus artifact digests. Do not describe local fixture tests as a successful installation, upgrade, or anonymous-network rehearsal, and do not describe GitHub checksums/attestations as Windows or Apple OS identity.

## Batch 6 release decision

The 2026-08-21 local acceptance gates are recorded in [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md). The canonical repository and Releases page were not anonymously reachable, the stable `latest.json` endpoint returned 404, local GitHub authorization was invalid, and named production signing/custody/reviewer inputs remain unavailable. Therefore no draft tag or workflow run was created and no platform was marked installed, upgraded, signed, notarized, uninstalled, or anonymously reverse-downloaded.

The 2026-08-25 resumption audit confirmed that the repository is public, anonymous `HEAD` resolves to `master`, the Releases page returns HTTP 200, and authenticated Actions access works. The follow-up configuration added strict three-platform `master` protection and protected both release Environments with reviewer `StoneLL1`, self-review allowed, and `master`/`app-v*` deployment rules. Secret-name lists and the repository variable-name list remain empty; the capability trust-key file is empty; and no tag, Release, prior signed upgrade baseline, or four-platform packaged evidence exists. The first-release `latest.json` endpoint therefore still returns the expected pre-release HTTP 404.

The first 2026-08-26 audit closed the updater public-identity decision: the owner selected the existing updater key pair, and the supplied public-key document matches both frozen repository values with key ID `0D274EE88AB90656`. It also confirmed `master` merge SHA `82690d5297d404c173b08102e88feab277280132` is three-platform green. A later owner action configured both updater secret names, capability secret `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX`, and repository variable `CAPABILITY_SIGNING_KEY_ID=llm-wiki-capability-v1`; secret values were never read back or written to the workspace.

The owner re-approved the single bootstrap exception for the `0.2.0` line on 2026-08-31 (the retired 2026-08-26 decision named `0.1.0`, which was never published): because no production package exists, only the prior-version upgrade row is waived and is replaced by required clean installs on Windows x64, macOS arm64, macOS x64, and Ubuntu 24.04 x64. The exception expires after `app-v0.2.0`; `0.2.1` and every later stable candidate require a real upgrade from an installed production-signed predecessor. Real-host execution is currently deferred, so this policy decision does not make the candidate a Go.

The public capability trust/acceptance contract is reviewed and merged, and its same-SHA CI is green. This remains a release No-Go, not permission to weaken updater or capability verification: resume only after separate user approval creates the immutable `app-v0.2.0-rc.1` tag, the release workflow seals `draft-release-bundle`, and the deferred four-platform `0.2.0` clean-install matrix in [`first-release-candidate-checklist.md`](first-release-candidate-checklist.md) passes. Do not approve `publish-stable` or expose the stable updater channel until that matrix passes and the user gives a separate explicit approval. From `0.2.1`, also require the real old-to-candidate upgrade matrix.

The 2026-09-01 owner publication decision supersedes the sequencing above for `app-v0.2.0` only: the immutable `app-v0.2.0-rc.1` tag exists, its workflow sealed a candidate whose Windows x64 real-machine acceptance passed with no blocker, and the owner explicitly approved creating `app-v0.2.0` and publishing the stable channel before the macOS arm64, macOS x64, and Ubuntu 24.04 x64 rows execute (recorded in [`first-release-candidate-checklist.md`](first-release-candidate-checklist.md) and [#34](https://github.com/StoneLL1/llm-wiki-desktop/issues/34)). The protected `desktop-release` environment review of `publish-stable` remains the final publication approval. This decision is not permission to weaken updater or capability verification; the pending rows must still be executed and recorded. From `0.2.1`, also require the real old-to-candidate upgrade matrix.

## Approval checklist

The `desktop-release` reviewer confirms all of the following before approving `publish-stable`:

- tag, commit SHA, workflow run, version, repository, and bundle identifier agree;
- 4 desktop installers/updaters and their `.sig` files exist;
- Windows/macOS descriptors contain the exact `not-required` OS-identity policy evidence and no certificate/account requirement has returned;
- release notes and known limitations clearly warn that Windows SmartScreen/unknown-publisher prompts and macOS Gatekeeper manual override may occur;
- the catalog has exactly `published definitions × supported targets` unique entries derived from the product manifest and all URLs use the same exact tag;
- `latest.json` has exactly the four supported Tauri platform keys and contains no mutable internal URL;
- SBOMs, GitHub attestation, checksums, release notes, known limitations, and packaged-smoke summary are present;
- Batch 6 acceptance evidence is attached when the release is intended for public stable users;
- for `0.2.0`, every four-platform clean-install row is complete and the prior-version upgrade cells alone cite the machine-checked one-time waiver; for `0.2.1` and later, every real old-to-candidate upgrade row is complete;
- no active incident, cryptographic-key availability gap, or production-endpoint anomaly is open.

## Rollback and incidents

### Bad manifest or artifact

Owner: release approver. Immediately mark the bad release as a draft. If release immutability rejects that transition, delete the whole release, permanently retire that tag, and record the incident; never try to reuse the immutable tag name. Stop further approvals, preserve the workflow evidence, and identify whether the manifest, signature, or asset changed. Never point `latest.json` to an older SemVer as a forced downgrade. Publish a higher, fully signed hotfix after the complete transaction passes.

### Updater key loss or compromise

Owner: `StoneLL1`. Stop publication. Attempt recovery of the exact existing key from owner-managed secure storage. If recovery is impossible, follow the bridge/manual-reinstall constraints in `release-identity-and-access.md`; never disable verification, reuse an OS certificate as the updater key, or publish an unsigned updater.

### Windows SmartScreen or macOS Gatekeeper blocking

Owner: `StoneLL1`. Treat the warning or block as expected under the explicit no-OS-certificate policy, not as proof that the updater signature failed. Verify the exact-tag checksum, GitHub attestation, and updater signature, then execute and document the platform's deliberate manual-override path on a clean acceptance host. Do not claim that these controls provide Authenticode, Developer ID, notarization, or publisher identity. If the manual path is unusable or unsafe for the target audience, stop that platform and revisit the OS-signing policy through a reviewed contract change.

### GitHub outage, 403/404/429/5xx, or unavailable anonymous endpoint

Owner: release approver. Keep or return the Release to draft, make no client authentication workaround, and wait for public anonymous reads to recover. Local Wiki, Search, and Edit continue to work. Retry the entire publisher verification after recovery; do not upload a partial release to an alternate unapproved origin.

### Emergency hotfix

Owner: release approver and affected subsystem owner. Create a higher SemVer tag from an approved default-branch commit, update release notes and limitations, and execute the complete workflow. The hotfix may narrow code changes, but it may not skip catalog, signing, SBOM, provenance, checksums, packaged smoke, protected approval, or anonymous post-publish verification.

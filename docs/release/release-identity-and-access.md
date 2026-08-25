# Release identity, repository access, and signing ownership

Status: Batch 6 local automated acceptance and cross-platform CI complete; the existing updater key pair is selected and its public half is frozen; repository access, branch protection, and solo-maintainer Environment approvals are configured; Public beta No-Go because capability trust, protected updater/capability signing inputs, signed-baseline, hardware, and remote-rehearsal blockers remain
Last verified: 2026-08-26

## Frozen public release coordinate

| Contract | Frozen value |
| --- | --- |
| Canonical repository | `StoneLL1/llm-wiki-desktop` |
| Visibility | Public |
| Local origin | `https://github.com/StoneLL1/llm-wiki-desktop.git` |
| Default branch | `master` |
| First public version | `0.1.0` |
| Stable tag | `app-vX.Y.Z` |
| Prerelease tag | `app-vX.Y.Z-rc.N` |
| First stable tag | `app-v0.1.0` |
| Stable updater manifest | `https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json` |
| Capability asset base | `https://github.com/StoneLL1/llm-wiki-desktop/releases/download/<exact-tag>/` |

The machine-readable source of truth is [`release/release-contract.json`](../../release/release-contract.json). The local `master` ref is the current default-branch baseline. A release tag is valid only when its commit is an ancestor of `master`; release workflows must use full Git history before evaluating that condition.

## Anonymous access evidence

The following fail-closed, no-credential probe was first executed on 2026-08-16 and rerun on 2026-08-25. It disables terminal prompts and both configured and request-scoped Git credentials, so a cached credential cannot make a private repository look public:

```powershell
$env:GIT_TERMINAL_PROMPT = '0'
$env:GCM_INTERACTIVE = 'never'
git -c credential.helper= -c http.extraHeader= ls-remote --symref https://github.com/StoneLL1/llm-wiki-desktop.git HEAD
```

The 2026-08-16 probe failed without prompting for a username. On 2026-08-25 the same no-credential probe succeeded and returned `refs/heads/master` at `9c2b6a6cef8534d0edb59f254b222c17d6d62711`. On 2026-08-26 it returned the newer merge SHA `82690d5297d404c173b08102e88feab277280132`; an independent unauthenticated `HEAD` request to the Releases page returned HTTP `200`. Public repository access and remote default-branch discovery are therefore closed. The frozen `latest.json` endpoint returned HTTP `404`, which is expected before the first Release exists, while every installer/updater asset probe remains Pending until a sealed draft candidate exists.

For every draft candidate, rerun the command above and also verify from an unsigned, logged-out client:

```powershell
curl.exe --fail --location --head https://github.com/StoneLL1/llm-wiki-desktop/releases
curl.exe --fail --location --head https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json
curl.exe --fail --location --head <each-installer-and-updater-asset-url>
```

The release page must remain reachable without credentials. `latest.json` and installer checks remain Pending until draft assets exist, then become mandatory before stable publication.

## Frozen application identity

| Identity | Value | State |
| --- | --- | --- |
| Product name | `LLM Wiki Desktop` | Frozen |
| Tauri identifier / Apple bundle identifier | `com.llmwiki.desktop` | Frozen |
| Windows publisher subject | Not configured | Authenticode is not required for the initial release; SmartScreen or unknown-publisher warnings are expected |
| Apple Team ID | Not configured | Developer ID signing and notarization are not required for the initial release; Gatekeeper manual override may be required |
| Updater signing public key | minisign key `0D274EE88AB90656` | Existing key pair selected; owner-supplied public bytes match the frozen contract and Tauri trust anchor; matching protected private-key inputs remain required |
| Capability signing public key ID | Not supplied; `capabilities/trusted-keys.json` is empty | Owner `StoneLL1` confirmed; public key and matching protected private key remain Batch 3A blockers |

`StoneLL1` is the confirmed sole maintainer, release approver, updater-key owner, and capability-key owner. Both protected Environments use `StoneLL1` as the required reviewer with self-review allowed because no second maintainer exists. The updater public key was supplied on 2026-08-20. On 2026-08-26 the owner explicitly selected that existing key pair for the first release, and the supplied `.pub` bytes were verified byte-for-byte against both committed copies. Its private half and password were not requested, read, logged, or written to the workspace. Production updater and capability signing material must remain only in their protected GitHub Environment secrets. No private key, password, PAT, certificate, or production secret may be committed to the repository.

The project deliberately does not require a backup custodian, Windows Authenticode identity, or Apple Developer ID/Team identity for the initial release. This accepts single-maintainer continuity risk and visible operating-system trust warnings; checksums and GitHub attestations do not turn an OS-unsigned installer into an OS-identified one. The capability public-key ID and matching protected capability/updater private inputs must still be configured before a release candidate can run. A missing or lost cryptographic signing key stops release; it never permits an unsigned updater, unsigned capability catalog, or signature-verification bypass.

## Updater signing key operations

The committed updater trust anchor is minisign public key ID `0D274EE88AB90656`. Its Base64-encoded public-key document is frozen in `release/release-contract.json` and `src-tauri/tauri.conf.json`; those values must remain byte-for-byte equal.

Only jobs in the protected `desktop-release` GitHub Environment may expose the corresponding private material during the atomic build/sign/publish transaction:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The updater private key must not be reused as a capability-catalog key, OS code-signing key, developer credential, or local project secret. `StoneLL1` owns the key and has explicitly chosen not to require a different backup custodian. An encrypted offline recovery copy remains strongly recommended, but it is not a release gate; loss of the only private key permanently breaks automatic-update continuity for clients that trust the committed public key.

Recovery and rotation are fail-closed. The current static `releases/latest` channel has one trust anchor and no version-aware routing or dual-key verification, so it cannot guarantee a lossless key rotation for clients that miss a bridge release:

1. Stop publication if the protected secret, password, or owner approval is unavailable.
2. Restore the exact existing key only through approved protected-environment secret administration, then produce a signed release candidate and verify an upgrade from the previous signed installer.
3. A planned rotation requires a separately approved migration design before changing this repository's committed key. It must keep an old-key-signed bridge manifest and artifact reachable for older clients while a new channel serves clients that already trust the new key, or add an audited dual-trust/version-aware mechanism. Shipping one bridge build through `releases/latest` and then replacing it is not sufficient.
4. If the project deliberately switches the single static channel after a bridge period, clients that missed the bridge require an explicit manually downloaded reinstall whose checksum, GitHub attestation, and new updater signature are verified. Windows/macOS OS-identity warnings remain expected under the current policy. Record that continuity loss in the release approval; never describe it as transparent rotation.
5. If the old private key is lost before a compatible migration reaches existing clients, in-place updater continuity is lost. Do not publish unsigned artifacts or disable verification; use the manual reinstall and incident process.

Current custody record: existing updater key pair selected; primary owner `StoneLL1`; backup custodian `not-required`; offline restore evidence `recommended-but-not-required`. The matching `desktop-release` Environment secrets remain a release blocker, although their absence does not prevent compiling or testing the Batch 4A backend with the committed public key.

## Workflow permissions and approvals

- Ordinary CI declares `contents: read` and has no publishing permission.
- Capability build/sign jobs inherit `contents: read`.
- The reusable capability workflow has `contents: read`, accepts the unified release tag, and uploads only same-run workflow artifacts. It cannot create or upload to a GitHub Release.
- `.github/workflows/desktop-release.yml` owns the atomic capability + four-platform desktop transaction. Only its final `publish-stable` job receives `contents: write`, and that job is protected by the `desktop-release` environment.
- The sealed candidate remains a workflow artifact through build, manifest, packaged-smoke, attestation, and full asset rehearsal. The protected publisher creates one draft only after those gates, uploads the complete bundle, then publishes and performs anonymous post-publish verification.
- Both `capability-release` and `desktop-release` require reviewer `StoneLL1`, allow sole-maintainer self-review, and allow deployments only from `master` or tags matching `app-v*`.

No remote release workflow rehearsal is claimed for Batch 5 or Batch 6. The 2026-08-25 configuration pass closed public access, `master` protection, required reviewers, and Environment deployment policy. The 2026-08-26 read-only audit confirmed that both Environment secret-name lists and the repository variable list remain empty, and that no tag or Release exists. The capability trust-key set, `CAPABILITY_SIGNING_KEY_ID`, capability private-key secret, updater private-key/password secrets, release tag, Release, and release assets are therefore still absent. These are release blockers, not local test failures. The complete Batch 6 decision and platform matrix are in [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md).

## Local and CI checks

```powershell
npm run check:release-config
npm run check:release-config:local
npm run test:final-four-redlines
npm run check:final-four-redlines
```

The first command validates versions, identity, endpoints, tag grammar, and workflow permission shape. The local variant additionally validates `origin` and the local default-branch ref. The quarantined redline tests are green only when the declared expected red/green state still matches the repository; they contain no skipped tests. The strict final command intentionally exits nonzero while any release blocker remains and is the command later Batch owners must turn green.

## Batch ownership of current redlines

| Contract | Owner |
| --- | --- |
| Structured BackendError presentation | Batch 1 |
| Provider secret-to-origin binding | Batch 2A |
| Mutation write-authority inventory | Batch 2B |
| Complete signed capability catalog | Batch 3A |
| Signed updater foundation | Batch 4A |
| Real global update offer UX | Batch 4B |
| Atomic complete stable release | Batch 5 |

Batch 0 records and guards these failures; it does not implement later Batch production behavior.

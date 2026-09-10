# Release identity, repository access, and signing ownership

> Current process: [release-runbook.md](release-runbook.md). The 2026-09-06 target configuration retains secrets and `master`/`app-v*` restrictions, with no required reviewer; these remote settings were applied after explicit owner approval and verified on 2026-09-06. `app-v0.2.0` is published. Earlier Batch statuses below are historical.

Status: Batch 6 local automated acceptance and cross-platform CI complete; updater and capability signing inputs are configured; the capability public trust anchor and the `0.2.0` one-time bootstrap policy (re-approved 2026-08-31) are merged to `master`; Public beta No-Go pending the sealed `app-v0.2.0-rc.1` candidate and the deferred four-platform clean-install acceptance
Last verified: 2026-08-26

## Frozen public release coordinate

| Contract | Frozen value |
| --- | --- |
| Canonical repository | `StoneLL1/llm-wiki-desktop` |
| Visibility | Public |
| Local origin | `https://github.com/StoneLL1/llm-wiki-desktop.git` |
| Default branch | `master` |
| First public version | `0.2.0` |
| Stable tag | `app-vX.Y.Z` |
| Prerelease tag | `app-vX.Y.Z-rc.N` |
| First stable tag | `app-v0.2.0` |
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
| Capability signing public key ID | `llm-wiki-capability-v1` | Public key prepared in `capabilities/trusted-keys.json`; matching protected secret and repository variable configured; reviewed merge and same-SHA CI remain required |

`StoneLL1` is the confirmed sole maintainer, release approver, updater-key owner, and capability-key owner. Both protected Environments use `StoneLL1` as the required reviewer with self-review allowed because no second maintainer exists. The updater public key was supplied on 2026-08-20. On 2026-08-26 the owner explicitly selected that existing key pair for the first release, and the supplied `.pub` bytes were verified byte-for-byte against both committed copies. The owner then configured the two updater secret names and generated the ring-compatible capability key `llm-wiki-capability-v1`. Its private half was sent directly to the protected `capability-release` Environment and an owner-only DPAPI-encrypted recovery copy was verified outside the workspace; only the raw public key is committed. Secret values were never read back or written to the repository. No private key, password, PAT, certificate, or production secret may be committed.

The project deliberately does not require a backup custodian, Windows Authenticode identity, or Apple Developer ID/Team identity for the initial release. This accepts single-maintainer continuity risk and visible operating-system trust warnings; checksums and GitHub attestations do not turn an OS-unsigned installer into an OS-identified one. The capability and updater protected inputs are now configured, but the public capability trust-anchor commit must still be reviewed, merged, and pass same-SHA CI before a candidate can run. A missing or lost cryptographic signing key stops release; it never permits an unsigned updater, unsigned capability catalog, or signature-verification bypass.

The capability recovery copy is Windows DPAPI-encrypted for the owner account. It is recovery evidence for the current owner machine, not a substitute for a separate offline custodian; the contract continues to record the accepted single-maintainer continuity risk and never records the private path or secret bytes.

## Updater signing key operations

The committed updater trust anchor is minisign public key ID `0D274EE88AB90656`. Its Base64-encoded public-key document is frozen in `release/release-contract.json` and `src-tauri/tauri.conf.json`; those values must remain byte-for-byte equal.

Only jobs in the protected `desktop-release` GitHub Environment may expose the corresponding private material during the atomic build/sign/publish transaction:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The updater private key must not be reused as a capability-catalog key, OS code-signing key, developer credential, or local project secret. `StoneLL1` owns the key and has explicitly chosen not to require a different backup custodian. An encrypted offline recovery copy remains strongly recommended, but it is not a release gate; loss of the only private key permanently breaks automatic-update continuity for clients that trust the committed public key.

Recovery and rotation are fail-closed. The current static `releases/latest` channel has one trust anchor and no version-aware routing or dual-key verification, so it cannot guarantee a lossless key rotation for clients that miss a bridge release:

1. Stop publication if the signing key is unavailable or cannot produce a valid signature; an unencrypted key may use an empty password.
2. Restore the exact existing key only through approved protected-environment secret administration, then produce a signed release candidate and verify an upgrade from the previous signed installer.
3. A planned rotation requires a separately approved migration design before changing this repository's committed key. It must keep an old-key-signed bridge manifest and artifact reachable for older clients while a new channel serves clients that already trust the new key, or add an audited dual-trust/version-aware mechanism. Shipping one bridge build through `releases/latest` and then replacing it is not sufficient.
4. If the project deliberately switches the single static channel after a bridge period, clients that missed the bridge require an explicit manually downloaded reinstall whose checksum, GitHub attestation, and new updater signature are verified. Windows/macOS OS-identity warnings remain expected under the current policy. Record that continuity loss in the release approval; never describe it as transparent rotation.
5. If the old private key is lost before a compatible migration reaches existing clients, in-place updater continuity is lost. Do not publish unsigned artifacts or disable verification; use the manual reinstall and incident process.

Current updater custody record: existing updater key pair selected; primary owner `StoneLL1`; backup custodian `not-required`; offline restore evidence `recommended-but-not-required`. Both matching `desktop-release` Environment secret names were confirmed on 2026-08-26 without reading back their values.

## Workflow permissions and approvals

- Ordinary CI uses read-only repository access for validation. Its narrowly scoped retry job has `actions: write` to retry a hosted-runner shutdown once.
- `.github/workflows/desktop-release.yml` owns the current release process: source-run and catalog preflight, exact-commit full source checks on macOS, four-platform builds with updater signatures and installation/launch smoke checks, separate capability publication, and final desktop publication.
- Engine reuse requires a completed canonical source run for the exact application tag, successful qualification jobs, unchanged engine inputs, and unexpired, provenance-bound artifacts. The source comes from `capability_source_run_id` or the repository variable `CAPABILITY_SOURCE_RUN_ID`; v0.2.1 uses `34439099432`. Changed engine inputs require rebuilding. Long-term restoration from a permanent release channel and unrestricted cross-version reuse are not implemented.
- The protected `publish-capabilities` job uses `capability-release` and `contents: write` to publish already signed, re-verified engine archives to `capabilities-vX.Y.Z`. It does not expose the capability private key. The channel is a prerelease with `latest=false`.
- The final `publish` job uses `desktop-release` and `contents: write`. It waits for source checks, all four desktop builds and smoke checks, and successful capability publication. The desktop release has eight public assets: four installers, two macOS updater archives, `latest.json`, and checksums. The verified non-empty capability catalog is embedded in each official application.
- Updater signing keys are exposed only to the protected desktop build jobs. The updater and capability trust anchors remain separate. Original engine provenance and current integration provenance are retained in Actions artifacts; remote upload names, sizes, and digests are checked before draft publication.
- Release environments retain `master`/`app-v*` deployment restrictions without a required reviewer. The maintainer initiates publication through a tag or dispatch. See the [current runbook](release-runbook.md) for retries and source-artifact expiry handling.

### Historical Batch 5/6 status

No remote release workflow rehearsal is claimed for Batch 5 or Batch 6. The 2026-08-25 configuration pass closed public access, `master` protection, required reviewers, and Environment deployment policy. On 2026-08-26, names-only audits confirmed both updater secrets, the capability private-key secret, and `CAPABILITY_SIGNING_KEY_ID=llm-wiki-capability-v1`; no tag or Release exists. The public capability trust anchor and first-release acceptance contract are prepared on a review branch. Reviewed merge, same-SHA CI, sealed release assets, and the deferred four-platform clean-install matrix remain release blockers, not local test failures. The complete Batch 6 decision and platform matrix are in [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md).

## Local and CI checks

```powershell
npm run check:release-config
npm run check:release-config:local
npm run test:final-four-redlines
npm run check:final-four-redlines
```

The first command validates versions, identity, endpoints, tag grammar, and public signing keys. The local variant additionally validates `origin` and the local default-branch ref. The quarantined redline tests are green only when the declared expected red/green state still matches the repository; they contain no skipped tests. The strict final command intentionally exits nonzero while any release blocker remains and is the command later Batch owners must turn green.

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

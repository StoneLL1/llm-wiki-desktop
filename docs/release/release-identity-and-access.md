# Release identity, repository access, and signing ownership

Status: Batch 6 local automated acceptance complete; Public beta No-Go because external signing, reviewer, public-access, authorized Actions access, and remote-rehearsal blockers remain
Last verified: 2026-08-21

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

The following fail-closed, no-credential probe was executed on 2026-08-16. It disables terminal prompts and both configured and request-scoped Git credentials, so a cached credential cannot make a private repository look public:

```powershell
$env:GIT_TERMINAL_PROMPT = '0'
$env:GCM_INTERACTIVE = 'never'
git -c credential.helper= -c http.extraHeader= ls-remote --symref https://github.com/StoneLL1/llm-wiki-desktop.git HEAD
```

It failed without prompting for a username. Independent unauthenticated `HEAD` requests to the Releases page and the frozen `latest.json` endpoint both returned HTTP `404`. Therefore public repository access, remote default-branch discovery, the Releases page, `latest.json`, and installer assets are release-blocking Pending items. This result must be fixed by making the confirmed repository publicly reachable; it must not be worked around with a client Authorization header, GitHub token, or PAT.

After the repository exists publicly, rerun the command above and record the returned `HEAD` symbolic ref. For every draft candidate, also verify from an unsigned, logged-out client:

```powershell
curl.exe --fail --location --head https://github.com/StoneLL1/llm-wiki-desktop/releases
curl.exe --fail --location --head https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json
curl.exe --fail --location --head <each-installer-and-updater-asset-url>
```

The release page must be reachable without credentials. `latest.json` and installer checks remain Pending until draft assets exist, then become mandatory before stable publication.

## Frozen application identity

| Identity | Value | State |
| --- | --- | --- |
| Product name | `LLM Wiki Desktop` | Frozen |
| Tauri identifier / Apple bundle identifier | `com.llmwiki.desktop` | Frozen |
| Windows publisher subject | Not supplied | Release blocker; human owner input required |
| Apple Team ID | Not supplied | Release blocker; human owner input required |
| Updater signing public key | minisign key `0D274EE88AB90656` | Public key supplied and committed for Batch 4A; owner, backup custodian, and recovery evidence remain release blockers |
| Capability signing public key ID | Not supplied; `capabilities/trusted-keys.json` is empty | Batch 3A blocker; human owner input required |

The intended signing-custodian and approval-owner role is the `StoneLL1` repository owner, but this is an unverified placeholder rather than a named human assignment. The updater public key was supplied on 2026-08-20 and is safe to commit; its private half and password were not requested, read, logged, or written to the workspace. No named backup custodian, protected-environment reviewer configuration, or recovery evidence is currently available. Production signing material must be placed only in protected GitHub Environment secrets with a separately verified offline backup. The updater key, capability key, Windows code-signing identity, and Apple Developer ID/Team identity are distinct contracts. No private key, certificate password, PAT, or production secret may be committed to the repository.

The exact publisher subject, Team ID, capability public-key ID, named updater owner, named backup custodian, and recovery evidence must replace the explicit `pending-human-input` fields before the first public stable release. A missing or lost key stops release; it never permits unsigned artifacts or signature verification bypass.

## Updater signing key operations

The committed updater trust anchor is minisign public key ID `0D274EE88AB90656`. Its Base64-encoded public-key document is frozen in `release/release-contract.json` and `src-tauri/tauri.conf.json`; those values must remain byte-for-byte equal.

Only jobs in the protected `desktop-release` GitHub Environment may expose the corresponding private material during the atomic build/sign/publish transaction:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The updater private key must not be reused as a capability-catalog key, OS code-signing key, developer credential, or local project secret. Before the first public stable release, a named primary owner and a different named backup custodian must each confirm the public key ID, verify that an encrypted offline backup can be recovered, and record the evidence location without recording secret material in Git.

Recovery and rotation are fail-closed. The current static `releases/latest` channel has one trust anchor and no version-aware routing or dual-key verification, so it cannot guarantee a lossless key rotation for clients that miss a bridge release:

1. Stop publication if the protected secret, password, owner approval, or offline recovery evidence is unavailable.
2. Restore the exact existing key only through approved protected-environment secret administration, then produce a signed release candidate and verify an upgrade from the previous signed installer.
3. A planned rotation requires a separately approved migration design before changing this repository's committed key. It must keep an old-key-signed bridge manifest and artifact reachable for older clients while a new channel serves clients that already trust the new key, or add an audited dual-trust/version-aware mechanism. Shipping one bridge build through `releases/latest` and then replacing it is not sufficient.
4. If the project deliberately switches the single static channel after a bridge period, clients that missed the bridge require an explicit manually downloaded, OS-signed reinstall. Record that continuity loss in the release approval; never describe it as transparent rotation.
5. If the old private key is lost before a compatible migration reaches existing clients, in-place updater continuity is lost. Do not publish unsigned artifacts or disable verification; use the manual reinstall and incident process.

Current custody record: primary owner `pending-human-input`; backup custodian `pending-human-input`; offline restore evidence `pending-human-input`. These are release blockers, although they do not prevent compiling or testing the Batch 4A backend with the public key.

## Workflow permissions and approvals

- Ordinary CI declares `contents: read` and has no publishing permission.
- Capability build/sign jobs inherit `contents: read`.
- The reusable capability workflow has `contents: read`, accepts the unified release tag, and uploads only same-run workflow artifacts. It cannot create or upload to a GitHub Release.
- `.github/workflows/desktop-release.yml` owns the atomic capability + four-platform desktop transaction. Only its final `publish-stable` job receives `contents: write`, and that job is protected by the `desktop-release` environment.
- The sealed candidate remains a workflow artifact through build, manifest, packaged-smoke, attestation, and full asset rehearsal. The protected publisher creates one draft only after those gates, uploads the complete bundle, then publishes and performs anonymous post-publish verification.
- The repository owner must configure required reviewers for both `capability-release` and `desktop-release`. Remote configuration remains unverified because anonymous repository access fails.

No remote workflow rehearsal is claimed for Batch 5 or Batch 6: the 2026-08-21 no-credential rerun still could not read the repository, the Releases page and stable `latest.json` returned 404, local GitHub authorization was invalid, and no draft tag or release asset exists. This is a release blocker, not a local test failure. The complete Batch 6 decision and platform matrix are in [`batch-6-acceptance-evidence.md`](batch-6-acceptance-evidence.md).

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

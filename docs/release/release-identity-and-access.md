# Release identity, repository access, and signing ownership

Status: Batch 0 frozen contract with external blockers recorded
Last verified: 2026-08-16

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
| Updater signing public key | Not supplied | Batch 4A blocker; human owner input required |
| Capability signing public key ID | Not supplied; `capabilities/trusted-keys.json` is empty | Batch 3A blocker; human owner input required |

The intended signing-custodian and approval-owner role is the `StoneLL1` repository owner, but this is an unverified placeholder rather than a named human assignment. No backup custodian, protected-environment reviewer configuration, or recovery evidence is currently available. Production signing material must be placed only in protected GitHub Environment secrets with a separately verified offline backup. The updater key, capability key, Windows code-signing identity, and Apple Developer ID/Team identity are distinct contracts. No private key, certificate password, PAT, or production secret may be committed to the repository.

The exact publisher subject, Team ID, updater public key, capability public-key ID, named backup custodian, and recovery evidence must replace the explicit `pending-human-input` fields before the first public stable release. A missing or lost key stops release; it never permits unsigned artifacts or signature verification bypass.

## Workflow permissions and approvals

- Ordinary CI declares `contents: read` and has no publishing permission.
- Capability build/sign jobs inherit `contents: read`.
- Only the final `publish-catalog` job currently elevates to `contents: write`, and it remains behind the `capability-release` environment.
- Manual capability publication now accepts only the frozen application tag grammar at the configured version, requires that the existing tag resolve to the selected default-branch commit, derives the canonical exact-tag asset base internally, and may upload only to a draft release.
- Batch 5 must replace independent capability publication with one atomic desktop publisher; only that final publisher may receive `contents: write`.
- The repository owner must configure required reviewers for `capability-release` and the future `desktop-release` environment. Remote configuration is currently unverified because anonymous repository access fails.

No workflow rehearsal is claimed for Batch 0: the canonical repository is not anonymously reachable, and no draft tag or release asset exists. This is a release blocker, not a local test failure.

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

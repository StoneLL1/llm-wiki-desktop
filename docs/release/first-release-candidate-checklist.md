# First updater/capability-signed draft and packaged acceptance checklist

Status: **Blocked pending a separately approved tag, sealed candidate, deferred four-platform clean-install acceptance, and separate final publication approval**

Prepared: 2026-08-26

Audited predecessor master (before this contract update): `82690d5297d404c173b08102e88feab277280132`

Reviewed signing-contract merge: `41dee7207778222c8b4e44c5cf7da25e87cc6ec9` — three-platform [same-SHA CI passed](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32933847144)

Historical intended version/tag as recorded on 2026-08-26: `0.1.0` / `app-v0.1.0` — **recorded then as not created; superseded by the correction below**

## 2026-08-30 Batch 9 preflight correction

Public read-only inspection now shows that `app-v0.1.0` exists and dereferences to `43cf323572f9e43cd59be93dfec8053fba6b3d8d`; its release workflow failed preflight, so this immutable tag must not be moved or reused. Local Batch 8 baseline `6dbd92c85ca5a670b5e8e4f1724813fbbb275b8b` is not public `master` (`df0e709ffb1a2571db4d96c459fd053a511ba24e`) and has no same-SHA hosted CI.

The current decision remains **No-Go**. The release owner must first reconcile the candidate onto the protected branch, obtain same-SHA CI, choose a reviewed new version/tag, re-audit protected input names with valid authentication, and separately authorize creation of that immutable tag. No sealed bundle or four-platform row has been accepted. See [Batch 9 preflight evidence](2026-08-30-batch-9-preflight-evidence.md).

The remaining `0.1.0`-specific prerequisites, waiver text, workflow steps, and matrix below are retained as historical evidence and must not be executed as a current checklist. Before Batch 9 resumes, rebaseline this checklist to the newly approved coordinate and explicitly decide its prior-version upgrade obligation; no `0.1.0` bootstrap waiver is inherited automatically.

That rebaseline is a machine-contract change, not a documentation-only tag rename: update `release/release-contract.json`, package/Tauri versions, SPEC, runbook, release notes, known limitations, and contract tests together; review them, run the full local gate from a clean exact commit, and require same-SHA hosted CI. Under the current workflow, anonymous public downloads occur only inside the separately approved `publish-stable` job after publication. Therefore the current operational sequence must be: sealed-bundle platform evidence → release-owner pre-publication Go/No-Go → separate publisher approval → atomic publish plus anonymous reverse-download verification/rollback → post-publication result record.

This checklist contains no secret value, private key, certificate, password, token, private signed URL, user path, or user content. A checked source/config item is not evidence that a real package was updater-signed, installed, upgraded, or anonymously downloaded; Windows Authenticode and Apple Developer ID/notarization are intentionally outside the initial policy.

## 1. Release-owner prerequisites

Complete these outside the workspace before requesting a remote candidate run. Record only the responsible person and evidence location, never secret material.

| Requirement | Current state | Completion evidence |
| --- | --- | --- |
| Public canonical repository; default branch `master` | Complete; anonymous `HEAD` resolves to candidate master | 2026-08-25 no-credential probe |
| Protected `master` with required CI and no force-push/deletion | Complete; strict Windows/macOS/Ubuntu checks, admin enforcement, PR gate, conversation resolution, and no force-push/deletion | 2026-08-25 authenticated rules audit |
| `capability-release` Environment, default-branch/tag policy, required reviewer | Complete; reviewer `StoneLL1`, self-review allowed, `master` and `app-v*` only | 2026-08-25 authenticated Environment audit |
| `desktop-release` Environment, default-branch/tag policy, required reviewer | Complete; reviewer `StoneLL1`, self-review allowed, `master` and `app-v*` only | 2026-08-25 authenticated Environment audit |
| Repository variable `CAPABILITY_SIGNING_KEY_ID` | Complete; `llm-wiki-capability-v1` configured on 2026-08-26 | Names-only/value-of-public-ID audit |
| `capability-release` secret name `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX` | Complete; protected secret name visible on 2026-08-26 | Names-only audit; value was not read back |
| `desktop-release` secret names `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Complete; both protected secret names visible on 2026-08-26 | Names-only audit; values were not read back |
| Existing updater key pair selected and public trust anchor committed | Complete; owner reconfirmed the existing pair on 2026-08-26 and the supplied `.pub` bytes match `release/release-contract.json` and `src-tauri/tauri.conf.json` | Reviewed contract and same-SHA CI; no private material in the workspace |
| Capability public key ID and public key committed to `capabilities/trusted-keys.json` | Complete on `master`: `llm-wiki-capability-v1`; reviewed merge and same-SHA CI passed | PR #5 merge `41dee7207778222c8b4e44c5cf7da25e87cc6ec9`; Actions run 32933847144 |
| Public identity fields in `release/release-contract.json` | Complete on `master`: updater and capability identities confirmed; backup custodian not required; owner DPAPI-encrypted capability recovery copy confirmed; Windows/Apple OS vendor identity not required | PR #5 merge `41dee7207778222c8b4e44c5cf7da25e87cc6ec9`; Actions run 32933847144 |
| Updater/capability key ownership and continuity policy | Complete decision: owner `StoneLL1`, no backup custodian required; encrypted offline recovery remains recommended | Reviewed contract; never record secret material |
| Windows Authenticode and Apple Developer ID/notarization | Explicitly not required for the initial release; user-facing OS warnings must remain documented | Reviewed contract and exact workflow policy evidence |
| Prior production-key-signed version for the real old-to-candidate upgrade | Not required for `0.1.0` only under the owner-approved one-time bootstrap waiver; mandatory from `0.1.1` | Machine-checked release contract and approval record |
| Real acceptance hosts | **Pending and intentionally deferred for now:** Windows x64, macOS arm64, macOS x64, Ubuntu 24.04 x64; `0.1.0` still requires clean-install acceptance before publication | Host owner and sanitized OS/build record |

Because `capabilities/trusted-keys.json` and the capability key ID in `release/release-contract.json` are public, reviewable values, they passed the required full gate, protected PR merge, and same-SHA three-platform CI before becoming release authority. Adding the protected secret alone was not sufficient. The private capability key must continue to match the committed public key and remain only in the protected Environment; the owner recovery copy remains encrypted outside the workspace.

The `0.1.0` waiver is narrow: it removes only the impossible prior-version upgrade row when no production release exists. It does not waive updater signatures, capability signatures, packaged smoke, clean installation, launch/restart, uninstall/project preservation, OS-warning evidence, or final protected approval. Starting with `0.1.1`, a real installed production version must upgrade to the candidate on all four targets.

## 2. Candidate workflow boundary

After every prerequisite above is reviewed:

1. Reverify `docs/release/release-notes.md` and `docs/release/known-limitations.md` for the exact candidate version.
2. Run `npm run check:release-config`, `npm run check:release-config:local`, `npm run test:final-four-redlines`, `npm run check:final-four-redlines`, and a from-beginning `npm run check`.
3. Require same-SHA Windows, macOS, and Ubuntu CI success for the commit containing the public signing identities/trust key.
4. Obtain separate explicit user approval before creating or pushing the newly reviewed immutable candidate tag. `app-v0.1.0` is already occupied; do not reuse or move it.
5. Bind the workflow run, tag, and exact 40-character commit SHA in the evidence record.
6. Allow the workflow to build and seal `draft-release-bundle`; do not approve `publish-stable`.
7. Complete the four-platform `0.1.0` clean-install matrix below from that sealed bundle. The user has deferred this execution; no row may be marked complete until it is actually run.
8. Stop before `publish-stable` and obtain a separate final publication approval only after the matrix is complete.

The workflow artifact named `draft-release-bundle` is the safe pre-publication candidate. The GitHub Release draft is created inside `publish-stable` immediately before reverse-download verification and stable publication; therefore approving `publish-stable` merely to obtain a GitHub Draft is prohibited without the user's separate final stable-release approval.

## 3. Sealed candidate contract

Record the workflow artifact name, byte size, and SHA-256 for every item. All URLs and provenance must bind to one tag, one commit, and one workflow run.

- Exactly one signed capability archive for every `published definition × supported target` pair derived from `capabilities/product-manifest.json` (44 entries for the current 11 × 4 manifest; never use 44 as a permanent hard-coded gate).
- Signed capability catalog and committed trust-key set.
- Windows x64 canonical NSIS installer/updater and updater `.sig`; record `windows-authenticode-not-required` policy evidence.
- macOS arm64 DMG/app updater archive and `.sig`; record `apple-developer-id-not-required` policy evidence.
- macOS x64 DMG/app updater archive and `.sig`; record independent `apple-developer-id-not-required` policy evidence.
- Linux x64 AppImage updater and `.sig`; record deb/rpm separately if published.
- Four-platform `latest.json`, flat checksums, Node/Rust SBOMs, provenance, GitHub attestation export, release notes, known limitations, and packaged-smoke summary.
- Exact OS-identity policy evidence for all four descriptors. Updater signatures remain mandatory and do not substitute for Windows/Apple publisher identity; checksums and attestations likewise must not be described as OS identity.

## 4. Real platform matrix

Each row must use a clean real host or VM with the named architecture. Attach sanitized evidence and both pre/post project-tree SHA-256 inventories. Do not record a private user path or project content.

| Target | Host / OS | `0.1.0` candidate artifact + SHA-256 | Clean install + launch + restart | Prior-version upgrade | Uninstall | Project bytes unchanged | Cryptographic/policy evidence | Result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Windows x64 | Pending | Pending | Pending | Waived for `0.1.0` only | Pending | Pending | Updater signature plus SmartScreen/unknown-publisher manual path, standard-user, locked-file/AV evidence Pending | **Pending** |
| macOS arm64 | Pending | Pending | Pending | Waived for `0.1.0` only | Pending | Pending | Updater signature plus Gatekeeper manual-override path; no Developer ID/notarization claim | **Pending** |
| macOS x64 | Pending | Pending | Pending | Waived for `0.1.0` only | Pending | Pending | Independent artifact/signature/manifest entry plus Gatekeeper manual-override path | **Pending** |
| Linux x64 | Pending | Pending | Pending | Waived for `0.1.0` only | Pending | Pending | AppImage executable/desktop integration; deb/rpm if shipped Pending | **Pending** |

For every target, execute the complete core journey in [`2026-08-16-pre-release-final-four-remediation-plan.md`](../superpowers/plans/2026-08-16-pre-release-final-four-remediation-plan.md) §19.3, the update-failure matrix in §19.4, and the security matrix in §19.5. At minimum this includes CJK/space/long-path projects; native/compatible/read-only/restricted/untrusted/recovery modes; capability interruption/resume and original Import continuation; provider-origin and revoke barriers; install blockers; network/manifest/signature failures; malicious Git/Agent and junction/symlink races; local-first operation during network failure; and post-uninstall byte preservation.

## 5. Go / No-Go record

The `0.1.0` candidate remains No-Go until all clean-install rows and cross-platform matrices are complete, anonymous asset/checksum/signature reads succeed, and there is no open updater/capability signing, incident, or known-security blocker. The prior-version upgrade cells alone are waived for `0.1.0`; they become mandatory for `0.1.1` and every later stable release. OS vendor identity remains intentionally absent and must be disclosed rather than treated as a blocker. Even then, stop and ask the user separately whether to approve `publish-stable`. Do not modify `latest`, publish the Release, or approve the protected publisher on the strength of this checklist alone.

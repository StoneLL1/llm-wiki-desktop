# First updater/capability-signed draft and packaged acceptance checklist

Status: **Blocked before remote candidate execution**

Prepared: 2026-08-26

Audited predecessor master (before this contract update): `82690d5297d404c173b08102e88feab277280132`

Intended version/tag: `0.1.0` / `app-v0.1.0` — **tag not created**

This checklist contains no secret value, private key, certificate, password, token, private signed URL, user path, or user content. A checked source/config item is not evidence that a real package was updater-signed, installed, upgraded, or anonymously downloaded; Windows Authenticode and Apple Developer ID/notarization are intentionally outside the initial policy.

## 1. Release-owner prerequisites

Complete these outside the workspace before requesting a remote candidate run. Record only the responsible person and evidence location, never secret material.

| Requirement | Current state | Completion evidence |
| --- | --- | --- |
| Public canonical repository; default branch `master` | Complete; anonymous `HEAD` resolves to candidate master | 2026-08-25 no-credential probe |
| Protected `master` with required CI and no force-push/deletion | Complete; strict Windows/macOS/Ubuntu checks, admin enforcement, PR gate, conversation resolution, and no force-push/deletion | 2026-08-25 authenticated rules audit |
| `capability-release` Environment, default-branch/tag policy, required reviewer | Complete; reviewer `StoneLL1`, self-review allowed, `master` and `app-v*` only | 2026-08-25 authenticated Environment audit |
| `desktop-release` Environment, default-branch/tag policy, required reviewer | Complete; reviewer `StoneLL1`, self-review allowed, `master` and `app-v*` only | 2026-08-25 authenticated Environment audit |
| Repository variable `CAPABILITY_SIGNING_KEY_ID` | **Pending; no repository variables exist** | Names-only audit |
| `capability-release` secret name `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX` | **Pending; secret-name list is empty** | Names-only audit |
| `desktop-release` secret names `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | **Pending; secret-name list is empty** | Names-only audit |
| Existing updater key pair selected and public trust anchor committed | Complete; owner reconfirmed the existing pair on 2026-08-26 and the supplied `.pub` bytes match `release/release-contract.json` and `src-tauri/tauri.conf.json` | Reviewed contract and same-SHA CI; no private material in the workspace |
| Capability public key ID and public key committed to `capabilities/trusted-keys.json` | **Pending; file is empty** | Reviewed commit and same-SHA CI |
| Public identity fields in `release/release-contract.json` | Owner decisions complete: `StoneLL1`; existing updater pair confirmed; backup custodian not required; Windows/Apple OS vendor identity not required. Capability key ID remains Pending | Reviewed commit and same-SHA CI |
| Updater/capability key ownership and continuity policy | Complete decision: owner `StoneLL1`, no backup custodian required; encrypted offline recovery remains recommended | Reviewed contract; never record secret material |
| Windows Authenticode and Apple Developer ID/notarization | Explicitly not required for the initial release; user-facing OS warnings must remain documented | Reviewed contract and exact workflow policy evidence |
| Prior production-key-signed version for the real old-to-candidate upgrade | **Pending; no prior signed package exists** | Exact version, artifact digest, signing evidence, installation source |
| Real acceptance hosts | **Pending:** Windows x64, macOS arm64, macOS x64, Ubuntu 24.04 x64 | Host owner and sanitized OS/build record |

Because `capabilities/trusted-keys.json` and the capability key ID in `release/release-contract.json` require public, reviewable values, completing them is a normal commit that must pass a fresh full gate and same-SHA three-platform CI. It is not accomplished by adding a secret alone. The private capability key must match the committed public key and remain only in the protected Environment.

## 2. Candidate workflow boundary

After every prerequisite above is reviewed:

1. Reverify `docs/release/release-notes.md` and `docs/release/known-limitations.md` for the exact candidate version.
2. Run `npm run check:release-config`, `npm run check:release-config:local`, `npm run test:final-four-redlines`, `npm run check:final-four-redlines`, and a from-beginning `npm run check`.
3. Require same-SHA Windows, macOS, and Ubuntu CI success for the commit containing the public signing identities/trust key.
4. Obtain separate explicit user approval before creating or pushing `app-v0.1.0`. Do not reuse or move a release tag.
5. Bind the workflow run, tag, and exact 40-character commit SHA in the evidence record.
6. Allow the workflow to build and seal `draft-release-bundle`; do not approve `publish-stable`.

The workflow artifact named `draft-release-bundle` is the safe pre-publication candidate. The GitHub Release draft is created inside `publish-stable` immediately before reverse-download verification and stable publication; therefore approving `publish-stable` merely to obtain a GitHub Draft is prohibited without the user's separate final stable-release approval.

## 3. Sealed candidate contract

Record the workflow artifact name, byte size, and SHA-256 for every item. All URLs and provenance must bind to one tag, one commit, and one workflow run.

- 20 unique signed capability archives: 5 packs × Windows x64, macOS arm64, macOS x64, Linux x64.
- Signed capability catalog and committed trust-key set.
- Windows x64 canonical NSIS installer/updater and updater `.sig`; record `windows-authenticode-not-required` policy evidence.
- macOS arm64 DMG/app updater archive and `.sig`; record `apple-developer-id-not-required` policy evidence.
- macOS x64 DMG/app updater archive and `.sig`; record independent `apple-developer-id-not-required` policy evidence.
- Linux x64 AppImage updater and `.sig`; record deb/rpm separately if published.
- Four-platform `latest.json`, flat checksums, Node/Rust SBOMs, provenance, GitHub attestation export, release notes, known limitations, and packaged-smoke summary.
- Exact OS-identity policy evidence for all four descriptors. Updater signatures remain mandatory and do not substitute for Windows/Apple publisher identity; checksums and attestations likewise must not be described as OS identity.

## 4. Real platform matrix

Each row must use a clean real host or VM with the named architecture. Attach sanitized evidence and both pre/post project-tree SHA-256 inventories. Do not record a private user path or project content.

| Target | Host / OS | Old updater-signed artifact + SHA-256 | Candidate artifact + SHA-256 | Install + launch | Upgrade + restart | Uninstall | Project bytes unchanged | Cryptographic/policy evidence | Result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Windows x64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | Updater signature plus SmartScreen/unknown-publisher manual path, standard-user, locked-file/AV evidence Pending | **Pending** |
| macOS arm64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | Updater signature plus Gatekeeper manual-override path; no Developer ID/notarization claim | **Pending** |
| macOS x64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | Independent artifact/signature/manifest entry plus Gatekeeper manual-override path | **Pending** |
| Linux x64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | AppImage executable/desktop integration; deb/rpm if shipped Pending | **Pending** |

For every target, execute the complete core journey in [`2026-08-16-pre-release-final-four-remediation-plan.md`](../superpowers/plans/2026-08-16-pre-release-final-four-remediation-plan.md) §19.3, the update-failure matrix in §19.4, and the security matrix in §19.5. At minimum this includes CJK/space/long-path projects; native/compatible/read-only/restricted/untrusted/recovery modes; capability interruption/resume and original Import continuation; provider-origin and revoke barriers; install blockers; network/manifest/signature failures; malicious Git/Agent and junction/symlink races; local-first operation during network failure; and post-uninstall byte preservation.

## 5. Go / No-Go record

The candidate remains No-Go until all rows and cross-platform matrices are complete, anonymous asset/checksum/signature reads succeed, and there is no open updater/capability signing, incident, or known-security blocker. OS vendor identity remains intentionally absent and must be disclosed rather than treated as a blocker. Even then, stop and ask the user separately whether to approve `publish-stable`. Do not modify `latest`, publish the Release, or approve the protected publisher on the strength of this checklist alone.

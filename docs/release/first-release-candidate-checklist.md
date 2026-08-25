# First signed draft and packaged acceptance checklist

Status: **Blocked before remote candidate execution**

Prepared: 2026-08-25

Candidate master: `9c2b6a6cef8534d0edb59f254b222c17d6d62711`

Intended version/tag: `0.1.0` / `app-v0.1.0` — **tag not created**

This checklist contains no secret value, private key, certificate, password, token, private signed URL, user path, or user content. A checked source/config item is not evidence that a real package was signed, installed, upgraded, notarized, or anonymously downloaded.

## 1. Release-owner prerequisites

Complete these outside the workspace before requesting a remote candidate run. Record only the responsible person and evidence location, never secret material.

| Requirement | Current state | Completion evidence |
| --- | --- | --- |
| Public canonical repository; default branch `master` | Complete; anonymous `HEAD` resolves to candidate master | 2026-08-25 no-credential probe |
| Protected `master` with required CI and no force-push/deletion | **Pending; branch is not protected** | GitHub branch-rules URL or reviewed screenshot |
| `capability-release` Environment, default-branch/tag policy, required reviewer | **Pending; Environment exists with no rules** | Named reviewer and policy record |
| `desktop-release` Environment, default-branch/tag policy, required reviewer | **Pending; Environment exists with no rules** | Named reviewer and policy record |
| Repository variable names `CAPABILITY_SIGNING_KEY_ID`, `WINDOWS_PUBLISHER_SUBJECT`, `APPLE_TEAM_ID` | **Pending; no repository variables exist** | Names-only audit |
| `capability-release` secret name `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX` | **Pending; secret-name list is empty** | Names-only audit |
| `desktop-release` secret names `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`, `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`, `KEYCHAIN_PASSWORD` | **Pending; secret-name list is empty** | Names-only audit |
| Capability public key ID and public key committed to `capabilities/trusted-keys.json` | **Pending; file is empty** | Reviewed commit and same-SHA CI |
| Public identity fields in `release/release-contract.json` | **Pending:** approval owner, updater owner/backup, capability owner/key, Windows publisher, Apple Team ID | Reviewed commit and same-SHA CI |
| Updater primary owner, different backup custodian, encrypted offline restore proof | **Pending** | Names and evidence location only |
| Windows and Apple certificate custodians | **Pending** | Named owners and issuer/expiry review, without certificate bytes/passwords |
| Prior production-key-signed version for the real old-to-candidate upgrade | **Pending; no prior signed package exists** | Exact version, artifact digest, signing evidence, installation source |
| Real acceptance hosts | **Pending:** Windows x64, macOS arm64, macOS x64, Ubuntu 24.04 x64 | Host owner and sanitized OS/build record |

Because `capabilities/trusted-keys.json` and `release/release-contract.json` require public, reviewable values, completing them is a normal commit that must pass a fresh full gate and same-SHA three-platform CI. It is not accomplished by adding a secret alone. The private capability key must match the committed public key and remain only in the protected Environment.

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
- Windows x64 canonical NSIS installer/updater and updater `.sig`.
- macOS arm64 DMG/app updater archive and `.sig`.
- macOS x64 DMG/app updater archive and `.sig`.
- Linux x64 AppImage updater and `.sig`; record deb/rpm separately if published.
- Four-platform `latest.json`, flat checksums, Node/Rust SBOMs, provenance, GitHub attestation export, release notes, known limitations, and packaged-smoke summary.
- Separate Windows Authenticode and Apple Developer ID/notarization/stapling evidence; updater signatures do not substitute for OS signing.

## 4. Real platform matrix

Each row must use a clean real host or VM with the named architecture. Attach sanitized evidence and both pre/post project-tree SHA-256 inventories. Do not record a private user path or project content.

| Target | Host / OS | Old signed artifact + SHA-256 | Candidate artifact + SHA-256 | Install + launch | Upgrade + restart | Uninstall | Project bytes unchanged | Signing evidence | Result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Windows x64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | Authenticode publisher, standard-user, locked-file/AV evidence Pending | **Pending** |
| macOS arm64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | Developer ID, codesign, Gatekeeper, notarization, staple Pending | **Pending** |
| macOS x64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | Independent artifact/signature/manifest entry plus Developer ID checks Pending | **Pending** |
| Linux x64 | Pending | Pending | Pending | Pending | Pending | Pending | Pending | AppImage executable/desktop integration; deb/rpm if shipped Pending | **Pending** |

For every target, execute the complete core journey in [`2026-08-16-pre-release-final-four-remediation-plan.md`](../superpowers/plans/2026-08-16-pre-release-final-four-remediation-plan.md) §19.3, the update-failure matrix in §19.4, and the security matrix in §19.5. At minimum this includes CJK/space/long-path projects; native/compatible/read-only/restricted/untrusted/recovery modes; capability interruption/resume and original Import continuation; provider-origin and revoke barriers; install blockers; network/manifest/signature failures; malicious Git/Agent and junction/symlink races; local-first operation during network failure; and post-uninstall byte preservation.

## 5. Go / No-Go record

The candidate remains No-Go until all rows and cross-platform matrices are complete, anonymous asset/checksum/signature reads succeed, and there is no open signing, custody, incident, or known-security blocker. Even then, stop and ask the user separately whether to approve `publish-stable`. Do not modify `latest`, publish the Release, or approve the protected publisher on the strength of this checklist alone.

# Batch 6 release acceptance evidence

Date: 2026-08-21

Candidate baseline: `5eac9315b32b8fcaad188e19e783716eb969bde5` on local `master`

Release coordinate: `StoneLL1/llm-wiki-desktop`, `0.1.0`
Decision: **Public beta No-Go; Batch 6 local automated acceptance complete, remote and packaged evidence Pending**

This record contains no private key, certificate, token, user project path, user content, or private signed URL. It distinguishes deterministic source-level evidence from real signed package evidence; the former must not be used to claim the latter.

## Local automated evidence

| Gate | Result |
| --- | --- |
| Batch 1–5 focused frontend regression | Passed: 24 files, 242 tests |
| Release config/assets/catalog/embed contracts | Passed: 30/30 |
| Tauri updater signature verifier | Passed: 1/1 |
| Capability tool contracts | Passed: Node 66/66; Python 9/9 |
| Complete frontend suite | Passed: 143/143 files, 1,212/1,212 tests |
| Provider/authority/Git/path/workflow/updater Rust coverage | Passed in the complete no-default-features gate: library 1,163/1,163; updater integration 14/14; every executed binary, integration, and doc-test target green |
| Final-four redline meta-contracts | Passed: 14/14 |
| Initial bundle contract and budget | Passed: 4/4; 602,413 B raw JS, 174,710 B gzip JS, 4 initial JS files |
| Local release coordinate/origin contract | Passed for the configured local origin and `master` ref |
| Strict final-four release gate | **Blocked as designed:** `capability-release-catalog` remains RED because the source fallback catalog and trusted-key set are not production release inputs |
| Complete `npm run check` from the beginning | Passed in 14m 40.6s after the final high-risk fixes |

The focused frontend run found one stale CI assertion left by Batch 5: `src/test/ci-contracts.test.ts` still expected the old `check:release-config` command and therefore did not lock the new release-asset and updater-signature subgates. Batch 6 updated only that contract and reran the full 24-file focused group successfully.

## Anonymous repository and endpoint evidence

The no-credential probes were rerun outside the restricted process sandbox on 2026-08-21:

- `git -c credential.helper= -c http.extraHeader= ls-remote --symref https://github.com/StoneLL1/llm-wiki-desktop.git HEAD` could not read anonymously and was prevented from prompting.
- `HEAD https://github.com/StoneLL1/llm-wiki-desktop/releases` returned HTTP 404.
- `HEAD https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json` returned HTTP 404.
- Local `gh` authentication for `StoneLL1` is invalid, so no authenticated Actions run can be inspected or dispatched from this machine.

Consequently there is no workflow run URL, draft tag, draft release, public `latest.json`, or anonymously downloadable installer to record. A local fixture rehearsal is green but is not a remote workflow rehearsal.

## Packaged and signing matrix

| Target | Required evidence | Batch 6 state |
| --- | --- | --- |
| Windows x64 | Canonical NSIS install, launch, old signed version upgrade, locked-file/AV/standard-user behavior, uninstall, project byte preservation, Authenticode evidence | Pending; no signed draft artifact or Windows publisher identity |
| macOS arm64 | Developer ID, notarization, staple, Gatekeeper, install/launch/upgrade/uninstall, project preservation | Pending; no Apple Team ID/certificate or macOS runner evidence |
| macOS x64 | Independent artifact/signature/manifest entry plus install/launch/upgrade | Pending; no Apple Team ID/certificate or macOS runner evidence |
| Linux x64 | AppImage permissions, desktop launch, old-version upgrade, uninstall/project preservation; deb/rpm smoke if published | Pending; no draft artifact or Linux packaged runner evidence |

The updater public key is committed, but its named primary owner, independent backup custodian, and offline restore evidence remain Pending. Capability signing key identity, Windows publisher subject, Apple Team ID, production protected-environment secrets, and required environment reviewers remain Pending. No temporary production key was generated and no signing verification was bypassed.

## Core journeys, recovery, and security

Source-level and integration tests cover structured errors, provider-origin binding, project write permits, revoke barriers, hardened Git/Agent lifetimes, handle-bound mutations, capability resume/health/Import continuation, signed updater state, cancellation, offer expiry, install guards, and atomic release asset contracts.

The following Batch 6 requirements remain release-blocking because they require real signed artifacts, clean machines, credentials, or OS-specific runners:

- old signed version to new signed version install/upgrade on all four targets;
- CJK/space/long-path, native/compatible/read-only/restricted/untrusted/recovery user journeys in packaged apps;
- OCR/ASR/browser capability interrupted download, recovery, install, health, and exact original Import item continuation on all targets;
- provider authorization, Chat/BYOK revoke, malicious Git/Agent, junction/symlink race, and secret-free support evidence in packaged apps;
- updater offline/DNS/TLS/HTTP/status/oversize/JSON/platform/SemVer/redirect/signature/crash/interrupted-install matrix;
- uninstall preserving every user project byte;
- Windows Authenticode and macOS Developer ID/notarization/staple verification;
- protected draft workflow rehearsal and anonymous reverse-download verification.

Local-first Wiki, Search, and Edit source-level gates remain independent of the unavailable remote updater. No packaged-network-failure claim is made without a packaged candidate.

## Exit criteria decision

| Plan criterion | State |
| --- | --- |
| FINAL-01 known security P1 code and negative evidence | Code-level closed; packaged adversarial evidence Pending |
| FINAL-02 structured user-facing errors | Closed by source and frontend regression evidence |
| FINAL-03 four-target capability install and original Import continuation | **Not closed** |
| FINAL-04 real old-to-new signed upgrade | **Not closed** |
| One tag/commit for every installer/catalog/manifest/signature | Local workflow contract green; no remote artifact set |
| Windows/macOS signing and notarization | **Not closed** |
| Update failure preserves old version and projects | Source contract green; packaged recovery evidence Pending |
| Two final reviews without unresolved P1/P2 | Closed; both final review passes reported P1/P2 clean |
| Full `npm run check` from the beginning | Closed; passed in 14m 40.6s after the final high-risk fixes |
| Draft release rehearsal and anonymous endpoint | **Not closed** |
| Graphify, progress, audits, runbook | Synchronized; final graph refresh produced 15,973 nodes, 46,089 edges, and 724 communities |

The remediation plan must therefore remain not Completed and the first public beta must not be published. The next action is external release-owner work: make the canonical repository publicly reachable, restore authorized GitHub access, configure named custodians/reviewers and production signing identities, then run the protected draft transaction and the complete four-platform packaged matrix without changing the fail-closed contracts.

# Batch 6 release acceptance evidence

Date: 2026-08-21
Last updated: 2026-08-26

Original candidate baseline: `5eac9315b32b8fcaad188e19e783716eb969bde5` on local `master`

Release coordinate: `StoneLL1/llm-wiki-desktop`, `0.1.0`
Original decision: **Public beta No-Go; Batch 6 local automated acceptance complete, remote and packaged evidence Pending**

This record contains no private key, certificate, token, user project path, user content, or private signed URL. It distinguishes deterministic source-level evidence from real signed package evidence; the former must not be used to claim the latter.

## 2026-08-26 updater identity confirmation and master closure

Audited predecessor baseline: `82690d5297d404c173b08102e88feab277280132` on public `master` before this updater-contract change

Decision: **Public beta No-Go; updater public identity and cross-platform source CI are closed, while capability trust, protected signing inputs, signed-baseline, and packaged-host evidence remain blocked**

- PR [#3](https://github.com/StoneLL1/llm-wiki-desktop/pull/3) merged normally as `82690d5297d404c173b08102e88feab277280132`; [Actions run 32870822902](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32870822902) completed successfully for Ubuntu, Windows, and macOS at that exact merge SHA.
- The owner selected the existing updater key pair for the first release. The supplied public-key document matches `release/release-contract.json` and `src-tauri/tauri.conf.json` byte-for-byte and identifies minisign key `0D274EE88AB90656`; no private key or password was requested, read, logged, or written to the workspace.
- The no-credential Git probe resolves `master` to the candidate SHA, the Releases page returns HTTP `200`, and the pre-release stable `latest.json` endpoint returns the expected HTTP `404`.
- Read-only GitHub metadata shows both protected Environment secret-name lists and the repository variable list are empty; no tag or Release exists. The matching updater protected secrets, capability trust/public key and protected secret, signed upgrade baseline, and four real acceptance hosts remain external release blockers.

## 2026-08-25 resumption audit

Current candidate baseline: `9c2b6a6cef8534d0edb59f254b222c17d6d62711` on public `master`

Decision: **Public beta No-Go; cross-platform source CI and local release rehearsal green, updater/capability-signed draft and packaged evidence blocked**

### Merge and CI evidence

- Final remediation push SHA `0783a16a0a4ce828cccbcab175a7de9fabb51186` passed Windows, Ubuntu, and macOS in [Actions run 32828848340](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32828848340).
- PR [#2](https://github.com/StoneLL1/llm-wiki-desktop/pull/2) used the exact head above and passed its merge-ref matrix in [Actions run 32832825787](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32832825787): Windows job `97755090746`, Ubuntu job `97755090991`, and macOS job `97755090998` all succeeded.
- PR #2 was merged normally at `2026-08-25T10:24:26Z` as `9c2b6a6cef8534d0edb59f254b222c17d6d62711`.
- The merge SHA itself passed [Actions run 32837062325](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32837062325): Ubuntu job `97768173201` completed at `10:46:01Z`, Windows job `97768173333` at `10:56:36Z`, and macOS job `97768173009` at `11:07:05Z`.

### Current local and remote release gates

| Gate | 2026-08-25 evidence |
| --- | --- |
| Release config | Passed: 30/30 Node contracts, updater signature verifier 1/1, version and source catalog checks |
| Local release coordinate | Passed in a clean shallow clone of public `master` at the exact merge SHA; the linked worktree exposes a separate `commondir` handling limitation documented in `gotchas.txt` |
| Final-four meta-contracts | Passed: 14/14 |
| Strict final-four gate | **Blocked as designed:** only `capability-release-catalog` remains RED because no committed capability trust key or release-mode 4-target × 5-pack catalog exists |
| Complete `npm run check` | Final from-beginning rerun passed in 12m54.2s: frontend 143/143 files and 1,213/1,213 tests; Rust library 1,180 passed with 4 intentional ignores; every integration and doc-test target green |
| Anonymous repository probe | Passed: `HEAD` resolves to `refs/heads/master` at the merge SHA |
| Anonymous Releases page | Passed: HTTP 200 |
| Stable `latest.json` | HTTP 404, expected before the first Release; remains mandatory after draft assets exist |
| GitHub authorization | Working as `StoneLL1`; repository viewer permission is `ADMIN` and Actions default workflow permission is `read` |
| Protected default branch | Complete on 2026-08-25: strict Windows/macOS/Ubuntu checks, admin enforcement, PR and conversation-resolution gates, force-push/deletion disabled |
| Required Environments | Complete on 2026-08-25: both require `StoneLL1`, allow sole-maintainer self-review, and permit only `master` or `app-v*` deployments |
| Protected secret names | **Pending:** both Environment secret-name lists are empty; no value was requested or read |
| Repository variable names | **Pending:** `CAPABILITY_SIGNING_KEY_ID` is absent; Windows publisher and Apple Team variables are intentionally not required |
| OS vendor identity policy | Owner decision complete: Windows Authenticode and Apple Developer ID/notarization are not required; OS warnings and manual-override evidence remain mandatory acceptance items |
| Tags and Releases | None exist; no stable tag, Draft, stable Release, or latest-channel mutation was created |

The release-owner handoff and ready-to-fill four-platform matrix are in [`first-release-candidate-checklist.md`](first-release-candidate-checklist.md). No release workflow was dispatched because the capability trust contract, protected updater/capability inputs, signed upgrade baseline, and real platform hosts are not ready. Branch protection, Environment approval, sole-maintainer ownership, no-backup-custodian, and no-OS-certificate decisions are now explicit. The `publish-stable` job was not approved or started.

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
| Windows x64 | Canonical NSIS install, launch, old updater-signed version upgrade, locked-file/AV/standard-user behavior, uninstall, project byte preservation, updater signature and SmartScreen/unknown-publisher manual path | Pending; no updater-signed draft artifact or Windows packaged-host evidence |
| macOS arm64 | Updater signature, Gatekeeper manual-override path, install/launch/upgrade/uninstall, project preservation; no Developer ID/notarization claim | Pending; no updater-signed draft artifact or macOS arm64 host evidence |
| macOS x64 | Independent artifact/signature/manifest entry plus Gatekeeper manual-override and install/launch/upgrade | Pending; no updater-signed draft artifact or macOS x64 host evidence |
| Linux x64 | AppImage permissions, desktop launch, old-version upgrade, uninstall/project preservation; deb/rpm smoke if published | Pending; no draft artifact or Linux packaged runner evidence |

The updater public key and owner `StoneLL1` are committed, and a separate backup custodian is explicitly not required. Capability signing key identity and the production protected-environment updater/capability secrets remain Pending. Windows publisher and Apple Team identity are intentionally not configured under the initial no-OS-certificate policy. No temporary production key was generated and no updater/capability signature verification was bypassed.

## Core journeys, recovery, and security

Source-level and integration tests cover structured errors, provider-origin binding, project write permits, revoke barriers, hardened Git/Agent lifetimes, handle-bound mutations, capability resume/health/Import continuation, signed updater state, cancellation, offer expiry, install guards, and atomic release asset contracts.

The following Batch 6 requirements remain release-blocking because they require real signed artifacts, clean machines, credentials, or OS-specific runners:

- old signed version to new signed version install/upgrade on all four targets;
- CJK/space/long-path, native/compatible/read-only/restricted/untrusted/recovery user journeys in packaged apps;
- OCR/ASR/browser capability interrupted download, recovery, install, health, and exact original Import item continuation on all targets;
- provider authorization, Chat/BYOK revoke, malicious Git/Agent, junction/symlink race, and secret-free support evidence in packaged apps;
- updater offline/DNS/TLS/HTTP/status/oversize/JSON/platform/SemVer/redirect/signature/crash/interrupted-install matrix;
- uninstall preserving every user project byte;
- Windows SmartScreen/unknown-publisher and macOS Gatekeeper manual-override acceptance without any false Authenticode, Developer ID, or notarization claim;
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
| Windows/macOS explicit no-OS-identity warning and manual-override evidence | **Not closed** |
| Update failure preserves old version and projects | Source contract green; packaged recovery evidence Pending |
| Two final reviews without unresolved P1/P2 | Closed; both final review passes reported P1/P2 clean |
| Full `npm run check` from the beginning | Closed; passed in 14m 40.6s after the final high-risk fixes |
| Draft release rehearsal and anonymous endpoint | **Not closed** |
| Graphify, progress, audits, runbook | Synchronized; final graph refresh produced 15,973 nodes, 46,089 edges, and 724 communities |

The remediation plan must therefore remain not Completed and the first public beta must not be published. Public access, branch protection, required reviewer, and owner decisions are now configured. The next external release-owner work is to establish the capability trust key, install the matching capability/updater protected secrets, identify an updater-signed upgrade baseline, and provide the four real acceptance hosts; only then may the protected draft transaction and complete packaged matrix run without changing the fail-closed updater/capability contracts.

# Batch 9 sealed-candidate preflight evidence

Date: 2026-08-30

Decision: **No-Go — the observed working-tree source gate is substantially green, but it is not bound to a clean immutable candidate and no authorized, same-SHA sealed candidate or four-platform packaged matrix exists.**

This is a preflight record, not sealed-candidate evidence. It contains no secret value, private key, certificate, password, token, private signed URL, user path, or user content. No tag, GitHub Draft, Release, `latest.json`, or protected publication approval was created or changed during this Batch.

## Exact revisions and public release state

- Local Batch 8 baseline: `6dbd92c85ca5a670b5e8e4f1724813fbbb275b8b` on local `master`. The gate below ran with preserved pre-existing tracked and untracked worktree changes, so it is diagnostic source evidence rather than an exact-SHA candidate attestation.
- Public `origin/master`: `df0e709ffb1a2571db4d96c459fd053a511ba24e`; local history is 31 commits ahead and 6 commits behind that ref. The local Batch 8 SHA therefore has no same-SHA public CI evidence.
- Public CI [run 32988273066](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32988273066) passed for `df0e709ffb1a2571db4d96c459fd053a511ba24e`, not for the local Batch 8 SHA.
- Immutable tag `app-v0.1.0` already dereferences to `43cf323572f9e43cd59be93dfec8053fba6b3d8d`. Its [Atomic desktop release run 32946646850](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32946646850) failed in repository preflight and produced no acceptable Batch 9 bundle. The tag must not be moved or reused.
- Tag `app-v0.1.0-rc.3` dereferences to public `master` SHA `df0e709ffb1a2571db4d96c459fd053a511ba24e`; [Desktop prerelease run 32987962019](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32987962019) remains waiting at the protected release boundary and is not a sealed Batch 9 candidate for the local Batch 8 implementation.
- The only visible published release is the prerelease `0.1.0-rc.2`; it cannot substitute for the exact-commit, manifest-derived Batch 9 artifact set.

## Local working-tree source-gate results

| Command | Result | Evidence |
| --- | --- | --- |
| `npm run check:import-source-media` | Passed | 32 scenarios, 26 contracts, 14 real-fixture categories, 9 forbidden-closure checks |
| `npm run check:import-v2-cutover` | Passed | Complete Import v2 cutover contract passed |
| `npm run test:capability-tools` | Passed outside the workspace sandbox | 81 Node tests, 6 OCR Python tests, 3 legacy-office Python tests; the first sandbox attempt was an environmental `spawnSync node.exe EPERM`, independently removed by the unrestricted rerun |
| `npm run check:release-config` | Passed | 43 Node tests, updater signature verification, 13 definitions, and 44 manifest-derived release entries; source catalog mode remained explicit |
| `npm run check:release-config:local` | Passed | Local package version `0.1.0` matches the frozen release contract; this source identity check does not make the occupied tag reusable |
| `npm run test:final-four-redlines` | Passed | 14/14 redline tests |
| `npm run check:final-four-redlines` | **Failed closed as designed** | 6/7 release redlines green; `capability-release-catalog` remains red because release mode requires the product-manifest-derived exact signed catalog, which exists only after sealing a real candidate |
| `npm run check` | Passed outside the workspace sandbox | From-beginning full gate passed in 12m35.2s; the first sandbox attempt reached Rust integration tests but was blocked by denied access to the default Windows app-data directory, and the unrestricted rerun passed from the beginning |

The strict final-four gate is intentionally not waived. A source/development fallback catalog is not evidence of a signed distributable catalog.

## Packaged acceptance matrix

| Target | Sealed artifacts | Clean install / restart / uninstall | Capability and Import journeys | Fault / scale / fail-closed journeys | Result |
| --- | --- | --- | --- | --- | --- |
| Windows x64 | Not produced for the exact Batch 8 SHA | Not run | Not run | Not run | Pending |
| macOS arm64 | Not produced for the exact Batch 8 SHA | Not run | Not run | Not run | Pending |
| macOS x64 | Not produced for the exact Batch 8 SHA | Not run | Not run | Not run | Pending |
| Ubuntu x64 | Not produced for the exact Batch 8 SHA | Not run | Not run | Not run | Pending |

No platform row is Passed. Artifact names, byte sizes, SHA-256 values, updater signatures, SBOMs, provenance, packaged-smoke results, same-tag URLs, anonymous downloads, project-tree inventories, and 201/1000/1001/10k plus >64 MiB journeys remain unavailable until an authorized sealed candidate exists.

## Blocking conditions before Batch 9 can resume

1. The release owner must approve a replacement first-stable coordinate and its prior-version upgrade obligation because `app-v0.1.0` is already occupied. Rebaseline `release/release-contract.json`, package/Tauri versions, SPEC, runbook, release notes, known limitations, checklist, and contract tests together; the old `0.1.0` bootstrap waiver must not transfer automatically.
2. Reconcile the local Batch 8 history with the protected public branch, create a reviewed final candidate commit, and rerun every local gate from a clean checkout bound to that exact SHA.
3. Obtain Windows/macOS/Ubuntu hosted CI success for the same exact candidate commit.
4. Re-audit protected Environment reviewers and required secret/variable **names only** with valid authenticated access; the local GitHub credential was invalid during this preflight, so older names-only evidence was not promoted to current evidence.
5. Obtain separate explicit approval to create the selected immutable candidate tag. This implementation-plan request is not that approval.
6. Seal one `draft-release-bundle`, keep `publish-stable` unapproved, and complete every four-platform real-host journey plus capability/Import repetition from that exact sealed bundle.
7. Record a release-owner **pre-publication Go/No-Go**. Only a separate final approval may start `publish-stable`; that job must publish and then perform anonymous reverse-download/public-candidate verification as one protected operation, with the documented fail-closed rollback path on verification failure.
8. Record the post-publication anonymous download and public capability/Import verification result separately. It is post-publication evidence, not a condition that can be satisfied before approving the current publisher workflow.

Until all eight conditions are closed, the release and Batch 9 remain **No-Go**.

# Batch 5 runner confinement release evidence

- Evidence date: 2026-08-30
- Starting commit: `80476b77a344c4a735aa70bebb8d21d895343cfc`
- Batch 5 commit: `7216fb57459e85eb8d31fd8752abce15b5a4ee6b`
- Historical gate result: **No-Go**
- Current decision status: **Superseded by ADR 0002; re-entry through Batch 5R**

## Decision revision

This file preserves the evidence for the original Batch 5 stop. It is not the current product gate.

On 2026-08-30 the product owner selected a functionality-first model: packs from the fixed official catalog are trusted application components after target, product-definition, hash, signature, manifest, protocol, and route-set verification. Cross-platform OS-level filesystem, network, and child-process confinement is no longer required before Batch 6 or Release. The current decision is [ADR 0002](../architecture/decisions/0002-trusted-official-capability-pack-execution.md).

Batch 5R is complete: the temporary `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE` stop is removed, official catalog install/resume is restored, and every declared route is probed before one runtime snapshot is published. Release remains **No-Go** because Batch 6 mutation UI, real official assets, four-target packaged journeys, and sealed-candidate acceptance remain open.

## Historical gate outcome

The original Batch 5 correctly stopped at its then-required platform-confinement feasibility gate. At commit `7216fb57459e85eb8d31fd8752abce15b5a4ee6b`, the downloadable native runner could not prove the former least-privilege filesystem, network, and child-process contract across Windows x64, macOS arm64, macOS x64, and Ubuntu x64. Per the plan in force at that time, multi-route activation did not proceed.

The historical platform analysis remains recorded in [ADR 0001](../architecture/decisions/0001-capability-runner-confinement-feasibility-stop.md). ADR 0001 no longer authorizes blocking future implementation.

## Implemented historical stop behavior

| Surface | Behavior at the Batch 5 commit | Batch 5R disposition |
| --- | --- | --- |
| App-global inventory | Signed entries report `installAllowed: false` and `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE`. | Completed: verified official catalog entries are installable; real catalog/target/task reasons remain independent facts. |
| App-global install/resume IPC | Rejects before task creation, download, extraction, probe, or activation. | Completed: verified official packs use the Batch 4 app-global coordinator and archive-identity single-flight. |
| Import continuation install | Rejects before continuation persistence. | Completed: durable continuation and per-item authority revalidation are restored. |
| Legacy Import install path | Uses the same confinement gate. | Completed: the compatibility path uses the enabled installer and the same all-route activation transaction. |
| Import requirement / ASR facts | Catalog availability is not presented as installability. | Completed: presentation derives installability from the real catalog, target, installed health, and task state. |
| Existing healthy runtime | Prior activation remains untouched. | Completed: all routes probe first; registry publication and activation-journal commit form one transaction that restores the previous snapshot on failure. |

## Revised release evidence ledger

| Evidence | Current result |
| --- | --- |
| Historical four-target feasibility analysis | Recorded; informative only, no longer a release blocker |
| Trusted official-pack decision | Accepted in ADR 0002 |
| Temporary mutation stop removed | **Complete — old backend/frontend confinement state and copy removed** |
| Official catalog install/resume path restored | **Complete — coordinator, Import continuation, and compatibility entry points enabled** |
| Browser multi-route all-or-nothing activation | **Complete — exact product route contract, pre-publication probes, one registry transaction** |
| Failure/interruption rollback preserves old healthy snapshot | **Complete for Batch 5R — commit-failure rollback plus prepared/probed/activated startup recovery tests** |
| Four-target real official-pack install/restart/route matrix | **Pending — Batches 8–9** |
| Batch 6 complete mutation UI | **Backend-unblocked; implementation pending in Batch 6** |
| Four-target malicious-runner OS confinement matrix | Removed from first-release acceptance; product must not claim sandboxing |

## Re-entry requirements

Batch 5R passed the functional re-entry gate. Verified official packs install through the app-global coordinator; all declared routes become visible in one atomic snapshot; probe or activation failure preserves the old healthy version and emits rollback receipt metadata; restart recovery is phase-aware and fails closed on malformed activation journals; health probes and execution use disposable/item invocation roots with signed pack-relative runtime arguments resolved before cwd changes; and duplicate install requests join one task. Batch 6 may now implement the full mutation surface without the former OS-confinement dependency.

## Batch 5R verification

- Focused Rust contracts covered official install single-flight, exact route contracts, failed-route pre-publication stop, registry rollback, activation-journal recovery, invocation-root request scoping, and the 39-format subprocess pipeline.
- Focused React contracts covered capability install state and Import OCR/ASR authorization dialogs after removing the historical confinement-only presentation state.
- After the sole dual-review pass, `npm run check` passed again from the beginning on 2026-08-30 in 19m05.3s, including 145 Vitest files / 1,281 tests, 1,247 Rust library tests with four explicit ignores, all Rust integration suites, production build, bundle budget, console scan, release configuration, and capability tooling.

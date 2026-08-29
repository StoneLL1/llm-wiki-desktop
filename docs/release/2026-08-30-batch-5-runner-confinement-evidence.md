# Batch 5 runner confinement release evidence

- Evidence date: 2026-08-30
- Starting commit: `80476b77a344c4a735aa70bebb8d21d895343cfc`
- Release decision: **No-Go**

## Gate outcome

Batch 5 stopped at its required platform-confinement feasibility gate. The current downloadable native runner cannot yet prove least-privilege filesystem, network, and child-process boundaries across Windows x64, macOS arm64, macOS x64, and Ubuntu x64. Per the implementation plan, signature verification was not accepted as a sandbox substitute and multi-route activation work did not proceed.

The accepted decision and platform analysis are recorded in `docs/architecture/decisions/0001-capability-runner-confinement-feasibility-stop.md`.

## Implemented stop behavior

| Surface | Evidence expected after this batch |
| --- | --- |
| App-global inventory | Remains readable without an active project; signed entries report `installAllowed: false` and `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE`. |
| App-global install/resume IPC | Rejects before task creation, download, extraction, probe, or activation. |
| Import continuation install | Rejects before continuation persistence, so a denied request cannot leave a resumable phantom continuation. |
| Legacy Import install path | Uses the same confinement gate before task or installer state is created. |
| Import requirement / ASR profile facts | Catalog availability is not presented as installability while confinement is unavailable. |
| Existing healthy runtime | Its facts and prior activation remain untouched. This stop does not sandbox or disable execution of an already-installed runner; such execution remains outside release evidence. |

## Evidence ledger

| Evidence | Result |
| --- | --- |
| Feasibility ADR covers all four release targets | Recorded; all four are `Not proven` |
| Stable fail-closed backend error | Implemented and unit tested |
| Coordinator and installer zero-write boundary | Catalog-backed coordinator requests create no task file; direct installer requests create no install root |
| Additive app-global presentation contract | Implemented and contract tested |
| Four-target real packaged malicious-runner matrix | **Missing — release blocker** |
| Browser multi-route all-or-nothing activation | **Not implemented — prohibited after feasibility stop** |
| Management install without a route | Existing Batch 4 command remains present but is now read-only blocked |
| Batch 6 mutation UI | **Blocked** |

## Verification record

The final command results and Batch 5 commit are recorded in the corresponding top entry of `progress.txt`. This document intentionally does not mark the four-platform matrix as passed: no source fixture, mock policy test, signed archive, or green unit test is equivalent to real packaged confinement evidence.

## Re-entry requirements

Supersede the ADR with a fixed confinement architecture, implement each production platform adapter or reduce the published platform/product commitment, and collect real packaged evidence for the full malicious matrix. Only then may multi-route atomic activation and Batch 6 mutation controls resume.

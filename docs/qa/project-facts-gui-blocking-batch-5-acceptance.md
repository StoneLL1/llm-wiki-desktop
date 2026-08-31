# Project Facts GUI blocking remediation Batch 5 acceptance

Date: 2026-08-29

Status: Batch 5 and the Windows installed-artifact P0 acceptance are complete. Real macOS/Linux frame and process captures remain an explicit release-before-publication matrix.

## Scope and final review closure

Batch 5 performed the plan-level final dual review, closed every valid finding, reran the complete repository gate from the beginning, rebuilt an exact-commit MSI from a clean Git archive, installed that MSI, reran the full packaged performance matrix, and refreshed Graphify. It did not implement work assigned to a later plan.

The one permitted review round contained a shared-context architecture/security review and a fresh-context blind-spot review. No further reviewer was started after fixes. The fixes made from that review were:

- A first `null -> authority` binding now supersedes any pre-binding request generation instead of attributing its late response to the new authority.
- Project-status and AI-capability demand is fail-closed while the cached entry authority differs from the live project authority, including stale-resource effects.
- Provider status holds the authority transition lock through project configuration and credential-status reads; the concurrency regression proves a trust revocation cannot return while that read is in progress.
- A real blur/hidden foreground-cycle token replaces elapsed-time focus pairing, so one background transition can trigger at most one Git refresh and never an Agent or Provider refresh.
- Packaged IPC evidence comes from an opt-in build-time Project Facts counter. The harness no longer tries to replace Tauri v2's non-writable invoke implementation.
- Source commit/tree, MSI, built executable, and installed executable are bound by a generated provenance document and verified before launch.
- Fixture schema v2 inventories and re-hashes every fixture file and independently verifies the Git tree, tracked-file count, required directories, and fake-Agent controls before launch.

The first post-review packaged run exposed a harness attribution error: one `git_status` IPC intentionally emits one MetadataIo authority span and one ProjectGit repository span. The old assertion counted both spans as separate focus invocations. The final harness uses the explicit IPC counter as the invocation gate and bounds each worker stage separately; the failed evidence was discarded, the correction was committed, the complete gate was rerun, and a new exact-commit MSI was built and measured.

## Artifact and fixture identity

| Field | Recorded value |
| --- | --- |
| Source commit built and measured | `a2b159e41a1fefb62f0bac53d63c1270674caaeb` |
| Source tree | `8af6e54b6a8b653ee7c42e1bc1e51d948506d32d` |
| Package/product/file version | `0.1.0` |
| MSI SHA-256 | `1dedf44501d247ed64d3f6d6b7bb1d90d883d050a5ed5f0d578cf90cfe54339e` |
| Built executable SHA-256 | `cf38c7f8a6747205c13a664a0b150858c49fb3d225d55b185f28ad613a265903` |
| Installed executable SHA-256 | `cf38c7f8a6747205c13a664a0b150858c49fb3d225d55b185f28ad613a265903` |
| Installed executable | `C:\Users\Aletta\AppData\Local\LLM Wiki Desktop\llm-wiki-desktop.exe` |
| Fixture manifest SHA-256 | `80ce6851c33bf28f099abe04cd98eee0221ddfa908ce3e9fb8e1b449b01540df` |
| Fixture aggregate hash | `d1156523bf9303b9d9b82f5380e1c576521ee5bc4c6be1fcc6fe10d3f574f08e` |
| Native fixture | 3 wiki pages, 240 support files, 251 Git-tracked files, tree `6f9dc5508efda511d3d2ee3da1775daeed0daf80` |
| Controls | markerless CJK directory; slow/fail/healthy wrappers for four Agent commands; 5,000 ms sleep with 3,000 ms application timeout |

The source was exported with `git archive` into a new temporary directory. The build used `VITE_PROJECT_FACTS_PERF_OBSERVER=1`, the Rust `performance-observers` feature, and an independent Cargo target. Tauri produced the MSI successfully and then returned non-zero only because this workstation has the public updater key but not the release private key. The exact MSI was installed in standard per-user mode after removing the prior same-version test product. The installed executable size and hash matched the built executable before measurement.

## Environment

- Windows with the Balanced power scheme (`381b4222-f694-41f0-9685-ff5bb260df2e`).
- Displays: 2880×1800 at 120 Hz and 2560×1440 at 319 Hz.
- Healthy NVMe SSD (`HFS001TEM9X174N`).
- Antivirus, real-time protection, and security service enabled; no AV-disabled comparison was run.
- Native window movement was paired with WebView frame-gap/Long Task/input-to-paint collection and per-process sampling.

## Installed acceptance results

| Scenario or gate | Final result |
| --- | --- |
| No-project idle, 60 s | Project Facts IPC `0/0/0`; Project Facts worker operations `0`; frame-gap p95 3.2 ms, max 8.1 ms; no >50 ms long task or >100 ms gap |
| No-project native drag, 30 s | Frame-gap p95 3.2 ms; no >50 ms long task or >100 ms gap |
| Project-ready idle, 60 s | Git/Agent/Provider IPC delta `0/0/0`; worker-operation delta `0`; app/WebView-only process tree with no Git or Agent child process |
| Project-ready native drag, 30 s | 921 successful moves, 0 failed; frame-gap p95 3.2 ms, max 6.3 ms; input-to-paint p95 3.2 ms; no >50 ms long task or >100 ms gap |
| 20 route switches | p95 30.4 ms, max 46.4 ms |
| 10 native focus cycles | Git IPC `6` (MetadataIo stages `6`, ProjectGit stages `6`); Agent IPC `0`; Provider IPC `0` |
| Slow fake Agents | 8 probe processes; maximum probe lifetime 3,000 ms; configured sleep 5,000 ms; no attributable long task |
| Provider credential missing | Completed without error |
| Provider credential normal | Secret status observed and temporary credential cleanup completed |
| Provider credential denied | Stable `SECRET_BACKEND_FAILED`; no >50 ms long task or >100 ms gap |
| Markerless compatible control | Stable `PROJECT_OPEN_REQUIRES_ASSESSMENT` |

The residual command GUI-dispatch self-times exclude worker execution:

| Command | Samples | p95 / max |
| --- | ---: | ---: |
| `open_project` | 2 | 0.1111 / 0.1111 ms |
| `set_active_project` | 6 | 0.0360 / 0.0360 ms |
| `list_exports` | 17 | 0.0358 / 0.0358 ms |

All are below the 4 ms p95 and 16 ms maximum thresholds. The result snapshot contains 52 anonymous blocking-work spans. The raw JSONL contains 53: a slow Agent request already counted in the `after_routes` IPC snapshot completed during deterministic application cleanup and appended its ProcessProbe span after the result snapshot was written. It was not launched by focus; the focus Agent IPC delta remains zero. Project Facts Git work is split into ten MetadataIo and ten ProjectGit spans; Agent work uses ProcessProbe and Provider work uses HeavyIo. Caller and worker thread identifiers differ for the measured work, and no path, credential, or secret value is present in the trace.

The current command inventory is 205 commands: 130 synchronous, 75 asynchronous, 60 Import commands, and 127 remaining blocking-synchronous commands. The Project Facts P0 synchronous count is zero, below the plan ceiling of 130.

Raw evidence:

- [`results/2026-08-29-project-facts-gui-blocking-batch-5-a2b159e4.provenance.json`](results/2026-08-29-project-facts-gui-blocking-batch-5-a2b159e4.provenance.json)
- [`results/2026-08-29-project-facts-gui-blocking-batch-5-a2b159e4.json`](results/2026-08-29-project-facts-gui-blocking-batch-5-a2b159e4.json)
- [`results/2026-08-29-project-facts-gui-blocking-batch-5-a2b159e4.trace.jsonl`](results/2026-08-29-project-facts-gui-blocking-batch-5-a2b159e4.trace.jsonl)

## Verification

- Focused frontend suites passed: six Project Facts/AppShell/hook files, 87 tests.
- Fixture/provenance Node tests passed, as did the nine command-execution contract tests.
- The focused provider trust-transition concurrency test passed.
- `workflow_routes` passed 10/10 and `workflow_update_wiki` passed 10/10 after stale executable-attestation fixtures were corrected to use the current test executable.
- A final from-beginning `npm run check` passed in 20m 23.2s. The frontend lane passed in 2m 48.5s, including 145 Vitest files and 1,273 tests. The Rust lane passed in 20m 23.1s, including 1,227 library tests, all integration binaries, and the formerly failing workflow-route assertions.
- The final exact-commit packaged matrix completed successfully and wrote the three bound evidence artifacts above.
- `graphify update .` completed successfully with 17,482 nodes, 49,785 edges, and 842 communities. It retained 756 fail-closed nodes from 82 still-present files that left the scan corpus, matching the repository's existing conservative update behavior.

## Remaining external matrix

Windows installed-artifact P0 is closed. Real macOS and Linux packaged frame/process captures remain required before publication on those platforms. Their absence is not represented as a pass and does not weaken the Windows result. No broader blocking-sync debt or later-batch refactor was pulled into Batch 5.

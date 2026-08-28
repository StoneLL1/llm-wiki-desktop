# Project Facts GUI blocking remediation Batch 4 acceptance

Date: 2026-08-29

Status: Windows P0 acceptance passed for the exact installed artifact. macOS/Linux real frame/process capture remains an explicit release-matrix item.

## Scope and decision

Batch 4 measured the packaged application first, then applied only residual fixes that met the plan's decision gate. The trace showed that `open_project`, `set_active_project`, and `list_exports` perform disk, Git, directory, or contended-lock work even when their JavaScript round trips are dominated by useful worker work. They were therefore moved behind bounded asynchronous lanes without changing command names or DTOs. No Git hardening check was removed or merged: the installed result already met the frame and focus gates, so the separate Git-process optimization gate stayed closed.

The installed run also exposed two frontend trigger defects within the Batch 4 acceptance surface: delayed `focus`/`visibilitychange` pairs could refresh Git twice, and a focus refresh could piggyback expired Agent/Provider facts. The shell now coalesces a restore pair for 500 ms and asks only for Git on that path. The final focus evidence is exactly 10 Git refreshes and zero Agent/Provider refreshes for 10 focus cycles.

## Artifact and fixture identity

| Field | Recorded value |
| --- | --- |
| Source commit installed and measured | `558e461f0109365e0259036b33c8e51249e92a74` |
| Package/product version | `0.1.0` |
| MSI SHA-256 | `6ba2a418816f4d8517d1b733dc2bcd810b7950d0226010247214b82fcfc826f3` |
| Installed executable | `C:\Users\Aletta\AppData\Local\LLM Wiki Desktop\llm-wiki-desktop.exe` |
| Installed executable SHA-256 | `9ba18e7b95c80f001ee95d0bc33cee2ca0e9d98eda4502f9a68f9a674fbab428` |
| Fixture manifest SHA-256 | `0c8e7295596d8bf78c8ec96e1e90ff78775f297fc56f5af662c74d5951db9669` |
| Fixture content hash | `8a7bda6d4daab3b1a29a8242ae5953a39ad318a541917919fdfe1861ea3e6818` |
| Native fixture | 3 wiki pages, 240 support files, 251 Git-tracked files, tree `6f9dc5508efda511d3d2ee3da1775daeed0daf80` |
| Controls | markerless CJK directory; four fake Agent commands; 5,000 ms sleep with 3,000 ms application timeout |

The exact MSI was built from a clean source snapshot at the recorded commit with the packaged performance observers enabled. Because Windows Installer can retain an older same-version component, the registered `0.1.0` product was removed before the exact MSI was installed; the installed executable hash was then compared with the built executable and matched.

## Environment

- Windows; Balanced power scheme (`381b4222-f694-41f0-9685-ff5bb260df2e`).
- Displays: 2880×1800 at 120 Hz and 2560×1440 at 319 Hz.
- Storage: healthy NVMe SSD (`HFS001TEM9X174N`).
- Antivirus, real-time protection, and security service were enabled. No AV-disabled comparison was run or needed.
- PresentMon was not installed. The equivalent evidence is WebView `requestAnimationFrame`/Long Task collection paired with real native-window movement and per-process CPU/I/O/handle sampling. WPR/WPA availability was recorded, but no ETW trace was required after the equivalent evidence met the gate.

## Installed acceptance results

| Scenario or gate | Result |
| --- | --- |
| No-project idle, 60 s | 0 Project Facts operations; frame-gap p95 3.2 ms, max 4.1 ms; 0 long tasks >50 ms; 0 gaps >100 ms |
| No-project native drag, 30 s | 914 successful moves; frame-gap p95 3.2 ms, max 18.5 ms; 0 long tasks >50 ms; 0 gaps >100 ms |
| Project-ready idle, 60 s | 0 Project Facts operations; app/WebView2-only process tree, with no Git or Agent child process |
| Project-ready native drag, 30 s | 899 successful moves; frame-gap p95 3.2 ms, max 4.3 ms; 0 long tasks >50 ms; 0 gaps >100 ms |
| 20 route switches | p95 25.9 ms, max 42 ms |
| 10 focus cycles | Git +10; Agent +0; Provider +0 |
| Slow fake Agents | 8 probe processes; maximum observed probe lifetime 3,000 ms; configured sleep 5,000 ms; no attributable long task |
| Provider credential missing | Completed without error |
| Provider credential normal | Secret mask observed; cleanup completed |
| Provider credential denied | Stable `SECRET_BACKEND_FAILED`; no frame gap >100 ms and no long task >50 ms |
| Markerless compatible control | Stable `PROJECT_OPEN_REQUIRES_ASSESSMENT`; no Git operation was added by the phase |

The residual command dispatcher self-times, which exclude worker run time, were:

| Command | Samples | p95 / max |
| --- | ---: | ---: |
| `open_project` | 2 | 0.0916 / 0.0916 ms |
| `set_active_project` | 6 | 0.0102 / 0.0102 ms |
| `list_exports` | 17 | 0.1048 / 0.1048 ms |

All are below the 4 ms p95 and 16 ms maximum thresholds. The explicit blocking-work phases contain 50 spans and preserve worker run duration separately from dispatcher self-time. The current execution inventory is 205 commands: 130 synchronous, 75 asynchronous, 60 Import commands, and 127 remaining blocking-synchronous commands. The reviewed ceiling is 127; Project Facts P0 synchronous count is zero.

Raw evidence:

- [`results/2026-08-29-project-facts-gui-blocking-batch-4-558e461f.json`](results/2026-08-29-project-facts-gui-blocking-batch-4-558e461f.json)
- [`results/2026-08-29-project-facts-gui-blocking-batch-4-558e461f.trace.jsonl`](results/2026-08-29-project-facts-gui-blocking-batch-4-558e461f.trace.jsonl)

## Review and verification

The one permitted final dual-review round was completed after implementation: the shared-context reviewer checked plan/architecture/security integration, and the fresh-context reviewer challenged attribution, fixture binding, cleanup, and missing-test claims. Valid findings were closed without starting another reviewer round. The resulting fixes include fail-closed credential denial, credential cleanup, explicit phase/process/focus evidence, dispatcher-versus-round-trip separation, all three residual commands, serialized activation, exact artifact/fixture binding, and spawned-process-tree accounting.

Verification summary:

- `npm run check:quick`: passed after the final changes (release contracts, command inventory tests, lint, production build, initial-bundle budget, console scan, and Rust core compilation).
- Focused frontend tests: `appShellActions.test.tsx` 37/37 and the combined `appShellActions.test.tsx` + `useProjectStatus.test.tsx` 46/46 passed; focused ESLint passed.
- No-default-features focused Rust blocking tests: 12/12 passed. GUI-feature compilation passed. The GUI-linked unit-test executable itself exits on this Windows host with `STATUS_ENTRYPOINT_NOT_FOUND` before running tests, so packaged CDP evidence—not that loader-dependent binary—is the numeric GUI proof.
- A from-start `npm run check` on the final Batch 4 HEAD completed all frontend tests (145 files, 1,271 tests), Rust library tests (1,226), and the long integration matrices except two repeatable pre-existing `workflow_routes` owner-state assertions: `health_complete_advertises_an_installed_claude_profile` and `health_complete_advertises_only_an_installed_supported_agent`. A focused unchanged rerun reproduced only those two failures. Neither their production/test files nor their contract were changed in Batch 4.
- `graphify update .` is run after the final scoped changes; generated Graphify churn remains outside this Batch commit because it was already dirty at the start.

## Remaining external matrix

Windows installed-artifact P0 is closed. macOS and Linux retain real packaged frame/process capture as a release-before-publication matrix; their build and automated semantics are covered by repository gates, but this report does not claim real-host frame evidence for them. Batch 5 retains the plan-level final delivery work and must not reinterpret this Windows evidence as those external captures.

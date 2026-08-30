# Graph native titlebar drag focus remediation — Batch 0 baseline

Date: 2026-08-31
Status: Batch 0 complete; the section 4.3 hard stop did not trigger. Production behavior remains unchanged and Batch 1 is unblocked.

## Scope and evidence method

This batch froze the Windows platform facts before changing the production foreground contract. An isolated detached worktree at source commit `9c70a44a794acc39a5c71cdc2b80be060f07d20d` produced a release executable and MSI with temporary opt-in observers for:

- DOM `blur` / `focus`, timestamp, `document.visibilityState`, and `document.hasFocus()`;
- raw Tauri `WindowEvent::Focused(bool)`;
- synchronous `GetForegroundWindow` / `GetWindowThreadProcessId` / current PID normalization at each raw event;
- `get_graph` calls in the Graph store;
- external `GetWindowRect` samples during real native-titlebar input.

The observer-only patch hash was `20d76686dd68611c839c70f0b5013daf1591121fcaa50515e00735bdf0182684`. It existed only in the temporary evidence build and was removed after capture. No observer, Win32 feature, foreground event, debounce, timer, Graph rendering change, or Graph backend change is part of the Batch 0 commit.

The drag stimulus used Win32 `SendInput` to press the real non-client titlebar, move at an approximately 16 ms cadence for 112 samples at 2 px × 1 px per step, and release in `finally`. `GetWindowRect` was read externally after every input step. `MoveWindow` was used only before each measurement to restore a fixed origin, never as the measured stimulus.

## Artifact and fixture binding

| Field | Value |
| --- | --- |
| Source commit | `9c70a44a794acc39a5c71cdc2b80be060f07d20d` |
| Source tree | `5d9d4087c12088ac37ad0210f5d2cf25ef1e4846` |
| Package version | `0.1.0` |
| Release executable SHA-256 | `c773ab99d2290aa0a7cfc36385511af381be95a5bb317905a88203342782d6dc` |
| MSI SHA-256 | `b9ee7ca84bf61618c2cb69fb535244d7adb31d594a8e17b36597ddaf49c10354` |
| Fixture aggregate hash | `d1156523bf9303b9d9b82f5380e1c576521ee5bc4c6be1fcc6fe10d3f574f08e` |
| Fixture Git tree | `6f9dc5508efda511d3d2ee3da1775daeed0daf80` |
| Fixture shape | 3 Wiki pages + 240 support files |
| Raw evidence SHA-256 | `6c5d97244abdc8098f335ac9bee24434bbac3409d3d3e9ea233f3f0067031359` |

The measured executable is the release executable produced with the bound MSI. Batch 0 did not replace the user's existing installed application; Batch 3 still owns installed-EXE final acceptance.

## Native drag baseline

Each row is one independent 112-sample titlebar drag.

| Surface | Round | Unmoved samples | Unmoved ratio | Native lag P95 | Native lag max | `get_graph` delta | Raw focus | Normalized foreground |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| Dashboard | 1 | 0/112 | 0.0% | 0 px | 0 px | 0 | false → true | true → true |
| Dashboard | 2 | 1/112 | 0.9% | 0 px | 2.24 px | 0 | false → true | true → true |
| Dashboard | 3 | 0/112 | 0.0% | 0 px | 0 px | 0 | false → true | true → true |
| Graph, current focus callback | 1 | 32/112 | 28.6% | 60.37 px | 71.55 px | 1 | false → true | true → true |
| Graph, current focus callback | 2 | 44/112 | 39.3% | 87.21 px | 98.39 px | 1 | false → true | true → true |
| Graph, current focus callback | 3 | 46/112 | 41.1% | 91.68 px | 102.86 px | 1 | false → true | true → true |
| Graph, old focus refresh suppressed | 1 | 0/112 | 0.0% | 0 px | 0 px | 0 | false → true | true → true |
| Graph, old focus refresh suppressed | 2 | 0/112 | 0.0% | 0 px | 0 px | 0 | false → true | true → true |
| Graph, old focus refresh suppressed | 3 | 0/112 | 0.0% | 0 px | 0 px | 0 | false → true | true → true |

The Dashboard control passed consistently. The current Graph path failed consistently and issued exactly one `get_graph` per titlebar drag. Suppressing only the old focus-triggered resource refresh restored Graph to the Dashboard control while leaving Graph rendering, Sigma, Canvas, cache, TTL, project scope, and Rust Graph services unchanged.

## Foreground discrimination matrix

| Operation | Observed raw focus | Foreground PID fact | Normalized foreground | `get_graph` delta |
| --- | --- | --- | --- | ---: |
| Native titlebar drag | false → true in all 9 rounds | foreground PID == app PID at both raw events | true → true; zero false | Graph current: 1; controls: 0 |
| Real Alt-Tab away and back | false → true | `15160 → 44596`, app PID `44596` | false → true | 1 under current logic |
| Real click into another process and back | false → true | `51340 → 44596`, app PID `44596` | false → true | 1 under current logic |
| Same-process native dialog | no raw parent-focus event on this run | dialog foreground PID `44596` == app PID `44596` | no false event; foreground remained app-owned | 0 |

The titlebar sequence never produced normalized `false`, while both real background mechanisms did produce stable normalized `false → true`. Therefore the Batch 0 hard stop did not trigger. The same-process dialog used a native Tauri dialog and an external foreground snapshot; it remained owned by the application process and did not cause a false background classification.

## Red contract

`src/hooks/useTaskEvents.test.tsx` now states the target behavior for titlebar-style `blur → focus`:

- notification permission epoch increments once;
- observed project resources are not invalidated;
- observed project resources are not revalidated.

Against the unchanged production hook, the focused run fails exactly at the new resource assertion: `invalidate` was called once with `{ projectId: "project-a", rootPath: "D:/wiki" }`. The remaining 15 tests in the target file pass. This intentional red test is the Batch 1 implementation target; it must not be weakened to make Batch 0 green.

## Verification and retained boundary

- Temporary observer frontend production build: passed.
- Temporary observer release/MSI build: passed; one MSI produced.
- Packaged raw capture: completed; raw JSON parses and contains 1,008 HWND samples across 9 real titlebar drags plus Alt-Tab, cross-process click, and native-dialog evidence.
- Focused red contract: expected failure, one failing target assertion and all other target-file tests passing.
- Repository lint and production frontend build: passed. `graphify update .` was attempted both in and outside the managed sandbox, but Windows denied the final `.graph.tmp.json` replacement with `WinError 5`; existing dirty outputs and the generated recovery backup `graphify-out/2026-08-31/` remain outside this commit, and cleanup was not forced.
- Production Graph files and backend Graph services: unchanged.

Raw evidence: [`evidence/graph-titlebar-drag/batch-0/2026-08-31-baseline.json`](evidence/graph-titlebar-drag/batch-0/2026-08-31-baseline.json).

Batch 1 retains the implementation of the typed application-foreground event and the frontend false-arm / true-consume state machine. Batch 2 retains the full regression matrix, Batch 3 retains the final installed-EXE benchmark/contract assets, and Batch 4 retains final dual review and full repository gating.

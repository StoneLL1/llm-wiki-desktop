# Project Facts GUI blocking remediation — Batch 0 baseline

Batch 0 freezes the current Project Facts behavior and the target contracts. It does not move a command to a worker, remove a timer, narrow focus refresh, coalesce an invalidated request, or suppress a same-value project publication. Those behavior changes remain deliberately red for Batches 1 and 2.

The machine-readable evidence is [the Batch 0 result](results/2026-08-28-project-facts-gui-blocking-batch-0.json). The command classifications and the reviewed total live in [the execution inventory](tauri-command-execution-inventory.json).

## Frozen red baseline

| Scenario | Current deterministic observation | Target owner |
| --- | --- | --- |
| No project | 0 calls to `git_status`, `detect_agents`, or `list_llm_providers` | Remains invariant |
| First active project mount | StrictMode consumers single-flight to 1 call per fact | Remains invariant |
| Active project, 60.1 idle seconds | Git 13 calls; Agent 3 calls; Provider 3 calls | Batch 2: no periodic request |
| Window focus after first load | +1 Git, +1 Agent, +1 Provider call | Batch 2: Git only, at most once |
| Invalidate during an active Provider request | 2 requests active concurrently | Batch 2: one active plus one merged pending intent |
| Same-value `agentRoute` publication | 1 store publication and a new `currentProject` object | Batch 2: semantic no-op |
| Slow fake Agent | Fixture defaults to 5,000 ms; failure mode exits 23 | Batches 1 and 3: worker execution, bounded probe/cache |

The 60-second counts are hook-level controlled-time observations. They intentionally describe the current polling shape rather than a packaged performance estimate. The current command registry has 205 commands: 136 sync, 69 async, and 133 blocking-sync. All three Project Facts P0 commands are still blocking-classified synchronous commands. The target is explicit but is not enforced as green until Batch 1: all three async and a blocking-sync ceiling of 130.

On this machine, a supplemental real `claude --help` probe completed successfully in 901.6 ms on 2026-08-28. This one sample is environment evidence only; the deterministic fake CLI is the repeatable slow/failing control.

## Fixed fixture

Generate the fixture into a new directory:

```powershell
node scripts/prepare-project-facts-packaged-fixtures.mjs --output-root $env:TEMP\llm-wiki-project-facts-batch-0
```

The manifest hash is `8a7bda6d4daab3b1a29a8242ae5953a39ad318a541917919fdfe1861ea3e6818`. It contains:

- `native-git-3-pages`: the complete current native directory skeleton and a Git repository with 3 wiki pages, 240 support files, 251 tracked files, fixed `main` branch, and fixed tree `6f9dc5508efda511d3d2ee3da1775daeed0daf80`;
- `markerless-control`: 3 CJK-named Markdown files with no `.git` or `.app` marker;
- `fake-agent-slow-bin`, `fake-agent-fail-bin`, and `fake-agent-healthy-bin`: `claude`, `codex`, `openclaw`, and `hermes` wrappers with the mode hard-coded so production `env_clear()` cannot erase the control.

The generator refuses an existing output root. Choose a mode by prepending that mode's bin directory to `PATH` only for the disposable packaged measurement process. Slow is fixed at 5,000 ms and failing exits 23. The fixture test exercises healthy/failing wrappers under a scrubbed environment and proves the slow implementation exceeds a bounded 100 ms observer.

## Packaged capture protocol

Use a release artifact built from the exact commit under measurement. Start it with `LLM_WIKI_BLOCKING_TRACE_PATH` pointing to a disposable JSONL file and the selected fake-Agent mode directory first on `PATH`, then record these four scenarios separately:

1. no project for 60 seconds;
2. open `native-git-3-pages`, wait for the first facts, then remain idle for 60 seconds;
3. background and refocus the same project window once;
4. repeat open and focus with the fake Agent in `slow` and then `fail` mode.

Repeat the open/focus comparison with `markerless-control`; no Git command should start for the markerless assessment path. Record invocation/process counts, caller/worker thread IDs, queue/run durations, work class, operation, and outcome only. Do not retain project paths, CLI stdout, filenames, Provider configuration, credentials, or secrets.

No same-SHA packaged artifact was available during this delegated Batch 0 run, so packaged duration and process-count fields remain `null` instead of being estimated. The fixture and capture protocol are ready; Batch 4 owns installed-app numeric acceptance.

## Trace privacy contract

The trace operation vocabulary is closed to `unspecified`, `project_facts_git_status`, `project_facts_agent_detection`, and `project_facts_provider_status`. Each JSONL object is restricted to:

```text
callerThread, class, errorCode, operation, outcome,
queueWaitNanos, runNanos, workerThread
```

Tests assert the exact field set, sanitize arbitrary error text, reject path-like operation labels, and prove a supplied private path/secret fragment cannot reach the trace. Batch 1 will attach the three Project Facts labels when it moves their command bodies through the existing coordinator.

## Reproduction

```powershell
npm run check:command-execution
npm run test -- src/hooks/useProjectStatus.test.tsx src/hooks/useAiCapabilities.test.tsx src/stores/projectFactsStore.test.ts src/components/app/appShellActions.test.tsx src/stores/projectStore.test.ts --reporter=verbose
cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features services::blocking_work
```

The normal tests are green red-baseline locks. The opt-in target lane executes the future async/no-polling/Git-only-focus/single-flight/no-op expectations directly and is expected to exit nonzero in Batch 0:

```powershell
npm run check:project-facts-target
```

Batch 1 makes the command portion green; Batch 2 makes the frontend portion green. This preserves the main repository gate without converting bad behavior into the only executable contract.

## Deferred ownership

- Batch 1 moves the three P0 commands to bounded blocking workers and activates their operation labels.
- Batch 2 removes timer-driven polling, narrows focus to Git, coalesces invalidation/force, and suppresses equal-route publication.
- Batch 3 reduces repeated Agent/Provider work.
- Batch 4 captures same-SHA packaged timings and process counts; broader blocking-sync debt stays outside this plan unless the evidence triggers it.

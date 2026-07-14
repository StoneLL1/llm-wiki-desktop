# Import V2 Frontend Release Evidence

Date: 2026-07-14
Migration prerequisite: `55ce16c`
Frontend certification baseline: `a60d809`
Branch: `codex/import-v2-frontend`

This document records evidence for the frontend cutover surface only. No real user project was migrated or activated, no Agent was installed, and no capability pack was downloaded.

## Automated evidence

Final command:

```text
npm run check
```

Result:

- Vitest: 93 test files passed, 541 tests passed.
- ESLint: passed with `--max-warnings=0`.
- TypeScript/Vite build: passed. `ImportView` remains a separate lazy chunk (`61.43 kB`, `13.79 kB` gzip in this build); it was not folded into the initial shell chunk.
- Console scan: passed, no unexpected `console.log` calls.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`: 597 library tests passed; all emitted integration-test binaries passed.
- Existing Rust warning: `FileTransaction::write`, `track`, and `capture_installed` are currently unused. This is an existing warning and not a frontend failure.

Focused release gate:

```text
npm run test -- src/features/import src/stores/importStore.test.ts src/services/importV2Api.test.ts src/test/ui-css-contracts.test.ts src/test/app-shell-architecture.test.ts
```

Result: 25 files and 117 tests passed.

The new `ImportV2.integration.test.tsx` covers selected-ready partial commit, failed-sibling retry, active-item cancellation, migration blocking without a V1 switch, and bounded 2,000-item queue rendering (200 initial rows, then incremental loading).

## Journey matrix

| Journey | Evidence | Result |
| --- | --- | --- |
| Empty draft and bootstrap loading | `ImportView.test.tsx` | Passed |
| Recovered session and project-scoped refresh | `useImportWorkflow.test.tsx`, Rust task recovery tests | Passed |
| One/multiple files, folder picker, repeated drops | `ImportSourceMethods.test.tsx`, `importV2Api.test.ts` | Passed at typed boundary; no real picker run |
| Generic URL and platform chips | `ImportSourceMethods.test.tsx`, Web/File Rust suites | Passed at typed boundary; no external fetch run |
| Unsupported phase-two platform | capability/platform presentation tests | Passed fail-closed |
| Active, ready, failed, partial commit | `ImportV2.integration.test.tsx`, queue/status tests | Passed |
| Retry, cancel, task refresh, restart | workflow/task tests and Rust task suites | Passed |
| Login, captcha, private target, capability wait | dedicated dialog tests and Web Rust suites | Passed at typed boundary; no real login/captcha run |
| Local Agent, BYOK, candidate Diff/merge | Agent/BYOK/Diff tests and Rust Agent suites | Passed; no Agent process or paid request run |
| Migration scan/plan/apply/resume and legacy history | migration dialog/history tests and Rust Migration suites | Apply remains intentionally disabled without backend-bound confirmation token |
| Project switch during awaited work | workflow epoch tests and Rust task scope tests | Passed |

## Accessibility and responsive evidence

- Semantic regions, headings, labeled controls, live queue updates, keyboard row selection, alert/status states, dialog Escape/focus behavior, and reduced-motion CSS contracts are covered by the frontend tests.
- Queue names retain full values in `title`; the queue is horizontally bounded and incrementally rendered.
- CSS contract tests cover method-pane stacking below 820px, shell drawer boundaries, sticky commit-bar layout, and the bounded queue paging affordance.
- Chinese and English locale JSON parses successfully and the composed Import V2 view has focused Chinese/English tests.
- A real GUI pass at 100%/125%/150% OS scaling, screen-reader pass, contrast inspection across every theme preset, and screenshots for light/dark Chinese/English were not run in this environment.

## Security and architecture evidence

- Active Import V2 entry points contain no direct Tauri `invoke`, filesystem read/write, URL fetch, process launch, secret storage, or legacy mutation command.
- The active-source scan found no `preview_import`, `fetch_import_url`, `preview_text_import`, or `confirm_import_preview` call.
- React receives only typed workflow and presentation DTOs. Paths, URL fetching, Git, Agent, capability installation, migration writes, and secrets remain backend-owned.
- BYOK shows bounded scope facts only. If no provider is configured, the UI shows an explicit unavailable state instead of opening an empty approval dialog.
- Legacy history is read-only. V2 history action buttons are hidden when the current typed backend surface cannot open a corresponding detail view.
- No real cookies, Authorization values, signed URLs, provider keys, browser profiles, or prompt-injection payloads were seeded into a GUI or persisted snapshot during this run. Backend threat and redaction suites passed.

## Review results

Two independent manual reviews were performed because review subagents were unavailable in this environment.

Review A — design intent and integration:

- Confirmed `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy ImportView` ownership remains intact.
- Confirmed all five backend package contracts are consumed through the typed V2 adapter and global task workflow.
- Found and fixed a partial-success semantic error: an unresolved sibling must remain actionable, but it must not disable committing a separately selected ready item.
- No remaining valid design or integration findings.

Review B — adversarial fresh-context pass:

- Scanned for direct IPC/filesystem/network/process/secret access, V1 mutation commands, no-op actions, and obsolete V1 source files.
- Found and fixed the empty BYOK-dialog path, the optional no-op inspector preview callback, stale V1 import CSS/translation keys, and unbounded queue rendering.
- No remaining valid source-level security, stale-project, or V1/V2 dual-write findings.

## Generated schema and workspace safety

- `src-tauri/gen/schemas/desktop-schema.json` and `windows-schema.json` were not modified by the final check.
- No generated schema was copied from the Core worktree.
- `UI-Frontend-design/` was not modified.
- No merge, push, reset, stash, real-project migration, activation, Agent installation, or capability download was performed.

## Remaining risks and release conditions

1. The backend migration apply contract requires a confirmation token derived from the backend plan/project identity, but the current presentation DTO exposes no token endpoint or token field. The frontend therefore keeps Apply/Resume disabled rather than reproducing the Rust algorithm. This must be resolved by a typed backend contract before migration UI can activate writes.
2. Only the Windows development environment was exercised. macOS and Linux builds, GUI behavior, native pickers, and platform-specific Tauri drag/drop were not run and must not be described as passed.
3. Real GUI visual, scaling, screen-reader, external-platform, login, Agent, paid BYOK, and capability-install checks remain release-environment work.

Conclusion: the frontend code and automated release gates are ready for review on this branch, but this evidence does not by itself establish merge readiness or real-project Import V2 cutover readiness until the migration confirmation-token contract and the unrun platform/GUI gates are completed.

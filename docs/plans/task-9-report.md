# Task 9 final Review B recovery closeout

## 2026-07-12 recovery parent binding

- Recovery now binds the project root and every existing target-parent component to stable native identities, rejects symlink/reparse components, and revalidates the complete chain immediately before journal-driven writes and deletes.
- Added a deterministic restart test that swaps `wiki/` after validation but before rollback deletion. Recovery fails closed with `IMPORT_V2_COMMIT_CONFLICT` and preserves both the displaced installed file and the replacement namespace.
- Confirmed focused Rust tests execute (not compile-only) with `--no-default-features` and a clean `CARGO_TARGET_DIR`; the GUI-linked Windows loader issue is isolated from the backend test runner.

## 2026-07-12 recovery preflight and installed identity

- Added a single `ImportV2Service` preflight that runs `FileTransaction::reconcile_project` while holding the same mutation lock used by session mutation. Create, load, add, recover, select, run-state mutations, and commit now reconcile before observing or changing Import V2 project state.
- Extended durable journal intents with the installed file's native identity: Unix device/inode and Windows volume serial/file index. Recovery requires both the desired hash and the persisted identity before deleting a newly installed file or restoring an overwritten file. Missing identity fails closed.
- Added restart recovery regressions for same-byte external namespace replacements of both new and overwritten destinations; recovery returns `IMPORT_V2_COMMIT_CONFLICT` and preserves the external file.
- Focused Rust tests compile successfully with a fresh Cargo target. Executing the Windows test binary is currently blocked by `STATUS_ENTRYPOINT_NOT_FOUND` in this environment; the original worktree target also contains stale absolute Tauri build paths.

## Remaining Review B work

- Replace the synthetic target-loop crash test with commit-service fault injection across every persistence boundary.
- Add deterministic recovery directory-swap/symlink TOCTOU coverage and harden the final mutation primitive if the test confirms the race.
- Run full Rust and `npm run check` after the Windows runtime issue is resolved.

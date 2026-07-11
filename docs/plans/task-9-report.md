# Task 9 final Review B recovery closeout

## 2026-07-12 recovery child-name quarantine

- Restart rollback now renames the current canonical child to a unique recovery guard through the retained parent namespace before any destructive decision. The quarantined object is opened relative to that same parent and must match both the journal's persisted installed identity and desired hash.
- New-file rollback deletes only the verified transaction-owned guard. Checked-overwrite rollback installs previous bytes with create-if-absent semantics; if an external child wins the canonical name, recovery preserves it, retains the verified guard, and reports the guard by project-relative path.
- Added a hook at the final unlink/install primitive and regressions for new and overwritten targets with same-byte and different-byte child replacements. The external child remains canonical in every case; conflicted overwrite recovery remains actionable without overwriting either object.
- Focused transaction tests (`26/26`), both real persistence-boundary crash matrices, clean full Rust (`565/565` plus integration suites), and unified `npm run check` (`455/455` frontend tests) pass.

## 2026-07-12 recovery parent binding

- Recovery now binds the project root and every existing target-parent component to stable native identities, rejects symlink/reparse components, and revalidates the complete chain immediately before journal-driven writes and deletes.
- Added a deterministic restart test that swaps `wiki/` after validation but before rollback deletion. Recovery fails closed with `IMPORT_V2_COMMIT_CONFLICT` and preserves both the displaced installed file and the replacement namespace.
- Confirmed focused Rust tests execute (not compile-only) with `--no-default-features` and a clean `CARGO_TARGET_DIR`; the GUI-linked Windows loader issue is isolated from the backend test runner.

## 2026-07-12 recovery preflight and installed identity

- Added a single `ImportV2Service` preflight that runs `FileTransaction::reconcile_project` while holding the same mutation lock used by session mutation. Create, load, add, recover, select, run-state mutations, and commit now reconcile before observing or changing Import V2 project state.
- Extended durable journal intents with the installed file's native identity: Unix device/inode and Windows volume serial/file index. Recovery requires both the desired hash and the persisted identity before deleting a newly installed file or restoring an overwritten file. Missing identity fails closed.
- Added restart recovery regressions for same-byte external namespace replacements of both new and overwritten destinations; recovery returns `IMPORT_V2_COMMIT_CONFLICT` and preserves the external file.
- Focused Rust tests compile successfully with a fresh Cargo target. Executing the Windows test binary is currently blocked by `STATUS_ENTRYPOINT_NOT_FOUND` in this environment; the original worktree target also contains stale absolute Tauri build paths.

## 2026-07-12 final Review B closure

- Journal recovery rejects symlink/reparse `.app`, journal directories, and journal files. Journal files are opened no-follow relative to the retained journal parent on Unix, while Windows retains the journal parent without `FILE_SHARE_DELETE`; cleanup is relative/handle-protected and cannot delete an outside replacement.
- New and checked-replace writes persist the candidate's native identity in the durable intent before the first OS install primitive. Same-volume `linkat`/rename preserves that identity, the immediate post-install fault hook runs before any later journal write, and reopening verifies the installed target identity.
- Final mutations are namespace-bound: Unix retains the validated parent fd and uses narrow `openat`, `linkat`, `unlinkat`, and `renameat` FFI; Windows retains a directory handle opened without `FILE_SHARE_DELETE` while the existing same-volume primitives run. Directory-swap injection proves an outside replacement remains untouched.
- Real commit-service crash coverage passes for every observed persistence boundary for both new writes and checked Wiki replacement. Focused transaction coverage passes on Windows, including supported reparse behavior; Unix-only symlink directory/file regressions compile under their platform gates.

## Verification

- Clean-target `cargo test --no-default-features`: library `563/563` and Import V2 integration `3/3` passed; the first sandboxed run reached `mvp_flow` with `8/9`, where the sole failure was an environment denial writing the test settings file under `%APPDATA%`. An elevated full rerun and unified `npm run check` are recorded in the delivery evidence.

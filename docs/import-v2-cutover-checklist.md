# Import V2 migration and cutover checklist

This checklist is a release gate. A failed or unrun item stays failed/unrun;
the verifier does not infer platform or package success from source code.

## Readiness evidence

- [ ] Core recovery/invariant suite passes on the exact contract version.
- [ ] File, Web, and Agent release reports record the same Core contract and pass their own gates.
- [ ] Clean, corrupt, ambiguous, Unicode/CJK, case-only, symlink/reparse, external-edit, no-Git, disk-full, cancel, crash/recovery, and repeat tests pass.
- [ ] Windows, macOS, and Linux acceptance is recorded separately. Unrun platforms are not passed.
- [ ] Every external tool has license, version, platform, hash/signature, size, and fallback evidence.
- [ ] Generated schemas were regenerated only after the command surface stabilized and the diff was reviewed.

The machine-readable source of truth is `docs/import-v2-cutover-evidence.json`;
`npm run check:import-v2-cutover` is read-only and fails closed.

## Apply and rollback boundary

1. Scan and plan are read-only and may be exported without confirmation.
2. Apply requires a token bound to the plan/project snapshot and a Core Git checkpoint when Git is available.
3. A no-Git project requires explicit acknowledgement that rollback is release-based.
4. Apply may write only `.app/source-index-v2.json` and
   `.app/import-v2-migration/report.json`, plus the shared Core journal while
   the transaction is in progress.
5. `raw/`, `wiki/`, `.app/source-index.json`, old task logs, and old import
   history remain byte- and timestamp-preserved.
6. Rollback means closing the new release and opening the prior release. The
   prior release reads the preserved legacy state and ignores V2 metadata; no
   V1/V2 dual-write toggle is supported.

## Soak window

The initial activation retains legacy mutation code and read-only history
compatibility. Removal is a separate approved change after the soak window,
with a new review and regenerated schemas. It is not part of initial apply or
activation.

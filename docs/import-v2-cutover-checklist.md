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
5. Activation records release-readiness evidence and writes only
   `.app/import-v2-migration/activation.json`; it is not required to create,
   stage, preview, retry, cancel, or commit a new Import V2 session.
6. `raw/`, `wiki/`, `.app/source-index.json`, old task logs, and old import
   history remain byte- and timestamp-preserved.
7. Rollback means closing the new release and opening the prior release. The
   prior release reads the preserved legacy state and ignores V2 metadata. The
   current release has one import write path: the Import V2 session pipeline.
   Legacy command names may remain as typed protocol compatibility, but they
   route to V2 or fail closed and never perform legacy writes.

## Legacy compatibility

Legacy source indexes, task logs, and import history remain read-only evidence
for migration and rollback. Their absence or unreadability must not prevent a
new V2 session on an otherwise valid project. No V1/V2 runtime switch or
dual-write mode is exposed by the current release.

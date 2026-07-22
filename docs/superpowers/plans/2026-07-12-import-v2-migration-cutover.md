# Import V2 Migration & Cutover Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Do not combine this plan with File Ingestion, Web Ingestion, or Agent Assistance implementation.

**Goal:** Migrate legacy import metadata into the completed Import V2 Core without moving or rewriting user content, then make V2 the single active import path through an observable, reversible release cutover.

**Architecture:** Treat legacy state as immutable evidence. A read-only scanner produces a deterministic migration plan and human-readable dry-run report; only an explicit confirmation may write new V2 metadata. The apply phase reuses Import Core's project mutation lock, preflight reconciliation, crash journal, path/identity checks, and atomic file helpers. Old `raw/`, `wiki/`, `.app/source-index.json`, task logs, and Git history remain untouched. V2 continues to own `.app/source-index-v2.json`; an activation record selects the new path without renaming or overwriting the legacy index. Cutover is a release gate, not a permanent dual-write system.

**Tech Stack:** Rust/Tauri v2, Serde JSON, existing Import V2 services and transaction helpers, React 19/TypeScript command contracts only where routing must change, Vitest, Rust unit/integration tests.

**Prerequisites:**

- Import Core branch `codex/import-v2-core` at or after `3bd282c6a86a5baa2d16660d1387b617e88a35a7` is integrated or available as the implementation base.
- File Ingestion, Web Ingestion, and Agent Assistance packages have passed their own release gates before the final cutover task is enabled.
- Read `docs/superpowers/specs/2026-07-11-import-v2-design.md` and `docs/superpowers/plans/2026-07-12-import-v2-open-source-research.md` before implementation.
- Do not copy the two currently uncommitted generated schema drifts from the Core worktree. Regenerate Tauri schemas deliberately only after the final command surface is stable.

---

## Stable Core interfaces this plan must reuse

- `ImportV2Service::{create_session, add_inputs, load_session, recover_session, register_engine, set_item_selected, run_item, commit_items_cancellable}`
- `ImportEngine::{descriptor, supports, execute}` and `EngineRegistry::{register, resolve}`; `execute` receives Core `CancellationToken`
- Core commands `create_import_session_v2`, `get_import_session_v2`, `add_import_items_v2`, `set_import_item_selection_v2`, `start_import_items_v2`, `confirm_import_session_v2`
- The same project mutation lock and preflight reconciliation used by every V2 API call
- Staging-only engine writes, backend-derived CreateNew destinations, CAS/no-follow/TOCTOU checks, and crash-durable commit journal
- Existing V2 index path `.app/source-index-v2.json`; legacy `.app/source-index.json` is never overwritten

## Non-negotiable migration rules

1. Scanning and planning are read-only and may run without confirmation.
2. Applying migration metadata requires explicit confirmation and a Git checkpoint when the project is a Git repository.
3. Never move, delete, rewrite, normalize, or touch timestamps on existing `raw/` or `wiki/` content.
4. Never mutate the legacy source index. Ambiguous records become `legacy_unmanaged`, not guessed links.
5. The migration may create V2 metadata that an older release ignores; rollback means running the prior release against preserved legacy state.
6. Do not dual-write legacy and V2 indexes. Shadow verification compares results but has no production mutation path.
7. A crash or cancellation must be resumable and idempotent.

---

### Task 1: Freeze migration contracts and a hostile legacy fixture corpus

**Files:**

- Create: `src-tauri/src/models/import_v2_migration.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/tests/fixtures/import_v2_migration/README.md`
- Create: `src-tauri/tests/fixtures/import_v2_migration/legacy-clean/`
- Create: `src-tauri/tests/fixtures/import_v2_migration/legacy-ambiguous/`
- Create: `src-tauri/tests/fixtures/import_v2_migration/legacy-corrupt/`
- Create: `src-tauri/tests/fixtures/import_v2_migration/unicode-windows-linux/`
- Create: `src-tauri/tests/import_v2_migration_contract.rs`

**Step 1: Write failing serialization and invariant tests**

Define and snapshot-test these public DTOs:

```rust
pub struct MigrationScanRequest {
    pub project_root: PathBuf,
}

pub struct LegacyInventory {
    pub schema_version: u32,
    pub fingerprint: String,
    pub records: Vec<LegacyRecord>,
    pub warnings: Vec<MigrationWarning>,
}

pub enum MigrationDecision {
    LinkExisting { source_id: String, confidence: MatchConfidence },
    CreateV2Record { proposed_source_id: String },
    LegacyUnmanaged { reason: String },
    Conflict { candidates: Vec<String>, reason: String },
}

pub struct MigrationPlan {
    pub plan_version: u32,
    pub inventory_fingerprint: String,
    pub candidates: Vec<MigrationCandidate>,
    pub summary: MigrationSummary,
}

pub enum MigrationStatus {
    NotScanned,
    DryRunReady,
    AwaitingConfirmation,
    Applying,
    Applied,
    VerificationFailed,
    Cancelled,
}
```

Tests must reject unknown schema versions, absolute paths stored as project-relative paths, duplicate candidate IDs, a `LinkExisting` decision without evidence, and path traversal in fixtures.

**Step 2: Run the focused test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_v2_migration_contract`

Expected: FAIL because the models do not exist.

**Step 3: Implement the smallest typed model surface**

Use tagged Serde enums and stable camelCase JSON. Store evidence as structured fields, never prose-only strings. Keep filesystem paths as validated project-relative strings in persisted reports.

**Step 4: Add fixtures covering real failure modes**

Include CJK and decomposed Unicode names, mixed Windows/POSIX separators, case-only collisions, missing raw files, missing wiki files, duplicate hashes, malformed JSON, symlinks/reparse-point stand-ins, an interrupted prior migration, and externally edited Markdown.

**Step 5: Re-run and verify GREEN**

Run the focused test, then `cargo test --manifest-path src-tauri/Cargo.toml import_v2_migration`.

**Step 6: Commit**

```text
test(import-v2): freeze migration contracts and fixture corpus
```

---

### Task 2: Build a read-only legacy inventory scanner

**Files:**

- Create: `src-tauri/src/services/import_v2/migration/scanner.rs`
- Create: `src-tauri/src/services/import_v2/migration/mod.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Create: `src-tauri/tests/import_v2_migration_scanner.rs`

**Step 1: Write failing read-only scanner tests**

Test that the scanner reads known legacy index/task metadata and inventories relevant `raw/` and `wiki/` paths without following symlinks or reparse points. Capture a full metadata snapshot before and after scanning and assert byte contents, mtimes, directory entries, and Git status are unchanged.

**Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_v2_migration_scanner`

**Step 3: Implement `LegacyScanner`**

```rust
pub trait LegacyScanner: Send + Sync {
    fn scan(&self, project_root: &Path) -> Result<LegacyInventory, BackendError>;
}
```

Use existing safe path utilities. Open metadata read-only, use `symlink_metadata`, enforce project-root containment, cap file count and metadata bytes, collect warnings instead of aborting on individual corrupt records, and compute a deterministic inventory fingerprint from normalized evidence.

**Step 4: Prove compatibility behavior**

Unknown legacy shapes must produce typed warnings and `legacy_unmanaged` candidates later; they must not be silently discarded. Secret-like fields in corrupt metadata must be redacted from logs and reports.

**Step 5: Re-run focused tests**

Expected: all scanner tests pass on Windows-style, POSIX-style, Unicode, corrupt, and symlink fixtures.

**Step 6: Commit**

```text
feat(import-v2): add read-only legacy inventory scanner
```

---

### Task 3: Implement deterministic correlation and confidence rules

**Files:**

- Create: `src-tauri/src/services/import_v2/migration/planner.rs`
- Create: `src-tauri/tests/import_v2_migration_planner.rs`
- Create: `docs/import-v2-migration-matching.md`

**Step 1: Write the matching truth table as failing parameterized tests**

Accept a link automatically only when evidence is unique and deterministic. Rank evidence in this order:

1. Exact existing stable source ID and matching content identity.
2. Exact original-source hash plus unique destination path.
3. Exact source hash plus exact normalized public URL for web sources.
4. Otherwise conflict or `LegacyUnmanaged`.

Filename similarity, title similarity, timestamps, and fuzzy text similarity may appear as suggestions but must never cause an automatic link.

**Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_v2_migration_planner`

**Step 3: Implement a pure planner**

```rust
pub trait MigrationPlanner: Send + Sync {
    fn plan(
        &self,
        inventory: &LegacyInventory,
        v2_index: &SourceIndex,
    ) -> Result<MigrationPlan, BackendError>;
}
```

The same inputs must produce byte-identical canonical JSON and candidate ordering on Windows, macOS, and Linux. Decisions include machine-readable evidence and rejection reasons.

**Step 4: Test collision and external-edit preservation**

Case-only path collisions, multiple legacy records sharing a hash, and a Markdown file whose content differs from recorded legacy metadata must never be auto-linked. The planner must not propose overwriting either copy.

**Step 5: Document the truth table**

Explain automatic, manual-review, unmanaged, and conflict outcomes in user-facing language. State explicitly that low confidence is safe and expected.

**Step 6: Re-run and commit**

```text
feat(import-v2): add deterministic legacy correlation planner
```

---

### Task 4: Produce a stable, inspectable dry-run report

**Files:**

- Create: `src-tauri/src/services/import_v2/migration/report.rs`
- Create: `src-tauri/tests/import_v2_migration_report.rs`
- Modify: `src-tauri/src/models/import_v2_migration.rs`

**Step 1: Write failing report tests**

Assert the report contains totals, automatic links, proposed new records, conflicts, unmanaged items, warnings, affected metadata paths, content paths guaranteed untouched, rollback statement, required confirmation, and the inventory fingerprint. Assert there are no secrets, browser cookies, auth headers, absolute home-directory paths, or raw HTML dumps.

**Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_v2_migration_report`

**Step 3: Implement canonical JSON plus Markdown rendering**

Return both formats from the service. During an unconfirmed dry run, keep them in memory or an explicitly selected export destination; do not write into the project automatically. A persisted apply report later lives at `.app/import-v2-migration/report.json` and is derived from the same canonical model.

**Step 4: Add snapshot and localization-boundary tests**

The backend owns codes and facts, not translated UI prose. Snapshot English diagnostic fallback only; expose stable reason codes for Chinese and English UI localization later.

**Step 5: Re-run and commit**

```text
feat(import-v2): add migration dry-run reports
```

---

### Task 5: Apply metadata through Core's transaction and recovery boundary

**Files:**

- Create: `src-tauri/src/services/import_v2/migration/apply.rs`
- Create: `src-tauri/tests/import_v2_migration_apply.rs`
- Modify: `src-tauri/src/services/import_v2/migration/mod.rs`
- Modify: only the minimal Core visibility needed in `src-tauri/src/services/import_v2/transaction.rs`

**Step 1: Write failing confirmation, CAS, and immutability tests**

Require a confirmation token tied to the plan fingerprint. Fail closed if the legacy inventory, V2 index generation, project identity, or relevant file identities changed after dry run. Before/after snapshots must prove old index, `raw/`, and `wiki/` are byte-for-byte unchanged.

**Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_v2_migration_apply`

**Step 3: Implement `MigrationService::apply_metadata`**

```rust
pub trait MigrationService: Send + Sync {
    fn scan(&self, request: MigrationScanRequest) -> Result<LegacyInventory, BackendError>;
    fn plan(&self, inventory: &LegacyInventory) -> Result<MigrationPlan, BackendError>;
    fn apply_metadata(
        &self,
        plan: &MigrationPlan,
        confirmation: MigrationConfirmation,
        cancellation: &CancellationToken,
    ) -> Result<MigrationApplyResult, BackendError>;
    fn resume(&self, project_root: &Path) -> Result<MigrationApplyResult, BackendError>;
}
```

Run under the existing per-project mutation lock and preflight reconciliation. Reuse Core transaction helpers; do not create a second journal protocol. Write only new/updated V2 metadata, `.app/import-v2-migration/report.json`, and the shared crash journal records required by Core.

**Step 4: Add the Git checkpoint boundary**

If the project is a Git repository, use the existing Git service to create or verify the required checkpoint before mutation. If Git is unavailable, surface a typed confirmation state explaining that rollback relies on preserved legacy metadata and the prior app release.

**Step 5: Inject failures at every atomic boundary**

Cover cancellation before first write, after journal prepare, after V2 index replacement, before report finalization, and after report finalization. Recovery must converge to one valid state without duplicate source records.

**Step 6: Re-run and commit**

```text
feat(import-v2): apply migration metadata with core transactions
```

---

### Task 6: Add resumable migration commands and TypeScript contracts

**Files:**

- Create: `src-tauri/src/commands/import_v2_migration.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/types/importV2Migration.ts`
- Create: `src/services/importV2MigrationApi.ts`
- Create: `src/services/importV2MigrationApi.test.ts`
- Create: `src-tauri/tests/import_v2_migration_commands.rs`

**Step 1: Write failing command/API contract tests**

Freeze these commands:

- `scan_import_v2_migration`
- `plan_import_v2_migration`
- `apply_import_v2_migration`
- `get_import_v2_migration_status`
- `resume_import_v2_migration`

Assert every command performs preflight reconciliation under the same project mutation lock before reading or mutating migration state. Only apply accepts confirmation.

**Step 2: Run and verify RED**

Run Rust command tests and `npm test -- --run src/services/importV2MigrationApi.test.ts`.

**Step 3: Implement thin commands and exact TS mirrors**

Commands validate DTOs and delegate to `MigrationService`. TypeScript types must exactly mirror Serde casing and discriminants. Do not introduce ad hoc string status parsing.

**Step 4: Add cancellation and restart tests**

The API exposes a task identifier compatible with the existing background task service. Simulate application restart and verify status/resume uses persisted journal state rather than frontend memory.

**Step 5: Re-run and commit**

```text
feat(import-v2): expose resumable migration contracts
```

---

### Task 7: Build a read-only legacy history compatibility adapter

**Files:**

- Create: `src-tauri/src/services/import_v2/migration/legacy_history.rs`
- Create: `src-tauri/tests/import_v2_legacy_history.rs`
- Modify: existing import-history service only at its composition boundary

**Step 1: Write failing compatibility tests**

Verify legacy history remains visible after V2 activation, is clearly marked `legacyReadOnly`, and cannot trigger retry, delete, replace-source, or destructive actions through V2 APIs. V2 history remains fully typed and actionable.

**Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test import_v2_legacy_history`

**Step 3: Implement a projection, not a migration rewrite**

Map legacy history into a read-only view DTO at read time. Do not rewrite old task logs merely to make them look V2-native. Preserve original timestamps and identifiers as evidence.

**Step 4: Test malformed and huge history sets**

Use bounded reads and typed warnings. One corrupt legacy entry must not hide the rest. Never emit secrets from old command logs.

**Step 5: Re-run and commit**

```text
feat(import-v2): preserve legacy history as read-only evidence
```

---

### Task 8: Add shadow verification and release-readiness gates

**Files:**

- Create: `src-tauri/src/services/import_v2/migration/verifier.rs`
- Create: `src-tauri/tests/import_v2_migration_verifier.rs`
- Create: `scripts/check-import-v2-cutover.mjs`
- Modify: `package.json`
- Create: `docs/import-v2-cutover-checklist.md`

**Step 1: Write failing readiness tests**

The gate must fail unless all of these are true:

- Core recovery/invariant suite passes.
- File, Web, and Agent package release gates are recorded as compatible with the same Core contract version.
- Migration fixtures produce no unexplained automatic links.
- Dry-run/apply/resume is idempotent across three supported OS path models.
- No GPL/AGPL/non-commercial runtime or copied implementation is present.
- External tool manifests include license, version, platform, hash/signature, size, and fallback.
- Legacy content/index immutability checks pass.
- All long tasks have progress, cancellation, logs, and restart recovery.

**Step 2: Run and verify RED**

Run: `npm run check:import-v2-cutover`

Expected: FAIL until the script and readiness evidence exist.

**Step 3: Implement a non-mutating verifier**

The verifier compares legacy inventory, migration plan, resulting V2 metadata, and filesystem snapshots. It must never call a legacy importer or dual-write production state. Produce machine-readable evidence for CI and a concise Markdown summary for human review.

**Step 4: Add negative tests**

Prove the gate blocks on stale plan fingerprints, missing package evidence, an accidental legacy-index write, unknown external-tool license, unbounded browser/yt-dlp capability, schema drift, or a failed recovery injection.

**Step 5: Re-run and commit**

```text
test(import-v2): add migration and cutover readiness gates
```

---

### Task 9: Activate V2 without overwriting legacy state

**Files:**

- Create: `src-tauri/src/models/import_backend_activation.rs`
- Create: `src-tauri/src/services/import_v2/activation.rs`
- Create: `src-tauri/tests/import_v2_activation.rs`
- Modify: backend import composition root and Tauri command registration
- Modify: frontend import workflow service/store routing only; do not redesign UI

**Step 1: Write failing activation and rollback tests**

Define an activation record at `.app/import-v2-migration/activation.json` containing schema version, activated Core contract version, migration report fingerprint, timestamp, and release version. Test that activation is refused before all readiness gates pass.

**Step 2: Run and verify RED**

Run focused Rust and frontend routing tests.

**Step 3: Implement one active write path**

After confirmed activation, new imports route only to V2 commands and `.app/source-index-v2.json`. The application may still read projected legacy history, but it must not write `.app/source-index.json` or invoke legacy import mutations. Do not rename V2 index to the legacy filename.

**Step 4: Preserve operational rollback**

Rollback is release-based: close the new release and open the prior release, which sees untouched legacy metadata/content and ignores V2 metadata. Do not add a casual in-app toggle that can interleave V1 and V2 writes. If an emergency deactivation command is required operationally, it must be an explicit confirmed maintenance action that disables all imports until a release rollback; it must not re-enable V1 writes.

**Step 5: Test concurrent/stale clients**

Old frontend calls, duplicate activation, a second app process, and stale confirmation tokens must fail safely. Current V2 calls remain serialized under the project mutation lock.

**Step 6: Re-run and commit**

```text
feat(import-v2): activate v2 as the sole import write path
```

---

### Task 10: Retire legacy mutation code only after a soak window

**Files:**

- Modify/Delete: legacy import command registrations and mutation services identified by `rg`
- Modify: affected Rust/TypeScript tests
- Modify: `SPEC/BACKEND_STRUCTURE.md`
- Modify: `SPEC/SPEC.md` or the actual authoritative spec file if named differently
- Modify: `SPEC/progress.txt`

**Step 1: Inventory before deletion**

Run `rg -n "legacy|import.*command|source-index\.json" src src-tauri`. Classify each hit as legacy mutation, read-only compatibility, unrelated import syntax, or documentation. Put the exact deletion list in the implementation progress record before changing files.

**Step 2: Write tests that prove no runtime path depends on legacy mutation code**

Run activation, history, project open, import creation, retry, cancellation, recovery, and rollback compatibility tests with legacy mutation registrations removed from the test composition root.

**Step 3: Remove only mutation paths**

Keep the legacy inventory scanner and read-only history projection for the documented support window. Never delete user legacy files. Remove code only after the soak-window acceptance criteria in `docs/import-v2-cutover-checklist.md` are met and approved.

**Step 4: Regenerate derived contracts deliberately**

Regenerate Tauri schemas after the command surface is final. Review the diff and commit only intentional schema changes. Do not carry forward the two uncommitted generated schema files from the Core implementation worktree.

**Step 5: Update architecture documents**

Document V2 as the sole write path, the read-only legacy support boundary, activation record, rollback procedure, and support-window removal criteria. Do not rewrite product decisions outside import scope.

**Step 6: Commit**

```text
refactor(import-v2): retire legacy import mutation path
```

---

### Task 11: Run the complete cutover acceptance matrix

**Files:**

- Modify: `docs/import-v2-cutover-checklist.md`
- Modify: `SPEC/progress.txt`
- Modify: `gotchas.txt` only if a subtle or recurring issue is discovered

**Step 1: Run focused migration suites**

```text
cargo test --manifest-path src-tauri/Cargo.toml import_v2_migration
npm test -- --run src/services/importV2MigrationApi.test.ts
npm run check:import-v2-cutover
```

**Step 2: Run platform/path acceptance**

On Windows, macOS, and Linux, cover clean legacy, corrupt metadata, case collision, Unicode/CJK, symlink/reparse attempt, external Markdown edits, no-Git project, interrupted apply, cancellation, disk-full injection, and prior-release rollback. Record exact versions and results; do not mark an unrun platform as passed.

**Step 3: Run the unified project check from the beginning**

Run: `npm run check`

Expected: all tests, lint, build/import resolution, console scan, Tauri GUI Rust compile, and Rust no-default-features tests pass.

**Step 4: Perform two independent reviews**

- Review A with shared context: verify design intent, Core integration, migration safety, and spec coverage.
- Review B with fresh context: search for blind spots, unsafe mutation, weak tests, license leakage, stale API names, and rollback ambiguity.

Fix every valid finding, rerun focused suites, then rerun `npm run check` from the beginning.

**Step 5: Final evidence and approval boundary**

Record commit list, exact checks, platform matrix, reviewer outcomes, remaining warnings, activation fingerprint, and rollback instructions. Stop before merge, push, activation against a user's real project, or legacy-code deletion unless each action has its required approval.

**Step 6: Commit documentation evidence**

```text
docs(import-v2): record migration and cutover acceptance
```

---

## Dependency order and parallelism

- Tasks 1–4 are planning/read-only and may start once Core contracts are available.
- Task 5 depends on Tasks 1–4 and completed Core transaction/recovery behavior.
- Tasks 6–7 depend on Task 5; they may proceed in parallel after its public service contract is stable.
- Task 8 depends on the completed release evidence from File Ingestion, Web Ingestion, Agent Assistance, and Tasks 1–7 here.
- Task 9 depends on a fully passing Task 8 and explicit product approval for cutover.
- Task 10 depends on Task 9 plus the approved soak window; it must not be bundled into initial activation.
- Task 11 is final and blocks delivery.

## Explicitly rejected approaches

- Renaming or overwriting `.app/source-index.json` during migration.
- Moving legacy `raw/` or `wiki/` files to make the tree look cleaner.
- Fuzzy matching titles or filenames as authoritative identity.
- Keeping permanent V1/V2 dual writes or an everyday UI toggle between backends.
- Reimplementing Import Core journaling, locking, identity validation, or atomic replacement.
- Rewriting legacy history into V2 format and losing original evidence.
- Deleting legacy state automatically after a successful apply.
- Copying generated schema drift or code from GPL/AGPL/non-commercial projects.

## Definition of done

- A dry run explains every legacy record without modifying the project.
- Confirmed apply writes only V2 migration metadata and is crash-safe, cancellable, resumable, and idempotent.
- Existing content and legacy metadata are proven unchanged by automated snapshots.
- Ambiguous records remain unmanaged or conflicting; nothing is guessed.
- New imports have exactly one mutation path: Import V2.
- Prior-release rollback remains possible because legacy state is preserved.
- All four package release gates, license checks, three-platform acceptance, two reviews, and `npm run check` pass.
- Legacy mutation code is retired only after an approved soak window; no merge, push, or user-project activation happens implicitly.

# Import V2 migration and cutover checklist

> Historical internal cutover boundary, partially superseded by the current Import / Source authority: [`superpowers/specs/2026-07-24-import-source-media-flow-design.md`](superpowers/specs/2026-07-24-import-source-media-flow-design.md). It must not be used to restore migration terminology or repair controls in the normal Import workbench.

## Current release gate

- The repeatable Batch 9 evidence is [`qa/import-source-media-flow-batch9-evidence.json`](qa/import-source-media-flow-batch9-evidence.json).
- `npm run check:import-source-media` is the read-only declaration gate for the exact 32 design scenarios, 26 contract rows, 14 real-fixture categories, and 9 forbidden closures. Every declared fixture must be consumed by its referenced executable test; the 38-format pipeline must use built-in production engines or `PackProcessEngine` registration rather than a direct `ImportEngine` double; retired Source callable surfaces must be absent; migration remains a Settings-only entry. The full `npm run check` is the execution result and runs every referenced Rust, Vitest, and Node suite.
- `npm run check:import-v2-cutover` remains a compatibility command and delegates to that same read-only gate.
- Historical platform activation flags in `docs/import-v2-cutover-evidence.json` are archival evidence only. They are not inferred as passed and are no longer a product-runtime prerequisite.

## Migration boundary

1. Migration scan and plan are read-only.
2. Apply remains separately confirmed, identity-bound, checkpointed where Git is available, and limited to migration metadata.
3. Migration UI is reachable only from Settings compatibility controls; the normal Import workbench contains no migration notice, blocking overlay, or automatic repair.
4. Legacy source indexes, task logs, import history, raw evidence, and Wiki content remain byte- and timestamp-preserved.
5. A new Import V2 session does not require migration activation and never dual-writes legacy state.

## Legacy compatibility boundary

- Compile may read `.app/source-index.json` only through `compile_legacy_adapter.rs`; its tests assert that the adapter does not modify the legacy index or Wiki.
- Source asset resolution may fall back to `raw/sources/{sourceId}/{versionId}/assets/**` only for read compatibility; canonical package assets win, traversal is rejected, and the legacy index/manifest/assets remain unchanged.
- Retired legacy Import commands, `ImportService`, `ExtractionService`, and the old import checkpoint command are absent from AppState, Tauri registration, command modules, and frontend callable surfaces. Import V2 is the sole normal Import write path.
- Retired legacy source list/delete/replace commands are absent from the Tauri registration surface. Dedicated typed Source package commands own move, delete, restore, reprocess, and AI candidate application.
- Removing either read-only fallback requires separate compatibility evidence; it is not bundled into normal Import or Source mutations.

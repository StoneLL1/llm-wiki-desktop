# Import V2 File Ingestion Implementation Plan

> Historical implementation plan. Engine and test research remains useful, but product behavior and storage boundaries are superseded by [`../specs/2026-07-24-import-source-media-flow-design.md`](../specs/2026-07-24-import-source-media-flow-design.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe folder/multi-file discovery and high-quality deterministic conversion for Markdown, DOC/DOCX, XLS/XLSX, PPT/PPTX, PDF, OCR, and optional local media while preserving originals and producing Import Core preview artifacts.

**Architecture:** Extend Import Core at final HEAD `3bd282c` rather than creating a parallel importer. A Rust discovery/router layer validates inputs and registers engines; heavyweight converters run as signed JSON-RPC stdio capability packs that can read only the authorized source and write only the item's staging directory. All formal writes remain in Core `ImportV2Service::commit_items_cancellable` and its private commit module, which supply project locking, preflight reconciliation, source versioning, expected-hash checks, Git checkpoints, and crash-durable atomic commit.

**Tech Stack:** Tauri v2/Rust, existing Import Core DTOs and `ImportEngine`, JSON-RPC 2.0 stdio packs, MarkItDown, Docling, LibreOffice, Office Oxide probation route, Tesseract, PaddleOCR, strict-LGPL FFmpeg, whisper.cpp, Vitest, Rust tests.

## Global Constraints

- Prerequisite: integrate `codex/import-v2-core` final HEAD `3bd282c6a86a5baa2d16660d1387b617e88a35a7` without the two generated Tauri schema drifts.
- No Import UI or visual components are implemented in this package; expose typed backend DTOs/events only.
- Project content remains Markdown + JSON + local files; no database.
- Originals are immutable versions under `raw/sources/<source-id>/<version-id>/`; converters never write formal project paths.
- Every `ImportV2Service` API call continues through its mutation-lock preflight reconciliation. New commands resolve `ProjectContext` through `AppState` and remain thin.
- Capability packs use the existing `ImportEngine` / `EngineRequest` / `EngineResult` and JSON-RPC stdio protocol. Outputs must be project-relative staging paths and must pass Core `QualityGate`.
- GPL/AGPL/non-commercial code is excluded. MarkItDown is a fallback, Docling is the PDF-layout route, Office Oxide remains probationary until independent Golden Corpus approval.
- Limits: 64 MiB default source cap for in-process OOXML, 4,096 archive entries, 64 MiB cumulative expansion, XLSX column maximum XFD/16,384; larger inputs must use a streaming capability route or fail with a stable resource-limit issue.
- Folder scan skips symlinks/reparse points, hidden/system entries, project-internal paths, unsupported entries, and cycles; every skip has a typed reason.
- CJK/Unicode filenames, Windows long paths/device names, macOS normalization, Linux case sensitivity, cancellation, partial success, and external Wiki edits are release gates.
- Run `npm run check` after every completed task; fix and rerun from the beginning on failure.
- Open-source evaluation details are recorded in `docs/superpowers/plans/2026-07-12-import-v2-open-source-research.md`.

## Planned File Structure

- `src-tauri/src/models/import_v2_file.rs`: scan requests/results, format identity, skip reasons, pack status, resource estimates.
- `src-tauri/src/services/import_v2/capability_pack.rs`: signed manifest validation, version selection, health state, pack process factory.
- `src-tauri/src/services/import_v2/file_discovery.rs`: bounded recursive discovery and format sniffing.
- `src-tauri/src/services/import_v2/file_router.rs`: deterministic route graph and fallback policy.
- `src-tauri/src/services/import_v2/pack_engine.rs`: `ImportEngine` adapter for JSON-RPC capability processes.
- `src-tauri/src/services/import_v2/native_file_engine.rs`: Markdown/text/CSV/local-HTML deterministic route.
- `src-tauri/src/commands/import_v2_file_commands.rs`: thin discovery/capability commands.
- `src/types/importV2File.ts`: TypeScript mirror only; no UI.
- `capabilities/*/manifest.json`: pinned pack declarations; binaries/models are release artifacts, not committed blobs.
- `tests/fixtures/import-v2/golden/files/`: project-owned fixtures and assertion manifests.
- `src-tauri/tests/import_v2_file_ingestion.rs`: backend end-to-end/security/performance contract.

## Open-Source Route Decision

| Route | Role | License / maturity | Size / platform decision |
| --- | --- | --- | --- |
| MarkItDown | general fallback and comparison baseline | MIT, Microsoft-maintained; official docs warn it is not a high-fidelity human converter | selective extras only; signed Python pack for Windows/macOS/Linux |
| Docling | PDF layout, reading order, tables | MIT, active and mature enough for a gated pack | large Python/model pack, on demand; text-layer route runs first |
| LibreOffice | DOC/XLS/PPT to OOXML | MPL/LGPL, long-lived | very large isolated pack; never linked into core |
| Office Oxide | direct Office fallback | MIT/Apache-2.0 but young 0.1.x project | probationary native pack; cannot become primary until corpus gate passes |
| Tesseract / PaddleOCR | basic and accurate CJK OCR | Apache-2.0; established / active | separate model packs; Paddle published only on reproducible supported triples |
| whisper.cpp / FFmpeg | local ASR and media preprocessing | MIT / strict LGPL build | small binary plus downloaded models; FFmpeg build flags and SBOM are fixed |

---

### Task 1: Freeze File Contracts and Golden Corpus

**Files:**
- Create: `src-tauri/src/models/import_v2_file.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src/types/importV2File.ts`
- Create: `src/types/importV2File.test.ts`
- Create: `tests/fixtures/import-v2/golden/files/manifest.json`
- Create: `src-tauri/tests/import_v2_file_contracts.rs`

**Interfaces:**
- Consumes: Core `ImportInput`, `ImportItem`, `ImportIssue`, `ImportArtifact`, `QualityReport`.
- Produces: `FileFormat`, `FileIdentity`, `FileSkipReason`, `DiscoveredFile`, `FileScanPolicy`, `FileScanResult`, `CapabilityRequirement` with camelCase serialization mirrored exactly in TypeScript.

- [ ] **Step 1: Write failing Rust and TypeScript contract tests**

```rust
#[test]
fn file_contract_serializes_stable_wire_names() {
    let value = serde_json::to_value(DiscoveredFile {
        source_path: r"C:\资料\报告.docx".into(),
        relative_path: "资料/报告.docx".into(),
        display_name: "报告.docx".into(),
        format: FileFormat::Docx,
        size_bytes: 42,
        identity: FileIdentity { extension: "docx".into(), magic: "zip_ooxml".into(), mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into() },
    }).unwrap();
    assert_eq!(value["format"], "docx");
    assert_eq!(value["relativePath"], "资料/报告.docx");
}
```

```ts
expect(FILE_FORMATS).toEqual(["markdown", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf"]);
expect(FILE_SKIP_REASONS).toContain("symlink_or_reparse_point");
```

- [ ] **Step 2: Run tests and verify RED**

Run: `npm run test -- src/types/importV2File.test.ts` and `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test import_v2_file_contracts`

Expected: TypeScript module missing and Rust import types unresolved.

- [ ] **Step 3: Implement exact DTOs and fixture manifest schema**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat { Markdown, Doc, Docx, Xls, Xlsx, Ppt, Pptx, Pdf }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileScanPolicy {
    pub max_depth: u32,
    pub max_files: u32,
    pub max_file_bytes: u64,
    pub include_hidden: bool,
}
```

The fixture manifest must store `format`, expected headings, ordered text sentinels, table cell sentinels, image count, page/sheet/slide count, minimum normalized coverage, and expected warning codes for every sample.

- [ ] **Step 4: Run contract tests and full check**

Expected: focused tests pass; `npm run check` exits 0.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models/import_v2_file.rs src-tauri/src/models/mod.rs src/types/importV2File.ts src/types/importV2File.test.ts tests/fixtures/import-v2/golden/files/manifest.json src-tauri/tests/import_v2_file_contracts.rs
git commit -m "test(import): freeze file ingestion contracts"
```

### Task 2: Add Signed Capability Pack Management

**Files:**
- Create: `src-tauri/src/services/import_v2/capability_pack.rs`
- Create: `src-tauri/src/services/import_v2/pack_engine.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Create: `src-tauri/tests/import_v2_capability_packs.rs`
- Create: `capabilities/document-standard/manifest.json`

**Interfaces:**
- Consumes: Core `ImportEngine`, `EngineDescriptor`, `EngineRequest`, `EngineResult`, `JsonRpcRequest<T>`, and `JsonRpcResponse<T>`.
- Produces: `CapabilityPackManifest`, `CapabilityPackManager::resolve(&CapabilityRequirement) -> Result<ResolvedCapabilityPack, BackendError>`, and `PackProcessEngine` registered through `ImportV2Service::register_engine`.

- [ ] **Step 1: Write failing tests for signature/hash/version/platform and health rollback**

```rust
#[test]
fn rejects_manifest_when_archive_hash_or_protocol_differs() {
    let err = manager.resolve_from_fixture("bad-hash.json").unwrap_err();
    assert_eq!(err.code, IMPORT_V2_CAPABILITY_INVALID);
}
```

Test an unsigned manifest, unsupported target triple, protocol mismatch, changed license expression, failed health check, path traversal in entrypoint, and selection of the last healthy side-by-side version.

- [ ] **Step 2: Run focused test and verify RED**

Expected: `CapabilityPackManager` is undefined.

- [ ] **Step 3: Implement manifest and process adapter**

```rust
pub struct CapabilityPackManifest {
    pub schema_version: u32,
    pub pack_id: String,
    pub version: String,
    pub protocol_version: String,
    pub target_triples: Vec<String>,
    pub archive_sha256: String,
    pub license_expression: String,
    pub entrypoint: String,
    pub compressed_bytes: u64,
    pub installed_bytes: u64,
}
```

`PackProcessEngine::execute` must invoke an already-installed immutable entrypoint without a shell, send one JSON-RPC request over stdin, parse structured progress/result messages, terminate the entire process tree on cancellation/timeout, and reject every artifact outside the request staging root.

- [ ] **Step 4: Verify pack tests and Core regression suite**

Run focused pack tests, `cargo test ... import_v2_core`, then `npm run check`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/capability_pack.rs src-tauri/src/services/import_v2/pack_engine.rs src-tauri/src/services/import_v2/mod.rs src-tauri/src/services/import_v2/orchestrator.rs src-tauri/tests/import_v2_capability_packs.rs capabilities/document-standard/manifest.json
git commit -m "feat(import): add signed capability pack runtime"
```

### Task 3: Implement Bounded Folder and Multi-File Discovery

**Files:**
- Create: `src-tauri/src/services/import_v2/file_discovery.rs`
- Create: `src-tauri/src/commands/import_v2_file_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/models/import_v2_file.rs`
- Create: `src-tauri/tests/import_v2_file_discovery.rs`

**Interfaces:**
- Produces: `FileDiscoveryService::scan(context, roots, policy, on_batch, is_cancelled) -> Result<FileScanResult, BackendError>` and thin command `add_import_paths_v2(AddImportPathsV2Request) -> Result<ImportSession, BackendError>`.
- `AddImportPathsV2Request` contains project identity, session ID, and source paths only; it never contains target paths.

- [ ] **Step 1: Write RED tests for repeated drops, recursion, skips, cycles, and path portability**

Create fixtures with two selected files plus a folder, nested CJK names, a symlink/junction, hidden file, unsupported executable, current project root, excessive depth, excessive file count, case-only collisions, and decomposed/composed Unicode names.

- [ ] **Step 2: Run and confirm missing discovery service**

- [ ] **Step 3: Implement breadth-first bounded scan and format sniffing**

```rust
pub fn identify_file(path: &Path, prefix: &[u8]) -> Result<FileIdentity, BackendError>;
```

Use extension + magic + MIME agreement. ZIP is not automatically DOCX/XLSX/PPTX: inspect `[Content_Types].xml` and required OOXML roots under the existing archive limits. Stream batches as soon as discovered; do not collect an unbounded tree before reporting.

- [ ] **Step 4: Add command contract/registration tests and verify GREEN**

Assert a second `add_import_paths_v2` appends unique items to the same session and that skipped entries appear in `FileScanResult` without creating failed placeholder items.

- [ ] **Step 5: Run `npm run check` and commit**

```bash
git add src-tauri/src/services/import_v2/file_discovery.rs src-tauri/src/commands/import_v2_file_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/models/import_v2_file.rs src-tauri/tests/import_v2_file_discovery.rs
git commit -m "feat(import): discover folders and multiple files safely"
```

### Task 4: Add Native Markdown, Text, CSV, and Local HTML Engine

**Files:**
- Create: `src-tauri/src/services/import_v2/native_file_engine.rs`
- Create: `src-tauri/src/services/import_v2/markdown_normalizer.rs`
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Create: `src-tauri/tests/import_v2_native_file_engine.rs`

**Interfaces:**
- Produces an `ImportEngine` with descriptor `{ engine_id: "builtin.native-file", engine_version: env!("CARGO_PKG_VERSION"), route: "file.native" }`.
- Engine writes `source.bin`, `document.md`, optional assets, and `metadata.json` below the supplied staging root and returns `EngineResult`.

- [ ] **Step 1: Write failing fidelity/security tests**

Cover frontmatter preservation, GFM tables, relative images, BOM/charset, CSV quoting, local HTML script/event removal, `javascript:` links, data-URI limits, and source snapshot byte equality.

- [ ] **Step 2: Verify RED, then implement minimal deterministic engine**

```rust
impl ImportEngine for NativeFileEngine {
    fn supports(&self, input: &ImportInput) -> bool;
fn execute(
    &self,
    request: &EngineRequest,
    cancellation: &CancellationToken,
) -> Result<EngineResult, BackendError>;
}
```

Never rewrite the original bytes. Normalize generated Markdown line endings and frontmatter only in the candidate.

- [ ] **Step 3: Verify Quality Gate integration**

Assert unsafe HTML hard-fails and ordinary fidelity losses produce typed warnings rather than silent omission.

- [ ] **Step 4: Run full check and commit**

```bash
git add src-tauri/src/services/import_v2/native_file_engine.rs src-tauri/src/services/import_v2/markdown_normalizer.rs src-tauri/src/services/import_v2/orchestrator.rs src-tauri/tests/import_v2_native_file_engine.rs
git commit -m "feat(import): add native lightweight document engine"
```

### Task 5: Build Modern Office Primary Routes and MarkItDown Fallback

**Files:**
- Create: `capabilities/document-standard/runner/`
- Create: `capabilities/document-standard/requirements.lock`
- Create: `capabilities/office-oxide/manifest.json`
- Create: `src-tauri/src/services/import_v2/file_router.rs`
- Create: `src-tauri/tests/import_v2_office_routes.rs`
- Create: `scripts/verify-import-pack.ps1`

**Interfaces:**
- `FileRoutePlanner::plan(FileFormat, CapabilitySnapshot) -> Vec<RouteAttempt>` returns ordered routes.
- DOCX/XLSX/PPTX route order: project modern parser -> MarkItDown fallback -> qualified Office Oxide -> Agent eligibility.

- [ ] **Step 1: Write route-order and Golden Corpus RED tests**

For DOCX assert headings/lists/footnotes/links/tables/images; XLSX formulas + displayed values/hidden sheets/range truncation; PPTX titles/body/notes/tables/images/charts. Require normalized text coverage >= 0.98 for clean text documents and exact sheet/slide counts.

- [ ] **Step 2: Build a locked MarkItDown pack without runtime installation**

The release build downloads pinned wheels in CI, verifies hashes, creates a self-contained archive, emits SBOM/licenses, and records actual archive/install/RSS numbers in the manifest. `requirements.lock` includes only `[docx,xlsx,pptx]` dependencies; PDF is excluded from this pack.

- [ ] **Step 3: Implement route attempts and probation gate**

```rust
pub struct RouteAttempt {
    pub route: &'static str,
    pub required_pack: Option<&'static str>,
    pub quality_floor: QualityFloor,
}
```

Office Oxide stays disabled by default until `office-oxide-qualification.json` records all critical assertions passing on Windows/macOS/Linux and no security/fuzz blocker.

- [ ] **Step 4: Run corpus, pack verification, and full check**

Expected: every route attempt produces an `AttemptRecord`; fallback occurs only after typed deterministic failure/quality rejection.

- [ ] **Step 5: Commit**

```bash
git add capabilities/document-standard capabilities/office-oxide src-tauri/src/services/import_v2/file_router.rs src-tauri/tests/import_v2_office_routes.rs scripts/verify-import-pack.ps1
git commit -m "feat(import): route modern Office conversion packs"
```

### Task 6: Add Isolated Legacy Office Conversion

**Files:**
- Create: `capabilities/office-legacy/manifest.json`
- Create: `capabilities/office-legacy/runner/`
- Modify: `src-tauri/src/services/import_v2/file_router.rs`
- Create: `src-tauri/tests/import_v2_legacy_office.rs`

**Interfaces:**
- DOC/XLS/PPT route: LibreOffice conversion -> validate OOXML -> modern Office route -> qualified Office Oxide direct parse -> Agent eligibility.
- Produces converted OOXML as cache artifact, never `SourceSnapshot`.

- [ ] **Step 1: Write RED tests for macro/network/profile isolation and conversion validation**

Assert a disposable profile, disabled macros/plugins/external updates, no inherited user profile, process-tree cancellation, signature validation, reopen validation, page/sheet/slide count comparison, and warning emission for OLE/ActiveX/animation loss.

- [ ] **Step 2: Implement argument-safe headless runner**

Invoke LibreOffice without a shell and with a unique `-env:UserInstallation=file:///...` profile. Reject any output not matching the expected OOXML magic/content type.

- [ ] **Step 3: Add Office Oxide direct fallback behind qualification**

The fallback receives original bytes read-only, writes only staging, and records its exact version in `AttemptRecord`.

- [ ] **Step 4: Verify cancellation, timeout, crash cleanup, corpus, and full check**

- [ ] **Step 5: Commit**

```bash
git add capabilities/office-legacy src-tauri/src/services/import_v2/file_router.rs src-tauri/tests/import_v2_legacy_office.rs
git commit -m "feat(import): convert legacy Office in isolated packs"
```

### Task 7: Add PDF Text-Layer and Docling Layout Routes

**Files:**
- Create: `capabilities/document-layout/manifest.json`
- Create: `capabilities/document-layout/runner/`
- Create: `src-tauri/src/services/import_v2/pdf_router.rs`
- Create: `src-tauri/tests/import_v2_pdf_routes.rs`

**Interfaces:**
- `PdfInspection` records page count, text characters per page, image-only pages, encryption, active actions, and estimated OCR pages.
- Route order: safe text layer -> Docling layout -> page-selective OCR -> Agent eligibility.

- [ ] **Step 1: Write frozen PDF RED tests**

Cover single/multi-column, repeated headers/footers, tables, mixed scan/text, encrypted, corrupt, JavaScript/actions, malformed object graph, CJK fonts, and 300 DPI clean scans.

- [ ] **Step 2: Implement cheap inspection and page-level route selection**

```rust
pub struct PdfPagePlan { pub page_index: u32, pub route: PdfPageRoute, pub reason: String }
```

Only low-text/low-quality pages enter OCR. Reject password-required documents with `user_action_required=true`; password remains process memory only.

- [ ] **Step 3: Build pinned Docling pack and normalize its structured output**

Convert tables/figures/reading order into the common Markdown contract; record page anchors and OCR provenance in metadata.

- [ ] **Step 4: Verify quality thresholds and full check**

Require 100% page count, no active content, >=98% normalized coverage for clean text PDFs, and warnings below scan OCR thresholds.

- [ ] **Step 5: Commit**

```bash
git add capabilities/document-layout src-tauri/src/services/import_v2/pdf_router.rs src-tauri/tests/import_v2_pdf_routes.rs
git commit -m "feat(import): add layout-aware PDF ingestion"
```

### Task 8: Add Basic and Accurate OCR Packs

**Files:**
- Create: `capabilities/ocr-basic/manifest.json`
- Create: `capabilities/ocr-cjk-accurate/manifest.json`
- Create: `src-tauri/src/services/import_v2/ocr_router.rs`
- Create: `src-tauri/tests/import_v2_ocr.rs`

**Interfaces:**
- `OcrRequest` contains page image paths, languages, layout hints, and thresholds; `OcrPageResult` contains text blocks, confidence, coordinates, and engine/model versions.

- [ ] **Step 1: Write RED tests for language/model routing and accuracy**

Use clean 300 DPI English/simplified/traditional Chinese scans, rotated/noisy scans, tables, and low-confidence regions. Require >=95% character accuracy for clean bilingual scans and >=95% non-empty table-cell accuracy.

- [ ] **Step 2: Implement Tesseract basic route**

Ship only pinned binary and selected traineddata files; validate model hashes and language availability before task start.

- [ ] **Step 3: Implement PaddleOCR optional route**

Publish only build triples proven in CI. If unavailable, set `waiting_capability` with explicit download size/license/disk requirement; never silently invoke cloud OCR.

- [ ] **Step 4: Verify model download interruption, cancellation, cache keys, and full check**

Cache key includes input hash, page preprocessing version, engine version, language set, and model hash.

- [ ] **Step 5: Commit**

```bash
git add capabilities/ocr-basic capabilities/ocr-cjk-accurate src-tauri/src/services/import_v2/ocr_router.rs src-tauri/tests/import_v2_ocr.rs
git commit -m "feat(import): add tiered local OCR packs"
```

### Task 9: Enforce Spreadsheet and Presentation Output Contracts

**Files:**
- Create: `src-tauri/src/services/import_v2/office_postprocess.rs`
- Modify: `src-tauri/src/services/import_v2/quality_gate.rs`
- Create: `src-tauri/tests/import_v2_office_quality.rs`

**Interfaces:**
- `WorkbookPlan` selects single-page, overview-plus-sheet-pages, or chunked output without changing the single source/version identity.
- `PresentationPlan` emits one ordered candidate document with slide anchors, speaker notes, tables, charts, and meaningful images.

- [ ] **Step 1: Write RED tests for size tiers and no silent truncation**

Assert formula and displayed value coexist, hidden sheets are named, every truncation has a warning and attached CSV, huge ranges are streamed, decorative slide images are excluded by deterministic rules, and speaker notes remain associated with the correct slide.

- [ ] **Step 2: Implement post-processing plans with bounded buffers**

```rust
pub enum WorkbookOutputMode { SinglePage, OverviewAndSheets, Chunked { rows_per_chunk: u32 } }
```

- [ ] **Step 3: Extend Quality Gate metrics**

Add `sheet_count_exact`, `slide_count_exact`, `non_empty_cell_coverage`, `formula_value_pairs`, and `meaningful_image_coverage` without weakening existing Core safety checks.

- [ ] **Step 4: Run adversarial large-file tests and full check**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/office_postprocess.rs src-tauri/src/services/import_v2/quality_gate.rs src-tauri/tests/import_v2_office_quality.rs
git commit -m "feat(import): enforce Office quality contracts"
```

### Task 10: Add Optional Media Preprocessing and Local ASR

**Files:**
- Create: `capabilities/media-runtime/manifest.json`
- Create: `capabilities/asr-whisper/manifest.json`
- Create: `src-tauri/src/services/import_v2/media_router.rs`
- Create: `src-tauri/tests/import_v2_media.rs`

**Interfaces:**
- Subtitle priority: human platform/local subtitle -> automatic subtitle -> embedded subtitle -> whisper.cpp ASR.
- Output artifacts: `Subtitle`, timestamped `Markdown`, cover/metadata; raw audio/video remains temporary.

- [ ] **Step 1: Write RED tests for subtitle priority and strict cleanup**

Assert no ASR when a preferred subtitle exists, strict-LGPL FFmpeg manifest flags, process-tree cancellation, model hash selection, temporary media deletion on success/failure/restart, and no video under formal raw paths.

- [ ] **Step 2: Build strict-LGPL FFmpeg and whisper.cpp packs**

Manifest records FFmpeg configure flags `--disable-gpl --disable-nonfree`, full component/license inventory, and source/build recipe. Models are separate resumable signed downloads.

- [ ] **Step 3: Implement streaming media route**

Use bounded chunks and backpressure; emit VTT/SRT and timestamped Markdown with engine/model version and language confidence.

- [ ] **Step 4: Run media tests and full check**

- [ ] **Step 5: Commit**

```bash
git add capabilities/media-runtime capabilities/asr-whisper src-tauri/src/services/import_v2/media_router.rs src-tauri/tests/import_v2_media.rs
git commit -m "feat(import): add local subtitle and ASR routing"
```

### Task 11: Integrate Routes, Errors, Progress, and Recovery

**Files:**
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Modify: `src-tauri/src/services/import_v2/file_router.rs`
- Modify: `src-tauri/src/models/import_v2.rs`
- Modify: `src/types/importV2.ts`
- Create: `src-tauri/tests/import_v2_file_orchestration.rs`

**Interfaces:**
- Extends `ImportIssue` with stable file error codes and recommended recovery actions while preserving current fields.
- Reuses Core task progress, cancellation, `AttemptRecord`, `PreviewReady`, partial commit, and `ImportBatchResult` semantics.

- [ ] **Step 1: Write RED orchestration tests**

Cover capability missing, password required, corrupt file, resource limit, conversion failure, parse failure, quality fail, cancellation at each route, app restart, two files with one failure, and repeated same-session additions.

- [ ] **Step 2: Add stable recovery actions**

```rust
pub enum ImportRecoveryAction { InstallCapability, Retry, SwitchParser, EnableOcr, InvokeAgent, Skip, ViewLog }
```

Hard deterministic failure exposes `InvokeAgent`; low-quality success exposes a manual optimization action but does not auto-run Agent in this package.

- [ ] **Step 3: Verify Core invariants**

Assert engines only write staging, Quality Gate runs once per final candidate, formal targets are backend-derived, commit prevalidates the full decision set, and an interrupted pack cannot bypass preflight reconciliation.

- [ ] **Step 4: Run full check and commit**

```bash
git add src-tauri/src/services/import_v2/orchestrator.rs src-tauri/src/services/import_v2/file_router.rs src-tauri/src/models/import_v2.rs src/types/importV2.ts src-tauri/tests/import_v2_file_orchestration.rs
git commit -m "feat(import): integrate deterministic file routes"
```

### Task 12: Release-Gate File Ingestion

**Files:**
- Create: `src-tauri/tests/import_v2_file_ingestion.rs`
- Create: `docs/qa/import-v2-file-ingestion.md`
- Modify: `SPEC/progress.txt`

**Interfaces:**
- Produces a release evidence report; no new production interface.

- [ ] **Step 1: Run complete Golden Corpus and adversarial suites**

Record per-format critical assertions, thresholds, engine versions, package sizes, installed sizes, peak RSS, runtime, and warnings. Fail if any critical assertion is silently lost.

- [ ] **Step 2: Run three-platform and performance gates**

Windows/macOS/Linux must scan a 10,000-entry fixture with first batch visible within 1 second on the documented reference device; cancellation enters cancelling within 1 second and all pack processes exit within 5 seconds. Verify the scheduler preserves max(20% free memory, 1 GiB).

- [ ] **Step 3: Run security/recovery gates**

Cover archive bombs, path traversal, symlink/reparse, macro/ActiveX, PDF actions, malicious HTML, password secrecy, disk-full, package corruption, download interruption, process crash, app crash, restart recovery, external Wiki edit, and partial success.

- [ ] **Step 4: Run final verification and two reviews**

Run `npm run check` from a clean target. Review A checks design/contract integration; Review B starts fresh and attacks containment, secrets, process cleanup, resource bounds, and missing corpus assertions. Fix valid findings and rerun from the beginning.

- [ ] **Step 5: Record milestone and commit**

Insert a newest-first `SPEC/progress.txt` entry without altering history, then commit only plan implementation evidence.

```bash
git add src-tauri/tests/import_v2_file_ingestion.rs docs/qa/import-v2-file-ingestion.md SPEC/progress.txt
git commit -m "test(import): certify file ingestion release gates"
```

## Self-Review Result

- Spec coverage: folder/multi-file/repeated drops, all declared formats, legacy conversion, PDF/OCR, Office quality, media/ASR, immutable source, staging-only engines, partial success, cancellation/recovery, capability distribution, security, performance, and three-platform gates are assigned to Tasks 1–12.
- Placeholder scan: no deferred implementation marker or undefined neighboring interface remains.
- Type/API consistency: all engines consume Core `EngineRequest` and `CancellationToken`, then return `EngineResult`; discovery adds inputs to Core sessions; formal writes remain solely in `ImportV2Service::commit_items_cancellable` and its private commit module.
- Dependency order: Tasks 1–4 establish contracts/runtime/native route; Tasks 5–10 add independent deterministic routes; Task 11 integrates them; Task 12 is the release gate.
